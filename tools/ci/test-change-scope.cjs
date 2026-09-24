const assert = require("node:assert/strict");
const { test } = require("node:test");
const { execFileSync, spawnSync } = require("node:child_process");
const { mkdtempSync, writeFileSync, readFileSync, mkdirSync, rmSync } = require("node:fs");
const { tmpdir } = require("node:os");
const { join, resolve } = require("node:path");
const { needsFullSuite, needsRscSuite, touchesDeployment } = require("./change-scope.cjs");

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
test("missing history cannot suppress tests", () => {
  const dir = mkdtempSync(join(tmpdir(), "uf-ci-scope-"));
  try {
    const output = join(dir, "output");
    execFileSync(process.execPath, [resolve(__dirname, "change-scope.cjs")], {
      cwd: dir,
      env: { ...process.env, BASE_SHA: "a".repeat(40), GITHUB_OUTPUT: output },
      stdio: "pipe",
    });
    assert.equal(readFileSync(output, "utf8"), "full=true\ncode=true\nrsc=true\nrelease=false\nversion=\ndeploy=true\n");
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
    // Cargo.toml reaches nothing the RSC job tests, so only the full run takes it.
    assert.equal(output, `full=${full}\ncode=true\nrsc=${full}\nrelease=${release}\nversion=${release ? "0.0.0-alpha.46" : ""}\ndeploy=${full}\n`, event);
  }
});
