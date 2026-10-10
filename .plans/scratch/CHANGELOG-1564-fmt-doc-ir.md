# paideia-as#1564 (FMT-2): Doc IR + deterministic layout engine

## Summary

Hand-rolled Wadler-style Doc IR and deterministic single-pass layout
engine, no external `pretty` dep. L-sized backbone for the rest of
`paideia-fmt` (FMT-3 walker, FMT-4+ per-form renderers).

## Added

- `crates/paideia-fmt/src/doc.rs` (~215 LOC)
  - `Doc { Nil, Text, Line, SoftLine, HardLine, Nest, Group, Concat,
    IfBreak }` enum, `Clone + Debug + Default (= Nil)`.
  - `IfBreak { when_break, when_flat }` (keyword-safe field names;
    builder `if_break(brk, flt)` matches issue's positional spec).
  - Builders: `text`, `nil`, `nest`, `group`, `concat`, `line`,
    `softline`, `hardline`, `if_break`, `separate`.
  - `concat` folds empty→`Nil` and single→inner (no `Concat` wrapper
    noise).
  - 6 inline unit tests on the builders.
- `crates/paideia-fmt/src/layout.rs` (~310 LOC)
  - `FormatOptions { max_width }`, `Default = 100` (matches style
    spec #1561).
  - `render(&Doc, &FormatOptions) -> String` — stack-based, no
    recursion; group mode decided once by `fits` probe against
    `budget = max_width - col` extended by pending stack.
  - `fits` probe: inner groups optimistically flat (rustfmt /
    Prettier convention), HardLine short-circuits to true.
  - 14 inline unit tests covering: empty, single text, concat,
    group-flat, group-break-with-nest, nested-groups partial fit,
    IfBreak flat/break branches, separate, SoftLine flat/break, Line
    flat, HardLine always-breaks + indent application, Unicode char
    counting.

## Wired

- `pub mod doc;` + `pub mod layout;` in `crates/paideia-fmt/src/lib.rs`.

## Algorithm notes

- Classic Wadler "best" with greedy group-fit, deterministic (no
  backtracking). Single pass with a column counter and (indent, mode,
  &Doc) stack.
- Deviations from the paper:
  - Groups within the `fits` probe assumed flat (rustfmt behaviour).
  - HardLine short-circuits `fits` → `true`.
  - IfBreak resolved against committed mode at render time; during
    probe resolves against the probe's own `Flat` assumption.
- Width counted in Unicode scalar values (`chars().count()`), not
  bytes; grapheme-cluster width deferred to FMT-10.

## Performance

- Core walk is O(n) in Doc node count; stack push/pop + `String`
  append dominate.
- `fits` probe is bounded by `max_width + lookahead-to-HardLine`,
  worst-case O(n * max_width) across the whole doc — fine for source
  files on the order of 10^4 nodes.
- No allocations per node beyond the initial `Vec` stack reserves
  (capacity 16 for main stack, `pending.len() + 1` per probe).
- Rough estimate: hundreds of ns per node on commodity hardware
  (dominated by `String` append + stack push); the probe amortises
  since most groups fit flat and short-circuit after a few chars.

## Not run

- Build not run — main should invoke `bash tools/build.sh` and
  re-invoke me with the error tail if it fails.
