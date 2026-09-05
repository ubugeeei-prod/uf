# Editors

Editor integrations live here and talk to the native `uf lsp` server.

Editor-specific glue stays small. Parsing, linting, formatting, route/type
generation and diagnostics stay in the Rust crates: every integration below is
a client, and none of them implements a language feature.

## Targets

| Editor | What is here | Working |
| --- | --- | --- |
| [VS Code](vscode) | An extension: `package.json`, four source files, a `.vscodeignore` | Yes, built from source. Not published. |
| [Cursor](cursor) | Nothing of its own — it installs the VS Code extension | Yes |
| [Neovim](neovim) | [`lua/uf.lua`](neovim/lua/uf.lua), using core `vim.lsp.start` | Yes |
| [Vim](vim) | [`uf.vim`](vim/uf.vim), a vim-lsp registration | Yes |
| [Helix](helix) | [`languages.toml`](helix/languages.toml) | Yes |
| [Emacs](emacs) | [`uf.el`](emacs/uf.el), for Eglot or lsp-mode | Yes |
| [Zed](zed) | [`extension.toml`](zed/extension.toml), the manifest only | **No** — Zed also needs a Rust/WASM half that cannot be built here. Its README says exactly what is missing and what the code has to be. |

Only VS Code is a package. The rest are configuration: a file to copy, four to
eighty lines long.

## What every one of them gets

`uf lsp` serves exactly this, and no integration claims more:

* **Diagnostics** — `textDocument/publishDiagnostics`, from `uf_lint`, pushed
  on open and on every change.
* **Formatting** — `textDocument/formatting`, from `uf_fmt`. One edit over the
  whole document, because the printer reprints from the syntax tree.
* **Code actions** — `quickfix` for the diagnostics with a mechanical answer,
  and `source.fixAll.uf` for all of them at once.
* **Hover** — the rule behind a diagnostic, what an import specifier names, and
  what a rule id in a suppression comment means.

Not go-to-definition, rename, references, completion, document symbols, the
type at a position, or organize imports. The server advertises none of them;
`tests/library/lsp.test.js` asserts that it does not, so a README here cannot
quietly start over-claiming.

## The one thing every integration has to get right

**Start the server in the project directory.**

`uf lsp` calls `load_config(".")` once, at start-up. Its working directory is
the only channel it has for a project's `fmt` options and lint levels, and
`uf lsp --cwd <dir>` does not work — the flag is a global option so the command
line accepts it, and then the command ignores it.

Getting this wrong fails quietly: the editor formats to uf's defaults instead of
to the project's, and reports the default lint levels rather than the configured
ones. The check is to format a file in the editor and then run `uf fmt --check`
in a terminal; if they disagree, the server was started somewhere else.

Each README says how its editor is told, and where its editor makes that hard.

## Editing `uf.config.js`

The server reads it once. Every integration's README says how to restart it;
the VS Code extension watches the file and restarts by itself.
