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

  // What no command walks into: `uf fmt`, `uf lint`, `uf check`, `uf test` and
  // `uf doc` all read this one list, which is why it is here rather than under
  // `lint`, where it used to be and where it only ever looked like one
  // command's business.
  //
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

  test: {
    // `uf run test:lib:coverage` measures the packages this repository ships,
    // and nothing else. The suite that drives them lives in `tests/library`,
    // and the docs site and the fixtures under `tests/` are not the product —
    // counting them would move the number for reasons nobody could act on.
    //
    // No threshold yet, deliberately. A gate is a promise about the next
    // change, and the honest first step is to publish the number and let it
    // settle; a threshold set on the first measurement is a threshold set by
    // whatever happened to be true that afternoon. #280 says what it is.
    coverage: {
      include: ["packages/"],
      reporters: ["text", "lcov"],
    },
  },

  // Every task below either declares `inputs` — the files it reads, which is
  // what `uf run` keys its cache on — or does not, in which case it runs every
  // time. Which side a task is on is a judgement, and the ones that are *not*
  // cached are the interesting half, so each of them says why on its own row.
  // Three reasons cover all of them:
  //
  //   * **cargo has a better cache than this one.** Every `cargo` task is
  //     always run. Cargo's fingerprints know about the rustc version, the
  //     feature resolution, the build scripts and the environment variables a
  //     build script read; a content hash over `crates/**` knows none of that,
  //     and a second, weaker cache in front of a stronger one can only ever be
  //     wrong. A no-op `cargo` invocation costs a fraction of a second anyway.
  //   * **the answer is not a function of files uf can name.** `test:lib` and
  //     `docs:build` run JavaScript on whichever Node is on `PATH`, against
  //     whatever `node_modules` holds. This repository has already been bitten
  //     by exactly that: the library suite is green on node 24 and not on node
  //     25, and no glob can say so.
  //   * **it reads something that is not a file.** `release:changelog` reads
  //     the git history; `release:verify` and `release:preflight` read the
  //     registry.
  //
  // `uf run ci --why` prints the reason for every task, from the runner rather
  // than from this comment, which is the version to trust.
  tasks: {
    // --- Getting a checkout working ------------------------------------
    //
    // Everything after the bootstrap is a `uf run`. The bootstrap itself
    // cannot be, and the reason is structural rather than an oversight:
    // `upstream/flow` is a *path* dependency, so cargo cannot build `uf`
    // until the submodule is checked out, and `uf run` needs a built `uf`.
    // One command breaks that circle, and it is in CONTRIBUTING.md.
    "upstream:sync": "tools/upstream/sync.sh",
    // The sync applies `tools/upstream/patches/flow` to the submodule after
    // checking it out, so a checkout is the pinned `facebook/flow` commit plus
    // the fixes uf needs and upstream has not taken. That step is worth a test
    // of its own because its failure mode is silence: a patch to a type checker
    // that is quietly not applied compiles, passes, and answers differently.
    // Every way one could go missing is a case in the script, against a scratch
    // submodule rather than the 40 MB one.
    "upstream:patches:test": {
      command: "tools/upstream/test-patches.sh",
      inputs: ["tools/upstream/sync.sh", "tools/upstream/test-patches.sh"],
    },
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
      // No `inputs`, so it runs every time. It executes Flow on the Node that
      // happens to be on `PATH`, through the transform cache in `.uf`, against
      // the `node_modules` tree `npm ci` installed — and its result genuinely
      // differs between node 24 and node 25. None of that is a path uf can
      // hash, so there is nothing honest to key on.
    },

    // The same suite, with V8's counters on, run from the repository root so
    // that `packages/*` is inside the project and the counts have somewhere to
    // land. `uf test#library` cannot do it: its project root is
    // `tests/library`, and coverage is about the project's own files, so every
    // package the suite exercises would be outside it.
    //
    // Not in `ci`, and the reason is the threshold rather than the cost —
    // collecting and mapping is 0.4 s on a run that takes a minute. Until
    // `test.coverage.thresholds` names a number this task cannot fail, and a
    // green check that cannot go red teaches people to ignore checks. So it is
    // one command rather than a gate, and what is left to do is choose the
    // number; #280 carries that argument.
    "test:lib:coverage": {
      command: "./target/release/uf test tests/library --coverage",
      dependsOn: ["build"],
    },

    // The linter, over this repository's own Flow. uf is the only thing that
    // can lint uf's packages, so a regression here is invisible to every other
    // check in the pipeline — which is why it is now in `ci` rather than
    // described there.
    //
    // Zero errors, and every warning from one of four rules:
    // `react/component-syntax`, `react/no-default-export-component`,
    // `flow/unsafe-getters-setters` and `react-native/platform-split`. `uf
    // lint` fails on errors only, so those are not a countdown to a broken
    // build — each of the four has a row in the default table calling it a
    // style preference, a migration aid, or a pattern that is legitimate in
    // some code, which is exactly what `warn` is for. A rule that should block
    // belongs at `error` in `crates/uf_config/src/lint.rs`; a warning nobody
    // intends to act on belongs at `off` with the argument written on its row.
    //
    // The condition rather than a count, deliberately. This paragraph used to
    // say "14 warnings over 322 files"; the file count was wrong within a
    // fortnight and would be wrong again the next time anybody added a file,
    // and a number nobody can act on is not the fact a reader needs. What they
    // need is whether the exclusion still holds, and that is a question about
    // *which rules*, which only changes when somebody changes one.
    // ubugeeei-prod/uf#433.
    //
    // Every suppression in the packages stands on its own line, names its rule,
    // and sits under the paragraph that argues it. Grouped by rule, because the
    // rule is what a reader can act on and a count is not — see the lesson
    // below, and #433:
    //
    //   * `flow/unclear-type`, for the types Flow cannot say. `Node<any>`,
    //     `Cell<any>`, `Binding<any, any>` and `Job<any>` where there is no
    //     existential and the parameter is invariant; the event constructors
    //     Flow's own libdef declares with writable init properties; `act`'s
    //     promise branch; the router's one cast. One of them is not a type at
    //     all: `expect.any`'s signature, which the rule reads as an `any`
    //     because it scans source text and `readonly any:` is not a shape it
    //     recognises as a property key.
    //   * `flow/unsafe-getters-setters`, in two files, for the `signal` getter
    //     `@uniflowed/query` puts on a request context so that *reading* it is
    //     what opts a query into cancellation.
    //   * `security/no-dangerously-set-inner-html`, three times, for the JSON
    //     and theme payloads a render has to write as `__html` because React
    //     escapes a text child.
    //   * `fetch/no-global-override`, once, in the package whose entire job is
    //     to intercept `fetch`.
    //
    // The two `flow/unclear-type` suppressions that had a fix somebody could go
    // and do are gone rather than counted: `fireEvent`'s proxy (#401), and
    // `expect`'s matcher indexer (#402), whose table now takes `mixed` the way
    // most of it always did while the published `Matchers` keeps the narrow
    // signature each name deserves. What is left ends in something Flow does
    // not have, or in a rule being right in general.
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
      // The same superset, for the same reason, over the same binary.
      inputs: ["**", "!upstream/**", "target/release/uf"],
    },

    // The formatter, over the same. `--check` rather than a write, because CI
    // reporting a diff is useful and CI committing one is not.
    "fmt:check": {
      command: "./target/release/uf fmt --check",
      dependsOn: ["build"],
      // A deliberate superset. What `uf fmt` opens is decided by uf's own
      // project discovery rather than by a list in this file, so a glob that
      // tried to reproduce that rule would be a guess — and a guess that came
      // out too *narrow* is the one failure this cache must not have: a file
      // that was never formatted, reported as formatted. So the input is
      // everything in the checkout except the vendored submodules, plus the
      // binary doing the work. It over-invalidates, which costs a run; it
      // cannot under-invalidate, which would cost the check.
      inputs: ["**", "!upstream/**", "target/release/uf"],
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
      // No `inputs`, and the reason is Vite rather than uf: the build runs
      // Node, resolves from `node_modules`, and writes through Vite's own
      // caches. Same argument as `test:lib`.
    },
    // The documentation site, in a browser, while you edit it.
    "docs:dev": {
      command: "./target/release/uf dev#docs",
      dependsOn: ["build"],
    },

    // And that every link in what it wrote resolves to something the same
    // build wrote. The manual has hundreds of internal links, and a page that
    // moves takes every link to its old name with it — `/guide/ci` was added
    // and nothing anywhere would have said a word if the pages pointing at it
    // had said `/guide/pipeline`, because each page prerenders on its own and
    // a dead `<a href>` renders perfectly.
    //
    // Over the output rather than the sources, which is the whole of it: an
    // anchor is checked against the ids the markdown pipeline actually
    // rendered, and `sitemap.xml` is written from the documents that were
    // prerendered, so a `<loc>` nothing answers is one build disagreeing with
    // itself. `tests/library/docs-nav.test.js` is the other half and does not
    // overlap: it reads `docs/app/_design/nav.js` before anything is rendered,
    // and caught the duplicate `/guide/state` that made the reading order a
    // cycle (#586). Neither subsumes the other — a page can exist and not be
    // built, and a route can be served with no page behind it.
    //
    // Offline, so it can block a pull request: every answer is a function of
    // the output directory. See `docs:links:external` for the other half.
    "docs:links": {
      command: "tools/ci/links-resolve.sh",
      dependsOn: ["docs:build"],
      // No `inputs`. What it reads is what `docs:build` wrote, and that task
      // is uncached for Vite's reasons; a cache in front of this one would
      // replay a verdict about a build that no longer exists.
    },
    "docs:links:test": {
      command: "tools/ci/test-links-resolve.sh",
      inputs: ["tools/ci/links-resolve.sh", "tools/ci/test-links-resolve.sh"],
    },
    // The same links, resolved over the network. Not in `ci`, and this is the
    // argument rather than an oversight: an external host that is slow, rate
    // limiting, or behind a captive portal would fail a pull request that
    // changed nothing, and a check that fails for reasons the author cannot
    // act on is a check people learn to re-run until it passes. Which is the
    // habit that makes the *real* failure invisible.
    //
    // So it is a task a contributor can run by hand and a schedule runs
    // nightly (`.github/workflows/links.yml`), and even there only a definite
    // answer counts: `404`, `410` and `451` are dead, and a timeout, a `429`
    // or a `5xx` is reported as unverified and passes, because none of those
    // is a statement about the link.
    "docs:links:external": {
      command: "tools/ci/links-resolve.sh --external",
      dependsOn: ["docs:build"],
    },

    // --- Security -------------------------------------------------------
    //
    // `security.yml` runs zizmor over the workflows and CodeQL over the
    // JavaScript. Both are worth having and neither looks at what uf ships to
    // a user, so between them they can be entirely green about a release that
    // published a private key. #524 asked for the half that is uf's own.
    //
    // Three passes, and each is written so that failing it produces a sentence
    // somebody can act on:
    //
    //   * `docs/security.md` is a threat model whose own rules say *"every
    //     guard has a test that fails without it"* and *"a row whose test does
    //     not exist on `main` yet is marked `todo`"*. Nothing enforced either
    //     sentence, so the table could say anything. Now a row that names a
    //     test names one that exists, a row that points nowhere says so, no
    //     CVE is answered two different ways by two rows, and — the finding
    //     that made this worth writing — every row is inside a table at all.
    //     Three were not, and GFM renders a `|`-line after a paragraph as
    //     prose, so they were invisible on the rendered page.
    //   * what `uf build` wrote. A static site is served byte for byte by
    //     whatever host it lands on, so a credential file, a well-known token
    //     shape, the build's own route manifest or the absolute path of the
    //     machine that built it are all published the moment they are in that
    //     directory.
    //   * what `uf new` writes, run rather than read: a manifest with no
    //     lifecycle scripts (`uf install` refuses one), a config that weakens
    //     no default, and a `.gitignore` that covers the two files in the env
    //     cascade meant to hold credentials.
    //
    // Not a dependency audit (#492) and not an install-script audit (#495) —
    // both are their own issues — and not a substitute for a row's own
    // regression test: this checks that the row points at something, never
    // that the something still asserts what the row claims.
    "security:scan": {
      command: "tools/ci/security-scan.sh",
      dependsOn: ["docs:build"],
      // No `inputs`: it reads what `docs:build` wrote and runs `uf new`
      // through the built binary, so its answer is not a function of files
      // that can be named here.
    },
    "security:scan:test": {
      command: "tools/ci/test-security-scan.sh",
      inputs: ["tools/ci/security-scan.sh", "tools/ci/test-security-scan.sh"],
    },

    // The response headers the documentation site is served with, checked
    // against the site itself.
    //
    // `security:scan` deliberately does not cover these — its own header says
    // why, and the reason is that the only headers in this repository are set
    // by a Cloudflare worker, which is a deployment rather than something
    // `uf build` writes, and asserting a server's behaviour by grepping its
    // source would be asserting a file. So this one drives the worker: it
    // imports `infra/cloudflare/workers/docs.js`, calls its `fetch` with an
    // `ASSETS` binding over `docs/dist/docs`, and compares the policy that
    // comes back against what the pages actually load. See #598.
    //
    // No `inputs`, and the same reason `docs:links` has none: what it reads is
    // what `docs:build` wrote, and a cache in front of this one would replay a
    // verdict about a build that no longer exists.
    "docs:csp": {
      command: "tools/ci/docs-csp.sh",
      dependsOn: ["docs:build"],
    },

    // The `Docs build` job, in one command.
    //
    // `uf run` takes one task, and three of the five checks below read the
    // site off disk — `docs:links`, `security:scan` and `docs:csp`, each of
    // which names `docs:build` as a dependency. Run separately that is three
    // builds of the same site, because `docs:build` declares no `inputs` and
    // is always run. One invocation is one graph and `docs:build` is one node
    // in it. Same shape as `ci` itself, for the same reason.
    "docs:verify": {
      command: "echo 'the site builds, and everything in it resolves'",
      dependsOn: [
        "docs:build",
        "docs:links",
        "docs:links:test",
        "security:scan",
        "security:scan:test",
        "docs:csp",
      ],
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

    // What the route cache is worth in front of a slow loader: the same page
    // served twice, with and without the cache `rendering.cache.route` turns
    // on. ubugeeei-prod/uf#277 asked for a number rather than a claim, and
    // this is it.
    //
    // Not in `ci`, for the reason above and one of its own: the loader is a
    // `setTimeout`, so the cold column is a measurement of a sleep and the
    // whole thing is a wall clock on whatever machine ran it. The behaviour it
    // is a number *about* is checked without a clock at all, in
    // `tests/library/cache.test.js`, which drives the store's own `now`.
    "bench:route-cache": {
      command:
        "UF_PROJECT_ROOT=. UF_BINARY=./target/release/uf node --import @uniflowed/host/register tools/bench/cache/route-cache.js",
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
    "release:closure": {
      command: "tools/release/verify-npm.sh --closure-only",
      // The offline phase reads the published list and each named package's
      // manifest, and returns before the phase that talks to the registry.
      inputs: [
        "packages/*/package.json",
        "tools/release/verify-npm.sh",
        "tools/release/published-packages.txt",
      ],
    },
    // And that a package somebody implemented is on its way to npm at all.
    // Ten were not, `@uniflowed/state` and `@uniflowed/effect` among them:
    // about 22,000 lines of Flow that `npm install` answered `ETARGET` for.
    // A list somebody adds to is a list somebody forgets, so the rule is
    // stated from the other side — a package that never calls
    // `nativeRuntimeRequired` has to be named in one of the two manifests.
    publishable: {
      command: "tools/ci/publishable.sh",
      // It reads two manifests and every `.js` under `packages/`, looking for
      // a `nativeRuntimeRequired(` call. That is the whole of it.
      inputs: [
        "packages/**/*.js",
        "tools/ci/publishable.sh",
        "tools/release/published-packages.txt",
        "tools/release/pending-packages.txt",
      ],
    },
    "publishable:test": {
      command: "tools/ci/test-publishable.sh",
      // A self-test: it builds its fixtures in a temp directory and runs the
      // script under test against them, so the two scripts are its inputs.
      inputs: ["tools/ci/publishable.sh", "tools/ci/test-publishable.sh"],
    },
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
    // The unit is the commit and not the `(#NNN)` in its subject, which is the
    // second way this went wrong: a commit GitHub did not stamp was not
    // unmatched, it was uncounted, and `uf@0.0.0-alpha.8` shipped one while
    // this check reported sixteen of sixteen.
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
    // And that the installer's banner fits an eighty-column terminal. The two
    // lines `curl … | sh` prints before it does anything are the first uf
    // anybody sees, and they shared one line until the tagline grew to
    // ninety-six columns with its indent — wrapping mid-sentence onto a second
    // line with no indent. The installer cannot ask how wide the terminal is,
    // so the width is a property of the text and is checked here.
    "install:banner": {
      command: "tools/ci/install-banner-fits.sh",
      // It extracts `uf_brand()` from the installer and measures what it
      // prints. Two files, and nothing else.
      inputs: ["infra/cloudflare/setup-assets/install.sh", "tools/ci/install-banner-fits.sh"],
    },
    "scripts:parse": {
      command: "tools/ci/scripts-parse.sh",
      // `git ls-files '*.sh'` minus `upstream/`, which is every tracked shell
      // script in the repository and they all live in these two directories.
      // A `.sh` that is not tracked is hashed here and not checked there —
      // over-invalidation, which is the safe side.
      inputs: ["tools/**/*.sh", "infra/**/*.sh"],
    },
    "scripts:parse:test": {
      command: "tools/ci/test-scripts-parse.sh",
      inputs: ["tools/ci/scripts-parse.sh", "tools/ci/test-scripts-parse.sh"],
    },
    // And that `integrations/` and the installer still agree. The three CI
    // integrations configure `install.sh` entirely through the environment,
    // and nothing else in this repository reads both sides — a renamed
    // variable would leave them passing something nothing reads, the
    // installer would fall back to its defaults, and the failure would be
    // `uf: command not found` in somebody else's pipeline.
    integrations: {
      command: "tools/ci/integrations-agree.sh",
      // Both sides of the agreement, and the script that compares them.
      inputs: [
        "integrations/**",
        "infra/cloudflare/setup-assets/install.sh",
        "tools/ci/integrations-agree.sh",
      ],
    },
    lockfile: {
      command: "tools/ci/lockfile-in-sync.sh",
      // The lock, the root manifest, and every workspace manifest the root's
      // own globs reach.
      inputs: [
        "package-lock.json",
        "package.json",
        "packages/*/package.json",
        "tools/ci/lockfile-in-sync.sh",
      ],
    },
    // The check reads the lock rather than regenerating it, so every way a
    // lock can fall behind has to be written down as a case. Its first
    // version compared bytes against a fresh `npm install
    // --package-lock-only`, which passed here and failed in CI over a
    // difference that could not be reproduced here afterwards.
    "lockfile:test": {
      command: "tools/ci/test-lockfile-in-sync.sh",
      inputs: ["tools/ci/lockfile-in-sync.sh", "tools/ci/test-lockfile-in-sync.sh"],
    },

    // And that the gate at the bottom of `ci.yml` still covers `ci.yml`. The
    // `CI` job there is red unless every job it names came back `success`, and
    // it is what stands between a `Toolchain` that does not compile and an
    // armed auto-merge onto `main`. #367 is the morning before it existed:
    // `Toolchain` failed to compile, every job that needs it reported
    // `skipped`, GitHub counted five skipped required checks as satisfied, and
    // the merge went through onto a `main` that `cargo check` exits 101 on.
    //
    // The gate only holds while its `needs:` names every job, which is a list
    // somebody has to add to — the same shape as the drift that left this
    // task list eight tasks behind the pipeline. So it is checked rather than
    // trusted, along with the other ways a required context can come back
    // `skipped` and be read as a pass.
    "ci:gate": {
      command: "tools/ci/gate-covers-every-job.sh",
      // Every workflow, not just `ci.yml`: `Zizmor` is a required context and
      // lives in `security.yml`.
      inputs: [".github/workflows/*.yml", "tools/ci/gate-covers-every-job.sh"],
    },
    "ci:gate:test": {
      command: "tools/ci/test-gate-covers-every-job.sh",
      inputs: ["tools/ci/gate-covers-every-job.sh", "tools/ci/test-gate-covers-every-job.sh"],
    },

    // The crates `cargo semver-checks` cannot compare, which is computed and
    // therefore capable of being wrong in two directions: too narrow and the
    // job fails for every pull request, which is what #633 did by adding a
    // crate that reaches `uf_flow` and appeared in nobody's list; too wide and
    // crates leave the gate with nothing going red. There is no `ci:semver`
    // task beside this one because the check needs a baseline revision, and
    // the workflow is the only place that knows which one.
    "ci:semver:test": {
      command: "tools/ci/test-semver-exclude.sh",
      inputs: ["tools/ci/semver-exclude.sh", "tools/ci/test-semver-exclude.sh"],
    },

    // And that a job which runs the workspace suite installs the runtimes the
    // suite starts. `bun_host.rs`, `deno_host.rs` and `permissions.rs` start
    // real processes and fail rather than skip when the runtime is absent —
    // deliberately, so a host claim nobody checked cannot be green — which
    // makes `cargo test --workspace` runnable only where all three are
    // installed.
    //
    // `publish.yml` was not such a job. `uf@0.0.0-alpha.14` was tagged, its
    // four binaries were built and released, and the publish job failed on six
    // Deno tests before publishing a single package: a tag, a GitHub release,
    // and nothing on npm. Deno had been added to `ci.yml`'s two suite-running
    // jobs and not to the third, which lives in another file. See #634.
    "ci:runtimes": {
      command: "tools/ci/workspace-suite-runtimes.sh",
      inputs: [
        ".github/workflows/*.yml",
        "crates/uf_cli/tests/*.rs",
        "tools/ci/workspace-suite-runtimes.sh",
      ],
    },
    "ci:runtimes:test": {
      command: "tools/ci/test-workspace-suite-runtimes.sh",
      inputs: ["tools/ci/workspace-suite-runtimes.sh", "tools/ci/test-workspace-suite-runtimes.sh"],
    },

    manifests: {
      command:
        "node -e \"for (const f of require('node:fs').globSync('packages/*/package.json')) JSON.parse(require('node:fs').readFileSync(f, 'utf8'))\"",
      inputs: ["packages/*/package.json"],
    },

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
        "upstream:patches:test",
        "install:banner",
        "scripts:parse",
        "scripts:parse:test",
        "integrations",
        "release:closure",
        "publishable",
        "publishable:test",
        "release:trust:test",
        "release:promote:test",
        "release:bump:test",
        "release:changelog",
        "release:changelog:test",
        "ci:gate",
        "ci:gate:test",
        "ci:runtimes",
        "ci:runtimes:test",
        // `docs:verify` rather than the five checks under it, because that is
        // what the `Docs build` job runs and this list is the whole of
        // `uf run` in `.github/workflows/`. It reaches `docs:links`,
        // `docs:links:test`, `security:scan`, `security:scan:test` and
        // `docs:csp`, and builds the site once for all of them.
        "docs:verify",
        // Not `docs:links:external`. It is the same check with the network
        // turned on, and an external host that is slow or rate limiting would
        // fail a pull request that changed nothing — which teaches people to
        // re-run a red check until it goes green. A schedule runs it instead,
        // in `.github/workflows/links.yml`, and `uf run docs:links:external`
        // is it on a laptop.
        //
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
