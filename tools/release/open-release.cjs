const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { execFileSync } = require("node:child_process");
const { VERSION, assertMaintainer, gh, api, git } = require("./policy.cjs");
/*::
import type { WorkflowRun } from "./policy.cjs";

// What `uf-release-state.json` holds between runs, so a release that stops
// part-way resumes where it stopped. The keys are the file's format.
type State = {
  repository: string,
  version: string,
  branch: string,
  pr?: ?number,
  worktree?: string,
  commit?: string,
  validationRun?: number,
  publishRun?: number,
  releaseRun?: number,
  editorsRun?: number,
};
type Workflow = "publish" | "release" | "editors";
type Inputs = { readonly [name: string]: string | number };
*/
const sleep = (ms /*: number */) /*: Promise<void> */ =>
  new Promise((resolve) => setTimeout(resolve, ms));

// Releases are `0.x.0`: a minor bump unless the command names another. The
// `0.0.0-alpha.N` series ended with alpha.47 (alpha.48 merged without a queue
// run and was never published), and the owner chose `0.1.0` as the next one.
// `patch` is for a fix-only release; `alpha`, `major` and an exact version
// still work.
const DEFAULT_BUMP /*: string */ = "minor";
const USAGE =
  "Usage: uf run release -- [minor|patch|major|alpha|<version>] [--dry-run] (default: minor)";
function requestedBump(argv /*: $ReadOnlyArray<string> */) /*: string */ {
  return argv.find((arg) => arg !== "--dry-run") || DEFAULT_BUMP;
}

