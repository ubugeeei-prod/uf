# CI

Normal PRs should get feedback within two minutes. Cold Rust builds and the
full browser integration suite can take longer; check Actions timings before
claiming that target is met.

- CI builds `uf` once and shares it with the other jobs. `UF_CI_PREBUILT=1`
  makes the local `build` task require that artifact instead of rebuilding it.
- Docs and brand edits run formatting, lint, metadata and site checks. They
  do not rerun the Rust workspace or unrelated platform tests. Source changes,
  unknown paths and missing Git history run the full suite.
- The Rust suite runs once with all features. A separate compile check covers
  disabled default features. Benchmarks are compiled with the `ci` profile;
  release packaging still uses `dist`.
- Sticky disks keep Cargo downloads and workspace build outputs. Only `main`
  saves snapshots. Each compiling job has its own target disk. Fresh disks
  can restore the previous Actions cache, without pruning workspace crates.
- Cargo checks source contents instead of checkout timestamps on the pinned
  nightly. It still decides which crates need rebuilding.
- Docs deployment downloads the site that passed the build job.

Run `node --test tools/ci/change-scope.test.cjs` after changing the scope or
prebuilt-binary rules. The `CI` gate still requires every job to succeed:
conditional work lives in steps, so a failed prerequisite cannot become an
accepted skipped job.
