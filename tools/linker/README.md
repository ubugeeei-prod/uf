# uf linker policy

Linux release and CI builds route through `tools/linker/uf-linker` with Rust's
stable `ld` linker flavor.

The wrapper prefers the `wild` binary from `wild-linker` when available, then
falls back to mold, LLVM lld, GNU gold, and finally the system linker. This
keeps the fast-linker path available without making local development depend on
one specific linker binary being preinstalled.

Because Rust's direct `ld` linker flavor bypasses the platform C compiler
driver, the wrapper asks `CC`, `cc`, `gcc`, or `clang` for the runtime library
directory and prepends that directory as `-L`. This keeps Linux CI builds able
to resolve compiler runtime libraries such as `libgcc_s` while still linking
through wild first.
