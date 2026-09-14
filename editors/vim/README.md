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
| Hover | `:LspHover`, including over a key of `uf.config.js`. |
| Completion | In `uf.config.js`: `<C-x><C-o>` with `setlocal omnifunc=lsp#complete`, or asyncomplete.vim and asyncomplete-lsp.vim to have it as you type. The keys valid at the cursor, with their documentation and type, the values of a key whose type is a fixed set, and in a tool spec the names and, after `@`, the versions. |

Format on save is off; the one-line autocommand is at the bottom of `uf.vim`.

## What you do not get

`:LspDefinition`, `:LspRename` and `:LspReferences` have no server behind them —
`uf lsp` advertises none of those providers — and completion answers in
`uf.config.js` only. `:LspHover` over a plain expression answers nothing,
because uf has no positional type query yet.

## Working directory

`uf lsp` reads `uf.config.js` once, at start-up, and that read is the only
source of your `fmt` options and lint levels. vim-lsp starts every server in
Vim's own working directory and has no per-server override, so `uf.vim` names
the project on the command line instead: it runs `uf lsp --cwd <root>`, with
the nearest directory above the file that has a `uf.config.js` in it. Vim can
be started anywhere, and no shell is involved, so this works on Windows too.

`g:uf_cd_to_root`, which older copies of `uf.vim` read to wrap the command in
`/bin/sh -c 'cd <root> && …'`, is no longer read and can be removed.

The symptom of the server reading the wrong configuration is quiet:
`:LspDocumentFormat` formatting to uf's defaults instead of yours. The check is
to format in Vim and then run `uf fmt --check` in a terminal.

## Editing `uf.config.js`

The server reads it once, at start-up. `:LspStopServer uf` and reopen the file
afterwards.
