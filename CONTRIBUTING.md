# Contributing

## Getting a checkout working

```sh
git clone https://github.com/ubugeeei-prod/uf
cd uf
nix develop
tools/upstream/sync.sh && cargo build --release --bin uf   # the bootstrap
uf run setup
```

After that one bootstrap line, everything in this repository is a `uf` command.
`uf run` on its own lists them.

The bootstrap cannot itself be a `uf` command, and the reason is structural
rather than an oversight: `upstream/flow` is a *path* dependency, so cargo
cannot build `uf` until the submodule is checked out, and `uf run` needs a
built `uf`. One command breaks that circle.

`uf run upstream:sync` fetches one shallow, blobless commit of Meta's official
Flow Rust port and checks out only `rust_port/`, which costs about 40 MB
instead of the full 190 MB repository. `uf_flow` builds against that port,
which is not published to crates.io. The sync is idempotent, so re-run it after
pulling a submodule bump — `uf run setup` does.

## Commit Style

Use focused conventional commits, for example:

```text
feat: add router manifest generation
test: cover large project discovery
docs: define framework defaults
```

## Implementation Policy

Core engines should be implemented in Rust. Config should be expressed through
`uf.config.js`; parser, linter, formatter, build, test, package, and
runtime engines should stay native.

Generated app projects should not require Babel, Jest, Yarn, npm scripts, or
`.flowconfig`. Project tasks belong in `uf.config.js` and run through `uf run`.
Package management belongs to `uf install`/`uf upgrade` and `@uniflowed/pm`;
runtime inference and acquisition belongs to `@uniflowed/rm`.

Hot paths should prefer `CompactString`, `SmallVec`, arenas, PHF tables, and
borrowed data over `String`, `Vec`, standard hash maps, and cloning. The
repository keeps a Vize-style `clippy.toml` policy so these can be tightened as
crate APIs stabilize.

Fuzzing, formal verification, benchmarks, scripts, and Nix support live under
`tools/` so the root stays focused on the Rust workspace and project metadata.
Use `nix develop ./tools/nix` for the pinned development environment.

## Verification

One command, and it is the one CI runs:

```sh
uf run ci
```

This repository is a uf project, so its pipeline uses the toolchain it ships
rather than a second description of the same checks. A check that is in CI and
not in `uf run ci` is a check a contributor cannot run before pushing, which is
the thing that arrangement exists to prevent.

The individual steps have names too, for when one of them is what you are
working on:

```sh
uf run rust:fmt:check   # cargo fmt --all -- --check
uf run rust:clippy      # cargo clippy --workspace --all-targets -- -D warnings
uf run rust:test        # cargo test --workspace
uf run rust:bench       # cargo bench --workspace --no-run
uf run fmt:check        # uf's own formatter, over this repository's Flow
uf run test:lib         # uf test#library, the @uniflowed/* suite
uf run docs:build       # uf build#docs
uf run docs:dev         # uf dev#docs, to look at the site while editing it
```

`rust-toolchain.toml` pins `nightly-2026-08-01`, and every command above uses
it. The pin is a requirement, not a preference: `uf` parses and type-checks Flow
with Meta's official Rust port, 23 of whose crates declare
`#![feature(box_patterns)]` — a feature the compiler removed around the
2026-09-01 nightly. That date is the newest nightly that still accepts it.

Run `uf run upstream:sync` before any cargo command; the port lives in the
`upstream/flow` submodule and nothing builds without it.

The `Upstream Flow` CI job builds the parser alone on the floating `nightly`
channel. It is an early warning that the port still compiles there, so the pin
can be advanced deliberately:

```sh
RUSTUP_TOOLCHAIN=nightly cargo test -p uf_flow
```

When GitHub Actions is configured, use Actions as the final merge gate.
