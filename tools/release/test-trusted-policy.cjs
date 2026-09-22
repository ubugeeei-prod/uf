const assert = require("node:assert/strict");
const { test } = require("node:test");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const workflow = fs.readFileSync(path.resolve(__dirname, "../../.github/workflows/release-policy.yml"), "utf8");
const source = workflow.split("          node <<'NODE'\n")[1].split("\n          NODE")[0].replace(/^ {10}/gm, "");

function check({ role = "maintain", files = [".github/release.json"], branch = "release/v1.0.0", comparison = "ahead", queueBase, fork = false } = {}) {
  const repo = "owner/project";
  const main = "a".repeat(40);
  const head = "b".repeat(40);
  const event = queueBase === undefined ? { pull_request: { number: 7 } } : { merge_group: { head_ref: `refs/heads/gh-readonly-queue/main/pr-7-${head}`, base_sha: queueBase } };
  const pr = { base: { ref: "main", sha: main }, head: { ref: branch, sha: head, repo: { full_name: fork ? "fork/project" : repo } }, user: { type: "User", login: "maintainer" }, changed_files: files.length };
  const response = (route) => {
    if (route === `repos/${repo}/pulls/7`) return pr;
    if (route.includes("/files?")) return files.map((filename) => ({ filename }));
    if (route.endsWith("/permission")) return { role_name: role };
    if (route.endsWith("/git/ref/heads/main")) return { object: { sha: main } };
    if (route.includes("/compare/")) return { status: comparison };
    throw new Error(`Unexpected API: ${route}`);
  };
  vm.runInNewContext(source, {
    require(name) {
      if (name === "node:fs") return { readFileSync: () => JSON.stringify(event) };
      if (name === "node:child_process") return { execFileSync(command, args) {
        assert.equal(command, "gh"); assert.equal(args[0], "api");
        return JSON.stringify(response(args[1]));
      } };
      throw new Error(`Unexpected module: ${name}`);
    },
    process: { env: { GITHUB_REPOSITORY: repo, GITHUB_EVENT_PATH: "event.json" } },
    console: { log() {} }, Buffer,
  });
}
test("trusted workflow accepts a maintainer release on current main", () => check());
test("ordinary PRs do not require release permissions", () => check({ role: "write", branch: "fix/typo", files: ["crates/uf_cli/src/lib.rs"] }));
test("a writer cannot release or replace the trusted release gate", () => {
  assert.throws(() => check({ role: "write" }), /maintain or admin/);
  assert.throws(() => check({ role: "write", branch: "fix/policy", files: [".github/workflows/release-policy.yml"] }), /maintain or admin/);
  assert.throws(() => check({ role: "write", branch: "fix/editors", files: [".github/workflows/editors.yml"] }), /maintain or admin/);
});
test("stale release heads, stale queue bases and fork releases are refused", () => {
  assert.throws(() => check({ comparison: "diverged" }), /current main/);
  assert.throws(() => check({ queueBase: "c".repeat(40) }), /current main/);
  check({ queueBase: "a".repeat(40) });
  assert.throws(() => check({ fork: true }), /branch in this repository/);
});
