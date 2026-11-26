# Rust performance

## String construction

Clippy rejects `std::format!` in production code. Tests and benchmark baselines
may use it. Plain concatenation should use `push_str` and `push`; formatted
output belongs in an existing buffer through `uf_infra::append!`. Owned short
strings use `uf_infra::cstr!`, which returns a `CompactString` directly. Its shared writer avoids duplicating
construction code and passes formatting arguments directly to the destination.

A borrowed argument should borrow that compact result. `uf_infra::into_string()` is an
explicit compatibility boundary for an API that still requires `String`; it
still allocates for inline results and shares the conversion code between
call sites. It is not a performance improvement by itself. Do not introduce it into a repeated append loop.

The dependency-free profiler writes its cold reports directly into their
output buffers. Its dependency graph must stay empty.

## Containers and scans

| Primitive | Current use |
| --- | --- |
| mimalloc | The three native executable allocators |
| bumpalo | Formatter document and indentation arenas |
| SmallVec | Comment lists, RSC lists, short line indexes, formatter fit stacks, import edge scratch buffers |
| CompactString | Short owned paths, names, and diagnostic fragments |
| memchr | Line boundaries and exact capacity planning for long line indexes |
| phf | Static keyword and rule tables |
| FxHash / hashbrown | Borrowed workspace-name lookup, with no cloned dependency keys |
| fixedbitset | Deduplicated dependency indices in workspace order |
| string-interner | One pooled copy of each export spelling per lint batch; module export sets hold 32-bit symbols |
| thin-vec | Import graph edge lists use one-word headers, including empty lists |
| index_vec | Typed 32-bit module IDs and module-indexed vectors in the import graph |
| nonmax | The optional first-code-line index occupies one word |
| simdutf8 | Shared UTF-8 validation |
| fast-float2 | SVG viewBox numbers, consumed by an iterator without collecting a vector |

An arena is for values that die together. Borrowed names need no interner;
workspace name lookup therefore borrows names, while the lint graph interns
names that must outlive each individual file scan.

`ecow`, `roaring`, `bstr`, and `aho-corasick` remain candidates
for cloned immutable strings, sparse integer sets, byte
text processing, and multi-pattern searches respectively. Add them at a
measured use site. Existing borrowed strings, dense bitsets, and one-pattern
`memchr` searches should not gain an extra allocation or indirection merely
to change their container.

## Verification

`crates/uf_infra/tests/allocation_budget.rs` enforces zero allocations for
short normalized paths, short line indexes, compact messages, and repeated
writes into a reserved output buffer. It also covers Unicode and long-input
spills. Short paths are normalized in place in compact storage; long paths
transfer the standard replacement buffer into `CompactString`. Long line
indexes allocate once.

The `primitives` benchmark compares the old and current implementations in one
process. CI also runs the same import graph fixture on the PR base and head,
counts allocations and bytes, records eleven-run median timing, and compares
stripped `ci-opt` binaries built with the same toolchain and profile. Those
binary numbers describe `ci-opt`, not the release profile with LTO. Results are
uploaded as `primitives-performance` by the Bench Compile job. Timings apply
to the recorded fixtures and are not a claim about every uf workload.

## Build speed

Two accelerators cut compile time without changing the compiler or what it
produces. Both work through `tools/ci/rustc-wrapper.sh`, Cargo's
`RUSTC_WRAPPER`. A wrapper adds no flag that Cargo's fingerprints can see, so
existing target directories and CI's sticky target disks stay warm.

- **Parallel frontend.** The pinned nightly accepts
  `-Zunstable-options --jobs-frontend=N`. The wrapper adds it only to a crate
  compiled from this checkout, and only when `--crate-name` matches
  `CARGO_CRATE_NAME`. That covers the workspace and the Flow port under
  `upstream/`, the crates an edit recompiles. Registry dependencies already
  compile side by side, so they reach rustc unchanged, and so do build-script
  probes and Cargo's `-vV` queries. `RUSTFLAGS` would have been simpler, but
  Cargo hashes it into every crate's metadata, so any change to it rebuilds
  every target directory from scratch. The dev shell and CI use 8 threads.
  In CI, `.github/actions/rust-cache` takes `frontend-jobs`; it defaults to
  `1` (off). Toolchain, Clippy, Test and Upstream typecheck set it to `8`.
  Bench compile measures compile output, and Upstream Flow is the canary for
  the moving nightly, so both keep the default.
- **kache, in the dev shell only.** `nix develop` provides
  [kache](https://github.com/kunobi-ninja/kache) 2.3.1. The release binaries
  are pinned in `flake.nix` to the release's own `.sha256` files. The wrapper
  hands every call to kache, and every worktree and target directory on the
  machine shares one content-addressed store. CI does not use kache: it
  rewrites the paths it hashes in debug information, so its output is not
  byte-identical to an uncached build, and release and measurement builds must
  not depend on it.

`UF_RUSTC_FRONTEND_JOBS=1` and `UF_KACHE=` (empty) turn either off when
entering the shell. `tools/ci/test-rustc-wrapper.sh` (`uf run
rust:wrapper:test`) holds which calls the wrapper changes and which jobs opt
in.

Local measurement on macOS arm64, `cargo check -p uf_check` from an empty
target. The machine was shared at a load average of 28 to 41, so these
numbers are directional only:

| Build | Wall |
| --- | --: |
| frontend off (`j1`) | 162 s |
| frontend `j8` | 100 s |
| `j8` + kache, empty store | 120 s |
| `j8` + kache, a new target directory against that store | 12 s |
