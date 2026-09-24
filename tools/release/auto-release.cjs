// Publish a release PR's merge without a person driving it.
//
//   GH_TOKEN=… GITHUB_REPOSITORY=owner/uf RELEASE_COMMIT=<sha> node tools/release/auto-release.cjs
//
// `.github/workflows/release-automation.yml` runs this on every push to
// `main`. Almost every push is not a release, and the script says so and exits.
// A push is a release when two things hold: `.github/release.json` changed in
// that commit, and the PR on the branch it names merged at exactly that
// commit. For a release, the script runs the three workflows a maintainer used
// to dispatch by hand, in order, and waits for each one:
//
//   1. `publish.yml`, with the successful `ci.yml` merge-queue run for this
//      commit as `validation_run`;
//   2. `release.yml`, with the publish run as `npm_run`;
//   3. `editors.yml`, with the version.
//
// Authorization is still `policy.cjs`, inside each of those workflows. This
// script only chooses what to dispatch. Those runs see `github-actions[bot]` as
// their actor, and the policy authorizes them on the release PR: a maintainer
// authored it, it merged at this commit, and the queue validated it.
//
// Each workflow is found by its run name before it is dispatched, so a run
// that `uf run release` (open-release.cjs) already started is waited on rather
// than duplicated, and so is one from an earlier attempt of this job. One
// failure is retried: `publish.yml`'s "Verify the npm release" job, when it is
// the only failed job, because npm's registry can lag behind a publish for
// longer than that job waits. It gets one rerun. Any other failure stops here,
// loudly, with the run's URL.

const { execFileSync } = require("node:child_process");
const policy = require("./policy.cjs");

const VERIFY_JOB = "Verify the npm release";
const POLL_MS = 30_000;
const FIND_TRIES = 20;
const RUN_DEADLINE_MS = 90 * 60 * 1000;

function post(path, body) {
  execFileSync("gh", ["api", "-X", "POST", path, "--input", "-"], {
    encoding: "utf8",
    input: JSON.stringify(body),
    stdio: ["pipe", "pipe", "inherit"],
  });
}
const IO = {
  api: policy.api,
  git: policy.git,
  post,
  sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
  log: (line) => console.log(line),
};

/**
 * The release this commit is the merge of, or `null` when it is not one.
 *
 * `.github/release.json` stays on `main` after a release, so a commit that
 * merely carries it is not a release. The commit has to change it, and the
 * release PR for the branch it names has to have merged at this commit. What
 * the file says is checked against the workspace version by `requestAt`, the
 * same check `policy.cjs` makes.
 */
function findRelease(commit, repository, io = IO) {
  if (!policy.SHA.test(commit || "")) throw new Error(`Not a commit: ${commit}`);
  let after;
  try {
    after = io.git("show", `${commit}:.github/release.json`);
  } catch {
    return null;
  }
  let before = null;
  try {
    before = io.git("show", `${commit}^:.github/release.json`);
  } catch {}
  if (before === after) return null;
  const request = policy.requestAt(commit, io);
  const owner = repository.split("/")[0];
  const prs = io.api(
    `repos/${repository}/pulls?state=closed&base=main&head=${encodeURIComponent(`${owner}:${request.branch}`)}&per_page=100`,
  );
  const pr = prs.find((item) => item.merged_at && item.merge_commit_sha === commit);
  if (!pr) return null;
  return { version: request.version, pr: pr.number };
}

/** The successful merge-queue `ci.yml` run for exactly this commit. */
function findValidation(commit, repository, io = IO) {
  const runs = io.api(
    `repos/${repository}/actions/workflows/ci.yml/runs?event=merge_group&head_sha=${commit}&per_page=100`,
  ).workflow_runs;
  for (const run of runs) {
    try {
      policy.assertValidation(run, commit, repository);
      return run.id;
    } catch {}
  }
  throw new Error(
    `No successful merge-queue CI run for ${commit}. A release PR that merged outside the queue cannot be published.`,
  );
}

