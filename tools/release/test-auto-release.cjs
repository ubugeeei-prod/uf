// Release automation: who may publish, which commits are releases, and how
// the three workflows are driven. Run with `node --test`.
const assert = require("node:assert/strict");
const { test } = require("node:test");
const { authorizePublication, AUTOMATION_ACTOR } = require("./policy.cjs");
const {
	findRelease,
	autoRelease,
	runWorkflow,
	VERIFY_JOB,
} = require("./auto-release.cjs");

const repository = "owner/project";
const commit = "c".repeat(40);
const version = "0.3.0";
const branch = `release/v${version}`;

/**
 * A repository in which PR #7 on `release/v0.3.0`, by `maintainer`, merged at
 * `commit`, and the merge queue validated it in run 100. `patch` changes one
 * fact at a time.
 */
function world(patch = {}) {
	const facts = {
		request: { version, branch },
		previousRequest: { version: "0.2.0", branch: "release/v0.2.0" },
		coreVersion: version,
		author: "maintainer",
		roles: { maintainer: "maintain", someone: "write", admin: "admin" },
		merged: true,
		mergeCommit: commit,
		validation: {
			id: 100,
			path: ".github/workflows/ci.yml",
			event: "merge_group",
			head_sha: commit,
			head_repository: { full_name: repository },
			status: "completed",
			conclusion: "success",
		},
		...patch,
	};
	const pr = {
		number: 7,
		merged: facts.merged,
		merged_at: facts.merged ? "2026-09-24T00:00:00Z" : null,
		merge_commit_sha: facts.merged ? facts.mergeCommit : null,
		base: { ref: "main", repo: { full_name: repository } },
		head: { ref: branch, repo: { full_name: repository } },
		user: { type: "User", login: facts.author },
	};
	const targets = [
		"x86_64-unknown-linux-gnu",
		"aarch64-unknown-linux-gnu",
		"x86_64-apple-darwin",
		"aarch64-apple-darwin",
		"x86_64-pc-windows-msvc",
	];
	const api = (route) => {
		if (route.startsWith(`repos/${repository}/pulls?`)) return [pr];
		if (route === `repos/${repository}/pulls/7`) return pr;
		const permission = /collaborators\/([^/]+)\/permission$/.exec(route);
		if (permission)
			return {
				role_name: facts.roles[decodeURIComponent(permission[1])] ?? "read",
			};
		if (route.includes("/git/matching-refs/tags/")) return [];
		if (route.endsWith("/git/ref/heads/main"))
			return { object: { sha: commit } };
		if (route.includes("/compare/")) return { status: "identical" };
		if (route.includes("/actions/workflows/ci.yml/runs?"))
			return { workflow_runs: facts.validation ? [facts.validation] : [] };
		if (route === `repos/${repository}/actions/runs/100`) {
			if (!facts.validation) throw new Error("HTTP 404");
			return facts.validation;
		}
		if (route.startsWith(`repos/${repository}/actions/runs/100/artifacts`))
			return {
				artifacts: targets.map((target) => ({
					name: `uf-release-${target}`,
					expired: false,
					size_in_bytes: 1,
				})),
			};
		throw new Error(`Unexpected API: ${route}`);
	};
	const git = (...args) => {
		const [command, spec] = args;
		if (command === "ls-tree") return "packages/core/package.json";
		assert.equal(command, "show");
		if (spec === `${commit}:.github/release.json`)
			return JSON.stringify(facts.request);
		if (spec === `${commit}^:.github/release.json`)
			return JSON.stringify(facts.previousRequest);
		if (spec === `${commit}:packages/core/package.json`)
			return JSON.stringify({ version: facts.coreVersion });
		throw new Error(`fatal: path does not exist: ${spec}`);
	};
	return { api, git };
}

function env(patch = {}) {
	return {
		GITHUB_REF: "refs/heads/main",
		GITHUB_REPOSITORY: repository,
		GITHUB_ACTOR: "maintainer",
		RELEASE_COMMIT: commit,
		RELEASE_VERSION: version,
		VALIDATION_RUN: "100",
		...patch,
	};
}
const quiet = (fn) => {
	const log = console.log;
	console.log = () => {};
	try {
		return fn();
	} finally {
		console.log = log;
	}
};

