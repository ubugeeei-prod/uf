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
function needsFullSuite(paths) {
  return (
    paths.length === 0 ||
    paths.some(
      (path) =>
        !(rootDocumentation.has(path) || path.startsWith("docs/") || path.startsWith("brand/")),
    )
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

function touchesDeployment(paths) {
  return (
    paths.length === 0 ||
    paths.some((path) => deploymentPaths.some((prefix) => path.startsWith(prefix)))
  );
}

if (require.main === module) {
  let full = true;
  let code = true;
  let release = false;
  let deploy = true;
  let paths;
  const base = process.env.BASE_SHA;
  try {
    if (!/^[a-f0-9]{40}$/.test(base || "") || /^0+$/.test(base)) throw new Error("No base commit");
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
    // The queue validates the final main merge once, before it can land.
    full = release && process.env.GITHUB_EVENT_NAME === "merge_group";
    deploy = full || touchesDeployment(paths);
    console.log(
      `${paths.length} changed files; code: ${code}; full suite: ${full}; deploy matrix: ${deploy}`,
    );
  }
  const version = release
    ? JSON.parse(require("node:fs").readFileSync("packages/core/package.json", "utf8")).version
    : "";
  appendFileSync(
    process.env.GITHUB_OUTPUT,
    `full=${full}\ncode=${code}\nrelease=${release}\nversion=${version}\ndeploy=${deploy}\n`,
  );
}
module.exports = { needsFullSuite, touchesDeployment };
