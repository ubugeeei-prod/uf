# Editors

Editor integrations live here and talk to the native `uf lsp` server.

## Targets

- VS Code: `editors/vscode`
- Neovim: `editors/neovim`
- Emacs: `editors/emacs`
- Vim: `editors/vim`
- Helix: `editors/helix`
- Zed: `editors/zed`
- Cursor: `editors/cursor`

Each integration should keep editor-specific glue small. Parsing, linting,
formatting, route/type generation, and diagnostics stay in Rust crates.
