# uf linker policy

Linux release and CI builds route through `tools/linker/uf-linker`.

The wrapper prefers Wild (`wild-linker`) when available, then falls back to mold,
LLVM lld, and finally the system C compiler. This keeps the fast-linker path
available without making local development or Blacksmith runners depend on one
specific linker binary being preinstalled.
