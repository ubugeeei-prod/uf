const { execFileSync } = require("node:child_process");
const { appendFileSync } = require("node:fs");

const rootDocumentation = new Set([
  "README.md",
  "CONTRIBUTING.md",
  "CHANGELOG.md",
  "LICENSE",
  "AGENTS.md",
]);

// Unknown paths and missing history run the full suite. Check both sides of
// renames so moving source into docs cannot hide a code change.
function needsFullSuite(paths /*: $ReadOnlyArray<string> */) /*: boolean */ {
  return (
    paths.length === 0 ||
    paths.some(
      (path) =>
        !(rootDocumentation.has(path) || path.startsWith("docs/") || path.startsWith("brand/")),
    )
  );
}

// What "RSC and browser testing" exercises: the RSC fixtures it runs, the
// packages those fixtures and `createTestApp` load, the RSC analysis, and the
// scripts that drive the job. A pull request touching any of them runs that
// job, rather than leaving it to the release queue to be the first to find
// out (#1434). Kept narrow on purpose: it starts a browser and builds an app,
// and most code changes cannot reach either.
const RSC_LANE = [
  "packages/router/",
  "packages/server/",
  "packages/vite/",
  "packages/test/",
  "packages/react-testing/",
  "packages/host/",
  "crates/uf_rsc/",
  "crates/uf_cli/tests/fixtures/rsc-test-app/",
  "crates/uf_cli/tests/fixtures/mcp-dev-app/",
  "crates/uf_cli/tests/fixtures/browser-interactions/",
  "tools/ci/test-browser.sh",
  "tools/ci/pinned-browser.sh",
  "tools/ci/change-scope.cjs",
  ".github/workflows/ci.yml",
];

/** Whether a change reaches what the RSC and browser job tests. */
function needsRscSuite(paths /*: $ReadOnlyArray<string> */) /*: boolean */ {
  return (
    paths.length === 0 || paths.some((path) => RSC_LANE.some((prefix) => path.startsWith(prefix)))
  );
}

// What the deploy matrix (`tools/deploy-matrix`, CI's `Deploy matrix` jobs)
// is about: the adapters and what they wrap, the build that writes them, the
// matrix itself, and the workflow and lockfile that run it. A pull request
// touching none of these skips the emulators; the release queue runs them
// whatever changed.
const deploymentPaths = [
  "packages/server/",
  "packages/router/",
  "packages/vite/",
  "packages/react/",
  "crates/uf_cli/src/commands/deploy",
  "crates/uf_cli/src/commands/build",
  "crates/uf_router/",
  "crates/uf_rsc/",
  "crates/uf_config/",
  "tools/deploy-matrix/",
  "tools/ci/edge-worker-smoke.sh",
  ".github/workflows/ci.yml",
  "package-lock.json",
];

/** Whether a change reaches what the deploy matrix tests. */
function touchesDeployment(paths /*: $ReadOnlyArray<string> */) /*: boolean */ {
  return (
    paths.length === 0 ||
    paths.some((path) => deploymentPaths.some((prefix) => path.startsWith(prefix)))
  );
}

if (require.main === module) {
  let full = true;
  let code = true;
  let rsc = true;
  let deploy = true;
  let release = false;
  let paths;
  const base = process.env.BASE_SHA ?? "";
  try {
    if (!/^[a-f0-9]{40}$/.test(base) || /^0+$/.test(base)) throw new Error("No base commit");
    paths = execFileSync("git", ["diff", "--name-only", "--no-renames", "-z", base, "HEAD"], {
      encoding: "utf8",
    })
      .split("\0")
      .filter(Boolean);
  } catch (error) {
    console.log(`Full suite: ${error.message}`);
  }
  if (paths) {
    // Authorization errors fail the job; they must never become a quick run.
    release = require("../release/policy.cjs").checkCandidate(base, paths);
    code = needsFullSuite(paths);
    rsc = needsRscSuite(paths);
    // The queue validates the final main merge once, before it can land.
    full = release && process.env.GITHUB_EVENT_NAME === "merge_group";
    rsc = rsc || full;
    deploy = full || touchesDeployment(paths);
    console.log(
      `${paths.length} changed files; code: ${String(code)}; rsc: ${String(rsc)}; deploy: ${String(deploy)}; full suite: ${String(full)}`,
    );
  }
  const version = release
    ? JSON.parse(require("node:fs").readFileSync("packages/core/package.json", "utf8")).version
    : "";
  // Written only on CI, where Actions names the file; a local run has nowhere to
  // write the answer and says so rather than passing `undefined` to `fs`.
  const output = process.env.GITHUB_OUTPUT;
  if (output == null) throw new Error("GITHUB_OUTPUT is not set");
  appendFileSync(
    output,
    `full=${String(full)}
code=${String(code)}
rsc=${String(rsc)}
release=${String(release)}
version=${version}
deploy=${String(deploy)}
`,
  );
}
module.exports = { needsFullSuite, needsRscSuite, touchesDeployment };
