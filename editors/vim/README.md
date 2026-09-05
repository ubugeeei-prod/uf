# uf for Vim

Vim has no LSP client of its own, so this is a registration for
[vim-lsp](https://github.com/prabirshrestha/vim-lsp). Neovim users want
[`editors/neovim`](../neovim) instead, which needs no plugin at all.

[`uf.vim`](uf.vim) is the whole integration.

## In place

```vim
Plug 'prabirshrestha/vim-lsp'
" after vim-lsp is on the runtime path
source /path/to/uf/editors/vim/uf.vim
```

Or copy it into `~/.vim/plugin/uf.vim`.

Options, before the `source` line:

```vim
let g:uf_executable = './node_modules/.bin/uf'
let g:uf_cd_to_root = 1
```

`:LspStatus` says whether the server came up, and `:LspDocumentDiagnostics`
lists what it found.

## What you get

Everything is answered by `uf lsp`. `tests/library/lsp.test.js` drives the
server and asserts each of these.

| | How |
| --- | --- |
| Diagnostics | Pushed on open and on every change. Signs in the gutter; `:LspDocumentDiagnostics`. The source is `uf` and the code is the rule id. |
| Formatting | `:LspDocumentFormat`. The same `uf_fmt` that `uf fmt` calls. |
| Quick fixes | `:LspCodeAction` on a diagnostic. |
| Fix all | `:LspCodeAction`, then pick "Fix all uf lint problems in this file". |
| Hover | `:LspHover` |

Format on save is off; the one-line autocommand is at the bottom of `uf.vim`.

## What you do not get

`:LspDefinition`, `:LspRename`, `:LspReferences` and completion have no server
behind them — `uf lsp` advertises none of those providers. `:LspHover` over a
plain expression answers nothing, because uf has no positional type query yet.

## Working directory

This is the rough edge of the Vim integration, and it is not papered over.

`uf lsp` reads `uf.config.js` from the directory it was started in, once, and
that read is the only source of your `fmt` options and lint levels. vim-lsp
starts a server in Vim's working directory and has no per-server override, and
`uf lsp --cwd` is accepted by the command and then ignored.

So one of:

* start Vim from the project root (`cd project && vim src/app.js`), or
* `:cd` to the project root before opening a Flow file, or
* set `let g:uf_cd_to_root = 1`, which wraps the command in
  `/bin/sh -c 'cd <root> && exec uf lsp'`. Needs a POSIX shell, so not Windows.

The symptom of getting it wrong is quiet: `:LspDocumentFormat` formatting to
uf's defaults instead of yours. The check is to format in Vim and then run
`uf fmt --check` in a terminal.

## Editing `uf.config.js`

The server reads it once, at start-up. `:LspStopServer uf` and reopen the file
afterwards.
