const { execFileSync } = require("node:child_process");
const fs = require("node:fs");

const VERSION = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(?:alpha|beta|rc)\.(0|[1-9]\d*))?$/;
const SHA = /^[a-f0-9]{40}$/;
function assertMaintainer(permission) {
  if (
    !(
      permission.user?.permissions?.maintain ||
      permission.user?.permissions?.admin ||
      ["maintain", "admin"].includes(permission.role_name)
    )
  ) {
    throw new Error("Releases require repository maintain or admin permission.");
  }
}
function assertRequest(request, version) {
  if (
    !VERSION.test(version) ||
    request.version !== version ||
    request.branch !== `release/v${version}`
  ) {
    throw new Error("Invalid release request. Use uf run release -- <bump>.");
  }
}
function assertPullRequest(pr, request, repository) {
  if (
    pr.base.ref !== "main" ||
    pr.base.repo.full_name !== repository ||
    pr.head.repo?.full_name !== repository ||
    pr.head.ref !== request.branch ||
    pr.user.type !== "User"
  ) {
    throw new Error(
      "Release PR must be authored by a maintainer in this repository and target main.",
    );
  }
}
function assertValidation(run, commit, repository) {
  if (
    run.path !== ".github/workflows/ci.yml" ||
    run.event !== "merge_group" ||
    run.head_sha !== commit ||
    run.head_repository?.full_name !== repository ||
    run.status !== "completed" ||
    run.conclusion !== "success"
  ) {
    throw new Error("Release requires successful merge-queue CI for the exact merged commit.");
  }
}
function gh(...args) {
  return execFileSync("gh", args, { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 }).trim();
}
function api(path) {
  return JSON.parse(gh("api", path));
}
function git(...args) {
  return execFileSync("git", args, { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 }).trim();
}
// Everything below that reads GitHub or the checkout does it through `io`, so
// the tests can answer for both. The defaults are the real `gh api` and `git`.
const IO = { api, git };
function requestAt(ref, io = IO) {
  const request = JSON.parse(io.git("show", `${ref}:.github/release.json`));
  const version = JSON.parse(io.git("show", `${ref}:packages/core/package.json`)).version;
  assertRequest(request, version);
  return request;
}
function releasePR(request, repository, io = IO) {
  const owner = repository.split("/")[0];
  const prs = io.api(
    `repos/${repository}/pulls?state=all&base=main&head=${encodeURIComponent(`${owner}:${request.branch}`)}&per_page=100`,
  );
  if (prs.length !== 1) throw new Error("Expected exactly one release PR for this version.");
  const pr = io.api(`repos/${repository}/pulls/${prs[0].number}`);
  assertPullRequest(pr, request, repository);
  assertMaintainer(
    io.api(`repos/${repository}/collaborators/${encodeURIComponent(pr.user.login)}/permission`),
  );
  return pr;
}
function checkCandidate(base, paths) {
  const previous = JSON.parse(git("show", `${base}:packages/core/package.json`)).version;
  const current = JSON.parse(fs.readFileSync("packages/core/package.json", "utf8")).version;
  const requested = paths.includes(".github/release.json");
  if (previous === current && !requested) return false;
  if (!requested || previous === current)
    throw new Error("A version bump and release request must be committed together.");
  const request = requestAt("HEAD");
  const repository = process.env.GITHUB_REPOSITORY;
  const event = JSON.parse(fs.readFileSync(process.env.GITHUB_EVENT_PATH, "utf8"));
  const pr = releasePR(request, repository);
  if (event.pull_request && event.pull_request.number !== pr.number)
    throw new Error("The release request belongs to a different PR.");
  const main = api(`repos/${repository}/git/ref/heads/main`).object.sha;
  if (event.merge_group) {
    assertQueueBase(event.merge_group.base_sha, main);
    assertQueueTree(main, pr.head.sha);
  } else if (event.pull_request) {
    git("merge-base", "--is-ancestor", main, pr.head.sha);
  } else if (process.env.GITHUB_EVENT_NAME === "push") {
    if (!pr.merged || pr.merge_commit_sha !== process.env.GITHUB_SHA)
      throw new Error("A release must land through its validated PR.");
    return false; // The exact merge commit already passed the full queue suite.
  } else throw new Error("Unsupported release validation event.");
  return true;
}
function assertQueueTree(main, head, queued = "HEAD") {
  git("merge-base", "--is-ancestor", main, head);
  // A squash commit has main as its parent, not the PR head. Its tree must
  // still match the up-to-date PR exactly in our single-entry merge queue.
  if (git("rev-parse", `${head}^{tree}`) !== git("rev-parse", `${queued}^{tree}`))
    throw new Error("The release merge group must contain exactly the current release PR tree.");
}
function assertArtifacts(artifacts) {
  for (const target of [
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
  ]) {
    const artifact = artifacts.find((item) => item.name === `uf-release-${target}`);
    if (!artifact || artifact.expired || artifact.size_in_bytes <= 0)
      throw new Error(`Validated archive is missing or expired: ${target}`);
  }
}
function assertQueueBase(base, main) {
  if (!SHA.test(main || "") || base !== main)
    throw new Error("The merge queue must rebuild against current main.");
}
function assertTagTarget(repository, version, commit, io = IO) {
  const ref = `refs/tags/uf@${version}`;
  const found = io
    .api(`repos/${repository}/git/matching-refs/tags/uf@${encodeURIComponent(version)}`)
    .find((tag) => tag.ref === ref);
  if (!found) return;
  let object = found.object;
  for (let depth = 0; object.type === "tag" && depth < 5; depth++)
    object = io.api(`repos/${repository}/git/tags/${object.sha}`).object;
  if (object.type !== "commit" || object.sha !== commit)
    throw new Error("The release tag already points to another commit; it will not be replaced.");
}
/**
 * The account `GITHUB_TOKEN` acts as. A `workflow_dispatch` that
 * `release-automation.yml` sends with its token runs as this actor.
 */
