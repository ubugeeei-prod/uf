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
| Diagnostics | Pushed on open and on every change, through Flymake. `M-x flymake-show-buffer-diagnostics`. The source is `uf` and the code is the rule id. |
| Formatting | `M-x eglot-format-buffer`. The same `uf_fmt` that `uf fmt` calls. |
| Quick fixes | `M-x eglot-code-actions` on a diagnostic. |
| Fix all | `(eglot-code-actions (point-min) (point-max) "source.fixAll" t)`, which applies every mechanical fix in the file at once. `M-x eglot-code-actions` also lists it. |
| Hover | `M-x eldoc`, or `eldoc-mode` in the echo area. |

Format on save is off. The hook to add is written out in `uf.el`.

## What you do not get

`xref-find-definitions`, `eglot-rename`, completion and `xref-find-references`
have nothing behind them: `uf lsp` advertises no definition, rename, completion
or references provider, and Eglot reports that the server does not support the
request. ElDoc over a plain expression says nothing rather than showing an empty
popup — uf has no positional type query yet.

## Working directory

`uf lsp` reads `uf.config.js` from the directory it was started in, once. `uf
lsp --cwd` is accepted and ignored, so the working directory is the only
channel. Eglot's is `default-directory` at start-up, which is the project root
`uf-project-find` reports. If you start a server from a buffer that is not in a
uf project, you get uf's defaults rather than an error.

## Editing `uf.config.js`

The server reads it once. `M-x eglot-reconnect` afterwards.
