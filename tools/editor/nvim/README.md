# Neovim Configuration for paideia-as

Neovim Lua configuration providing LSP and tree-sitter integration for paideia-as.

## Features

- Filetype registration for `.pdx` files.
- Tree-sitter highlighting via `nvim-treesitter` (grammar installed from paideia-as repository).
- LSP integration with `paideia-lsp` for code completion, diagnostics, and navigation.

## Installation

1. Ensure `paideia-lsp` is installed and available on your `PATH`.
2. Ensure the following plugins are installed (via your preferred plugin manager):
   - `nvim-treesitter`
   - `nvim-lspconfig`

3. Copy `lua/paideia.lua` to your Neovim config:
   ```bash
   cp lua/paideia.lua ~/.config/nvim/lua/
   ```

4. Add to your `~/.config/nvim/init.lua`:
   ```lua
   require("paideia").setup()
   ```

## Tree-sitter Grammar Installation

After adding the configuration, run:

```vim
:TSInstall paideia
```

Or manually:

```bash
cd ~/.local/share/nvim/site/pack/packer/start/nvim-treesitter
./install.sh paideia
```

## Testing

Open a `.pdx` file and verify:
- Syntax highlighting works (tree-sitter).
- LSP features work (`:LspInfo` should show `paideia-lsp`).

## Tested Versions

- Neovim: 0.9.0 and later
- nvim-treesitter: latest
- nvim-lspconfig: latest
- paideia-lsp: phase-2-m8-013 baseline

## Integration Status

Full plugin integration and configuration templates deferred to m8-014.
CI integration (end-to-end test lane) deferred to m8-014 closure (awaiting CI restoration).
