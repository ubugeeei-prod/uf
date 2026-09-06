// @flow
//
// uf's own project.
//
// This repository is a uf project, and every check that runs in CI is a task
// here — so `uf run ci` on a laptop is the same thing the pipeline runs, and a
// check cannot be added to one without the other noticing.
//
// It is also the only honest way to find out what using uf is like. The
// `Library` job runs `uf test`, the documentation is built by `uf build`, and
// the formatter and linter that check the JavaScript in this repository are
// `uf fmt` and `uf lint`. When one of them is wrong, it is wrong here first.
//
// The Rust half is still cargo's, because that is cargo's job. uf runs the
// tasks; it does not pretend to be a Rust build system.

import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  // Not an application: this repository is the toolchain, and the sites it
  // builds live in `docs/` with configs of their own.
  app: {
    router: { enabled: false },
  },

  fmt: {
    nonFlow: {
      // Named rather than left to the default, and the difference is the
      // point: uf's default is a suggestion, and `uf fmt` warns when a
      // suggested formatter is missing rather than failing. This repository
      // installs Biome, its `uf fmt --check` is a required check, and a run
      // that could not look at the JSON must not pass as if it had. Saying so
      // here is how a project asks for that. See ubugeeei-prod/uf#441.
      formatter: "biome",
    },
  },

  lint: {
    // `upstream/` is Meta's and React's source, vendored as submodules. It is
    // not ours to format or lint, and a diff there would be lost on the next
    // sync.
    //
    // `crates/` is Rust. The only JavaScript under it is test fixtures — for
    // the formatter, which needs badly formatted input, and for the checker,
    // which needs input that fails to check. Both are exercised by the Rust
    // tests that own them. Checking them again from the repository root would
    // report every fixture's deliberate defect as this project's defect: 1868
    // of the 3782 type errors `uf check` used to report here came from
    // `crates/uf_fmt/tests/fixtures` alone.
    ignore: ["upstream", "crates", "dist", "target", "node_modules"],
  },

  tasks: {
    // --- Getting a checkout working ------------------------------------
    //
    // Everything after the bootstrap is a `uf run`. The bootstrap itself
    // cannot be, and the reason is structural rather than an oversight:
    // `upstream/flow` is a *path* dependency, so cargo cannot build `uf`
    // until the submodule is checked out, and `uf run` needs a built `uf`.
    // One command breaks that circle, and it is in CONTRIBUTING.md.
    "upstream:sync": "tools/upstream/sync.sh",
    // React's compiler crates, Relay's compiler crates, and React Native's
    // codegen and Libraries — pinned in `tools/upstream/repos.txt`, fetched on
    // request rather than in every job because nothing in the cargo graph
    // depends on them yet. docs/architecture.md says what each is for and what
    // uf does instead today.
    "upstream:sync:integrations": "tools/upstream/sync.sh --integrations",
    setup: {
      command: "echo 'ready: run `uf run ci` to check everything'",
      dependsOn: ["upstream:sync", "build"],
    },

    // --- Rust ----------------------------------------------------------
    "rust:fmt": "cargo fmt --all",
    "rust:fmt:check": "cargo fmt --all -- --check",
    "rust:clippy": "cargo clippy --workspace --all-targets -- -D warnings",
    "rust:test": "cargo test --workspace",
    "rust:bench": "cargo bench --workspace --no-run",
    "rust:metadata": "cargo metadata --format-version 1 --locked",

    // The parser and the checker are vendored, and they are the thing
    // everything else reads, so they get a pass of their own.
    "flow:clippy": "cargo clippy -p uf_flow --all-targets -- -D warnings",
    "flow:test": "cargo test -p uf_flow",

    // --- The toolchain, used on itself ---------------------------------
    //
    // Each of these is a first-class uf command run against a workspace of
    // this repository, not a script that reimplements one. `uf test#library`
    // is the command a contributor types; the task exists so CI types it too,
    // and so `uf run ci` covers it.
    build: "cargo build --release --bin uf",

    // Every `@uniflowed/*` package is Flow, so `cargo test` cannot run a line
    // of it. These are uf tests, run by the runner this repository ships.
    "test:lib": {
      command: "./target/release/uf test#library",
      dependsOn: ["build"],
    },

    // The linter, over this repository's own Flow. uf is the only thing that
    // can lint uf's packages, so a regression here is invisible to every other
    // check in the pipeline — which is why it is now in `ci` rather than
    // described there.
    //
    // Zero errors and 14 warnings over 322 files. `uf lint` fails on errors
    // only, so the warnings are not a countdown to a broken build: they are
    // `react/component-syntax`, `react/no-default-export-component`,
    // `flow/unsafe-getters-setters` and `react-native/platform-split` — four
    // rules whose own rows in the default table call them style preferences,
    // migration aids, or patterns that are legitimate in some code, which is
    // exactly what `warn` is for. A rule that should block belongs at `error`
    // in `crates/uf_config/src/lint.rs`; a warning that nobody intends to act
    // on belongs at `off` with the argument written on its row.
    //
    // Seven suppressions stand in the packages, each on the line, under the
    // paragraph that argues it: `Node<any>` and `Cell<any>` where Flow has no
    // existential and the types are invariant in their parameter, the event
    // constructors that Flow's own libdef declares with writable init
    // properties, `fireEvent`'s proxy (#401), `act`'s promise branch, the
    // router's one cast, and `expect`'s matcher indexer (#402). Two have a fix
    // somebody can go and do and are filed; the rest end in something Flow
    // does not have. Two more suppressions stand outside that rule, and both
    // are a rule being right in general: `Object.assign` onto a callable in
    // `@uniflowed/test`, which an object spread cannot produce, and the theme
    // bootstrap in the documentation layout, which has to be `__html` because
    // React escapes a text child. Nine directives were added to reach zero, and
    // `uniflowed/unknown-lint-suppression` keeps every suppression in the tree
    // naming a rule that exists.
    //
    // The numbers were wrong here twice, which is its own lesson: this said
    // 315 errors and named `flow/react-intrinsic-overlap` (89) and
    // `react/hooks-rules` (86) as the largest groups when both reported
    // nothing — the first is one of sixteen rules that need type inference uf
    // does not implement yet, so it is skipped rather than passing — and then
    // 153 errors including eight in `packages/test`'s fake timers, which #237
    // had already fixed by teaching the compiler that a `useX` name is a hook
    // only where the module says React. ubugeeei-prod/uf#225 has the count
    // this replaces.
    "check:lib": {
      command: "./target/release/uf lint",
      dependsOn: ["build"],
    },

    // The formatter, over the same. `--check` rather than a write, because CI
    // reporting a diff is useful and CI committing one is not.
    "fmt:check": {
      command: "./target/release/uf fmt --check",
      dependsOn: ["build"],
    },

    // The formatter, over Flow nobody here wrote.
    //
    // `fmt:check` and the guarantee tests both run against sources written
    // by people who knew what the printer does. The fifteen repositories in
    // `tools/corpus/repos.txt` are about 8,100 Flow modules that were not,
    // and they reach the corners of the grammar that a hand-written corpus
    // reaches for last.
    //
    // Not in `ci`: the fixtures are ~1 GB of other people's code, and a
    // check that fails on a fresh clone teaches people to ignore failures.
    // The test skips when they are absent, so `rust:test` stays honest
    // either way.
    "corpus:sync": "tools/corpus/sync.sh",
    "fmt:corpus": {
      command: "cargo test -p uf_fmt --test upstream_corpus -- --nocapture",
      dependsOn: ["corpus:sync"],
    },

    // The documentation site, built by the framework it documents. The script
    // stages the brand assets — shared with the README and the release pages,
    // so they live at the repository root — into Vite's public directory
    // first, and then runs `uf build#docs`.
    "docs:build": {
      command: "UF_BIN=./target/release/uf tools/docs/build.sh",
      dependsOn: ["build"],
    },
    // The documentation site, in a browser, while you edit it.
    "docs:dev": {
      command: "./target/release/uf dev#docs",
      dependsOn: ["build"],
    },

    // --- Measurement ----------------------------------------------------
    //
    // Deliberately not in `ci`, and for two different reasons.
    //
    // `bench:tui` needs React Ink, which is a dependency of the benchmark and
    // of nothing else in this repository — `npm install` inside
    // `tools/bench/tui` first. Its byte counts are deterministic and *are*
    // checked in CI, by `tests/library/tui.test.js`, which asserts uf's half of
    // the table in `docs/app/guide/tui/_uf.page.mdx` against the renderer. What
    // this task adds is Ink's half and the wall clock, and a wall clock on a
    // shared build agent is a measurement of the agent.
    //
    // `bench:tui:startup` answers the question ubugeeei-prod/uf#316 says has to
    // be answered before a uf command is written in Flow: what a Flow entry
    // point on the Capability JS Host costs before it draws anything. Run it
    // twice — the first run of a new `uf` binary pays for every transform and
    // the second pays for none.
    "bench:tui": {
      command:
        "UF_PROJECT_ROOT=. UF_BINARY=./target/release/uf node --import @uniflowed/host/register tools/bench/tui/bench.js",
      dependsOn: ["build"],
    },
    "bench:tui:startup": {
      command:
        "UF_PROJECT_ROOT=. UF_BINARY=./target/release/uf node --import @uniflowed/host/register tools/bench/tui/startup.js",
      dependsOn: ["build"],
    },

    // --- Release --------------------------------------------------------
    //
    // Each is a step the release workflow runs, named so it can be run by
    // hand first. A release step nobody can rehearse is a release step that
    // is debugged in production.
    //
    // `release:preflight` is the one to run before tagging: a name `npm trust`
    // has not bound fails the publish job *after* the names before it have
    // gone out, which half-sends a release. It also reports where each name's
    // `latest` points, which is the other half of #408 and is checked here
    // because nothing in the pipeline can see it.
    "release:preflight": "tools/release/preflight.sh",
    // And the step that moves `latest`, after the release. `publish.yml` sends
    // a prerelease on the `alpha` tag — right, and it stays that way, because
    // a prerelease must not displace a stable release — so while these
    // packages have no stable release nothing moves `latest` at all: it sat
    // where the first publish left it — `0.0.0-alpha.1` on five names,
    // `0.0.0-alpha.2` on twelve — through seven releases, and that is what
    // `npm install @uniflowed/react` gave a person.
    //
    // It cannot be a step in the publish job. That job authenticates with the
    // OIDC id-token GitHub mints for it, and npm exchanges that token for
    // `npm publish` and nothing else — `npm dist-tag add` is an authenticated
    // `PUT` that asks a 2FA account for a one-time password. So it is a
    // person's step, like `release:bootstrap` and the `npm trust` bind beside
    // it, and the script says so at length.
    "release:promote": "tools/release/promote-latest.sh",
    "release:promote:test": "tools/release/test-promote-latest.sh",
    // What actually reached npm, read from the registry. `publish.yml` runs
    // it after publishing, because `uf@0.0.0-alpha.2` had a tag, a GitHub
    // release and nothing on npm, and nothing noticed. See #142.
    "release:verify": "tools/release/verify-npm.sh",
    // The offline half of it: a published package whose dependency is not
    // published resolves to nothing. No network, so `ci` runs it.
    "release:closure": "tools/release/verify-npm.sh --closure-only",
    // And that a package somebody implemented is on its way to npm at all.
    // Ten were not, `@uniflowed/state` and `@uniflowed/effect` among them:
    // about 22,000 lines of Flow that `npm install` answered `ETARGET` for.
    // A list somebody adds to is a list somebody forgets, so the rule is
    // stated from the other side — a package that never calls
    // `nativeRuntimeRequired` has to be named in one of the two manifests.
    publishable: "tools/ci/publishable.sh",
    "publishable:test": "tools/ci/test-publishable.sh",
    // Not in `ci.dependsOn` here, and that is a sequencing detail rather than
    // an exception: the release branch rewrites that list wholesale to close
    // an eight-task gap between it and the pipeline, and adding two names to
    // the old list would conflict with it for nothing. They go in with that
    // list, which cannot name a task that does not exist yet.
    "install:test": "tools/release/test-install.sh",
    // The bootstrap is run by hand, once, by one person, and every mistake
    // in it costs a round trip and blocks a release. It has cost three, each
    // a shape of `npm trust github` that no test was strict enough to see.
    "release:trust:test": "tools/release/test-trust-npm.sh",
    // And the bump, whose two mistakes were both about the *second* run: a
    // changelog written after the version moved, and a resumed release that
    // was told its Cargo.toml has no version.
    "release:bump:test": "tools/release/test-bump-version.sh",
    // And that a released version's changelog says what the release contains.
    // `uf@0.0.0-alpha.5` went out with twenty-two commits in it and twelve in
    // its notes: the section is written before the branch stops waiting for
    // CI, and what merges meanwhile is in the tarball and in nobody's notes.
    "release:changelog": "tools/ci/changelog-covers-the-release.sh",
    "release:changelog:test": "tools/ci/test-changelog-covers.sh",
    // `npm trust` binds a name the registry already has and cannot create
    // one, so a name that has never been published is published once by a
    // person and is the workflow's from then on.
    "release:bootstrap": "tools/release/bootstrap-publish.sh",
    "release:manifest": "tools/release/build-manifest.sh",
    "release:package": "tools/release/package-binaries.sh",
    "release:bump": "tools/release/bump-version.sh",

    // --- Manifests -----------------------------------------------------
    //
    // `npm ci` refuses a lock that disagrees with the manifests, and it
    // refuses it in every job that installs: six went red at once when a
    // package was added and the lock was not regenerated, each reporting a
    // package none of those jobs is about. This says it once, where the
    // manifests are the subject.
    // And that every shell script in the repository parses under the shell CI
    // uses. `uf@0.0.0-alpha.8` published all seventeen packages and the release
    // was reported as failed, because `verify-npm.sh` would not parse: an
    // unquoted here-document whose body used backticks as punctuation, which a
    // shell reads as command substitution. It had been correct for weeks —
    // under bash, which is `/bin/sh` on a laptop, where dash is `/bin/sh` on
    // the runner. `sh -n` reads and does not run, so this costs milliseconds.
    "scripts:parse": "tools/ci/scripts-parse.sh",
    "scripts:parse:test": "tools/ci/test-scripts-parse.sh",
    lockfile: "tools/ci/lockfile-in-sync.sh",
    // The check reads the lock rather than regenerating it, so every way a
    // lock can fall behind has to be written down as a case. Its first
    // version compared bytes against a fresh `npm install
    // --package-lock-only`, which passed here and failed in CI over a
    // difference that could not be reproduced here afterwards.
    "lockfile:test": "tools/ci/test-lockfile-in-sync.sh",

    manifests:
      "node -e \"for (const f of require('node:fs').globSync('packages/*/package.json')) JSON.parse(require('node:fs').readFileSync(f, 'utf8'))\"",

    // --- The whole thing -----------------------------------------------
    //
    // What CI runs, in one command. A check that is in the pipeline and not
    // here is a check a contributor cannot run before pushing.
    //
    // It had drifted eight tasks wide: the pipeline grew `flow:clippy`,
    // `flow:test`, `rust:bench`, `release:closure`, `release:trust:test` and
    // three more, and each was added to a workflow without being added here —
    // including `release:closure`, whose own comment says "no network, so `ci`
    // runs it" while `ci` did not. The list is the whole of `uf run` in
    // `.github/workflows/`, and `install:test` is the one deliberate omission,
    // below.
    ci: {
      command: "echo 'every check passed'",
      dependsOn: [
        "rust:fmt:check",
        "rust:clippy",
        "rust:test",
        "rust:bench",
        "flow:clippy",
        "flow:test",
        "fmt:check",
        "check:lib",
        "test:lib",
        "docs:build",
        "rust:metadata",
        "manifests",
        "lockfile",
        "lockfile:test",
        "scripts:parse",
        "scripts:parse:test",
        "release:closure",
        "publishable",
        "publishable:test",
        "release:trust:test",
        "release:promote:test",
        "release:bump:test",
        "release:changelog",
        "release:changelog:test",
        // Not `install:test`. It packages a release before installing it, and
        // packaging needs `wild-linker`, which CI installs in that job and a
        // laptop has no reason to have. A `uf run ci` that fails on a fresh
        // checkout for a missing linker teaches people to ignore failures,
        // which costs more than this check earns here — it still runs in the
        // pipeline, where the linker is present.
      ],
    },
  },
});
