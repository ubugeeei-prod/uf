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
  -- Clients kept off buffers in a uf project. The default is
  -- ts_ls, tsserver, vtsls, denols and typescript-tools; {} keeps them.
  exclude = { "ts_ls", "vtsls" },
})
```

`:UfRestart` restarts the server, which is how an edit to `uf.config.js` takes
effect.

## TypeScript's server, in a uf project only

Most Neovim configurations also start `ts_ls` or `vtsls` for `javascript`
buffers, and it reads a Flow file as TypeScript: `component`, `hook`, `match`
and every annotation come back as errors beside uf's. `setup` detaches the
clients named in `exclude` from a buffer **in a uf project**, and drops what
they publish about files there, whatever started them — nvim-lspconfig,
`vim.lsp.enable`, a plugin. In any other project they attach as before.

On Neovim 0.11 and later you can instead keep `ts_ls` from starting in a uf
project at all, with the `root_dir` contract: a `root_dir` function that never
calls `on_dir` does not start the client for that buffer.

```lua
vim.lsp.config("ts_ls", {
  root_dir = function(bufnr, on_dir)
    if vim.fs.root(bufnr, { "uf.config.js" }) then
      return -- a uf project: uf serves it
    end
    on_dir(vim.fs.root(bufnr, { "tsconfig.json", "jsconfig.json", "package.json", ".git" }))
  end,
})
```

This replaces nvim-lspconfig's own `root_dir` for `ts_ls` (which also skips Deno
projects), so the markers above are yours to keep in step.

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
root — see below — or add `"--cwd"` and the root to `cmd` in a function.

## What you get

Everything here is answered by `uf lsp`, from the same crates `uf lint`,
`uf fmt` and `uf inspect` use. `tests/library/lsp.test.js` drives the server and
asserts each of them.

| | How |
| --- | --- |
| Diagnostics | Pushed on open and on every change. `:lua vim.diagnostic.open_float()` shows the message; the source is `uf` and the code is the rule id. Flow's type errors join them once typing pauses: source `flow`, the code Flow's own (`incompatible-type`), and every location the message refers to as related information. |
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
server has no handler. Hover and definitions answer nothing while the file does not
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

The server reads it once. After changing it, `:UfRestart` (or `:LspRestart uf`
with nvim-lspconfig).

## What is tested

`test/run.lua` runs in headless Neovim in the Editors workflow, with
`editors/test/fake-lsp.cjs` standing in for both `uf lsp` and `ts_ls`: uf
attaches in a uf project and is started in its root, `ts_ls` is detached there
and its diagnostics dropped, neither changes outside a uf project, and
`:UfRestart` starts a new client. It has been run on Neovim 0.8, 0.11 and 0.12.
