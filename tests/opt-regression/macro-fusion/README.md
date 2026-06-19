# Macro-Fusion Optimization Pass Regression Corpus

Test harness for validating the macro-fusion pass (O1504) diagnostics.

## Fixtures

The `corpus/` directory contains `.pdx` (PaideiaOS assembly) files that exercise
macro-fusion alignment:

1. **01_aligned_cmp_je.pdx** — A CMP+JE pair already within a 16-byte fetch window (no padding needed).
2. **02_crossing_cmp_jne.pdx** — A CMP+JNE pair that crosses a 16-byte boundary (requires padding).
3. **03_test_je_aligned.pdx** — A TEST+JE pair within a fetch window (no padding needed).

## Running Tests

```bash
# Run just the macro-fusion corpus tests
cargo test --test runner -p paideia-opt-macro-fusion

# Run with output
cargo test --test runner -p paideia-opt-macro-fusion -- --nocapture
```

## Design Notes

- Macro fusion is an optimization available on Sandy Bridge (Intel) and Bulldozer (AMD) onward.
- The pass aligns CMP/TEST+Jcc pairs within 16-byte fetch boundaries.
- See `design/optimization-passes.md` §4 for architectural details.
