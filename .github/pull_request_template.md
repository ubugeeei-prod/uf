## Summary

## Verification

From a clean checkout, materialize the upstream Flow submodule first —
Cargo resolves the `flow_parser` path dependency during metadata loading:

```sh
tools/upstream/sync.sh
```

Pinned release toolchain (1.98.0):

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cargo bench --workspace --no-run`

Nightly, for the `upstream/flow` Rust port behind `--all-features`:

- [ ] `cargo +nightly clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo +nightly test --workspace --all-features`
