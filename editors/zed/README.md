# uf for Zed

[`extension.toml`](extension.toml) declares the language server. It is not the
whole extension, and this file says plainly what is missing and why.

## The state of this integration

Zed has no setting that adds an arbitrary language server. A server reaches Zed
as an extension, and an extension that provides one is a Rust crate compiled to
WebAssembly: the manifest names it, and a `zed::Extension` implementation
returns the command to run.

The manifest is here. **The Rust half is not**, and that is a decision rather
than an oversight: it needs `zed_extension_api` from crates.io pinned to the Zed
version being targeted and a `wasm32-wasip1` toolchain, neither of which is in
this project's lockfile or its CI, so nothing here could build it or check that
it compiles. A file written against a guessed-at API version, that no test
touches, would look like an integration and not be one.

So: Zed is **not** working today. What follows is the four lines it needs, for
whoever adds it with a Zed checkout to build against.

## What the Rust half has to do

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

`worktree.which` rather than a bare `"uf"`: it searches the worktree's own
`node_modules/.bin` before `PATH`, which is what gives a project the copy of uf
it pinned, the same order the VS Code extension uses.

And the process must run **in the worktree root**. `uf lsp` reads `uf.config.js`
from its working directory, once, at start-up, and that read is the only source
of a project's `fmt` options and lint levels. `uf lsp --cwd` is not a way
around it: the flag is accepted by the command and then ignored. Zed starts a
language server in the worktree root, so opening the project folder — the one
with `uf.config.js` in it — is what makes this correct.

## What it would give you

Once the Rust half exists, `uf lsp` answers exactly this much, and
`tests/library/lsp.test.js` drives the server and asserts each one:

* **Diagnostics**, pushed on open and on every change, source `uf`, code the
  rule id.
* **Formatting**, from the same `uf_fmt` that `uf fmt` calls. Set
  `"formatter": "language_server"` for JavaScript in Zed's settings.
* **Quick fixes** and a **fix-all** action (`source.fixAll.uf`).
* **Hover**: the rule behind a diagnostic, what an import specifier names, what
  a rule id in a suppression comment means.

Not go-to-definition, rename, references, completion, or the type at a
position. `uf lsp` advertises none of them.

## Meanwhile

Zed also runs `uf` as a plain command. `uf fmt`, `uf lint` and `uf check` in a
terminal are the same crates the server would answer from, and a task in
`.zed/tasks.json` gets them onto a keybinding.
