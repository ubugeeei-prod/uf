# CI

Normal PRs target two-minute feedback. They run unit tests, formatting, lint,
metadata, library tests and docs checks. Full integration tests and platform
checks run once for release PRs in the merge queue, before merge.

A PR also runs the release-only suites it can break (#1434), chosen by
`tools/ci/change-scope.cjs`:

- `rust_tests`: a PR touching `crates/<name>/` runs that crate's integration
  tests and `uf_cli`'s (command-wide invariants such as `uf explain` and
  completion) through `tools/ci/rust-integration.sh`, with `--no-fail-fast`.
  `Cargo.toml`, `Cargo.lock` or `rust-toolchain.toml` runs every crate's.
  `uf_cli`'s `vite.rs`, six minutes on its own, runs only when the deploy
  matrix's scope does.
- `deno`: a PR touching `npm/`, `tests/library/`, `tools/ci/deno-library*`,
  any crate, or the lockfiles runs `Library (Deno 2.9.7)` for real.

- CI builds `uf` once. `UF_CI_PREBUILT=1` makes dependent tasks require that
  artifact instead of building it again. Local development still builds it.
- Docs and brand edits keep site checks without unrelated Rust checks.
  Unknown paths still run code checks. Missing history runs the full suite.
- Sticky disks retain Cargo downloads and workspace outputs. Only `main`
  saves snapshots. Compiling jobs use separate target disks. Fresh disks can
  restore the previous Actions cache without pruning workspace crates.
- Cargo checks contents instead of checkout timestamps on the pinned nightly.
- The Rust suite runs once with all features. Another job compiles disabled
  default features. Benchmark compile checks use `ci`; release archives use
  `dist`.
- Hydration checks cover every page at both widths in eight browser tabs.
- Docs deployment reuses the site artifact from its build job.

The `CI` gate requires successful jobs. Only the release artifact job may be
skipped, and only when the scope job confirms this is not a release candidate.
A failed prerequisite still fails the gate.

Run the quick regression checks with:

```sh
node --test tools/ci/test-change-scope.cjs tools/release/test-policy.cjs
```

Cold Rust builds and release validation can exceed two minutes. Compare
Actions timings after caches are warm before reporting the target as met.
