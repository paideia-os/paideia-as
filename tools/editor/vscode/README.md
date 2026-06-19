# VS Code Extension for paideia-as

Minimal VS Code extension providing language support for paideia-as.

## Features

- Syntax highlighting via TextMate grammar fallback.
- LSP integration with `paideia-lsp` for language features (code completion, diagnostics, go-to-definition).
- Smart bracket pairing and comment shortcuts.

## Installation

1. Ensure `paideia-lsp` is installed and available on your `PATH`.
2. Clone this repository or copy the extension directory.
3. In VS Code, open the Extensions view (`Ctrl+Shift+X` / `Cmd+Shift+X`) and drag the extension folder into the window, or use:
   ```bash
   code --install-extension /path/to/paideia-as
   ```

## Building

```bash
npm install
npm run compile
```

## Tested Versions

- VS Code: 1.75.0 and later
- paideia-lsp: phase-2-m8-013 baseline

## Integration Status

Full tree-sitter highlighting (via `tree-sitter-paideia`) deferred to m8-014; TextMate grammar provides basic fallback.
CI integration (end-to-end test lane) deferred to m8-014 closure (awaiting CI restoration).
