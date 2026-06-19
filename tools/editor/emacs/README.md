# Emacs Major Mode for paideia-as

Emacs major mode providing syntax highlighting and LSP integration for paideia-as.

## Features

- Syntax highlighting with paideia-specific keyword recognition.
- Comment handling (line comments with `//`).
- LSP integration with `paideia-lsp` for code completion, diagnostics, and navigation.
- Auto-associates `.pdx` files with `paideia-mode`.

## Installation

1. Ensure `paideia-lsp` is installed and available on your `PATH`.
2. Ensure `lsp-mode` is installed (via MELPA or manual installation).
3. Copy `paideia-mode.el` to your Emacs configuration directory or load it directly:
   ```elisp
   (load-file "/path/to/paideia-mode.el")
   ```
   Or add to your `init.el`:
   ```elisp
   (add-to-list 'load-path "/path/to/editor/emacs")
   (require 'paideia-mode)
   ```

## Configuration

To enable LSP support, ensure `lsp-mode` is loaded before `paideia-mode`. Add to `init.el`:

```elisp
(require 'lsp-mode)
(require 'paideia-mode)
```

Then open a `.pdx` file and run `M-x lsp` to start the language server.

## Tested Versions

- Emacs: 28.0 and later
- lsp-mode: 9.0 and later
- paideia-lsp: phase-2-m8-013 baseline

## Integration Status

Tree-sitter highlighting deferred to m8-014 (via `tree-sitter` Emacs mode).
CI integration (end-to-end test lane) deferred to m8-014 closure (awaiting CI restoration).
