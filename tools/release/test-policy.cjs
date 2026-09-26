const assert = require("node:assert/strict");
const { test } = require("node:test");
const {
	versionAt,
	assertMaintainer,
	assertRequest,
	assertPullRequest,
	assertValidation,
	assertQueueBase,
	assertQueueTree,
	assertArtifacts,
} = require("./policy.cjs");
const {
	nextVersion,
	requestedBump,
	DEFAULT_BUMP,
} = require("./open-release.cjs");
const repository = "owner/project";
const commit = "a".repeat(40);
const request = {
	version: "0.0.0-alpha.46",
	branch: "release/v0.0.0-alpha.46",
};

test("only maintainers and administrators may create a release", () => {
	for (const role_name of ["maintain", "admin"])
		assert.doesNotThrow(() => assertMaintainer({ role_name }));
	assert.doesNotThrow(() =>
		assertMaintainer({
			role_name: "custom",
			user: { permissions: { maintain: true } },
		}),
	);
	for (const role_name of ["write", "triage", "read", undefined])
		assert.throws(() => assertMaintainer({ role_name, permission: "write" }));
});
test("the request must name its version and dedicated branch", () => {
	assertRequest(request, request.version);
	assert.throws(() => assertRequest(request, "0.0.0-alpha.47"));
	assert.throws(() =>
		assertRequest({ ...request, branch: "feature" }, request.version),
	);
	assert.throws(() =>
		assertRequest(
			{ version: "$(false)", branch: "release/v$(false)" },
			"$(false)",
		),
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
		assertPullRequest(
			{ ...pr, base: { ...pr.base, ref: "develop" } },
			request,
			repository,
		),
	);
	assert.throws(() =>
		assertPullRequest({ ...pr, user: { type: "Bot" } }, request, repository),
	);
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

test("a release is a minor bump unless another is named, and leaves the alpha series for 0.1.0", () => {
	assert.equal(DEFAULT_BUMP, "minor");
	assert.equal(requestedBump([]), "minor");
	assert.equal(requestedBump(["--dry-run"]), "minor");
	assert.equal(requestedBump(["--dry-run", "patch"]), "patch");
	assert.equal(requestedBump(["alpha"]), "alpha");
	assert.equal(requestedBump(["0.2.0", "--dry-run"]), "0.2.0");
	assert.equal(nextVersion("0.0.0-alpha.48", requestedBump([])), "0.1.0");
	assert.equal(nextVersion("0.0.0-alpha.48", "0.1.0"), "0.1.0");
	assert.equal(nextVersion("0.1.0", requestedBump([])), "0.2.0");
	assert.equal(nextVersion("0.1.0", "patch"), "0.1.1");
	assert.equal(nextVersion("0.1.1", "minor"), "0.2.0");
	assert.throws(() => nextVersion("0.1.0", "0.1.0"));
	assert.throws(() => nextVersion("0.1.0", "0.0.0-alpha.49"));
	assert.throws(() => nextVersion("0.1.0", "nope"), /default: minor/);
});

test("queue validation rejects an older main base", () => {
	assertQueueBase(commit, commit);
	assert.throws(() => assertQueueBase("b".repeat(40), commit));
	assert.throws(() => assertQueueBase(undefined, commit));
});

test("publication refuses missing or expired native archives before npm is touched", () => {
	const artifacts = [
		"x86_64-unknown-linux-gnu",
		"aarch64-unknown-linux-gnu",
		"x86_64-apple-darwin",
		"aarch64-apple-darwin",
		"x86_64-pc-windows-msvc",
	].map((target) => ({
		name: `uf-release-${target}`,
		expired: false,
		size_in_bytes: 100,
	}));
	assertArtifacts(artifacts);
	assert.throws(() => assertArtifacts(artifacts.slice(1)));
	assert.throws(() =>
		assertArtifacts(
			artifacts.map((item, i) => (i ? item : { ...item, expired: true })),
		),
	);
	assert.throws(() =>
		assertArtifacts(
			artifacts.map((item, i) => (i ? item : { ...item, size_in_bytes: 0 })),
		),
	);
});

test("a squash queue commit accepts the current PR tree, but rejects stale or different trees", () => {
	const { execFileSync } = require("node:child_process");
	const { mkdtempSync, writeFileSync, rmSync } = require("node:fs");
	const { tmpdir } = require("node:os");
	const { join } = require("node:path");
	const previous = process.cwd();
	const directory = mkdtempSync(join(tmpdir(), "uf-release-queue-"));
	const git = (...args) =>
		execFileSync("git", args, {
			encoding: "utf8",
			stdio: ["ignore", "pipe", "pipe"],
		}).trim();
	try {
		process.chdir(directory);
		git("init", "-b", "main");
		git("config", "user.name", "Test");
		git("config", "user.email", "test@example.invalid");
		writeFileSync("version", "1");
		git("add", ".");
		git("commit", "-m", "initial");
		const main = git("rev-parse", "HEAD");
		git("checkout", "-b", "release");
		writeFileSync("version", "2");
		git("commit", "-am", "release");
		const head = git("rev-parse", "HEAD");
		git("checkout", "-b", "queue", main);
		git("merge", "--squash", "release");
		git("commit", "-m", "queue squash");
		assert.throws(() => git("merge-base", "--is-ancestor", head, "HEAD"));
		assertQueueTree(main, head);
		writeFileSync("version", "3");
		git("commit", "-am", "unexpected change");
		assert.throws(() => assertQueueTree(main, head), /current release PR tree/);
		git("checkout", "main");
		git("commit", "--allow-empty", "-m", "main advanced");
		assert.throws(() => assertQueueTree(git("rev-parse", "HEAD"), head, head));
	} finally {
		process.chdir(previous);
		rmSync(directory, { recursive: true, force: true });
	}
});

test("the command stops after queue failure and handles a merge during polling", async () => {
	const { readFileSync } = require("node:fs");
	const { resolve } = require("node:path");
	const { runInNewContext } = require("node:vm");
	const source = readFileSync(resolve(__dirname, "open-release.cjs"), "utf8");
	for (const merged of [false, true]) {
		const module = { exports: {} };
		const requireMock = (name) =>
			name === "./policy.cjs"
				? {
						gh: (command) =>
							JSON.stringify(
								command === "pr"
									? {
											state: "OPEN",
											mergeStateStatus: "UNSTABLE",
											statusCheckRollup: [],
											autoMergeRequest: null,
										}
									: {
											data: {
												repository: {
													pullRequest: {
														state: merged ? "MERGED" : "OPEN",
														mergeCommit: { oid: commit },
														autoMergeRequest: null,
														mergeQueueEntry: null,
													},
												},
											},
										},
							),
					}
				: require(name);
		runInNewContext(source + "\nmodule.exports.waitForMerge = waitForMerge;", {
			require: requireMock,
			module,
		});
		const result = module.exports.waitForMerge({ repository, pr: 123 });
		if (merged) assert.equal(await result, commit);
		else await assert.rejects(result, /no longer queued/);
	}
});

test("core versions remain readable before and after the workspace rename", () => {
	for (const directory of ["packages", "npm"]) {
		const io = {
			git(command, ...args) {
				if (command === "ls-tree") return `${directory}/core/package.json`;
				assert.equal(command, "show");
				assert.equal(args[0], `HEAD:${directory}/core/package.json`);
				return '{"version":"0.10.0"}';
			},
		};
		assert.equal(versionAt("HEAD", io), "0.10.0");
	}
});
