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
| Hover | `K`. The rule behind a diagnostic, what an import specifier names, or what a rule id in a suppression comment means. |

## What you do not get

No go-to-definition, no rename, no completion, no references, no document
symbols, and **no type on hover** — `uf lsp` does not advertise any of them, so
`vim.lsp.buf.definition()` will tell you the server has no handler. Hover over a
plain expression answers nothing rather than an empty popup; the reason is in
`crates/uf_cli/src/commands/dev/hover.rs`.

## Working directory

`uf lsp` reads `uf.config.js` from the directory it was started in, once. That
read is the only channel it has for your `fmt` options and lint levels — there
is no request that can tell it otherwise, and `uf lsp --cwd` does not work (the
flag is accepted and ignored).

`lua/uf.lua` handles this by passing `cmd_cwd = root`. If you write your own
configuration, do the same, or start Neovim from the project root. The symptom
of getting it wrong is quiet: formatting to uf's defaults rather than to yours.

## Editing `uf.config.js`

The server reads it once. After changing it, restart the client:

```lua
vim.cmd("LspRestart uf")
```