function runsNamed(repository, workflow, title, io) {
  return io
    .api(
      `repos/${repository}/actions/workflows/${workflow}/runs?event=workflow_dispatch&branch=main&per_page=100`,
    )
    .workflow_runs.filter((run) => run.display_title === title);
}

async function waitFor(repository, id, attempt, io) {
  const until = Date.now() + RUN_DEADLINE_MS;
  for (;;) {
    const run = io.api(`repos/${repository}/actions/runs/${id}`);
    if (run.status === "completed" && run.run_attempt >= attempt) return run;
    if (Date.now() > until) throw new Error(`${run.html_url} is still running after 90 minutes.`);
    await io.sleep(POLL_MS);
  }
}

function failedJobs(repository, run, io) {
  return io
    .api(`repos/${repository}/actions/runs/${run.id}/attempts/${run.run_attempt}/jobs?per_page=100`)
    .jobs.filter((job) => job.conclusion === "failure")
    .map((job) => job.name);
}

/**
 * Run `workflow` with `inputs` unless a run named `title` exists, and wait
 * for it to succeed. Returns the run.
 */
async function runWorkflow(repository, workflow, title, inputs, io = IO) {
  let [run] = runsNamed(repository, workflow, title, io);
  if (run) {
    io.log(`${title}: found ${run.html_url}`);
  } else {
    io.post(`repos/${repository}/actions/workflows/${workflow}/dispatches`, {
      ref: "main",
      inputs,
    });
    for (let tries = 0; !run && tries < FIND_TRIES; tries++) {
      await io.sleep(POLL_MS / 10);
      [run] = runsNamed(repository, workflow, title, io);
    }
    if (!run) throw new Error(`${title}: dispatched, but no run with that name appeared.`);
    io.log(`${title}: started ${run.html_url}`);
  }
  let done = await waitFor(repository, run.id, run.run_attempt, io);
  let retried = false;
  while (done.conclusion !== "success") {
    const failed = done.conclusion === "failure" ? failedJobs(repository, done, io) : [];
    const lagging = workflow === "publish.yml" && failed.length === 1 && failed[0] === VERIFY_JOB;
    if (!lagging || retried)
      throw new Error(
        `${title} ended with ${done.conclusion}${failed.length ? ` (${failed.join(", ")})` : ""}: ${done.html_url}`,
      );
    retried = true;
    io.log(`${title}: npm had not caught up; rerunning "${VERIFY_JOB}" once`);
    io.post(`repos/${repository}/actions/runs/${done.id}/rerun-failed-jobs`, {});
    done = await waitFor(repository, done.id, done.run_attempt + 1, io);
  }
  io.log(`${title}: succeeded`);
  return done;
}

async function autoRelease(env = process.env, io = IO) {
  const repository = env.GITHUB_REPOSITORY;
  const commit = env.RELEASE_COMMIT;
  const release = findRelease(commit, repository, io);
  if (!release) {
    io.log(`${commit} is not the merge of a release PR; nothing to publish.`);
    return null;
  }
  const { version } = release;
  io.log(`${commit} is the merge of release PR #${release.pr} for ${version}.`);
  const validation = findValidation(commit, repository, io);
  const inputs = { version, commit, validation_run: String(validation) };
  const npm = await runWorkflow(
    repository,
    "publish.yml",
    `Publish ${version} (${commit})`,
    inputs,
    io,
  );
  await runWorkflow(
    repository,
    "release.yml",
    `Release ${version} (${commit})`,
    { ...inputs, npm_run: String(npm.id) },
    io,
  );
  await runWorkflow(repository, "editors.yml", `Editors ${version}`, { version }, io);
  io.log(`Released ${version}: https://github.com/${repository}/releases/tag/uf%40${version}`);
  return release;
}

module.exports = { findRelease, findValidation, runWorkflow, autoRelease, VERIFY_JOB };
if (require.main === module)
  autoRelease().catch((error) => {
    console.error(`::error::${error.message}`);
    process.exitCode = 1;
  });
