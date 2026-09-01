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
`uniflowed.config.flow`; parser, linter, formatter, build, test, package, and
runtime engines should stay native.

Generated app projects should not require Babel, Jest, Yarn, or `.flowconfig`.

Hot paths should prefer `CompactString`, `SmallVec`, arenas, PHF tables, and
borrowed data over `String`, `Vec`, standard hash maps, and cloning. The
repository keeps a Vize-style `clippy.toml` policy so these can be tightened as
crate APIs stabilize.

## Verification

Run the local gate before opening a PR:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo bench --workspace --all-features --no-run
```

When GitHub Actions is configured, use Actions as the final merge gate.
