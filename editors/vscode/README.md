# uf for VS Code

A language client for `uf lsp`. It starts one server per uf project in the
window, and everything it shows — diagnostics, formatting, quick fixes, hover,
go to definition, completion — is answered by that server, from the same crates
`uf lint`, `uf fmt`, `uf inspect` and `uf check` call.

## Install

The extension is `uniflowed.uf`. Each uf release attaches it to its GitHub
release as `uf-vscode-<version>.vsix`, with a `.sha256` beside it, and `uf`
installs the one that matches it:

```sh
uf editor install vscode        # or `cursor`
```

That downloads the `.vsix` from the release of the `uf` you run, refuses it if
its digest does not match, and hands it to `code --install-extension` (or
`cursor`). `--version 0.9.0` takes another release's, and `--vsix FILE`
installs one you have. By hand, download both files from the
[release](https://github.com/ubugeeei-prod/uf/releases), check the digest with
`sha256sum -c`, and run `code --install-extension uf-vscode-<version>.vsix`.

**Not in the registries yet.** The release job also publishes to the
[Visual Studio Marketplace](https://marketplace.visualstudio.com/items?itemName=uniflowed.uf)
and to [Open VSX](https://open-vsx.org/extension/uniflowed/uf) — the registry
Cursor, VSCodium and other VS Code forks install from — but only with the
publisher tokens the repository owner creates (`VSCE_PAT`, `OVSX_PAT`). No
release has had them, so every release so far has skipped both steps by name,
and `code --install-extension uniflowed.uf` finds nothing. Once they are set,
that command and a search for **uf** in the Extensions view work, and VS Code
updates the extension from then on.

The extension's version is not uf's, because the Marketplace takes no semver
prerelease: uf `0.1.0` is extension `0.1.9999`, and uf `0.0.0-alpha.46` is the
pre-release `0.0.46` (the Extensions view offers "Install Pre-Release" for
those). `release/version.js` has the mapping; it keeps the order of uf's
versions, a release after its prereleases. The extension starts whichever `uf`
it finds (below), not a copy of its own, so the two need not match.

To run a build of your own, package it from a uf checkout:

```sh
cd editors/vscode
npm install                     # vscode-languageclient, the only dependency
npx @vscode/vsce package        # writes uf-0.0.0.vsix locally
code --install-extension uf-0.0.0.vsix
```

Every run of the Editors workflow also uploads the packaged `.vsix` as an
artifact.

To work on it instead, open `editors/vscode` in VS Code and press F5, which
launches an Extension Development Host with it loaded.

## What works

Each line below is asserted against the real server by
`tests/library/lsp.test.js`, which drives `uf lsp` over framed messages the way
an editor does. The extension's own half — binary resolution, the arguments,
the working directory, the settings — is covered by
`tests/library/vscode-extension.test.js`. Both run under `uf test`, so
they are in `uf run ci`.

| | Where it comes from | How you use it |
| --- | --- | --- |
| **Diagnostics** | `uf_lint`, pushed on open and on every change; Flow's type errors, once typing pauses | Squiggles and the Problems panel. Source `uf`, code the rule id; for a type error, source `flow`, Flow's error code, and each location the message refers to listed under it. |
| **Formatting** | `uf_fmt`, the same `format_source` `uf fmt` calls | Format Document, or `uf.formatOnSave`. |
| **Quick fixes** | `textDocument/codeAction`, kind `quickfix` | The lightbulb on a diagnostic. |
| **Fix all** | kind `source.fixAll.uf` | `editor.codeActionsOnSave`, or the lightbulb. |
| **Hover** | `textDocument/hover` | The rule behind a diagnostic, what an import specifier names, what a rule id in a suppression comment means, and a key of `uf.config.js`. Anywhere else in a Flow file, the type under the cursor as Flow infers it, printed as Flow (`const greeting: string`). |
| **Go to Definition** | `textDocument/definition`, from Flow's inference | F12. Across files, into a package under `node_modules`, and into the project's `flow-typed/`. |
| **Go to Type Definition** | `textDocument/typeDefinition`, from Flow's inference | The declaration of the named types in the type under the cursor: for `const user: User`, `type User`. |
| **Completion** | `textDocument/completion` | In `uf.config.js`: the keys valid where you are typing, each with its documentation and type; after `"`, the values of a key whose type is a fixed set (`quotes: "single" \| "double"`); `true` and `false` for a boolean; in a tool spec (`runtime: "node@26"`), the names its key takes and, after `@`, that tool's versions, newest first. In any other Flow file, from Flow's inference: after `value.`, the members of `value`'s type with their types, and elsewhere the names in scope. |

Completion reads `@uniflowed/config`'s own Flow type — the declaration
`defineConfig` checks the file against — compiled into `uf`, so it needs nothing
installed, cannot offer a key uf does not read, and shows the words written
above each key in that declaration. It works while the file is half-typed and
does not parse, which is when you want it. A key the object already has is not
offered again, and nothing is offered under `vite`, whose options uf passes to
Vite unread rather than re-declaring.

A tool's versions — each major, then its releases, a hundred at a time until
what you type narrows them — come from the release list uf caches under
`$XDG_CACHE_HOME/uf/index`, and completion never waits for one:
the first time a tool is asked about with nothing cached, the list is fetched in
the background and its versions appear a keystroke or two later.

The extension needed no change for any of this: `uf.config.js` is one of the
files it already hands the server, and VS Code registers a completion provider
from the capabilities the server advertises. The Flow language mode keeps the built-in JavaScript providers out of the way.

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

Rename, find references, document highlights and the outline are the server's
(Flow's own services), across the project's files. **Signature help** is not:
`uf lsp` advertises no provider for it, and this extension does not add one; a
language client cannot invent what the server does not answer. A rename of a
name declared under `node_modules` or in a library definition is refused
rather than half made.

**Types need the file to parse.** Hover and go to definition answer nothing
while the file has a syntax error, because there is no inference to ask;
completion still answers after `value.`. The syntax error itself is reported
(`flow/syntax`); type errors come back once it parses.

**The first type answer waits for the project to be read.** The server reads
the whole project, as `uf check` does, in the background when it starts; a
hover that arrives first waits for that.

**Organize imports is not offered.** uf has no import-order opinion to organise
them by, so the server does not advertise `source.organizeImports`.

## Settings

| Setting | Default | |
| --- | --- | --- |
| `uf.server.path` | `""` | The `uf` binary. Absolute or relative to the workspace folder; `${workspaceFolder}` and a leading `~` are expanded. Empty means: search. |
| `uf.formatOnSave` | `false` | Format Flow files with the server's formatter on save. |
| `uf.trace.server` | `"off"` | `messages` or `verbose` logs the JSON-RPC traffic to the uf output channel. |
| `uf.workspace.disableBuiltinValidation` | `true` | In a uf project, turn off VS Code's built-in JavaScript validation for that folder, once. See below. |

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
* **uf: Configure Workspace for Flow** — writes the per-project settings below
  to the folder's `.vscode/settings.json`, after listing them: the two
  validation switches and uf as the default formatter for `javascript` and
  `javascriptreact`. A value the project already set is kept and reported.
* **uf: Show Status and Commands** — the menu behind the status bar item.

## The status bar

One item, on the right, while the window has a uf project: `✓ uf 0.2.0` with the
version the server reported in `initialize`, a spinner while it starts, a
warning when that version is older than the extension supports (0.1.0), and an
error when the server stopped or no binary was found. With several uf folders it
shows the worst of them and lists each in its tooltip. Clicking it opens the
commands above.

An old `uf` still starts: lint findings and formatting from it are better than
none. The extension says so once, with the version it found and what to do.

## Flow language mode and highlighting

In a uf project, `.js`, `.jsx`, `.mjs` and `.cjs` use the **Flow**
language mode. It provides proper signature contexts for `component` and
`hook`: names, parameters, type annotations, generics, exact objects and
`renders` types. JavaScript expressions and JSX reuse VS Code's built-in
grammar. Fenced `flow` signatures in hover use the same grammar and theme.

VS Code's TypeScript service does not match Flow documents, so its inaccurate
`any` hover, completion and semantic coloring do not appear beside uf's
inferred types. The built-in extension remains enabled for `.ts` files and
JavaScript outside the uf project.

`uf editor setup vscode` (or `cursor`) writes `[flow]` formatter/on-save
settings. The extension selects Flow with the public `setTextDocumentLanguage`
API only for files inside the uf folder, including multi-root windows. It
leaves file associations unchanged; window-wide associations would affect
JavaScript files opened from outside the uf project too.

A project that explicitly associates `*.js` with another language keeps it.
Choose **Flow** from the language-mode picker to opt in for that document, or
remove the custom association and reopen the file.
`"uf.workspace.disableBuiltinValidation": false` opts out of automatic
configuration and language selection. The setup notification offers **Undo**.

JavaScript validation settings are retained for older/custom setups that keep
the JavaScript language id. They are written once per folder, preserve values
the project already set, and never modify TypeScript validation.

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
lint levels. `uf lsp --cwd <dir>` would name the same folder; the extension sets
the process's working directory instead, so the project is stated once.

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
src/version.js     whether the uf the server reported is new enough
src/status.js      what the status bar item says for each server state
src/workspace.js   the settings a uf project gets, and which to write
src/extension.js   the glue: VS Code and vscode-languageclient
syntaxes/          the Flow injection grammar
release/version.js the version a uf release is published as; not packaged
```

Everything but `extension.js` is pure and has no `require("vscode")` in it,
which is what lets it be tested without an editor host. `extension.js` itself is
tested by `tests/library/vscode-extension-host.test.js` against a stand-in for
the `vscode` and `vscode-languageclient` modules: activation starts `uf lsp` in
the project folder, the status bar shows the reported version, an old `uf` is
called old once, the validation settings are written once and only where the
project set nothing, and every contributed command is registered.

## What no test here covers

The Editors CI lane starts real VS Code with an isolated profile in both a
single-folder and a multi-root workspace. It verifies one inferred Flow hover
without TypeScript's `any` entry, completion, formatting, preserved settings,
and unchanged JavaScript/TypeScript outside the uf project. TextMate tests
exercise complete signatures and JSX with the built-in JavaScript grammar,
and verify distinct colors in both Dark+ and Light+ for source and hover.

Native editor shortcuts, notification placement and integration with custom
third-party themes are still manual checks.
