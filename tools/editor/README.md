# Editor Configuration Recipes for paideia-as

This directory contains minimum-viable configuration recipes for integrating paideia-as with four major editors: VS Code, Helix, Emacs, and Neovim.

## Overview

Each editor recipe wires up two core features:

1. **`paideia-lsp`**: Language server providing code completion, diagnostics, and navigation.
2. **`tree-sitter-paideia`**: Syntax highlighting grammar (tree-sitter format).

All recipes assume `.pdx` files as the canonical paideia-as source extension.

## Directory Structure

```
tools/editor/
├── vscode/                    # VS Code extension scaffold
│   ├── package.json
│   ├── client/
│   │   └── extension.ts        # LSP client setup
│   ├── language-configuration.json
│   ├── syntaxes/
│   │   └── paideia.tmLanguage.json   # Fallback TextMate grammar
│   ├── tsconfig.json
│   └── README.md
├── helix/
│   ├── languages.toml          # Helix language configuration
│   ├── runtime/queries/paideia/highlights.scm  # Tree-sitter queries
│   └── README.md
├── emacs/
│   ├── paideia-mode.el         # Major mode for Emacs
│   └── README.md
├── nvim/
│   ├── lua/paideia.lua         # Neovim Lua configuration
│   └── README.md
└── README.md                   # This file
```

## Editor-Specific Guides

- **[VS Code](./vscode/README.md)**: Full extension with LSP client and TextMate fallback grammar. Supports tree-sitter highlighting (deferred to m8-014).
- **[Helix](./helix/README.md)**: Language config with LSP and tree-sitter grammar.
- **[Emacs](./emacs/README.md)**: Major mode with LSP support and keyword highlighting.
- **[Neovim](./nvim/README.md)**: Lua configuration with tree-sitter and LSP integration.

## Prerequisites

All editors require:

1. **`paideia-lsp`**: Language server executable, installed and on `PATH`.
2. **Tree-sitter CLI** (for Helix and Neovim manual builds): `cargo install tree-sitter-cli` or via package manager.

Optional:

- **VS Code**: Node.js, npm, TypeScript compiler (for building the extension).
- **Emacs**: `lsp-mode` (MELPA package).
- **Neovim**: `nvim-treesitter`, `nvim-lspconfig` (plugin manager).

## Installation Quick Start

For each editor, follow the README in its subdirectory:

```bash
# VS Code
cd tools/editor/vscode && npm install && npm run compile

# Helix
cp tools/editor/helix/languages.toml ~/.config/helix/languages.toml
cp -r tools/editor/helix/runtime/queries ~/.config/helix/

# Emacs
load-file tools/editor/emacs/paideia-mode.el
# Or add to init.el: (require 'paideia-mode)

# Neovim
cp tools/editor/nvim/lua/paideia.lua ~/.config/nvim/lua/
# Then add to init.lua: require("paideia").setup()
```

## Scope and Status

**Phase 2, Milestone 8 (m8-013)**: Minimum-viable recipes shipped. Each editor can:
- Recognize `.pdx` files.
- Connect to `paideia-lsp`.
- Display syntax highlighting (TextMate fallback for VS Code; tree-sitter for others).

**Deferred to m8-014**:
- Full tree-sitter integration for VS Code (via WebAssembly tree-sitter).
- CI integration: end-to-end test lanes (awaiting CI restoration; see `.plans/m8-014-ci-lane.txt`).
- Advanced features: snippet support, formatter integration, debugger adapters.

## Testing

To verify a configuration:

1. Create a test file: `touch test.pdx`.
2. Add paideia-as code.
3. Open in the editor.
4. Verify:
   - File is recognized (icon, color scheme change).
   - Syntax highlighting appears.
   - LSP connects (`paideia-lsp` starts and responds to queries).

## Contributing

To extend a recipe:

1. Refer to the editor's official documentation (LSP client, tree-sitter integration).
2. Update the corresponding README with new features and tested versions.
3. Ensure changes don't break backward compatibility with the minimum-viable recipe.

## References

- [paideia-as Language Server (paideia-lsp)](../../../design/toolchain/language-server.md)
- [Tree-Sitter Grammar for paideia-as](./tree-sitter-paideia/)
- [VS Code LSP Client](https://code.visualstudio.com/api/language-extensions/language-server-extension-guide)
- [Helix Language Configuration](https://docs.helix-editor.com/languages.html)
- [Emacs lsp-mode](https://emacs-lsp.github.io/lsp-mode/)
- [Neovim nvim-lspconfig](https://github.com/neovim/nvim-lspconfig)
