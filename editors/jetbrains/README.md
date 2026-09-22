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
diagnostics, formatting, quick fixes, `source.fixAll.uf`, hover, and completion
inside `uf.config.js`.

What does not work is the same too: go-to-definition, rename, references,
document symbols, completion outside `uf.config.js`, and the type at a position
are not advertised by `uf lsp`.
