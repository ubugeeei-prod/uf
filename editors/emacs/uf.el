;;; uf.el --- Start uf lsp for Flow files in a uf project -*- lexical-binding: t; -*-

;; Configuration, not a package: two forms for Eglot and one alternative for
;; lsp-mode.  Nothing here implements a language feature; `uf lsp' answers all
;; of them from the same crates `uf lint' and `uf fmt' use.
;;
;; The part that matters is the project root.  `uf lsp' reads `uf.config.js'
;; from its own working directory, once, at start-up, and that read is where a
;; project's formatter width, quote style and lint levels come from.  Eglot
;; starts a server with `default-directory' set to the project root, so making
;; project.el agree that a directory with a `uf.config.js' is a project is what
;; makes the server read the right file.  `uf lsp --cwd' is not an alternative:
;; the flag is accepted and ignored.

;;; Code:

(require 'project)

(defgroup uf nil
  "The uf toolchain's language server."
  :group 'tools
  :prefix "uf-")

(defcustom uf-executable "uf"
  "The uf binary.  A project's own copy is `./node_modules/.bin/uf'."
  :type 'string
  :group 'uf)

(defconst uf-config-file "uf.config.js"
  "uf's single configuration surface, and therefore its project marker.")

(defun uf-project-root (&optional dir)
  "The uf project containing DIR, or nil."
  (locate-dominating-file (or dir default-directory) uf-config-file))

(defun uf-project-find (dir)
  "Make a directory with a `uf.config.js' a project.el project.
Added to `project-find-functions' below so that Eglot starts the server
in the directory whose configuration it is about to read."
  (let ((root (uf-project-root dir)))
    (and root (cons 'transient (expand-file-name root)))))

(add-hook 'project-find-functions #'uf-project-find)

(defun uf-server-program (&rest _)
  "The command Eglot should run."
  (list uf-executable "lsp"))

;; The file types `uf_lint''s `flow/syntax' parses: .js, .jsx, .mjs and .cjs.
(defconst uf-major-modes
  '(js-mode js-ts-mode javascript-mode js2-mode rjsx-mode jsx-mode)
  "Major modes uf claims.")

(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               (cons uf-major-modes #'uf-server-program)))

;;;###autoload
(defun uf-start ()
  "Start `uf lsp' for the current buffer with Eglot."
  (interactive)
  (unless (uf-project-root)
    (user-error "Not in a uf project: no %s above %s" uf-config-file default-directory))
  (call-interactively #'eglot))

;; Format on save, off by default: add this to your configuration if you want
;; it.  `eglot-format-buffer' asks the server, which answers from the same
;; `uf_fmt' that `uf fmt' calls.
;;
;;   (add-hook 'js-mode-hook
;;             (lambda ()
;;               (when (uf-project-root)
;;                 (add-hook 'before-save-hook #'eglot-format-buffer nil t))))

;; lsp-mode instead of Eglot.  `lsp-mode' finds the root through project.el as
;; well, so `uf-project-find' above is doing the same work for both.
;;
;;   (with-eval-after-load 'lsp-mode
;;     (add-to-list 'lsp-language-id-configuration '(js-mode . "javascript"))
;;     (lsp-register-client
;;      (make-lsp-client
;;       :new-connection (lsp-stdio-connection (lambda () (list uf-executable "lsp")))
;;       :activation-fn (lsp-activate-on "javascript" "javascriptreact")
;;       :server-id 'uf)))

(provide 'uf)
;;; uf.el ends here
