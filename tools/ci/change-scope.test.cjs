const assert = require("node:assert/strict");
const { test } = require("node:test");
const { execFileSync, spawnSync } = require("node:child_process");
const { mkdtempSync, writeFileSync, readFileSync, mkdirSync, rmSync } = require("node:fs");
const { tmpdir } = require("node:os");
const { join, resolve } = require("node:path");
const { needsFullSuite } = require("./change-scope.cjs");

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
    "package-lock.json",
    ".github/workflows/ci.yml",
    "uf.config.js",
    "new-input.txt",
  ])
    assert.equal(needsFullSuite(["README.md", path]), true, path);
  assert.equal(needsFullSuite([]), true);
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
    assert.equal(readFileSync(output, "utf8"), "full=true\n");
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
    const output = join(dir, "output");
    execFileSync(process.execPath, [resolve(__dirname, "change-scope.cjs")], {
      cwd: dir,
      env: { ...process.env, BASE_SHA: base, GITHUB_OUTPUT: output },
    });
    assert.equal(readFileSync(output, "utf8"), "full=true\n");
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
