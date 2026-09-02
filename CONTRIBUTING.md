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

The pinned 1.98.0 release toolchain cannot compile the upstream Flow Rust port
yet, because the port still uses the unstable `!` type. `--all-features` therefore
runs on nightly, which is also what the `Upstream Flow` CI job does:

```sh
cargo +nightly clippy --workspace --all-targets --all-features -- -D warnings
cargo +nightly test --workspace --all-features
```

Once `never_type` reaches stable (Rust 1.100), `uf_flow`'s `upstream-parser`
feature becomes the default and the stable jobs go back to `--all-features`.

When GitHub Actions is configured, use Actions as the final merge gate.
