# uf for VS Code

A language client for `uf lsp`. It starts one server per uf project in the
window, and everything it shows — diagnostics, formatting, quick fixes, hover —
is answered by that server, from the same crates `uf lint`, `uf fmt` and
`uf inspect` call.

Not published to any marketplace. Build it from this directory.

## Install

```sh
cd editors/vscode
npm install                     # vscode-languageclient, the only dependency
npx @vscode/vsce package        # writes uf-0.0.0.vsix
code --install-extension uf-0.0.0.vsix
```

To work on it instead, open `editors/vscode` in VS Code and press F5, which
launches an Extension Development Host with it loaded.

The manifest is `"private": true`, so `npm publish` refuses it. Nothing here is
published to a marketplace and there is no token anywhere in this repository;
`vsce package` writing a local `.vsix` is as far as it goes. That `private` line
is also the one to remove if a future `vsce` starts objecting to it.

## What works

Each line below is asserted against the real server by
`tests/library/lsp.test.js`, which drives `uf lsp` over framed messages the way
an editor does. The extension's own half — binary resolution, the arguments,
the working directory, the settings — is covered by
`tests/library/vscode-extension.test.js`. Both run under `uf test`, so
they are in `uf run ci`.

| | Where it comes from | How you use it |
| --- | --- | --- |
| **Diagnostics** | `uf_lint`, pushed on open and on every change | Squiggles and the Problems panel. Source `uf`, code the rule id. |
| **Formatting** | `uf_fmt`, the same `format_source` `uf fmt` calls | Format Document, or `uf.formatOnSave`. |
| **Quick fixes** | `textDocument/codeAction`, kind `quickfix` | The lightbulb on a diagnostic. |
| **Fix all** | kind `source.fixAll.uf` | `editor.codeActionsOnSave`, or the lightbulb. |
| **Hover** | `textDocument/hover` | The rule behind a diagnostic, what an import specifier names, what a rule id in a suppression comment means. |

Quick fixes are offered only where uf's answer is mechanical.
`flow/deprecated-type` has one — `bool` becomes `boolean`. `flow/unclear-type`
deliberately has none, because "fix" would mean choosing between `mixed`, an
opaque type and a generated router type on the author's behalf.

To fix everything uf can fix when a file is saved:

```jsonc
"editor.codeActionsOnSave": { "source.fixAll.uf": "explicit" }
```

`"source.fixAll": "explicit"` also matches — code action kinds are
hierarchical, and asking for the parent selects uf's child kind.

## What does not work, and will not until the server serves it

`uf lsp` advertises no definition, rename, references, completion or document
symbol provider, so **go to definition, rename, find references and completion
do nothing** for Flow files. This extension does not add them; a language client
cannot invent what the server does not answer.

**Hover does not show the type at a position.** It answers a diagnostic, an
import specifier or a rule id, and nothing for a plain expression — deliberately
nothing rather than an empty popup, which reads as a confident "no type".
`crates/uf_cli/src/commands/dev/hover.rs` says exactly what `uf_check` would
have to expose for that to change.

**Organize imports is not offered.** uf has no import-order opinion to organise
them by, so the server does not advertise `source.organizeImports`.

## Settings

| Setting | Default | |
| --- | --- | --- |
| `uf.server.path` | `""` | The `uf` binary. Absolute or relative to the workspace folder; `${workspaceFolder}` and a leading `~` are expanded. Empty means: search. |
| `uf.formatOnSave` | `false` | Format Flow files with the server's formatter on save. |
| `uf.trace.server` | `"off"` | `messages` or `verbose` logs the JSON-RPC traffic to the uf output channel. |

`uf.formatOnSave` is off because an extension that reformats a file the moment
it is installed is an extension people uninstall. The other route is VS Code's
own: `"editor.formatOnSave": true` with uf chosen as the default formatter for
JavaScript.

## Commands

* **uf: Restart Language Server** — stops every server and starts them again,
  re-resolving the binary. Installing uf, or repointing `uf.server.path` at a
  build of your own, needs this and not a window reload.
* **uf: Show Language Server Log** — the output channel, which lists every path
  that was searched for the binary.

## Finding the binary

In this order, for each project folder:

1. `uf.server.path`, if you set one. A path that is not there is **an error
   naming it**, not a quiet fall-through — otherwise someone debugging a local
   build of uf would silently be running the released one.
2. `<workspace>/node_modules/.bin/uf`. The version a project pinned is the
   version its files were formatted and linted with; a different global one
   would disagree with CI.
3. `PATH`, for a global install.

`PATH` is searched by the extension rather than left to the process spawn, so
"nothing has it" is a notification with a way out rather than a spawn error in a
channel nobody opened. Every path tried goes to the output channel.

## Activation, and one server per project

The extension activates on `workspaceContains:uf.config.js` and starts a server
for each workspace folder that has a `uf.config.js` at its root — that file is
uf's single configuration surface, and `uf_config::CONFIG_FILES` is exactly that
one name. A folder without one gets no server, which is the intended outcome: a
uf server started in a project that never opted into uf would report uf's lint
rules against code that never asked for them.

The server is started **in** the project folder, and that is load-bearing.
`uf lsp` reads `uf.config.js` from its working directory, once, at start-up, and
that read is the only source of the project's formatter width, quote style and
lint levels. `uf lsp --cwd <dir>` is not an alternative: the flag is accepted by
the command line and then ignored by the command.

Editing `uf.config.js` restarts that folder's server, because the server has no
way to be told about the change and a stale one formats to the wrong width
without saying so.

A nested uf project — a `uf.config.js` below the folder you opened — is not
picked up. Open that folder, or add it to the workspace.

## Language

JavaScript with Flow types, like the rest of this repository, written in Flow's
comment syntax: `/*:: type T = … */` and `/*: T */`.

The extension host loads CommonJS from disk with no transform, so a file with
`function f(x: string)` in it would be a syntax error at activation, and real
annotations would need a build step between the source and the file VS Code
loads — a place for the two to differ. Comment types are the same language,
parsed and checked by the same Flow parser (`uf lint` reports a syntax error
inside a `/*:: … */` block, which is how you can tell they are not just
comments), formatted by `uf fmt`, and run by Node unmodified. So the bytes VS
Code loads are the bytes `tests/library/vscode-extension.test.js` loads.

TypeScript was not used and is not needed here.

## Layout

```
src/binary.js      finding `uf`: the setting, node_modules/.bin, PATH
src/project.js     what counts as a uf project, and which files it claims
src/client.js      the process to start, and reading the settings
src/extension.js   the glue: VS Code and vscode-languageclient
```

The first three are pure and have no `require("vscode")` in them, which is what
lets them be tested without an editor host.

## What no test here covers

An extension host cannot be started in this repository's CI, so these need a
person with VS Code open:

* that VS Code registers the providers from the server's capabilities,
* that `editor.codeActionsOnSave` reaches `source.fixAll.uf`,
* that the missing-binary notification and its buttons appear,
* that `uf.formatOnSave` runs on save,
* that the `uf.config.js` watcher restarts the server.

The pieces underneath each of them are tested; the wiring in VS Code is not.
