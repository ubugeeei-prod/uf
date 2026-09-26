# Editors

Editor integrations live here and talk to the native `uf lsp` server.

Editor-specific glue stays small. Parsing, linting, formatting, route/type
generation and diagnostics stay in the Rust crates: every integration below is
a client, and none of them implements a language feature.

## Targets

| Editor | What is here | Working |
| --- | --- | --- |
| [VS Code](vscode) | An extension: `package.json`, four source files, a `.vscodeignore` | Yes. CI packages it; each release attaches the `.vsix` to its GitHub release, which `uf editor install vscode` installs. Publishing `uniflowed.uf` to the Visual Studio Marketplace and Open VSX waits for the owner's `VSCE_PAT` and `OVSX_PAT`; until then those steps skip by name. |
| [Cursor](cursor) | Nothing of its own — it installs the VS Code extension: `uf editor install cursor`, or from Open VSX once it is published there | Yes |
| [Neovim](neovim) | [`lua/uf.lua`](neovim/lua/uf.lua), using core `vim.lsp.start`; keeps `ts_ls`/`vtsls` off uf projects | Yes. CI runs it in headless Neovim against a stand-in server. |
| [Vim](vim) | [`uf.vim`](vim/uf.vim), a vim-lsp registration | Yes; not run by CI. A TypeScript server vim-lsp also registers has to be disabled per project by hand. |
| [Helix](helix) | [`languages.toml`](helix/languages.toml), per project, replacing `typescript-language-server` | Yes; not run by CI. |
| [Emacs](emacs) | [`uf.el`](emacs/uf.el), for Eglot or lsp-mode; uf in a uf project, the usual server elsewhere | Yes. CI byte-compiles it and runs its ERT tests in batch Emacs. |
| [Zed](zed) | [`extension.toml`](zed/extension.toml) and a `zed_extension_api` Rust extension, installed as a dev extension; serves only worktrees with `uf.config.js` | CI tests it on the host and packages it with Zed's own `zed-extension`, as the registry would; not yet in Zed's registry, and not run in Zed by CI. |
| [JetBrains](jetbrains) | An [LSP4IJ](https://plugins.jetbrains.com/plugin/23257-lsp4ij) template to import | Through LSP4IJ rather than a native plugin; not run in an IDE by CI. The IDE's own JavaScript checking stays on beside uf's. |

VS Code and Zed are packages. The rest are configuration: a file to copy, four
to eighty lines long, or an editor plugin configured to start `uf lsp`.

## What every one of them gets

`uf lsp` serves exactly this, and no integration claims more:

* **Diagnostics** — `textDocument/publishDiagnostics`, from `uf_lint`, pushed
  on open and on every change; and Flow's type errors, the ones `uf check`
  reports, pushed beside them once typing pauses — for the edited file and
  every other open file that may import it. A type error's source is `flow`,
  its code is Flow's, and each location its message refers to is related
  information.
* **Formatting** — `textDocument/formatting`, from `uf_fmt`. One edit over the
  whole document, because the printer reprints from the syntax tree.
* **Code actions** — `quickfix` for the diagnostics with a mechanical answer,
  and `source.fixAll.uf` for all of them at once.
* **Hover** — the rule behind a diagnostic, what an import specifier names,
  what a rule id in a suppression comment means, a key of `uf.config.js`, and
  anywhere else in a Flow file the type under the cursor as Flow infers it.
* **Go to definition** and **go to type definition** — Flow's own answers,
  across files, into `node_modules` and into `flow-typed/`.
* **Completion** — in `uf.config.js`, the keys valid at the cursor, each with
  the documentation and type `@uniflowed/config` declares for it, the values
  of a key whose type is a fixed set, and in a tool spec like
  `runtime: "node@26"` the names and, after `@`, the versions. It works while
  the file is half-typed, and it needs nothing installed: the declaration it
  reads is compiled into `uf`, and versions come from release lists uf caches
  and refreshes in the background, never on a request's time. In any other
  Flow file, Flow's completion service: after `value.` the members of
  `value`'s type with their types, and elsewhere the names in scope.
* **Find references**, **document highlights** and **rename** (with
  `prepareRename`) — Flow's references service, in the file and in every
  project file that reaches the definition through its imports, properties
  included. A rename edits every one of those files, and is refused for a name
  a package under `node_modules` or a library definition declares.
* **Document symbols** — the file's outline, nested, from Flow's own provider.

Not signature help, workspace symbols, inlay hints, organize imports or
auto-imports. The server advertises none of them;
`tests/library/lsp.test.js` asserts that it does not, so a README here cannot
quietly start over-claiming.

## Keeping TypeScript's server off Flow files

Every editor here that ships or commonly runs a JavaScript language server —
VS Code's built-in service, Zed's vtsls, Neovim's `ts_ls`, Helix's and Emacs's
typescript-language-server — reads a Flow file as TypeScript and reports
`component`, `hook`, `match` and every type annotation as errors. Each
integration turns that off **in a uf project only**, so the same editor keeps
it in a TypeScript project; its README says how, and what it cannot turn off.

## The one thing every integration has to get right

**Tell the server which project it is serving.**

`uf lsp` reads the project's configuration once, at start-up: from the directory
`--cwd` names, or else from its own working directory. That read is the only
channel it has for a project's `fmt` options and lint levels, and there is no
request that can change it later. Starting the server in the project directory
and passing `uf lsp --cwd <dir>` are two ways of saying the same thing; every
integration here does one of them.

Getting this wrong fails quietly: the editor formats to uf's defaults instead of
to the project's, and reports the default lint levels rather than the configured
ones. The check is to format a file in the editor and then run `uf fmt --check`
in a terminal; if they disagree, the server was started somewhere else.

Each README says how its editor is told, and where its editor makes that hard.

## Editing `uf.config.js`

The server reads it once. Every integration's README says how to restart it;
the VS Code extension watches the file and restarts by itself.
