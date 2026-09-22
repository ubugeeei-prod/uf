const assert = require("node:assert/strict");
const { test } = require("node:test");
const {
  assertMaintainer,
  assertRequest,
  assertPullRequest,
  assertValidation,
  assertQueueBase,
  assertArtifacts,
} = require("./policy.cjs");
const { nextVersion } = require("./open-release.cjs");
const repository = "owner/project";
const commit = "a".repeat(40);
const request = { version: "0.0.0-alpha.46", branch: "release/v0.0.0-alpha.46" };

test("only maintainers and administrators may create a release", () => {
  for (const role_name of ["maintain", "admin"])
    assert.doesNotThrow(() => assertMaintainer({ role_name }));
  assert.doesNotThrow(() =>
    assertMaintainer({ role_name: "custom", user: { permissions: { maintain: true } } }),
  );
  for (const role_name of ["write", "triage", "read", undefined])
    assert.throws(() => assertMaintainer({ role_name, permission: "write" }));
});
test("the request must name its version and dedicated branch", () => {
  assertRequest(request, request.version);
  assert.throws(() => assertRequest(request, "0.0.0-alpha.47"));
  assert.throws(() => assertRequest({ ...request, branch: "feature" }, request.version));
  assert.throws(() =>
    assertRequest({ version: "$(false)", branch: "release/v$(false)" }, "$(false)"),
  );
});
test("release PRs must come from this repository, target main and have a human author", () => {
  const pr = {
    base: { ref: "main", repo: { full_name: repository } },
    head: { ref: request.branch, repo: { full_name: repository } },
    user: { type: "User" },
  };
  assertPullRequest(pr, request, repository);
  assert.throws(() =>
    assertPullRequest(
      { ...pr, head: { ...pr.head, repo: { full_name: "fork/project" } } },
      request,
      repository,
    ),
  );
  assert.throws(() =>
    assertPullRequest({ ...pr, base: { ...pr.base, ref: "develop" } }, request, repository),
  );
  assert.throws(() => assertPullRequest({ ...pr, user: { type: "Bot" } }, request, repository));
});
test("only successful queue validation of the exact commit is publishable", () => {
  const run = {
    path: ".github/workflows/ci.yml",
    event: "merge_group",
    head_sha: commit,
    head_repository: { full_name: repository },
    status: "completed",
    conclusion: "success",
  };
  assertValidation(run, commit, repository);
  for (const patch of [
    { head_sha: "b".repeat(40) },
    { event: "pull_request" },
    { event: "push" },
    { path: ".github/workflows/other.yml" },
    { status: "in_progress" },
    { conclusion: "failure" },
    { conclusion: "cancelled" },
    { head_repository: { full_name: "fork/project" } },
  ]) {
    assert.throws(
      () => assertValidation({ ...run, ...patch }, commit, repository),
      JSON.stringify(patch),
    );
  }
});
test("release versions are calculated from the checkout, without the installed CLI version", () => {
  assert.equal(nextVersion("0.0.0-alpha.45", "alpha"), "0.0.0-alpha.46");
  assert.equal(nextVersion("1.2.3", "alpha"), "1.2.4-alpha.1");
  assert.equal(nextVersion("1.2.3", "minor"), "1.3.0");
  assert.equal(nextVersion("1.2.3", "major"), "2.0.0");
  assert.equal(nextVersion("1.2.3", "patch"), "1.2.4");
  assert.throws(() => nextVersion("1.2.3", "nope"));
  assert.throws(() => nextVersion("1.2.3", "1.2.3"));
  assert.throws(() => nextVersion("1.2.3", "1.2.2"));
  assert.throws(() => nextVersion("1.2.3", "01.3.0"));
});

test("queue validation rejects an older main base", () => {
  assertQueueBase(commit, commit);
  assert.throws(() => assertQueueBase("b".repeat(40), commit));
  assert.throws(() => assertQueueBase(undefined, commit));
});

test("publication refuses missing or expired native archives before npm is touched", () => {
  const artifacts = ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu", "x86_64-apple-darwin", "aarch64-apple-darwin", "x86_64-pc-windows-msvc"].map((target) => ({ name: `uf-release-${target}`, expired: false, size_in_bytes: 100 }));
  assertArtifacts(artifacts);
  assert.throws(() => assertArtifacts(artifacts.slice(1)));
  assert.throws(() => assertArtifacts(artifacts.map((item, i) => i ? item : { ...item, expired: true })));
  assert.throws(() => assertArtifacts(artifacts.map((item, i) => i ? item : { ...item, size_in_bytes: 0 })));
});
