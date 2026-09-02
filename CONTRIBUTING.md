# Contributing

## Clone

`uf_flow` builds against Meta's official Flow Rust port, which is not published
to crates.io, so the repository carries it as the `upstream/flow` submodule.
Cargo resolves path dependencies even when the feature that uses them is off, so
the submodule must exist before any cargo command:

```sh
git clone https://github.com/ubugeeei-prod/uf
cd uf
tools/upstream/sync.sh
```

`tools/upstream/sync.sh` fetches one shallow, blobless commit and checks out only
`rust_port/`, which costs about 40 MB instead of the full 190 MB repository. It
is idempotent, so re-run it after pulling a submodule bump.

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

Run the local gate before opening a PR:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo bench --workspace --no-run
```

`rust-toolchain.toml` pins `nightly-2026-08-01`, and every command above uses
it. The pin is a requirement, not a preference: `uf` parses and type-checks Flow
with Meta's official Rust port, 23 of whose crates declare
`#![feature(box_patterns)]` — a feature the compiler removed around the
2026-09-01 nightly. That date is the newest nightly that still accepts it.

Run `tools/upstream/sync.sh` before any cargo command; the port lives in the
`upstream/flow` submodule and nothing builds without it.

The `Upstream Flow` CI job builds the parser alone on the floating `nightly`
channel. It is an early warning that the port still compiles there, so the pin
can be advanced deliberately:

```sh
RUSTUP_TOOLCHAIN=nightly cargo test -p uf_flow
```

When GitHub Actions is configured, use Actions as the final merge gate.
