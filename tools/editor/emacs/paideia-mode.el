;;; paideia-mode.el --- Major mode for paideia-as -*- lexical-binding: t -*-

(require 'lsp-mode nil t)

(defvar paideia-mode-syntax-table
  (let ((table (make-syntax-table)))
    (modify-syntax-entry ?/ ". 12" table)
    (modify-syntax-entry ?\n ">" table)
    table)
  "Syntax table for paideia-mode.")

(defvar paideia-keywords
  '("module" "structure" "functor" "fn" "let" "val" "type" "sig"
    "pack" "unpack" "in" "with" "handle" "perform" "use" "import"
    "if" "then" "else" "match" "case" "of" "do"
    "linear" "affine" "ordered" "true" "false"))

(defvar paideia-font-lock-defaults
  `((,(regexp-opt paideia-keywords 'words) . font-lock-keyword-face)))

(define-derived-mode paideia-mode prog-mode "paideia"
  "Major mode for editing paideia-as source."
  :syntax-table paideia-mode-syntax-table
  (setq font-lock-defaults '(paideia-font-lock-defaults))
  (setq-local comment-start "// ")
  (setq-local comment-end ""))

(add-to-list 'auto-mode-alist '("\\.pdx\\'" . paideia-mode))

(when (require 'lsp-mode nil t)
  (with-eval-after-load 'lsp-mode
    (add-to-list 'lsp-language-id-configuration '(paideia-mode . "paideia"))
    (lsp-register-client
     (make-lsp-client :new-connection (lsp-stdio-connection "paideia-lsp")
                      :major-modes '(paideia-mode)
                      :server-id 'paideia-lsp))))

(provide 'paideia-mode)
;;; paideia-mode.el ends here
