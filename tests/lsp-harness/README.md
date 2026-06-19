# lsp-harness: LSP library testing harness

End-to-end correctness tests for paideia-lsp's public API (diagnostics, hover,
definition, references handlers) exercised programmatically via direct library
calls, not JSON-RPC stdio.

## Tests

### Active Correctness Tests (4)

1. **correctness_diagnostics_publish_on_change** — Malformed document produces
   at least one ERROR-level diagnostic.

2. **correctness_hover_returns_linear_class_on_linear_prefix** — Hovering on a
   `linear:` prefix identifier returns markdown containing "Linear".

3. **correctness_definition_lands_on_first_occurrence** — Jumping to definition
   of an identifier returns the first occurrence's range.

4. **correctness_references_returns_all_occurrences_across_documents** —
   References query across two open documents returns all occurrences
   (multi-file).

### Latency Probe (1, #[ignore]'d)

**latency_single_char_change_under_100ms** — Single-character mutation on a
1000-line synthetic document re-diagnosed with cache under 100ms wall clock.
Debug builds blow the budget; enable in release CI with `cargo test --release`.

## Architecture

- `src/lib.rs` — Fixture helpers (test_url, create_document) + public re-exports
  of paideia-lsp's diagnostic/hover/navigation handlers.
- `tests/harness.rs` — The 5 test cases.

No JSON-RPC, no stdio round-trip. Tests use DocumentStore + in-memory Url
directly.