test("a maintainer's manual dispatch is authorized as before", () => {
	assert.equal(
		quiet(() => authorizePublication(env(), world())).via,
		"maintainer",
	);
	assert.equal(
		quiet(() => authorizePublication(env({ GITHUB_ACTOR: "admin" }), world()))
			.via,
		"maintainer",
	);
});
test("a person without maintain permission cannot dispatch publication", () => {
	assert.throws(
		() => authorizePublication(env({ GITHUB_ACTOR: "someone" }), world()),
		/maintain or admin/,
	);
	assert.throws(
		() => authorizePublication(env({ GITHUB_ACTOR: "stranger" }), world()),
		/maintain or admin/,
	);
});
test("automation is authorized by the release PR, not by its own permission", () => {
	const result = quiet(() =>
		authorizePublication(env({ GITHUB_ACTOR: AUTOMATION_ACTOR }), world()),
	);
	assert.deepEqual(result, { pr: 7, via: "automation" });
});
test("automation cannot publish a release PR a non-maintainer authored", () => {
	assert.throws(
		() =>
			authorizePublication(
				env({ GITHUB_ACTOR: AUTOMATION_ACTOR }),
				world({ author: "someone" }),
			),
		/maintain or admin/,
	);
});
test("automation cannot publish an unmerged PR, or one merged at another commit", () => {
	const bot = env({ GITHUB_ACTOR: AUTOMATION_ACTOR });
	assert.throws(
		() => authorizePublication(bot, world({ merged: false })),
		/not merged at this commit/,
	);
	assert.throws(
		() => authorizePublication(bot, world({ mergeCommit: "d".repeat(40) })),
		/not merged at this commit/,
	);
});
test("automation cannot publish without the queue's validation of the commit", () => {
	const bot = env({ GITHUB_ACTOR: AUTOMATION_ACTOR });
	assert.throws(
		() => authorizePublication(bot, world({ validation: null })),
		/404/,
	);
	const failed = {
		...world().api(`repos/${repository}/actions/runs/100`),
		conclusion: "failure",
	};
	assert.throws(
		() => authorizePublication(bot, world({ validation: failed })),
		/successful merge-queue CI/,
	);
	const pushed = { ...failed, conclusion: "success", event: "push" };
	assert.throws(
		() => authorizePublication(bot, world({ validation: pushed })),
		/successful merge-queue CI/,
	);
});
test("publication is refused off main, whoever dispatches it", () => {
	for (const actor of ["maintainer", AUTOMATION_ACTOR])
		assert.throws(
			() =>
				authorizePublication(
					env({ GITHUB_ACTOR: actor, GITHUB_REF: "refs/heads/x" }),
					world(),
				),
			/from main/,
		);
});
test("publication without inputs or a repository is refused before anything is read", () => {
	const nothing = {
		api: () => assert.fail("read GitHub"),
		git: () => assert.fail("read git"),
	};
	for (const name of ["RELEASE_COMMIT", "RELEASE_VERSION", "VALIDATION_RUN"])
		assert.throws(
			() => authorizePublication(env({ [name]: undefined }), nothing),
			/Invalid publication inputs/,
		);
	assert.throws(
		() => authorizePublication(env({ GITHUB_REPOSITORY: undefined }), nothing),
		/GITHUB_REPOSITORY is not set/,
	);
});

test("a release commit changes release.json and is its PR's merge commit", () => {
	assert.deepEqual(findRelease(commit, repository, world()), {
		version,
		pr: 7,
	});
});
test("a commit that leaves release.json as it was is not a release", () => {
	const w = world({ previousRequest: { version, branch } });
	assert.equal(findRelease(commit, repository, w), null);
});
test("a commit whose release PR is unmerged, or merged elsewhere, is not a release", () => {
	assert.equal(findRelease(commit, repository, world({ merged: false })), null);
	assert.equal(
		findRelease(commit, repository, world({ mergeCommit: "d".repeat(40) })),
		null,
	);
});
test("a release request that disagrees with the workspace version fails loudly", () => {
	assert.throws(
		() => findRelease(commit, repository, world({ coreVersion: "0.2.0" })),
		/Invalid release request/,
	);
});

