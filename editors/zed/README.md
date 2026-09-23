# uf for Zed

[`extension.toml`](extension.toml) declares the language server, and
[`src/lib.rs`](src/lib.rs) is the Rust/WASM half Zed loads to start it.

## The state of this integration

Zed has no setting that adds an arbitrary language server. A server reaches Zed
as an extension, and an extension that provides one is a Rust crate compiled to
WebAssembly. This directory has both halves: the manifest Zed reads and the
`zed_extension_api` implementation CI checks with `wasm32-wasip1`.

The command it returns is intentionally small:

```rust
fn language_server_command(
    &mut self,
    _: &zed::LanguageServerId,
    worktree: &zed::Worktree,
) -> zed::Result<zed::Command> {
    Ok(zed::Command {
        command: worktree
            .which("uf")
            .ok_or_else(|| "uf is not installed: https://uniflowed.dev".to_string())?,
        args: vec!["lsp".to_string()],
        env: worktree.shell_env(),
    })
}
```

Two things are load-bearing.

`worktree.which` rather than a bare `"uf"`: it searches the worktree shell's
`PATH`. Add `node_modules/.bin` to that path when using a project-pinned uf.

And the process must run **in the worktree root**. `uf lsp` reads `uf.config.js`
from its working directory, once, at start-up, and that read is the only source
of a project's `fmt` options and lint levels. `uf lsp --cwd <root>` would name
the same directory; Zed starts a language server in the worktree root, so
opening the project folder — the one with `uf.config.js` in it — is enough.

## What it would give you

`uf lsp` answers exactly this much, and `tests/library/lsp.test.js` drives the
server and asserts each one:

* **Diagnostics**, pushed on open and on every change, source `uf`, code the
  rule id.
* **Formatting**, from the same `uf_fmt` that `uf fmt` calls. Set
  `"formatter": "language_server"` for JavaScript in Zed's settings.
* **Quick fixes** and a **fix-all** action (`source.fixAll.uf`).
* **Hover**: the rule behind a diagnostic, what an import specifier names, what
  a rule id in a suppression comment means, and a key of `uf.config.js`; over
  anything else in a Flow file, the type as Flow infers it.
* **Go to definition** and **go to type definition**, from Flow's inference:
  across files, into `node_modules`, and into `flow-typed/`.
* **Completion**: in `uf.config.js`, the keys valid at the cursor with their
  documentation and type, the values of a key whose type is a fixed set, and a
  tool spec's names and, after `@`, its versions; in any other Flow file, after
  `.` the members of the value's type with their types, and elsewhere the names
  in scope.

Not rename, references or signature help. `uf lsp` advertises none of them.

## Install

Run `cargo build --manifest-path editors/zed/Cargo.toml --release --target
wasm32-wasip1`, then install this directory as a Zed dev extension. `uf` still
has to be installed in the project or on `PATH`; the extension only starts it.
