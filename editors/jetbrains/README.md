# uf for JetBrains

JetBrains IDEs load `uf lsp` through the
[LSP4IJ](https://plugins.jetbrains.com/plugin/23257-lsp4ij) plugin, a generic
LSP client from Red Hat. There is no JetBrains-native plugin in this
repository. What is here is an LSP4IJ template, [`lsp4ij-template`](lsp4ij-template),
that fills in the server for you.

## Setup

1. Install LSP4IJ from the JetBrains Marketplace (**Settings → Plugins**).
2. Open LSP4IJ's *New Language Server* dialog: the **+** at the top of
   **Settings → Languages & Frameworks → Language Servers**, or the menu on the
   right of the LSP console.
3. In the **Template** list, choose **Import from custom template…** and select
   the `editors/jetbrains/lsp4ij-template` directory of a uf checkout (or a copy
   of it).
4. Check the **Server** tab, then **OK**. The server starts the next time a
   JavaScript file in the project is opened.

The template fills in exactly this, which is also what to type by hand on the
**Server** and **Mappings** tabs if you would rather not import it:

| | |
| --- | --- |
| Name | `uf` |
| Command, macOS and Linux | `sh -c "uf lsp --cwd '$PROJECT_DIR$'"` |
| Command, Windows | `cmd /c uf lsp --cwd "$PROJECT_DIR$"` |
| File name patterns | `*.js`, `*.mjs`, `*.cjs` as `javascript`; `*.jsx` as `javascriptreact` |

`$PROJECT_DIR$` is an IDE macro; LSP4IJ expands it and shows the resolved
command under the field. The `sh -c` and `cmd /c` wrappers are LSP4IJ's own
advice: they give the server the `PATH` your shell has, which an IDE started
from the Dock or the Start menu does not always inherit.

### A project-pinned uf

To run the copy a project installed rather than the one on `PATH`, change the
command to name it:

```text
$PROJECT_DIR$/node_modules/.bin/uf lsp --cwd $PROJECT_DIR$
```

## Why `--cwd`

`uf lsp` reads `uf.config.js` once, when it starts, from the directory `--cwd`
names, or else from its own working directory. That read is the only source of
the project's `fmt` options and lint levels. LSP4IJ does not document the
directory it starts a server in, so the template names the project on the
command line rather than relying on it. Open the directory that holds
`uf.config.js` as the project; a uf project nested inside the one you opened is
not picked up.

Editing `uf.config.js` needs a restart of the server, from its context menu in
the LSP console.

## What works

LSP4IJ receives the same server capabilities as the other editors:
diagnostics, formatting, quick fixes, `source.fixAll.uf`, hover (including the
type under the cursor), go to definition and go to type definition, and
completion — in `uf.config.js` from its schema, elsewhere from Flow's
inference. `tests/library/lsp.test.js` drives the real server and asserts each
of them, including that `--cwd` is the directory the configuration is read
from.

What does not work is the same too: rename, references, document symbols and
signature help are not advertised by `uf lsp`.

## What no test here covers

No JetBrains IDE runs in this repository's CI. That LSP4IJ imports the template,
expands the macro, and shows the diagnostics has been written from LSP4IJ's
documentation of its template format and has not been checked in an IDE by
this repository's tests.
