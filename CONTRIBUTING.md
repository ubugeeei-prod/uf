# Contributing

## Commit Style

Use focused conventional commits, for example:

```text
feat: add router manifest generation
test: cover large project discovery
docs: define framework defaults
```

## Implementation Policy

Core engines should be implemented in Rust. Config should be expressed through
`uf.config.flow`; parser, linter, formatter, build, test, package, and
runtime engines should stay native.

Generated app projects should not require Babel, Jest, Yarn, npm scripts, or
`.flowconfig`. Project tasks belong in `uf.config.flow` and run through `uf run`.
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
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo bench --workspace --all-features --no-run
```

When GitHub Actions is configured, use Actions as the final merge gate.
