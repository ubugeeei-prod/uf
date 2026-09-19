const { check } = require("./breaking-codemods.cjs");
const fs = require("node:fs"),
  path = require("node:path"),
  os = require("node:os");
const assert = require("node:assert/strict");
const { execFileSync } = require("node:child_process");
const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-breaking-codemods-"));
const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();
try {
  fs.mkdirSync(path.join(root, "tools/codemods"), { recursive: true });
  fs.writeFileSync(path.join(root, "tools/codemods/catalog.json"), "{}");
  fs.writeFileSync(path.join(root, "tools/codemods/breaking-changes.json"), "[]");
  fs.writeFileSync(path.join(root, "CHANGELOG.md"), "# Changes\n");
  git("init", "-q");
  git("config", "user.email", "fixture@example.invalid");
  git("config", "user.name", "Fixture");
  git("add", ".");
  git("-c", "commit.gpgsign=false", "commit", "-qm", "chore: initial");
  const base = git("rev-parse", "HEAD");
  git("-c", "commit.gpgsign=false", "commit", "--allow-empty", "-qm", "feat!: remove a setting");
  assert.equal(check(root, base).length, 1);
  fs.appendFileSync(
    path.join(root, "CHANGELOG.md"),
    "No codemod (feat!: remove a setting): replacement depends on the application's backend.\n",
  );
  assert.deepEqual(check(root, base), []);
  fs.writeFileSync(path.join(root, "CHANGELOG.md"), "");
  fs.writeFileSync(path.join(root, "implementation.rs"), "// implementation");
  fs.writeFileSync(path.join(root, "old-config.js"), "// fixture");
  fs.writeFileSync(
    path.join(root, "tools/codemods/catalog.json"),
    JSON.stringify({ settings: { implementation: "implementation.rs", fixture: "old-config.js" } }),
  );
  fs.writeFileSync(
    path.join(root, "tools/codemods/breaking-changes.json"),
    JSON.stringify([{ change: "feat!: remove a setting", codemod: "settings" }]),
  );
  assert.deepEqual(check(root, base), []);
  fs.unlinkSync(path.join(root, "old-config.js"));
  assert.equal(check(root, base).length, 1);
  process.stdout.write(
    "Breaking-change coverage: rejection, explanation, codemod and missing fixture passed.\n",
  );
} finally {
  fs.rmSync(root, { recursive: true, force: true });
}