/** Workflows that finish as `script` says, recording every dispatch and rerun. */
function actions(script = {}) {
	const runs = [];
	const posted = [];
	let id = 200;
	const base = world();
	const api = (route) => {
		const listed =
			/actions\/workflows\/([a-z]+\.yml)\/runs\?event=workflow_dispatch/.exec(
				route,
			);
		if (listed)
			return {
				workflow_runs: runs
					.filter((run) => run.workflow === listed[1])
					.map(view),
			};
		const jobs = /actions\/runs\/(\d+)\/attempts\/(\d+)\/jobs/.exec(route);
		if (jobs) {
			const run = runs.find((item) => item.id === Number(jobs[1]));
			return { jobs: run.attempts[Number(jobs[2]) - 1].jobs };
		}
		const one = /actions\/runs\/(\d+)$/.exec(route);
		if (one && Number(one[1]) >= 200)
			return view(runs.find((run) => run.id === Number(one[1])));
		return base.api(route);
	};
	const view = (run) => {
		const attempt = run.attempts[run.attempts.length - 1];
		return {
			id: run.id,
			display_title: run.title,
			html_url: `https://example.invalid/runs/${run.id}`,
			status: "completed",
			conclusion: attempt.conclusion,
			run_attempt: run.attempts.length,
		};
	};
	const titleOf = (workflow, inputs) =>
		workflow === "editors.yml"
			? `Editors ${inputs.version}`
			: `${workflow === "publish.yml" ? "Publish" : "Release"} ${inputs.version} (${inputs.commit})`;
	const post = (route, body) => {
		posted.push({ route, body });
		const dispatch = /actions\/workflows\/([a-z]+\.yml)\/dispatches$/.exec(
			route,
		);
		if (dispatch) {
			const attempts = (
				script[dispatch[1]] || [{ conclusion: "success", jobs: [] }]
			).slice();
			runs.push({
				id: id++,
				workflow: dispatch[1],
				title: titleOf(dispatch[1], body.inputs),
				attempts: [attempts.shift()],
				next: attempts,
			});
			return;
		}
		const rerun = /actions\/runs\/(\d+)\/rerun-failed-jobs$/.exec(route);
		if (rerun) {
			const run = runs.find((item) => item.id === Number(rerun[1]));
			run.attempts.push(
				run.next.shift() || { conclusion: "success", jobs: [] },
			);
			return;
		}
		throw new Error(`Unexpected POST ${route}`);
	};
	return {
		io: { api, git: base.git, post, sleep: async () => {}, log: () => {} },
		runs,
		posted,
	};
}
const verifyFailed = {
	conclusion: "failure",
	jobs: [
		{ name: "Authorize npm publication", conclusion: "success" },
		{ name: "Trusted npm Publish", conclusion: "success" },
		{ name: VERIFY_JOB, conclusion: "failure" },
	],
};

