# uf for Cursor

Cursor is a VS Code fork and uses the same extension format, so there is no
second extension here. [`editors/vscode`](../vscode) is the Cursor extension.

Duplicating it would mean two copies of the binary resolution, the settings and
the restart command, drifting apart one fix at a time.

## In place

Build a `.vsix` from `editors/vscode` and install it:

```sh
cd editors/vscode
npm install
npx @vscode/vsce package        # writes uf-0.0.0.vsix
cursor --install-extension uf-0.0.0.vsix
```

Or, to work on it, copy `editors/vscode` into `~/.cursor/extensions/uf/` and
restart Cursor. Cursor reads `~/.cursor/extensions`, not `~/.vscode/extensions`,
so an extension installed in VS Code is not automatically installed here.

Nothing is published to any marketplace.

## What is shared, and what is not

Everything: the manifest, the four settings, both commands, and the language
client. The extension activates on `workspaceContains:uf.config.js`, starts one
`uf lsp` per uf project with that project as the working directory, and wires
diagnostics, formatting, quick fixes, `source.fixAll.uf` and hover.

The settings are spelled the same — `uf.server.path`, `uf.formatOnSave`,
`uf.trace.server` — and go in Cursor's own `settings.json`.

`editors/vscode/README.md` is the reference for all of it, including what
`uf lsp` deliberately does not serve.

## The one difference worth knowing

Cursor's own AI features read the same diagnostics VS Code does, so a uf lint
finding is visible to them. uf does not integrate with them and does not need
to: the extension is a language client, and everything it shows comes from
`uf lsp`.