function nextVersion(current /*: string */, bump /*: string */) /*: string */ {
  if (VERSION.test(bump)) {
    // `VERSION` admits only these channels; a stable version sorts last.
    const rank /*: { readonly [channel: string]: number } */ = {
      alpha: 0,
      beta: 1,
      rc: 2,
      stable: 3,
    };
    const parts = (version /*: string */) /*: Array<number> */ => {
      const [base, channel] = version.split("-");
      const [name, number] = (channel || "stable.0").split(".");
      return [...base.split(".").map(Number), rank[name], Number(number)];
    };
    const before = parts(current);
    const after = parts(bump);
    const difference = after
      .map((value, index) => value - before[index])
      .find((value) => value !== 0);
    if (!(difference != null && difference > 0))
      throw new Error("The release version must be newer than current main.");
    return bump;
  }
  const match = /^(\d+)\.(\d+)\.(\d+)(?:-(alpha|beta|rc)\.(\d+))?$/.exec(current);
  if (!match) throw new Error(`Unsupported current version: ${current}`);
  const [, major, minor, patch, channel, number] = match;
  if (bump === "alpha")
    return channel === "alpha"
      ? `${major}.${minor}.${patch}-alpha.${Number(number) + 1}`
      : `${major}.${minor}.${Number(patch) + 1}-alpha.1`;
  if (bump === "patch") return `${major}.${minor}.${Number(patch) + 1}`;
  if (bump === "minor") return `${major}.${Number(minor) + 1}.0`;
  if (bump === "major") return `${Number(major) + 1}.0.0`;
  throw new Error(USAGE);
}
function writeNotes(state /*: { readonly version: string, ... } */) /*: void */ {
  const tag = git("describe", "--tags", "--match", "uf@*", "--abbrev=0", "origin/main");
  const notes = git("log", "--no-merges", "--format=- %s (%h)", `${tag}..origin/main`);
  const text = fs.readFileSync("CHANGELOG.md", "utf8");
  const heading = `## uf@${state.version}\n`;
  const section = `${heading}\n${notes}\n\n`;
  const start = text.indexOf(heading);
  if (start < 0) {
    const first = text.search(/^## /m);
    fs.writeFileSync(
      "CHANGELOG.md",
      first < 0 ? `${text}\n${section}` : text.slice(0, first) + section + text.slice(first),
    );
  } else {
    const rest = text.slice(start + heading.length);
    const next = rest.search(/^## /m);
    fs.writeFileSync(
      "CHANGELOG.md",
      text.slice(0, start) + section + (next < 0 ? "" : rest.slice(next)),
    );
  }
}
function refreshBranch(state /*: State */) /*: void */ {
  git("fetch", "origin", "main", state.branch);
  let worktree = state.worktree;
  if (!worktree || !fs.existsSync(worktree)) {
    worktree = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "uf-release-resume-")), "repo");
    state.worktree = worktree;
    git("worktree", "add", "--detach", worktree, `origin/${state.branch}`);
  }
  const previous = process.cwd();
  try {
    process.chdir(worktree);
    git("merge", "--ff-only", `origin/${state.branch}`);
    git("merge", "--no-edit", "origin/main");
    writeNotes(state);
    git("add", "CHANGELOG.md");
    if (git("diff", "--cached", "--name-only"))
      git("commit", "-m", `chore(release): refresh ${state.version} notes`);
    git("push", "origin", `HEAD:refs/heads/${state.branch}`);
  } finally {
    process.chdir(previous);
  }
}
const FAILED_CONCLUSIONS /*: $ReadOnlyArray<?string> */ = [
  "FAILURE",
  "TIMED_OUT",
  "ACTION_REQUIRED",
];
async function waitForMerge(state /*: State */) /*: Promise<string> */ {
  const number = String(state.pr);
  let reported = "";
  const until = Date.now() + 6 * 60 * 60 * 1000;
  while (Date.now() < until) {
    const pr = JSON.parse(
      gh(
        "pr",
        "view",
        number,
        "--repo",
        state.repository,
        "--json",
        "state,mergeStateStatus,mergeCommit,statusCheckRollup,headRefOid,autoMergeRequest",
      ),
    );
    if (pr.state === "MERGED") return pr.mergeCommit.oid;
    if (pr.state === "CLOSED") throw new Error(`Release PR #${number} was closed without merging.`);
    if (pr.mergeStateStatus === "BEHIND") {
      refreshBranch(state);
      console.log("Updated the release PR with current main; waiting for new checks.");
      await sleep(30000);
      continue;
    }
    const failed = pr.statusCheckRollup.filter(
      (check /*: { readonly conclusion: ?string, readonly state?: string, ... } */) =>
        FAILED_CONCLUSIONS.includes(check.conclusion) || check.state === "FAILURE",
    );
    if (failed.length)
      throw new Error(
        `Release PR checks failed: ${failed.map((check /*: { readonly name?: string, readonly context?: string, ... } */) => check.name || check.context).join(", ")}. Fix or rerun them, then repeat the release command.`,
      );
    if (!pr.autoMergeRequest) {
      const [owner, name] = state.repository.split("/");
      const current = JSON.parse(
        gh(
          "api",
          "graphql",
          "-f",
          "query=query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){state mergeCommit{oid} autoMergeRequest{enabledAt} mergeQueueEntry{state}}}}",
          "-f",
          `owner=${owner}`,
          "-f",
          `name=${name}`,
          "-F",
          `number=${number}`,
        ),
      ).data.repository.pullRequest;
      if (current.state === "MERGED") return current.mergeCommit.oid;
      if (!current.autoMergeRequest && !current.mergeQueueEntry)
        throw new Error(
          `Release PR #${number} is no longer queued. Check the merge-queue results, fix or rerun them, then repeat the release command.`,
        );
    }
    const pending = pr.statusCheckRollup.filter(
      (check /*: { readonly status?: string, readonly state?: string, ... } */) =>
        check.status !== "COMPLETED" && check.state !== "SUCCESS",
    ).length;
    const message = `PR #${number}: ${pr.mergeStateStatus}, ${pending} checks pending`;
    if (message !== reported) {
      console.log(message);
      reported = message;
    }
    await sleep(30000);
  }
  throw new Error("Release is still pending. Repeat the command to resume.");
}
async function waitRun(repository /*: string */, id /*: number */) /*: Promise<WorkflowRun> */ {
  const until = Date.now() + 6 * 60 * 60 * 1000;
  while (Date.now() < until) {
    const run /*: WorkflowRun */ = api(`repos/${repository}/actions/runs/${id}`);
    if (run.status === "completed") {
      if (run.conclusion !== "success")
        throw new Error(
          `Workflow ${run.html_url} ended with ${String(run.conclusion)}. Fix or rerun failed jobs, then repeat the command.`,
        );
      return run;
    }
    await sleep(30000);
  }
  throw new Error(`Workflow ${id} is still running. Repeat the command to resume.`);
}
// The state key that records each workflow's run: `${workflow}Run`.
const RUN_KEYS /*: { readonly [Workflow]: "publishRun" | "releaseRun" | "editorsRun" } */ = {
  publish: "publishRun",
  release: "releaseRun",
  editors: "editorsRun",
};
async function dispatch(
  state /*: State */,
  workflow /*: Workflow */,
  title /*: string */,
  inputs /*: Inputs */,
  save /*: () => void */,
) /*: Promise<WorkflowRun> */ {
  const key = RUN_KEYS[workflow];
  let id = state[key];
  if (!id) {
    // Find an existing run after an interrupted dispatch, before starting one.
    const find = () /*: ?WorkflowRun */ =>
      api(
        `repos/${state.repository}/actions/workflows/${workflow}.yml/runs?event=workflow_dispatch&branch=main&per_page=100`,
      ).workflow_runs.find((run /*: WorkflowRun */) => run.display_title === title);
    let run = find();
    if (!run) {
      const args = [
        "workflow",
        "run",
        `${workflow}.yml`,
        "--repo",
        state.repository,
        "--ref",
        "main",
      ];
      for (const [key, value] of Object.entries(inputs)) args.push("-f", `${key}=${value}`);
      gh(...args);
      for (let retry = 0; retry < 20 && !run; retry++) {
        await sleep(3000);
        run = find();
      }
    }
    if (!run)
      throw new Error("Workflow dispatch is not visible yet. Repeat the command to resume.");
    id = run.id;
    state[key] = id;
    save();
  }
  const previous /*: WorkflowRun */ = api(`repos/${state.repository}/actions/runs/${id}`);
  if (previous.status === "completed" && previous.conclusion !== "success") {
    gh("run", "rerun", String(id), "--repo", state.repository, "--failed");
    for (let retry = 0; retry < 20; retry++) {
      await sleep(3000);
      const current /*: WorkflowRun */ = api(`repos/${state.repository}/actions/runs/${id}`);
      if (current.run_attempt > previous.run_attempt || current.status !== "completed") break;
    }
  }
  console.log(`Waiting for https://github.com/${state.repository}/actions/runs/${id}`);
  return waitRun(state.repository, id);
}
async function main() /*: Promise<void> */ {
  const root = git("rev-parse", "--show-toplevel");
  process.chdir(root);
  const common = git("rev-parse", "--path-format=absolute", "--git-common-dir");
  const stateFile = path.join(common, "uf-release-state.json");
  const repository = gh("repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner");
  const actor = api("user").login;
  assertMaintainer(
    api(`repos/${repository}/collaborators/${encodeURIComponent(actor)}/permission`),
  );
  let state /*: State | null */ = fs.existsSync(stateFile)
    ? JSON.parse(fs.readFileSync(stateFile, "utf8"))
    : null;
  if (state && state.repository !== repository)
    throw new Error("Saved release belongs to another repository.");
  const save = () => fs.writeFileSync(stateFile, `${JSON.stringify(state, null, 2)}\n`);
  if (!state) {
    const bump = requestedBump(process.argv.slice(2));
    git("fetch", "origin", "main", "--tags");
    const current = JSON.parse(git("show", "origin/main:npm/core/package.json")).version;
    const version = nextVersion(current, bump);
    if (version === current) throw new Error("The release must advance the current version.");
    const branch = `release/v${version}`;
    if (process.argv.includes("--dry-run")) {
      console.log(
        JSON.stringify(
          {
            repository,
            actor,
            current,
            version,
            branch,
            base: git("rev-parse", "origin/main"),
            steps: [
              "release PR",
              "quick PR checks",
              "full validation in the merge queue",
              "npm publication and verification",
              "native publication and tag",
            ],
          },
          null,
          2,
        ),
      );
      return;
    }
    const existing = JSON.parse(
      gh(
        "pr",
        "list",
        "--repo",
        repository,
        "--head",
        branch,
        "--state",
        "all",
        "--json",
        "number,state",
      ),
    );
    if (existing.length > 1 || existing[0]?.state === "CLOSED")
      throw new Error("Release branch has a closed or ambiguous PR; inspect it before retrying.");
    state = { repository, version, branch, pr: existing[0]?.number } /*:: as State */;
    save();
  }
  if (process.argv.includes("--dry-run")) {
    console.log(JSON.stringify(state, null, 2));
    return;
  }
  if (!state.pr) {
    let worktree = state.worktree;
    if (!worktree) {
      worktree = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "uf-release-pr-")), "repo");
      state.worktree = worktree;
      save();
      git("worktree", "add", "-b", state.branch, worktree, "origin/main");
    }
    const previous = process.cwd();
    try {
      process.chdir(worktree);
      if (
        !fs.existsSync(".github/release.json") ||
        JSON.parse(fs.readFileSync(".github/release.json", "utf8")).version !== state.version
      ) {
        writeNotes(state);
        execFileSync("tools/upstream/sync.sh", { stdio: "inherit" });
        execFileSync("tools/release/bump-version.sh", [state.version], { stdio: "inherit" });
        fs.writeFileSync(
          ".github/release.json",
          `${JSON.stringify({ version: state.version, branch: state.branch }, null, 2)}\n`,
        );
        git(
          "add",
          "Cargo.toml",
          "Cargo.lock",
          "package-lock.json",
          "npm",
          "docs/package.json",
          "CHANGELOG.md",
          ".github/release.json",
        );
        git("commit", "-m", `chore(release): uf@${state.version}`);
      }
      git("push", "-u", "origin", state.branch);
      const body = path.join(path.dirname(worktree), "release-pr.md");
      fs.writeFileSync(
        body,
        `Release uf ${state.version}.\n\nFull integration tests and all native archives are validated before merge. The merge queue checks current main. Publication reuses those archives; no tag is pushed by this command.\n`,
      );
      const found = JSON.parse(
        gh(
          "pr",
          "list",
          "--repo",
          repository,
          "--head",
          state.branch,
          "--state",
          "open",
          "--json",
          "url",
        ),
      );
      const url =
        found[0]?.url ||
        gh(
          "pr",
          "create",
          "--repo",
          repository,
          "--base",
          "main",
          "--head",
          state.branch,
          "--title",
          `chore(release): uf@${state.version}`,
          "--body-file",
          body,
        );
      state.pr = Number(url.split("/").pop());
      save();
    } finally {
      process.chdir(previous);
    }
  }
  let commit = state.commit;
  if (!commit) {
    gh("pr", "merge", String(state.pr), "--repo", repository, "--auto", "--squash");
    commit = await waitForMerge(state);
    state.commit = commit;
    save();
  }
  let validationRun = state.validationRun;
  if (!validationRun) {
    const runs /*: Array<WorkflowRun> */ = api(
      `repos/${repository}/actions/workflows/ci.yml/runs?event=merge_group&head_sha=${commit}&per_page=100`,
    ).workflow_runs;
    const run = runs.find(
      (run) =>
        run.head_sha === commit && run.status === "completed" && run.conclusion === "success",
    );
    if (!run) throw new Error("No successful merge-queue validation found for the merged commit.");
    validationRun = run.id;
    state.validationRun = validationRun;
    save();
  }
  const inputs = {
    version: state.version,
    commit,
    validation_run: validationRun,
  };
  const npm = await dispatch(
    state,
    "publish",
    `Publish ${state.version} (${commit})`,
    inputs,
    save,
  );
  await dispatch(
    state,
    "release",
    `Release ${state.version} (${commit})`,
    { ...inputs, npm_run: npm.id },
    save,
  );
  await dispatch(state, "editors", `Editors ${state.version}`, { version: state.version }, save);
  const release = JSON.parse(
    gh(
      "release",
      "view",
      `uf@${state.version}`,
      "--repo",
      repository,
      "--json",
      "url,isDraft,assets",
    ),
  );
  if (release.isDraft || !release.assets.some((asset) => asset.name === "manifest.json"))
    throw new Error("The public release is incomplete.");
  console.log(`Released ${state.version}: ${release.url}`);
  fs.unlinkSync(stateFile);
}
module.exports = { nextVersion, requestedBump, DEFAULT_BUMP };
if (require.main === module)
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
