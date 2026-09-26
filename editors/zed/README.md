# uf for Zed

[`extension.toml`](extension.toml) declares the language server, and
[`src/`](src) is the Rust/WebAssembly half Zed loads to start it.

## Why an extension

Zed has no setting that adds an arbitrary language server. A server reaches Zed
as an extension, and an extension that provides one is a Rust crate compiled to
WebAssembly against `zed_extension_api`. It does one thing here: when Zed opens
a JavaScript file in a worktree, it tells Zed which `uf` to run, and with which
arguments.

The crate is its own Cargo workspace (the empty `[workspace]` in
[`Cargo.toml`](Cargo.toml)), so it never joins the repository's workspace build.

## Install

The extension is not in Zed's extension registry yet, so it is installed as a
dev extension from a uf checkout. Zed compiles it itself; you need Rust
installed through `rustup`, which is how Zed adds the `wasm32-wasip1` target.

1. In Zed, open the command palette and run **zed: install dev extension**.
2. Select the `editors/zed` directory.
3. Open the folder that holds your project's `uf.config.js`.

`uf` itself has to be installed, in the project or on `PATH`; the extension only
starts it.

## Which `uf`

In this order, per worktree — the VS Code extension's order:

1. `lsp.uf.binary.path` in Zed's settings, when set: absolute, relative to the
   worktree, or starting with `~/`. It is used as written, never skipped in
   favour of another `uf`.
2. `node_modules/.bin/uf` in the worktree, the copy the project pinned.
3. `uf` on the worktree shell's `PATH`.

With none of them, Zed shows an error listing the places looked at.

A Zed extension cannot ask whether a file exists, only read one as text. So
step 2 finds the launcher script npm, pnpm and yarn write into
`node_modules/.bin`, and not a native executable copied or linked there; name
one of those with the setting. A wrong `binary.path` is reported by Zed when the
spawn fails, naming the path.

```jsonc
// settings.json
{
  "lsp": {
    "uf": {
      "binary": {
        "path": "node_modules/.bin/uf",
        // Optional. Replaces the default `["lsp", "--cwd", <worktree>]`.
        // "arguments": ["lsp"],
        // Optional. Added to the shell's environment.
        // "env": { "RUST_LOG": "debug" }
      }
    }
  },
  "languages": {
    "JavaScript": {
      // Format with uf rather than Prettier.
      "formatter": { "language_server": { "name": "uf" } },
      "format_on_save": "on"
    }
  }
}
```

## Which project

The server is started as `uf lsp --cwd <worktree root>`. `uf lsp` reads
`uf.config.js` once, at start-up, from the directory `--cwd` names, and that
read is the only source of a project's `fmt` options and lint levels. So open
the folder that holds `uf.config.js`; a uf project nested below the folder you
opened is not picked up. Editing `uf.config.js` needs **editor: restart
language server**.

Zed calls every `.js`, `.jsx`, `.mjs` and `.cjs` file JavaScript, so that is the
one language the server is attached to.

## Only in a uf project, and without Zed's TypeScript servers there

Zed's default `language_servers` for JavaScript is `["!typescript-language-server",
"vtsls", "..."]`, and `"..."` means every other server registered for the
language — which, once this extension is installed, includes `uf`. So Zed asks
the extension for a command in *every* JavaScript worktree. The extension
answers only in a worktree with `uf.config.js` at its root; anywhere else it
declines, with a message saying so, and the worktree keeps the servers it would
have had. (To stop Zed asking at all, add `"!uf"` to
`languages.JavaScript.language_servers` in your user settings; a project's own
list, below, names `uf` explicitly and wins.)

In a uf project, vtsls — Zed's TypeScript server for JavaScript — reads a Flow
file as TypeScript and reports `component`, `hook`, `match` and every
annotation as errors. Commit `.zed/settings.json` to turn it off for that
project only:

```jsonc
// .zed/settings.json
{
  "languages": {
    "JavaScript": {
      "language_servers": ["uf", "!vtsls", "!typescript-language-server", "..."],
      "formatter": { "language_server": { "name": "uf" } },
      "format_on_save": "on"
    }
  }
}
```

A project's list replaces the user's rather than merging with it. Zed's worktree
trust applies: until you trust the project, Zed does not start language servers
its `.zed/settings.json` asks for.

## What it gives you

`uf lsp` answers exactly this much, and `tests/library/lsp.test.js` drives the
server and asserts each one:

* **Diagnostics**, pushed on open and on every change, source `uf`, code the
  rule id; and Flow's type errors once typing pauses, source `flow`, code
  Flow's own, with every location the message refers to.
* **Formatting**, from the same `uf_fmt` that `uf fmt` calls.
* **Quick fixes** and a **fix-all** action (`source.fixAll.uf`).
* **Hover**: the rule behind a diagnostic, what an import specifier names, what
  a rule id in a suppression comment means, and a key of `uf.config.js`; over
  anything else in a Flow file, the type as Flow infers it.
* **Go to definition** and **go to type definition**, from Flow's inference:
  across files, into `node_modules`, and into `flow-typed/`.
* **Syntax highlighting** is Zed's own for JavaScript, a tree-sitter grammar
  that does not know Flow's `component`, `hook`, `match` or `renders`; there
  is no maintained tree-sitter grammar for modern Flow to ship instead, so
  those lines are highlighted as the JavaScript grammar parses them.
* **Completion**: in `uf.config.js`, the keys valid at the cursor with their
  documentation and type, the values of a key whose type is a fixed set, and a
  tool spec's names and, after `@`, its versions; in any other Flow file, after
  `.` the members of the value's type with their types, and elsewhere the names
  in scope.
* **References, rename and the outline**, from Flow's services, across the
  project's files.

Not signature help. `uf lsp` does not advertise it.

## What is tested, and what is not

The Editors workflow (`.github/workflows/editors.yml`) runs on every change
here. It checks formatting, runs `cargo test` on the host — which binary is
chosen and why, the arguments, the environment, and that the settings key and
the language name agree with `extension.toml`, and that only a worktree with
`uf.config.js` is served — runs clippy for the host and
for `wasm32-wasip1`, and builds the release `.wasm`.

No Zed runs in CI. That Zed loads the extension, starts `uf lsp` for a uf
project and shows its diagnostics has to be checked by a person with Zed open.
