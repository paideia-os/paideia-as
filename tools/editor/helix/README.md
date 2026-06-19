# Helix Configuration for paideia-as

Helix language configuration for paideia-as with LSP and tree-sitter integration.

## Features

- Syntax highlighting via tree-sitter (via `paideia` grammar).
- LSP integration with `paideia-lsp` for language features.
- Smart indentation and comment handling.

## Installation

1. Ensure `paideia-lsp` is installed and available on your `PATH`.
2. Ensure you have the tree-sitter CLI tool installed: `cargo install tree-sitter-cli`.
3. Copy `languages.toml` to your Helix config directory:
   ```bash
   cp languages.toml ~/.config/helix/languages.toml
   ```
   Or append its contents to `~/.config/helix/languages.toml` if it already exists.

4. Copy the `runtime/queries/paideia/` directory to your Helix runtime:
   ```bash
   cp -r runtime/queries/paideia ~/.config/helix/runtime/queries/
   ```

## Building Tree-sitter Grammar

The tree-sitter grammar is fetched automatically by Helix during first use. If you need to rebuild manually:

```bash
cd ~/.config/helix/runtime/grammars/paideia
npm install
npm run build
```

## Tested Versions

- Helix: 24.03 and later
- paideia-lsp: phase-2-m8-013 baseline

## Integration Status

Full runtime setup and grammar caching deferred to m8-014.
CI integration (end-to-end test lane) deferred to m8-014 closure (awaiting CI restoration).
