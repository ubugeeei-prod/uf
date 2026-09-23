# uf for Helix

Helix speaks LSP without a plugin, so the whole integration is
[`languages.toml`](languages.toml).

## In place

Per project, which is where it belongs — uf is a per-project toolchain:

```sh
mkdir -p .helix && cp path/to/uf/editors/helix/languages.toml .helix/languages.toml
```

Or globally, merged into `~/.config/helix/languages.toml`.

Then, from the project root:

```sh
hx src/app.js
```

`:lsp-workspace-command` and `:log-open` tell you whether the client came up.
`hx --health javascript` lists the language servers Helix will use for the
language.

## What you get

Everything is answered by `uf lsp`. `tests/library/lsp.test.js` drives the
server and asserts each of these.

| | How |
| --- | --- |
| Diagnostics | Pushed on open and on every change; shown in the gutter, `Space + d` for the picker. The source is `uf` and the code is the rule id. |
| Formatting | `:format`, and on write, because `auto-format = true`. |
| Quick fixes | `Space + a` on a diagnostic. |
| Fix all | `Space + a` offers "Fix all uf lint problems in this file" as a separate action; Helix has no dedicated fix-all binding. |
| Hover | `Space + k`, including over a key of `uf.config.js`; over anything else in a Flow file, the type as Flow infers it. |
| Definition | `gd`, across files, into `node_modules` and into `flow-typed/`. |
| Type definition | `gy`: the declaration of the named types in the type under the cursor. |
| Completion | As you type, after `"` and after `.`; `Ctrl + x` in insert mode asks for it. In `uf.config.js`, the keys valid at the cursor, with their documentation and type, the values of a key whose type is a fixed set, and in a tool spec the names and, after `@`, the versions. In any other Flow file, after `.` the members of the value's type with their types, and elsewhere the names in scope. |

## What you do not get

`gr`, rename and signature help have no server behind them — `uf lsp`
advertises none of them, and Helix will say the language server does not
support the request. Hover and `gd` answer nothing while the file does not
parse, since there is no inference to ask.

The `language-servers = ["uf"]` lines **replace** Helix's defaults for
JavaScript and JSX. If you want `typescript-language-server` alongside uf,
list both — and expect them to disagree about formatting and types, since only
one of them reads your `uf.config.js` and only one of them is Flow.

## Working directory

`uf lsp` reads `uf.config.js` once, at start-up — from the directory `--cwd`
names, or else from the directory it is started in — and that read is the only
source of your `fmt` options and lint levels. This configuration relies on the
second: its `args` are a fixed `["lsp"]`, with no project root in them.

`roots = ["uf.config.js"]` is what tells Helix where the workspace is. Start
`hx` from the project root. The way to check is to format a file in Helix and
run `uf fmt --check` in a terminal: if they disagree, the server is reading a
different configuration than you are.

## Editing `uf.config.js`

The server reads it once, at start-up. `:lsp-restart` afterwards.
