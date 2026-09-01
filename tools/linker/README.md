# uf linker policy

Linux release and CI builds route through `tools/linker/uf-cc-linker`. That
wrapper calls the platform `cc` driver with `-B<absolute tools/linker path>/`,
which makes the driver pick `tools/linker/ld`.

`tools/linker/ld` delegates to `tools/linker/uf-linker`. The wrapper prefers the
`wild` binary from `wild-linker` when available, then falls back to mold, LLVM
lld, GNU gold, and finally the system linker. This keeps the fast-linker path
available without making local development depend on one specific linker binary
being preinstalled.

Keeping `cc` as the driver matters: it supplies the target runtime library
search paths, startup objects, and dynamic linker flags that direct `ld`
invocations would need to reconstruct by hand.
