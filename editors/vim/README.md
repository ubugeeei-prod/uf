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
| Diagnostics | Pushed on open and on every change. Signs in the gutter; `:LspDocumentDiagnostics`. The source is `uf` and the code is the rule id. Flow's type errors join them once typing pauses: source `flow`, the code Flow's own (`incompatible-type`), and every location the message refers to as related information. |
| Formatting | `:LspDocumentFormat`. The same `uf_fmt` that `uf fmt` calls. |
| Quick fixes | `:LspCodeAction` on a diagnostic. |
| Fix all | `:LspCodeAction`, then pick "Fix all uf lint problems in this file". |
| Hover | `:LspHover`, including over a key of `uf.config.js`; over anything else in a Flow file, the type as Flow infers it. |
| Definition | `:LspDefinition`: across files, into `node_modules` and into `flow-typed/`. |
| Type definition | `:LspTypeDefinition`: the declaration of the named types in the type under the cursor. |
| Completion | `<C-x><C-o>` with `setlocal omnifunc=lsp#complete`, or asyncomplete.vim and asyncomplete-lsp.vim to have it as you type. In `uf.config.js`, the keys valid at the cursor, with their documentation and type, the values of a key whose type is a fixed set, and in a tool spec the names and, after `@`, the versions. In any other Flow file, after `.` the members of the value's type with their types, and elsewhere the names in scope. |

Format on save is off; the one-line autocommand is at the bottom of `uf.vim`.

## TypeScript's server

Vim has no JavaScript language server of its own. If your vim-lsp setup also
registers `typescript-language-server` for `javascript` (vim-lsp-settings
does), it reads a Flow file as TypeScript and reports `component`, `hook`,
`match` and every annotation as errors beside uf's. vim-lsp chooses servers
by file type, not by project, so `uf.vim` cannot turn it off for uf projects
only; with vim-lsp-settings, disable it for a uf project in that project's
local vimrc (`let g:lsp_settings = {'typescript-language-server': {'disabled': 1}}`).
That is vim-lsp-settings' documented option, and not checked here.

## What you do not get

`:LspRename` and `:LspReferences` have no server behind them — `uf lsp`
advertises neither provider. `:LspHover` and `:LspDefinition` answer nothing
while the file does not parse, since there is no inference to ask.

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
