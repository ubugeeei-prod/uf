const { execFileSync } = require("node:child_process");
const fs = require("node:fs");

// These files run under plain `node`, so their types are Flow comments: the
// shapes below are the parts of GitHub's REST responses the policy reads.
/*::
export type Repository = { readonly full_name: string, ... };
export type Permission = {
  readonly role_name?: string,
  readonly user?: ?{ readonly permissions?: ?{ readonly maintain?: boolean, readonly admin?: boolean, ... }, ... },
  ...
};
export type ReleaseRequest = { readonly version: string, readonly branch: string, ... };
export type PullRequest = {
  readonly number: number,
  readonly base: { readonly ref: string, readonly repo: Repository, ... },
  readonly head: { readonly ref: string, readonly sha: string, readonly repo: ?Repository, ... },
  readonly user: { readonly type: string, readonly login: string, ... },
  readonly merged?: boolean,
  readonly merged_at?: ?string,
  readonly merge_commit_sha?: ?string,
  ...
};
export type WorkflowRun = {
  readonly id: number,
  readonly path: string,
  readonly event: string,
  readonly head_sha: string,
  readonly head_branch?: ?string,
  readonly head_repository?: ?Repository,
  readonly status: string,
  readonly conclusion: ?string,
  readonly display_title: string,
  readonly html_url: string,
  readonly run_attempt: number,
  ...
};
export type Artifact = { readonly name: string, readonly expired: boolean, readonly size_in_bytes: number, ... };
export type GitObject = { readonly type: string, readonly sha: string, ... };
// A GitHub REST GET, parsed. The response is JSON off the wire, so each
// caller names the shape it reads.
export type Api = (path: string) => any;
export type Git = (...args: Array<string>) => string;
export type IO = { readonly api: Api, readonly git: Git, ... };
export type Env = { readonly [name: string]: ?string, ... };
*/

