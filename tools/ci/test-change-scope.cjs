const assert = require("node:assert/strict");
const { test } = require("node:test");
const { execFileSync, spawnSync } = require("node:child_process");
const { mkdtempSync, writeFileSync, readFileSync, mkdirSync, rmSync } = require("node:fs");
const { tmpdir } = require("node:os");
const { join, resolve } = require("node:path");
const {
  needsFullSuite,
  needsRscSuite,
  touchesDeployment,
  needsDenoLibrary,
  rustIntegrationScope,
} = require("./change-scope.cjs");

test("documentation edits retain site checks without the workspace suite", () => {
  assert.equal(
    needsFullSuite(["README.md", "docs/app/guide/start/$page.mdx", "brand/icon.svg"]),
    false,
  );
});
test("source, dependencies, CI, unknown paths and empty diffs require the suite", () => {
  for (const path of [
    "crates/uf_cli/src/main.rs",
    "packages/host/register.js",
    "Cargo.lock",
    "README.js",
    "LICENSE.rs",
    "package-lock.json",
    ".github/workflows/ci.yml",
    "uf.config.js",
    "new-input.txt",
  ])
    assert.equal(needsFullSuite(["README.md", path]), true, path);
  assert.equal(needsFullSuite([]), true);
});
test("a change to what the RSC and browser job exercises runs it on the pull request", () => {
  for (const path of [
    "packages/router/internal/action-endpoint.js",
    "packages/server/fetch.js",
    "packages/vite/internal/flight.js",
    "packages/test/app.js",
    "crates/uf_rsc/src/lib.rs",
    "crates/uf_cli/tests/fixtures/rsc-test-app/tests/notes-app.test.js",
    "tools/ci/test-browser.sh",
    ".github/workflows/ci.yml",
  ])
    assert.equal(needsRscSuite(["README.md", path]), true, path);
  assert.equal(needsRscSuite([]), true);
});
test("a change nothing in the RSC and browser job reaches leaves it to the full suite", () => {
  for (const path of [
    "packages/ui/button.js",
    "crates/uf_fmt/src/lib.rs",
    "docs/app/guide/testing-server/$page.mdx",
    "crates/uf_cli/tests/fixtures/rsc-split-app/app/$page.js",
  ])
    assert.equal(needsRscSuite([path]), false, path);
});
test("the deploy matrix runs for adapters, the router, the build and itself", () => {
  for (const path of [
    "packages/server/lambda.js",
    "packages/router/internal/action-endpoint.js",
    "packages/vite/driver.js",
    "crates/uf_cli/src/commands/deploy/static_host.rs",
    "tools/deploy-matrix/matrix.json",
    ".github/workflows/ci.yml",
  ])
    assert.equal(touchesDeployment(["README.md", path]), true, path);
  for (const path of ["crates/uf_fmt/src/lib.rs", "packages/ui/index.js", "docs/app/guide/start/$page.mdx"])
    assert.equal(touchesDeployment([path]), false, path);
  assert.equal(touchesDeployment([]), true);
});
// #1434: these ran only in the release queue, which was the first to fail.
test("a crate change runs that crate's integration tests and uf_cli's", () => {
  const exists = (path) => !path.startsWith("crates/uf_gone/");
  // #1417 changed only uf_cli and broke `uf explain`'s invariant in its tests.
  assert.equal(rustIntegrationScope(["crates/uf_cli/src/commands/sqlc.rs"], exists), "uf_cli");
  assert.equal(
    rustIntegrationScope(["crates/uf_fmt/src/lib.rs", "crates/uf_check/tests/intl.rs", "README.md"], exists),
    "uf_check uf_cli uf_fmt",
  );
  // A crate the change deleted has no tests left to run.
  assert.equal(rustIntegrationScope(["crates/uf_gone/src/lib.rs"], exists), "uf_cli");
  for (const path of ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml"])
    assert.equal(rustIntegrationScope(["crates/uf_fmt/src/lib.rs", path], exists), "workspace", path);
  assert.equal(rustIntegrationScope([], exists), "workspace");
});
test("the lane's own files run uf_cli's integration tests, and nothing else runs any", () => {
  for (const path of [".github/workflows/ci.yml", "tools/ci/change-scope.cjs", "tools/ci/rust-integration.sh"])
    assert.equal(rustIntegrationScope([path]), "uf_cli", path);
  for (const path of ["packages/server/fetch.js", "docs/app/guide/start/$page.mdx", "tools/ci/test-browser.sh"])
    assert.equal(rustIntegrationScope([path]), "", path);
});
test("the Deno library lane runs for the library, its script and the binary it drives", () => {
  for (const path of [
    // #1384 added a Vite test file that cannot load under Deno.
    "packages/vite/build-passes.test.js",
    "tests/library/payload.test.js",
    "tools/ci/deno-library.js",
    "tools/ci/deno-library.test.js",
    // #1427 changed how the host reports a worker's death.
    "crates/uf_test/src/host.rs",
    "crates/uf_runtime/src/lib.rs",
    "Cargo.lock",
    "package-lock.json",
    ".github/workflows/ci.yml",
  ])
    assert.equal(needsDenoLibrary(["README.md", path]), true, path);
  for (const path of ["docs/app/guide/start/$page.mdx", "tools/ci/test-browser.sh", "examples/simple-sns-native/App.js"])
    assert.equal(needsDenoLibrary([path]), false, path);
  assert.equal(needsDenoLibrary([]), true);
});
test("missing history cannot suppress tests", () => {
  const dir = mkdtempSync(join(tmpdir(), "uf-ci-scope-"));
  try {
    const output = join(dir, "output");
    execFileSync(process.execPath, [resolve(__dirname, "change-scope.cjs")], {
      cwd: dir,
      env: { ...process.env, BASE_SHA: "a".repeat(40), GITHUB_OUTPUT: output },
      stdio: "pipe",
    });
    assert.equal(readFileSync(output, "utf8"), "full=true\ncode=true\nrsc=true\nrelease=false\nversion=\ndeploy=true\ndeno=true\nrust_tests=\n");
  } finally {
    rmSync(dir, { recursive: true });
  }
});
test("a source file moved into docs still runs the suite", () => {
  const dir = mkdtempSync(join(tmpdir(), "uf-ci-rename-"));
  try {
    const git = (...args) =>
      execFileSync("git", args, {
        cwd: dir,
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
      }).trim();
    git("init");
    git("config", "user.name", "Test");
    git("config", "user.email", "test@example.invalid");
    writeFileSync(join(dir, "source.js"), "export default 1;\n");
    git("add", ".");
    git("commit", "-m", "initial");
    const base = git("rev-parse", "HEAD");
    mkdirSync(join(dir, "docs"));
    git("mv", "source.js", "docs/source.js");
    git("commit", "-m", "move");
    const paths = git("diff", "--name-only", "--no-renames", base, "HEAD").split("\n");
    assert.equal(needsFullSuite(paths), true);
  } finally {
    rmSync(dir, { recursive: true });
  }
});
test("CI reuses its executable, fails if missing, and local tasks still build", () => {
  const dir = mkdtempSync(join(tmpdir(), "uf-ci-build-"));
  try {
    mkdirSync(join(dir, "target/release"), { recursive: true });
    const calls = join(dir, "calls");
    writeFileSync(join(dir, "cargo"), '#!/bin/sh\nprintf "%s\\n" "$*" >> "$CALLS"\n', {
      mode: 0o755,
    });
    const run = (prebuilt) =>
      spawnSync("sh", [resolve(__dirname, "build-toolchain.sh")], {
        cwd: dir,
        env: {
          ...process.env,
          PATH: `${dir}:${process.env.PATH}`,
          CALLS: calls,
          UF_CI_PREBUILT: prebuilt,
        },
      });
    assert.notEqual(run("1").status, 0);
    writeFileSync(join(dir, "target/release/uf"), "#!/bin/sh\nexit 0\n", {
      mode: 0o755,
    });
    assert.equal(run("1").status, 0);
    assert.equal(run("0").status, 0);
    assert.equal(readFileSync(calls, "utf8"), "build --release --bin uf\n");
  } finally {
    rmSync(dir, { recursive: true });
  }
});

// Exercise the emitted workflow outputs, including the release-artifact flag.
test("only the final release merge group gets full validation", () => {
  const { runInNewContext } = require("node:vm");
  const source = readFileSync(resolve(__dirname, "change-scope.cjs"), "utf8");
  for (const [event, release, full] of [
    ["pull_request", true, false],
    ["merge_group", true, true],
    ["merge_group", false, false],
    ["push", false, false],
  ]) {
    let output;
    const module = { exports: {} };
    const requireMock = (name) => {
      if (name === "node:child_process") return { execFileSync: () => "Cargo.toml\0" };
      if (name === "node:fs") return {
        appendFileSync: (_file, value) => { output = value; },
        readFileSync: () => JSON.stringify({ version: "0.0.0-alpha.46" }),
        existsSync: () => true,
      };
      if (name === "../release/policy.cjs") return { checkCandidate: () => release };
      throw new Error(`Unexpected dependency: ${name}`);
    };
    requireMock.main = module;
    runInNewContext(source, {
      require: requireMock, module,
      process: { env: { BASE_SHA: "a".repeat(40), GITHUB_EVENT_NAME: event, GITHUB_OUTPUT: "output" } },
      console: { log() {} },
    });
    // Cargo.toml reaches nothing the RSC job tests, so only the full run takes
    // it. It reaches every crate's integration tests and the binary the Deno
    // lane drives, so the pull request runs both; the full run has its own.
    assert.equal(output, `full=${full}\ncode=true\nrsc=${full}\nrelease=${release}\nversion=${release ? "0.0.0-alpha.46" : ""}\ndeploy=${full}\ndeno=true\nrust_tests=${full ? "" : "workspace"}\n`, event);
  }
});