test("a release commit dispatches publish, release and editors in order", async () => {
	const { io, posted } = actions();
	const result = await autoRelease(
		{ GITHUB_REPOSITORY: repository, RELEASE_COMMIT: commit },
		io,
	);
	assert.deepEqual(result, { version, pr: 7 });
	assert.deepEqual(
		posted.map((item) => item.route.split("/").slice(-2).join("/")),
		[
			"publish.yml/dispatches",
			"release.yml/dispatches",
			"editors.yml/dispatches",
		],
	);
	assert.deepEqual(posted[0].body, {
		ref: "main",
		inputs: { version, commit, validation_run: "100" },
	});
	assert.deepEqual(posted[1].body.inputs, {
		version,
		commit,
		validation_run: "100",
		npm_run: "200",
	});
	assert.deepEqual(posted[2].body.inputs, { version });
});
test("automation without a commit or a repository fails before reading anything", async () => {
	const { io, posted } = actions();
	const blind = {
		...io,
		api: () => assert.fail("read GitHub"),
		git: () => assert.fail("read git"),
	};
	await assert.rejects(
		autoRelease({ GITHUB_REPOSITORY: repository }, blind),
		/Not a commit: undefined/,
	);
	await assert.rejects(
		autoRelease({ RELEASE_COMMIT: commit }, blind),
		/GITHUB_REPOSITORY is not set/,
	);
	assert.deepEqual(posted, []);
});
test("a commit that is not a release dispatches nothing", async () => {
	const { io, posted } = actions();
	const other = {
		...io,
		git: world({ previousRequest: { version, branch } }).git,
	};
	assert.equal(
		await autoRelease(
			{ GITHUB_REPOSITORY: repository, RELEASE_COMMIT: commit },
			other,
		),
		null,
	);
	assert.deepEqual(posted, []);
});
test("a release with no successful queue run dispatches nothing and fails", async () => {
	const { io, posted } = actions();
	const bare = world({ validation: null });
	const api = (route) =>
		route.includes("ci.yml/runs") ? bare.api(route) : io.api(route);
	await assert.rejects(
		autoRelease(
			{ GITHUB_REPOSITORY: repository, RELEASE_COMMIT: commit },
			{ ...io, api },
		),
		/No successful merge-queue CI run/,
	);
	assert.deepEqual(posted, []);
});
test("a lagging npm verification is rerun once, and then the release goes on", async () => {
	const { io, posted } = actions({
		"publish.yml": [verifyFailed, { conclusion: "success", jobs: [] }],
	});
	await autoRelease(
		{ GITHUB_REPOSITORY: repository, RELEASE_COMMIT: commit },
		io,
	);
	assert.equal(
		posted.filter((item) => item.route.endsWith("rerun-failed-jobs")).length,
		1,
	);
	assert.equal(
		posted.filter((item) => item.route.endsWith("dispatches")).length,
		3,
	);
});
test("a verification that fails twice stops the release", async () => {
	const { io, posted } = actions({
		"publish.yml": [verifyFailed, verifyFailed],
	});
	await assert.rejects(
		autoRelease({ GITHUB_REPOSITORY: repository, RELEASE_COMMIT: commit }, io),
		new RegExp(`Publish ${version} .* ended with failure \\(${VERIFY_JOB}\\)`),
	);
	assert.equal(
		posted.filter((item) => item.route.endsWith("rerun-failed-jobs")).length,
		1,
	);
	assert.ok(!posted.some((item) => item.route.includes("release.yml")));
});
test("any other failure stops the release without a rerun", async () => {
	const published = {
		conclusion: "failure",
		jobs: [{ name: "Trusted npm Publish", conclusion: "failure" }],
	};
	const { io, posted } = actions({ "publish.yml": [published] });
	await assert.rejects(
		autoRelease({ GITHUB_REPOSITORY: repository, RELEASE_COMMIT: commit }, io),
		/Trusted npm Publish/,
	);
	assert.ok(!posted.some((item) => item.route.endsWith("rerun-failed-jobs")));
	const released = {
		conclusion: "failure",
		jobs: [{ name: VERIFY_JOB, conclusion: "failure" }],
	};
	const second = actions({ "release.yml": [released] });
	await assert.rejects(
		autoRelease(
			{ GITHUB_REPOSITORY: repository, RELEASE_COMMIT: commit },
			second.io,
		),
		/Release 0\.3\.0/,
	);
	assert.ok(
		!second.posted.some((item) => item.route.endsWith("rerun-failed-jobs")),
	);
});
test("a run that already exists is waited on rather than dispatched again", async () => {
	const { io, posted } = actions();
	io.post(`repos/${repository}/actions/workflows/editors.yml/dispatches`, {
		ref: "main",
		inputs: { version },
	});
	posted.length = 0;
	await runWorkflow(
		repository,
		"editors.yml",
		`Editors ${version}`,
		{ version },
		io,
	);
	assert.deepEqual(posted, []);
});
