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
| Hover | `Space + k`. |

## What you do not get

`gd`, `gr`, rename and completion have no server behind them — `uf lsp`
advertises none of them, and Helix will say the language server does not
support the request. Hover over a plain expression answers nothing: uf has no
positional type query yet.

The `language-servers = ["uf"]` lines **replace** Helix's defaults for
JavaScript and JSX. If you want `typescript-language-server` alongside uf for
completion, list both — and expect them to disagree about formatting, since
only one of them reads your `uf.config.js`.

## Working directory

`uf lsp` reads `uf.config.js` from the directory it is started in, once, and
that read is the only source of your `fmt` options and lint levels. `--cwd` is
accepted by the command and ignored, so it is not a way out.

`roots = ["uf.config.js"]` is what tells Helix where the workspace is. Start
`hx` from the project root. The way to check is to format a file in Helix and
run `uf fmt --check` in a terminal: if they disagree, the server is reading a
different configuration than you are.

## Editing `uf.config.js`

The server reads it once, at start-up. `:lsp-restart` afterwards.
