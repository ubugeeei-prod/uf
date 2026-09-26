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
  "npm/router/",
  "npm/server/",
  "npm/vite/",
  "npm/test/",
  "npm/react-testing/",
  "npm/host/",
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
  "npm/server/",
  "npm/router/",
  "npm/vite/",
  "npm/react/",
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

// The Rust integration tests (`crates/*/tests/*.rs`) and the Deno library lane
// used to run only in the release queue, so a pull request could merge green
// while it broke one of them and the release PR was the first to find out: it
// was ejected from the queue twice for uf@0.2.0 (#1434). A pull request that
// can break them runs them now.

// A change here can break any crate, so every crate's integration tests run.
const RUST_WORKSPACE = ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml"];
// Not a crate, but what decides how the tests below run: a change to it runs
// `uf_cli`'s, so the pull request changing the lane is the one that exercises it.
const RUST_LANE = [
  "tools/ci/change-scope.cjs",
  "tools/ci/rust-integration.sh",
  ".github/workflows/ci.yml",
];
const CRATE = /^crates\/([a-z0-9_]+)\//;

/**
 * Whose integration tests a change reaches: "workspace" for every crate, else
 * the crates it touched, space-separated, plus `uf_cli` — whose tests hold
 * command-wide invariants (`uf explain`, completion, every command) that a
 * change to any crate can break — or "" for none.
 */
function rustIntegrationScope(
  paths /*: $ReadOnlyArray<string> */,
  exists /*: (path: string) => boolean */ = require("node:fs").existsSync,
) /*: string */ {
  if (paths.length === 0 || paths.some((path) => RUST_WORKSPACE.includes(path))) return "workspace";
  const crates /*: Set<string> */ = new Set();
  let touched = paths.some((path) => RUST_LANE.includes(path));
  for (const path of paths) {
    const crate = CRATE.exec(path)?.[1];
    if (crate == null) continue;
    touched = true;
    // A crate deleted by the change has nothing left to test, though `uf_cli`
    // still has its invariants to check.
    if (exists(`crates/${crate}/Cargo.toml`)) crates.add(crate);
  }
  if (!touched) return "";
  crates.add("uf_cli");
  return [...crates].sort().join(" ");
}

// What the Deno library lane runs: the library's own tests, the lane's script
// and exceptions, and the `uf` binary and host preload it runs them through.
// Every crate is in it because the binary is built from all of them. The lane
// itself takes well under a minute.
const DENO_LANE = [
  "npm/",
  "tests/library/",
  "tools/ci/deno-library",
  "crates/",
  "Cargo.toml",
  "Cargo.lock",
  "package.json",
  "package-lock.json",
  "uf.config.js",
  "tools/ci/change-scope.cjs",
  ".github/workflows/ci.yml",
];

/** Whether a change reaches what the Deno library lane tests. */
function needsDenoLibrary(paths /*: $ReadOnlyArray<string> */) /*: boolean */ {
  return (
    paths.length === 0 || paths.some((path) => DENO_LANE.some((prefix) => path.startsWith(prefix)))
  );
}

if (require.main === module) {
  let full = true;
  let code = true;
  let rsc = true;
  let deploy = true;
  let deno = true;
  let rustTests = "";
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
    deno = full || needsDenoLibrary(paths);
    // The full suite runs every integration test already.
    rustTests = full ? "" : rustIntegrationScope(paths);
    console.log(
      `${paths.length} changed files; code: ${String(code)}; rsc: ${String(rsc)}; deploy: ${String(deploy)}; deno: ${String(deno)}; rust integration: ${rustTests || "none"}; full suite: ${String(full)}`,
    );
  }
  const version = release
    ? JSON.parse(require("node:fs").readFileSync("npm/core/package.json", "utf8")).version
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
deno=${String(deno)}
rust_tests=${rustTests}
`,
  );
}
module.exports = {
  needsFullSuite,
  needsRscSuite,
  touchesDeployment,
  needsDenoLibrary,
  rustIntegrationScope,
};
