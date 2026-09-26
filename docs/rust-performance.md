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
