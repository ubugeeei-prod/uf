# CI

Normal PRs target two-minute feedback. They run unit tests, formatting, lint,
metadata, library tests and docs checks. Full integration tests and platform
checks run for release PRs and their merge-queue commits.

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