const VERSION /*: RegExp */ =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(?:alpha|beta|rc)\.(0|[1-9]\d*))?$/;
const SHA /*: RegExp */ = /^[a-f0-9]{40}$/;
const MAINTAINER_ROLES /*: $ReadOnlyArray<?string> */ = ["maintain", "admin"];
function assertMaintainer(permission /*: Permission */) /*: void */ {
  if (
    !(
      permission.user?.permissions?.maintain ||
      permission.user?.permissions?.admin ||
      MAINTAINER_ROLES.includes(permission.role_name)
    )
  ) {
    throw new Error("Releases require repository maintain or admin permission.");
  }
}
function assertRequest(request /*: ReleaseRequest */, version /*: string */) /*: void */ {
  if (
    !VERSION.test(version) ||
    request.version !== version ||
    request.branch !== `release/v${version}`
  ) {
    throw new Error("Invalid release request. Use uf run release -- <bump>.");
  }
}
function assertPullRequest(
  pr /*: PullRequest */,
  request /*: ReleaseRequest */,
  repository /*: string */,
) /*: void */ {
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
function assertValidation(
  run /*: WorkflowRun */,
  commit /*: string */,
  repository /*: string */,
) /*: void */ {
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
// A variable the workflow must set. Actions always sets `GITHUB_*`; without
// one, nothing below could name the repository or event it is authorizing.
function requireEnv(env /*: Env */, name /*: string */) /*: string */ {
  const value = env[name];
  if (value == null) throw new Error(`${name} is not set`);
  return value;
}
function gh(...args /*: Array<string> */) /*: string */ {
  return execFileSync("gh", args, {
    encoding: "utf8",
    maxBuffer: 16 * 1024 * 1024,
  }).trim();
}
function api(path /*: string */) /*: any */ {
  return JSON.parse(gh("api", path));
}
function git(...args /*: Array<string> */) /*: string */ {
  return execFileSync("git", args, {
    encoding: "utf8",
    maxBuffer: 16 * 1024 * 1024,
  }).trim();
}
// Everything below that reads GitHub or the checkout does it through `io`, so
// the tests can answer for both. The defaults are the real `gh api` and `git`.
const IO /*: IO */ = { api, git };
/** Read the core version from either side of the workspace directory migration. */
function versionAt(ref /*: string */, io /*: IO */ = IO) /*: string */ {
  const files = io
    .git("ls-tree", "--name-only", ref, "npm/core/package.json", "packages/core/package.json")
    .split("\n");
  const manifest = files.includes("npm/core/package.json")
    ? "npm/core/package.json"
    : "packages/core/package.json";
  return JSON.parse(io.git("show", `${ref}:${manifest}`)).version;
}
function requestAt(ref /*: string */, io /*: IO */ = IO) /*: ReleaseRequest */ {
  const request /*: ReleaseRequest */ = JSON.parse(io.git("show", `${ref}:.github/release.json`));
  const version = versionAt(ref, io);
  assertRequest(request, version);
  return request;
}
function releasePR(
  request /*: ReleaseRequest */,
  repository /*: string */,
  io /*: IO */ = IO,
) /*: PullRequest */ {
  const owner = repository.split("/")[0];
  const prs /*: Array<PullRequest> */ = io.api(
    `repos/${repository}/pulls?state=all&base=main&head=${encodeURIComponent(`${owner}:${request.branch}`)}&per_page=100`,
  );
  if (prs.length !== 1) throw new Error("Expected exactly one release PR for this version.");
  const pr /*: PullRequest */ = io.api(`repos/${repository}/pulls/${prs[0].number}`);
  assertPullRequest(pr, request, repository);
  assertMaintainer(
    io.api(`repos/${repository}/collaborators/${encodeURIComponent(pr.user.login)}/permission`),
  );
  return pr;
}
function checkCandidate(base /*: string */, paths /*: Array<string> */) /*: boolean */ {
  const previous = versionAt(base);
  const current = versionAt("HEAD");
  const requested = paths.includes(".github/release.json");
  if (previous === current && !requested) return false;
  if (!requested || previous === current)
    throw new Error("A version bump and release request must be committed together.");
  const request = requestAt("HEAD");
  const repository = requireEnv(process.env, "GITHUB_REPOSITORY");
  const event = JSON.parse(fs.readFileSync(requireEnv(process.env, "GITHUB_EVENT_PATH"), "utf8"));
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
function assertQueueTree(
  main /*: string */,
  head /*: string */,
  queued /*: string */ = "HEAD",
) /*: void */ {
  git("merge-base", "--is-ancestor", main, head);
  // A squash commit has main as its parent, not the PR head. Its tree must
  // still match the up-to-date PR exactly in our single-entry merge queue.
  if (git("rev-parse", `${head}^{tree}`) !== git("rev-parse", `${queued}^{tree}`))
    throw new Error("The release merge group must contain exactly the current release PR tree.");
}
function assertArtifacts(artifacts /*: $ReadOnlyArray<Artifact> */) /*: void */ {
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
function assertQueueBase(base /*: ?string */, main /*: ?string */) /*: void */ {
  if (!SHA.test(main || "") || base !== main)
    throw new Error("The merge queue must rebuild against current main.");
}
function assertTagTarget(
  repository /*: string */,
  version /*: string */,
  commit /*: string */,
  io /*: IO */ = IO,
) /*: void */ {
  const ref = `refs/tags/uf@${version}`;
  const found /*: ?{ readonly ref: string, readonly object: GitObject, ... } */ = io
    .api(`repos/${repository}/git/matching-refs/tags/uf@${encodeURIComponent(version)}`)
    .find((tag /*: { readonly ref: string, ... } */) => tag.ref === ref);
  if (!found) return;
  let object /*: GitObject */ = found.object;
  for (let depth = 0; object.type === "tag" && depth < 5; depth++)
    object = io.api(`repos/${repository}/git/tags/${object.sha}`).object;
  if (object.type !== "commit" || object.sha !== commit)
    throw new Error("The release tag already points to another commit; it will not be replaced.");
}
/**
 * The account `GITHUB_TOKEN` acts as. A `workflow_dispatch` that
 * `release-automation.yml` sends with its token runs as this actor.
 */
const AUTOMATION_ACTOR /*: string */ = "github-actions[bot]";

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
function authorizeDispatcher(
  actor /*: ?string */,
  repository /*: string */,
  io /*: IO */ = IO,
) /*: "automation" | "maintainer" */ {
  if (actor === AUTOMATION_ACTOR) return "automation";
  assertMaintainer(
    io.api(`repos/${repository}/collaborators/${encodeURIComponent(actor || "")}/permission`),
  );
  return "maintainer";
}
function authorizePublication(
  env /*: Env */ = process.env,
  io /*: IO */ = IO,
) /*: { pr: number, via: "automation" | "maintainer" } */ {
  if (env.GITHUB_REF !== "refs/heads/main")
    throw new Error("Publication must be dispatched from main.");
  const repository = requireEnv(env, "GITHUB_REPOSITORY");
  const { RELEASE_COMMIT: commit, RELEASE_VERSION: version, VALIDATION_RUN: runId } = env;
  if (
    commit == null ||
    version == null ||
    runId == null ||
    !SHA.test(commit) ||
    !VERSION.test(version) ||
    !/^\d+$/.test(runId)
  )
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
  const who = via === "automation" ? "release automation" : String(env.GITHUB_ACTOR);
  console.log(`Authorized ${version} from PR #${pr.number} at ${commit} (dispatched by ${who})`);
  return { pr: pr.number, via };
}
module.exports = {
  versionAt,
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
  requireEnv,
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
