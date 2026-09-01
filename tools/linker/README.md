# uf linker policy

Linux release and CI builds route through `tools/linker/uf-linker` with Rust's
GNU direct linker flavor.

The wrapper prefers the `wild` binary from `wild-linker` when available, then
falls back to mold, LLVM lld, GNU gold, and finally the system linker. This
keeps the fast-linker path available without making local development depend on
one specific linker binary being preinstalled.
