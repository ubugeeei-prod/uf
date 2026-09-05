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
    // check in the pipeline.
    //
    // Not in `ci` yet, and the reason is written down rather than left to be
    // rediscovered: it reports 315 errors today. The largest groups are
    // `flow/unclear-type` (125), `flow/react-intrinsic-overlap` (89) and
    // `react/hooks-rules` (86), and each needs looking at on its own terms —
    // some are real findings in uf's packages, and some are rules that are
    // wrong the way `flow/ambiguous-object-type` was wrong.
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
    // by people who knew what the printer does. React, Metro, Relay and
    // React Native are about 5,800 Flow modules that were not, and they
    // reach the corners of the grammar that a hand-written corpus reaches
    // for last.
    //
    // Not in `ci`: the fixtures are ~240 MB of submodule, and a check that
    // fails on a fresh clone teaches people to ignore failures. The test
    // skips when they are absent, so `rust:test` stays honest either way.
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

    // --- Release --------------------------------------------------------
    //
    // Each is a step the release workflow runs, named so it can be run by
    // hand first. A release step nobody can rehearse is a release step that
    // is debugged in production.
    //
    // `release:preflight` is the one to run before tagging: a name `npm trust`
    // has not bound fails the publish job *after* the names before it have
    // gone out, which half-sends a release.
    "release:preflight": "tools/release/preflight.sh",
    "install:test": "tools/release/test-install.sh",
    "release:manifest": "tools/release/build-manifest.sh",
    "release:package": "tools/release/package-binaries.sh",
    "release:bump": "tools/release/bump-version.sh",

    // --- Manifests -----------------------------------------------------
    manifests:
      "node -e \"for (const f of require('node:fs').globSync('packages/*/package.json')) JSON.parse(require('node:fs').readFileSync(f, 'utf8'))\"",

    // --- The whole thing -----------------------------------------------
    //
    // What CI runs, in one command. A check that is in the pipeline and not
    // here is a check a contributor cannot run before pushing.
    ci: {
      command: "echo 'every check passed'",
      dependsOn: [
        "rust:fmt:check",
        "rust:clippy",
        "rust:test",
        "fmt:check",
        "test:lib",
        "docs:build",
        "rust:metadata",
        "manifests",
      ],
    },
  },
});
