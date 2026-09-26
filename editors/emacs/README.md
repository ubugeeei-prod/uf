# uf for Emacs

`uf lsp` through Eglot (built in since Emacs 29), or lsp-mode.
[`uf.el`](uf.el) is the whole integration — configuration, not a package.

## In place

```elisp
(add-to-list 'load-path "/path/to/uf/editors/emacs")
(require 'uf)
```

Then open a `.js` file inside a project with a `uf.config.js` and `M-x uf-start`
(or plain `M-x eglot`). To start it automatically, and format on save:

```elisp
(setq uf-format-on-save t)
(add-hook 'js-mode-hook #'uf-eglot-ensure)
(add-hook 'js-ts-mode-hook #'uf-eglot-ensure)
```

`uf-eglot-ensure` starts Eglot only in a uf project.

## TypeScript's server, and a TypeScript project

Eglot and lsp-mode both give JavaScript buffers typescript-language-server by
default (Eglot on Emacs 30; Emacs 31's Eglot tries `rass ts` first), and it
reads a Flow file as TypeScript — `component`, `hook`, `match` and every
annotation come back as errors. `uf.el` makes uf the server **in a uf project
only**:

* **Eglot.** `uf.el` puts its entry first in `eglot-server-programs`, and its
  contact function answers `uf lsp` in a uf project. Anywhere else it answers
  with whatever the rest of `eglot-server-programs` gives the buffer's mode, so
  a TypeScript project keeps typescript-language-server.
* **lsp-mode.** `uf.el` registers a client, `uf`, whose activation function
  accepts a buffer only in a uf project, at priority 1 — above `flow-ls` (-1)
  and `ts-ls` (-2). lsp-mode starts the highest-priority client that accepts a
  buffer, so a uf project gets uf and nothing else changes. To be explicit per
  project as well, commit a `.dir-locals.el` (`lsp-disabled-clients` is marked
  safe as a directory-local variable):

```elisp
((js-mode . ((lsp-disabled-clients . (ts-ls jsts-ls tsgo deno-ls))))
 (js-ts-mode . ((lsp-disabled-clients . (ts-ls jsts-ls tsgo deno-ls)))))
```

A different binary — this project's own copy, or a build you are debugging:

```elisp
(setq uf-executable "./node_modules/.bin/uf")
```

`uf.el` also adds `uf-project-find` to `project-find-functions`, so a directory
with a `uf.config.js` is a project.el project. That is not decoration: Eglot
starts a server with `default-directory` set to the project root, and that
directory is the one `uf lsp` reads its configuration from.

With lsp-mode instead, `(require 'uf)` before `lsp` starts is all: the client
is registered when lsp-mode loads.

## What you get

Everything is answered by `uf lsp`. `tests/library/lsp.test.js` drives the
server and asserts each of these.

| | How |
| --- | --- |
| Diagnostics | Pushed on open and on every change, through Flymake. `M-x flymake-show-buffer-diagnostics`. The source is `uf` and the code is the rule id. Flow's type errors join them once typing pauses: source `flow`, the code Flow's own (`incompatible-type`), and every location the message refers to as related information. |
| Formatting | `M-x eglot-format-buffer`. The same `uf_fmt` that `uf fmt` calls. |
| Quick fixes | `M-x eglot-code-actions` on a diagnostic. |
| Fix all | `(eglot-code-actions (point-min) (point-max) "source.fixAll" t)`, which applies every mechanical fix in the file at once. `M-x eglot-code-actions` also lists it. |
| Hover | `M-x eldoc`, or `eldoc-mode` in the echo area, including over a key of `uf.config.js`; over anything else in a Flow file, the type as Flow infers it. |
| Definition | `M-.` (`xref-find-definitions`): across files, into `node_modules` and into `flow-typed/`. |
| Type definition | `M-x eglot-find-typeDefinition`: the declaration of the named types in the type at point. |
| Completion | `completion-at-point` (`C-M-i`), which Eglot feeds from the server, or Company or Corfu on top of it. In `uf.config.js`, the keys valid at the cursor, with their documentation and type, the values of a key whose type is a fixed set, and in a tool spec the names and, after `@`, the versions. In any other Flow file, after `.` the members of the value's type with their types, and elsewhere the names in scope. |

Format on save is off unless `uf-format-on-save` is set.

## What you do not get

Signature help has nothing behind it: `uf lsp` advertises no provider for it.
`eglot-rename`, `xref-find-references` and Imenu's outline are the server's,
and need the file to parse, as ElDoc and `M-.` do, since there is no inference
to ask otherwise.

## Working directory

`uf lsp` reads `uf.config.js` once, at start-up: from the directory `--cwd`
names, or else from the directory it was started in. `uf.el` relies on the
second. Eglot's is `default-directory` at start-up, which is the project root
`uf-project-find` reports. If you start a server from a buffer that is not in a
uf project, you get uf's defaults rather than an error.

## Editing `uf.config.js`

The server reads it once. `M-x eglot-reconnect` (or `M-x lsp-workspace-restart`)
afterwards.

## What is tested

`test/uf-tests.el` runs in batch Emacs in the Editors workflow: which directory
is a uf project, the command a buffer gets (`uf lsp --cwd <root>`), that a
buffer outside a uf project gets the server the rest of
`eglot-server-programs` gives it — a plain entry or a contact function — and
that lsp-mode's `uf` client accepts only uf projects. `uf.el` is byte-compiled
there too. Eglot and lsp-mode themselves are not started by the tests.
