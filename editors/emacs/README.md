# uf for Emacs

`uf lsp` through Eglot (built in since Emacs 29), or lsp-mode.
[`uf.el`](uf.el) is the whole integration — configuration, not a package.

## In place

```elisp
(add-to-list 'load-path "/path/to/uf/editors/emacs")
(require 'uf)
```

Then open a `.js` file inside a project with a `uf.config.js` and `M-x uf-start`
(or plain `M-x eglot`). To start it automatically:

```elisp
(add-hook 'js-mode-hook
          (lambda () (when (uf-project-root) (eglot-ensure))))
```

A different binary — this project's own copy, or a build you are debugging:

```elisp
(setq uf-executable "./node_modules/.bin/uf")
```

`uf.el` also adds `uf-project-find` to `project-find-functions`, so a directory
with a `uf.config.js` is a project.el project. That is not decoration: Eglot
starts a server with `default-directory` set to the project root, and that
directory is the one `uf lsp` reads its configuration from.

The lsp-mode form is in `uf.el` as a comment, three lines below the Eglot one.

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

Format on save is off. The hook to add is written out in `uf.el`.

## What you do not get

`eglot-rename` and `xref-find-references` have nothing behind them: `uf lsp`
advertises no rename or references provider, and Eglot reports that the server
does not support the request. ElDoc and `M-.` say nothing while the file does
not parse, since there is no inference to ask.

## Working directory

`uf lsp` reads `uf.config.js` once, at start-up: from the directory `--cwd`
names, or else from the directory it was started in. `uf.el` relies on the
second. Eglot's is `default-directory` at start-up, which is the project root
`uf-project-find` reports. If you start a server from a buffer that is not in a
uf project, you get uf's defaults rather than an error.

## Editing `uf.config.js`

The server reads it once. `M-x eglot-reconnect` afterwards.
