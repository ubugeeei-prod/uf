# uf for JetBrains

JetBrains IDEs load `uf lsp` through the
[LSP4IJ](https://plugins.jetbrains.com/plugin/23257-lsp4ij) plugin. There is no
JetBrains-native plugin in this repository; this file is the supported setup for
the generic LSP bridge.

## Setup

1. Install LSP4IJ.
2. Add a user-defined language server named `uf`.
3. Set the command to the project-pinned binary first, falling back to `uf` on
   `PATH` when your IDE configuration cannot express a workspace-relative
   command.

```json
{
  "command": ["./node_modules/.bin/uf", "lsp"],
  "workingDirectory": "$PROJECT_DIR$",
  "fileExtensions": ["js", "jsx", "mjs", "cjs"]
}
```

The working directory matters. `uf lsp` reads `uf.config.js` once when it
starts, so the LSP4IJ server has to start in the project root or pass the same
directory with `--cwd`.

## What works

LSP4IJ receives the same server capabilities as the other editors:
diagnostics, formatting, quick fixes, `source.fixAll.uf`, hover (including the
type under the cursor), go to definition and go to type definition, and
completion — in `uf.config.js` from its schema, elsewhere from Flow's
inference.

What does not work is the same too: rename, references, document symbols and
signature help are not advertised by `uf lsp`.
