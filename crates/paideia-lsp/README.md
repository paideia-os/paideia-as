# paideia-lsp

**Status:** Shipped — Language Server Protocol server for paideia-as, wrapped around the elaborator.

Built on `tower-lsp`. Serves hover, go-to-definition, references,
completion, code actions, semantic tokens, inlay hints, and
formatting (via `paideia-fmt`). The `paideia-as lsp` subcommand is
the integration point for the four shipped editor configurations
(`tools/editor/vscode/`, `helix/`, `emacs/`, `nvim/`).

As of UNI-13 (Wave UNI-W6), the Unicode position-encoding bridge
goes through `paideia_as_unicode::lsp_bridge` — client-side UTF-16
column offsets are translated to UTF-8 byte offsets at the document
boundary so the elaborator never sees anything but valid UTF-8.

## Example

Launch the server (stdio transport):

```sh
cargo run --release --bin paideia-lsp
# or:
./target/release/paideia-as lsp
```

Editor configuration lives per-editor under `tools/editor/`. See
the editor's own `README.md` for installation.

## See also

- [`../../design/toolchain/lsp-phase3.md`](../../design/toolchain/lsp-phase3.md)
  — Phase 3 m4 architecture closure.
- `paideia-as-elaborator` — the elaborator surface this server
  wraps.
- `paideia-as-unicode` — UTF-16 ↔ UTF-8 position translation (UNI-13).
- `paideia-fmt` — formatting backend.
