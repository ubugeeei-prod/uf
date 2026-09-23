;;; uf-tests.el --- Tests for uf.el -*- lexical-binding: t; -*-

;; emacs --batch -L editors/emacs -l ert -l editors/emacs/test/uf-tests.el \
;;       -f ert-run-tests-batch-and-exit
;;
;; What uf.el decides, in a real Emacs: which directory is a uf project,
;; which command a buffer gets, and that a buffer outside a uf project gets
;; the server it would have had without uf.el.  Neither Eglot nor lsp-mode is
;; started; `tests/library/lsp.test.js' covers the server itself.

;;; Code:

(require 'ert)
(require 'uf)

(defun uf-tests--project (with-config)
  "A temporary project, with a `uf.config.js' when WITH-CONFIG."
  (let ((root (file-name-as-directory (make-temp-file "uf-emacs-" t))))
    (make-directory (expand-file-name "app" root) t)
    (when with-config
      (with-temp-file (expand-file-name "uf.config.js" root) (insert "export default {};\n")))
    root))

(defmacro uf-tests--in (root mode &rest body)
  "Run BODY with `default-directory' in ROOT's app/ and `major-mode' MODE."
  (declare (indent 2))
  `(let ((default-directory (expand-file-name "app/" ,root))
         (major-mode ,mode))
     ,@body))

(ert-deftest uf-root-is-the-directory-with-the-config ()
  (let ((root (uf-tests--project t)))
    (uf-tests--in root 'js-mode
      (should (equal (file-name-as-directory (expand-file-name (uf-project-root)))
                     (expand-file-name root)))
      (should (equal (cdr (project-current nil default-directory)) (expand-file-name root))))))

(ert-deftest uf-no-root-without-the-config ()
  (uf-tests--in (uf-tests--project nil) 'js-mode
    (should-not (uf-project-root))))

(ert-deftest uf-eglot-starts-uf-in-a-uf-project ()
  (let ((root (uf-tests--project t))
        (uf-executable "uf"))
    (uf-tests--in root 'js-mode
      (should (equal (uf-eglot-contact)
                     (list "uf" "lsp" "--cwd" (expand-file-name root)))))))

(ert-deftest uf-eglot-leaves-other-projects-their-server ()
  (let ((eglot-server-programs
         (list (cons uf-major-modes #'uf-eglot-contact)
               '(((js-mode :language-id "javascript") (js-ts-mode :language-id "javascript"))
                 . ("typescript-language-server" "--stdio"))
               '(python-mode . ("pylsp")))))
    (uf-tests--in (uf-tests--project nil) 'js-mode
      (should (equal (uf-eglot-contact) '("typescript-language-server" "--stdio"))))
    (uf-tests--in (uf-tests--project nil) 'js-ts-mode
      (should (equal (uf-eglot-contact) '("typescript-language-server" "--stdio"))))))

(ert-deftest uf-eglot-calls-a-contact-function-it-falls-back-to ()
  (let ((eglot-server-programs
         (list (cons uf-major-modes #'uf-eglot-contact)
               (cons '(js-mode) (lambda (interactive) (list "rass" "ts" (if interactive "i" "n")))))))
    (uf-tests--in (uf-tests--project nil) 'js-mode
      (should (equal (uf-eglot-contact t) '("rass" "ts" "i"))))))

(ert-deftest uf-eglot-is-registered-first ()
  (require 'eglot)
  (should (eq (cdr (car eglot-server-programs)) #'uf-eglot-contact)))

(ert-deftest uf-lsp-mode-activates-only-in-a-uf-project ()
  (let ((root (uf-tests--project t)))
    (uf-tests--in root 'js-mode
      (should (uf--lsp-activate-p "app/button.js" 'js-mode))
      (should-not (uf--lsp-activate-p "app/notes.py" 'python-mode))))
  (uf-tests--in (uf-tests--project nil) 'js-mode
    (should-not (uf--lsp-activate-p "app/button.js" 'js-mode))))

;;; uf-tests.el ends here
