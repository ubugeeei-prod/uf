# uf for Neovim

`uf lsp`, over stdio, for the Flow files of a uf project. Configuration rather
than a plugin: [`lua/uf.lua`](lua/uf.lua) is about eighty lines and uses core
`vim.lsp.start`, so it needs no plugin manager and no `nvim-lspconfig`.

## In place

Copy `lua/uf.lua` into your configuration (`~/.config/nvim/lua/uf.lua`) and
call it:

```lua
require("uf").setup()
```

With options:

```lua
require("uf").setup({
  -- A binary that is not on PATH: this project's own copy, say.
  cmd = { "./node_modules/.bin/uf", "lsp" },
  format_on_save = true,
})
```

A buffer gets a client when its file is `javascript` or `javascriptreact` **and**
a `uf.config.js` is found in it or above it. Nothing starts in a project that is
not a uf project.

If you already drive your servers with `nvim-lspconfig`, the equivalent is:

```lua
vim.lsp.config("uf", {
  cmd = { "uf", "lsp" },
  filetypes = { "javascript", "javascriptreact" },
  root_markers = { "uf.config.js" },
})
vim.lsp.enable("uf")
```

Note that this form does not set `cmd_cwd`, so start Neovim in the project
root — see below.

## What you get

Everything here is answered by `uf lsp`, from the same crates `uf lint`,
`uf fmt` and `uf inspect` use. `tests/library/lsp.test.js` drives the server and
asserts each of them.

| | How |
| --- | --- |
| Diagnostics | Pushed on open and on every change. `:lua vim.diagnostic.open_float()` shows the message; the source is `uf` and the code is the rule id. |
| Formatting | `:lua vim.lsp.buf.format({ name = "uf" })`, or `format_on_save = true`. The same `uf_fmt` that `uf fmt` calls. |
| Quick fixes | `:lua vim.lsp.buf.code_action()` on a diagnostic. Offered only where uf's answer is mechanical — `flow/deprecated-type` has one; a rule that would have to guess at intent deliberately does not. |
| Fix all | `:lua vim.lsp.buf.code_action({ context = { only = { "source.fixAll" } }, apply = true })` |
| Hover | `K`. The rule behind a diagnostic, what an import specifier names, what a rule id in a suppression comment means, or what a key of `uf.config.js` is for; anywhere else in a Flow file, the type under the cursor as Flow infers it. |
| Definition | `<C-]>`, since Neovim points `tagfunc` at the server, or `:lua vim.lsp.buf.definition()`. Across files, into `node_modules`, and into `flow-typed/`. |
| Type definition | `:lua vim.lsp.buf.type_definition()`: the declaration of the named types in the type under the cursor. |
| Completion | `<C-x><C-o>` in insert mode, because Neovim sets `omnifunc` for a buffer whose server can complete, or `vim.lsp.completion.enable(true, client_id, bufnr, { autotrigger = true })` to have it as you type. In `uf.config.js`: the keys valid at the cursor, with their documentation and type, after `"` the values of a key whose type is a fixed set, and in a tool spec the names and, after `@`, the versions. In any other Flow file: after `.`, the members of the value's type with their types, and elsewhere the names in scope. |

## What you do not get

No rename, no references, no document symbols and no signature help — `uf lsp`
does not advertise any of them, so `vim.lsp.buf.rename()` will tell you the
server has no handler. Type errors are not pushed as diagnostics; `uf check`
reports them. Hover and definitions answer nothing while the file does not
parse, since there is no inference to ask.

## Working directory

`uf lsp` reads `uf.config.js` once, at start-up: from the directory `--cwd`
names, or else from the directory it was started in. That read is the only
channel it has for your `fmt` options and lint levels — there is no request
that can tell it otherwise.

`lua/uf.lua` handles this by passing `cmd_cwd = root`. If you write your own
configuration, do the same, pass `cmd = { "uf", "lsp", "--cwd", root }`, or
start Neovim from the project root. The symptom
of getting it wrong is quiet: formatting to uf's defaults rather than to yours.

## Editing `uf.config.js`

The server reads it once. After changing it, restart the client:

```lua
vim.cmd("LspRestart uf")
```
