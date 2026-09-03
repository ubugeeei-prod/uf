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
    ignore: ["upstream", "dist", "target", "node_modules"],
  },

  tasks: {
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
    build: "cargo build --release --bin uf",

    // Every `@uniflowed/*` package is Flow, so `cargo test` cannot run a line
    // of it. These are uf tests, run by the runner this repository ships.
    "test:lib": {
      command: "./target/release/uf --cwd tests/library test",
      dependsOn: ["build"],
    },
    "docs:build": {
      command: "tools/docs/build.sh",
      dependsOn: ["build"],
    },
    "install:test": "tools/release/test-install.sh",

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
        "test:lib",
        "rust:metadata",
        "manifests",
      ],
    },
  },
});
