;;; uf.el --- Start uf lsp for Flow files in a uf project -*- lexical-binding: t; -*-

;; Configuration, not a package: Eglot (built into Emacs 29) or lsp-mode,
;; told to start `uf lsp' for the JavaScript buffers of a uf project and
;; nothing else.  Nothing here implements a language feature; `uf lsp' answers
;; all of them from the same crates `uf lint' and `uf fmt' use.
;;
;; Three things matter, and each is decided by one function below:
;;
;; * The project root (`uf-project-find').  `uf lsp' reads `uf.config.js'
;;   once, at start-up -- from the directory `--cwd' names, or else from its
;;   own working directory -- and that read is where a project's formatter
;;   width, quote style and lint levels come from.  Eglot starts a server with
;;   `default-directory' set to the project root, so making project.el agree
;;   that a directory with a `uf.config.js' is a project is what makes the
;;   server read the right file.
;;
;; * Which server a JavaScript buffer gets (`uf-eglot-contact',
;;   `uf--lsp-activate-p').  Emacs's JavaScript modes get
;;   typescript-language-server from Eglot and lsp-mode by default, and it
;;   reads a Flow file as TypeScript: `component', `hook', `match' and every
;;   annotation come back as errors.  In a uf project uf is the server instead;
;;   anywhere else the buffer gets whatever it would have got without this
;;   file.  So loading uf.el changes nothing in a TypeScript project.
;;
;; * Format on save (`uf-format-on-save'), off by default.

;;; Code:

(require 'project)
(require 'seq)

(declare-function eglot-ensure "eglot")
(declare-function eglot-managed-p "eglot")
(declare-function eglot-format-buffer "eglot")
(declare-function lsp-format-buffer "lsp-mode")
(declare-function lsp-register-client "lsp-mode")
(declare-function make-lsp-client "lsp-mode")
(declare-function lsp-stdio-connection "lsp-mode")
(defvar eglot-server-programs)
(defvar lsp-language-id-configuration)
(defvar lsp-mode)

(defgroup uf nil
  "The uf toolchain's language server."
  :group 'tools
  :prefix "uf-")

(defcustom uf-executable "uf"
  "The uf binary.  A project's own copy is `./node_modules/.bin/uf'."
  :type 'string
  :group 'uf)

(defcustom uf-format-on-save nil
  "Format a uf project's JavaScript buffers with `uf lsp' before saving."
  :type 'boolean
  :group 'uf)

(defconst uf-config-file "uf.config.js"
  "uf's single configuration surface, and therefore its project marker.")

;; The file types `uf_lint''s `flow/syntax' parses: .js, .jsx, .mjs and .cjs.
(defconst uf-major-modes
  '(js-mode js-ts-mode javascript-mode js2-mode rjsx-mode js-jsx-mode jsx-mode)
  "Major modes uf claims, in a uf project.")

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

(defun uf-server-command ()
  "The command that starts uf's language server for the current buffer.
The project is named with `--cwd' as well as by the directory the client
starts the server in, so it is right however the client decides that."
  (let ((root (uf-project-root)))
    (if root
        (list uf-executable "lsp" "--cwd" (expand-file-name root))
      (list uf-executable "lsp"))))

;;;; Eglot

(defun uf--mode-spec-modes (spec)
  "The major modes an `eglot-server-programs' mode SPEC names.
SPEC is a mode, a list of modes, a (MODE :language-id ID) entry, or a
list mixing modes and such entries."
  (cond
   ((symbolp spec) (list spec))
   ((and (consp spec) (keywordp (cadr spec))) (list (car spec)))
   ((consp spec) (mapcar (lambda (item) (if (consp item) (car item) item)) spec))
   (t nil)))

(defun uf--mode-spec-matches-p (spec mode)
  "Whether the `eglot-server-programs' mode SPEC covers MODE."
  (seq-some (lambda (candidate)
              (and (symbolp candidate) (provided-mode-derived-p mode candidate)))
            (uf--mode-spec-modes spec)))

(defun uf--other-contact (programs mode interactive)
  "The contact PROGRAMS would give MODE if uf's own entry were not there.
Functions are called with INTERACTIVE, as Eglot calls them."
  (let ((entry (seq-find (lambda (entry)
                           (and (not (eq (cdr entry) #'uf-eglot-contact))
                                (uf--mode-spec-matches-p (car entry) mode)))
                         programs)))
    (when entry
      (let ((contact (cdr entry)))
        (if (functionp contact) (funcall contact interactive) contact)))))

(defun uf-eglot-contact (&optional interactive)
  "The server Eglot should start for the current buffer.
`uf lsp' in a uf project; otherwise whatever the rest of
`eglot-server-programs' says, so a TypeScript project keeps its server.
INTERACTIVE is passed on to a contact function found there."
  (if (uf-project-root)
      (uf-server-command)
    (and (boundp 'eglot-server-programs)
         (uf--other-contact eglot-server-programs major-mode interactive))))

(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs (cons uf-major-modes #'uf-eglot-contact)))

(defun uf-eglot-ensure ()
  "Start Eglot for this buffer if it is in a uf project.
For a major mode hook: (add-hook \\='js-mode-hook #\\='uf-eglot-ensure)."
  (when (and (uf-project-root) (fboundp 'eglot-ensure))
    (eglot-ensure)
    (when uf-format-on-save
      (add-hook 'before-save-hook #'uf--format-buffer nil t))))

(defun uf--format-buffer ()
  "Format the buffer with whichever client serves it."
  (cond
   ((and (fboundp 'eglot-managed-p) (eglot-managed-p)) (eglot-format-buffer))
   ((and (boundp 'lsp-mode) lsp-mode (fboundp 'lsp-format-buffer)) (lsp-format-buffer))))

;;;###autoload
(defun uf-start ()
  "Start `uf lsp' for the current buffer with Eglot."
  (interactive)
  (unless (uf-project-root)
    (user-error "Not in a uf project: no %s above %s" uf-config-file default-directory))
  (call-interactively #'eglot))

;;;; lsp-mode

(defun uf--lsp-activate-p (_file-name mode)
  "Whether lsp-mode should start uf for a buffer in MODE.
Only in a uf project, so a TypeScript project keeps `ts-ls'."
  (and (seq-some (lambda (claimed) (provided-mode-derived-p mode claimed)) uf-major-modes)
       (uf-project-root)
       t))

;; lsp-mode starts the highest-priority client whose activation function
;; accepts the buffer.  `ts-ls' is -2 and `flow-ls' -1; uf is 1, and accepts
;; only buffers in a uf project, so there it wins and elsewhere it is never
;; a candidate.
(with-eval-after-load 'lsp-mode
  (dolist (mode uf-major-modes)
    (add-to-list 'lsp-language-id-configuration (cons mode "javascript")))
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection #'uf-server-command)
    :activation-fn #'uf--lsp-activate-p
    :priority 1
    :server-id 'uf)))

(provide 'uf)
;;; uf.el ends here
