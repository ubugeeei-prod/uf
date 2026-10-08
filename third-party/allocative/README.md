# allocative 0.3.6

A copy of the crates.io package, patched so it builds on Rust 1.100 and later.

`Infallible` is an alias of `!` as of that release. allocative 0.3.6 implements
`Allocative` for both, and the `!` impl is compiled on nightly. Those are one
impl twice, so the crate does not build. The upstream repository was archived
before the never type stabilized, and 0.3.6 is the last release. The `!` impl
is removed here. `starlark_map` 0.14.2, which the Flow port uses, depends on
this crate.