const AUTOMATION_ACTOR = "github-actions[bot]";

/**
 * Who may start a publication run, and on whose authority.
 *
 * A person who dispatches `publish.yml` or `release.yml` by hand must be a
 * maintainer, which is how releases have always been authorized. When
 * `release-automation.yml` dispatches them, the actor is `GITHUB_TOKEN`'s bot,
 * whose repository permission says nothing about the release. That run
 * therefore stands on the release PR instead: a maintainer authored it
 * ([`releasePR`] checks that), it merged at this exact commit, and the merge
 * queue validated that commit. [`authorizePublication`] checks all three for
 * every dispatch, whoever sent it. Only a workflow in this repository, running
 * on `main` with `actions: write`, can act as the bot. A workflow change needs
 * a maintainer (`release-policy.yml`).
 */
function authorizeDispatcher(actor, repository, io = IO) {
  if (actor === AUTOMATION_ACTOR) return "automation";
  assertMaintainer(
    io.api(`repos/${repository}/collaborators/${encodeURIComponent(actor || "")}/permission`),
  );
  return "maintainer";
}
function authorizePublication(env = process.env, io = IO) {
  if (env.GITHUB_REF !== "refs/heads/main")
    throw new Error("Publication must be dispatched from main.");
  const repository = env.GITHUB_REPOSITORY;
  const { RELEASE_COMMIT: commit, RELEASE_VERSION: version, VALIDATION_RUN: runId } = env;
  if (!SHA.test(commit || "") || !VERSION.test(version || "") || !/^\d+$/.test(runId || ""))
    throw new Error("Invalid publication inputs.");
  const via = authorizeDispatcher(env.GITHUB_ACTOR, repository, io);
  const request = requestAt(commit, io);
  assertRequest(request, version);
  assertTagTarget(repository, version, commit, io);
  const pr = releasePR(request, repository, io);
  if (!pr.merged || pr.merge_commit_sha !== commit)
    throw new Error("The release PR has not merged at this commit.");
  const main = io.api(`repos/${repository}/git/ref/heads/main`).object.sha;
  const comparison = io.api(`repos/${repository}/compare/${commit}...${main}`);
  if (!["ahead", "identical"].includes(comparison.status))
    throw new Error("Release commit is not on main.");
  assertValidation(io.api(`repos/${repository}/actions/runs/${runId}`), commit, repository);
  assertArtifacts(
    io.api(`repos/${repository}/actions/runs/${runId}/artifacts?per_page=100`).artifacts,
  );
  if (env.NPM_RUN) {
    const run = io.api(`repos/${repository}/actions/runs/${env.NPM_RUN}`);
    if (
      run.path !== ".github/workflows/publish.yml" ||
      run.event !== "workflow_dispatch" ||
      run.conclusion !== "success" ||
      run.head_branch !== "main" ||
      run.display_title !== `Publish ${version} (${commit})`
    )
      throw new Error("The npm publication and verification must succeed first.");
  }
  const who = via === "automation" ? "release automation" : env.GITHUB_ACTOR;
  console.log(`Authorized ${version} from PR #${pr.number} at ${commit} (dispatched by ${who})`);
  return { pr: pr.number, via };
}
module.exports = {
  VERSION,
  SHA,
  assertMaintainer,
  assertRequest,
  assertPullRequest,
  assertValidation,
  assertQueueBase,
  assertQueueTree,
  assertArtifacts,
  gh,
  api,
  git,
  checkCandidate,
  requestAt,
  releasePR,
  AUTOMATION_ACTOR,
  authorizeDispatcher,
  authorizePublication,
};
if (require.main === module) {
  try {
    authorizePublication();
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
