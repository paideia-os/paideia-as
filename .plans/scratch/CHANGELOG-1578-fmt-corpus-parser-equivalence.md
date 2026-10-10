# FMT-14 (paideia-as#1578): corpus parser-equivalence harness

## Summary

Second half of the "formatter must not drift semantics" contract (FMT-13
locks idempotence; FMT-14 locks structural AST stability). Walks every
`.pdx` file in the paideia-as corpus (~1,200 files) and asserts

    parse(src) ≡ parse(format(src))

where `≡` ignores spans but compares every `NodeKind`, every non-span
payload (literal values, mutability, atomic ordering, calling
convention, …) and every leaf identifier text. Together with FMT-13 this
means neither save churn (idempotence) nor silent AST reshaping
(parser-equivalence) can slip through the formatter.

A file that parses pre-format but fails to parse post-format is a hard
`BrokeParse` fail — the regression this test exists to catch. Files
that fail to parse up front (`tests/*/corpus/reject/**`,
`*_malformed_*.pdx`, `parse_error_*.pdx`, `lex_error.pdx` — ~86 files
total) are classified `Skipped` without an explicit path blocklist, and
remain the responsibility of the parser's own reject-corpus tests.

## Files changed

- `crates/paideia-fmt/src/ast_eq.rs` — new (~900 LOC, inflated by
  Rust's one-arm-per-variant match discipline across the ~40 ExprData,
  ~17 ItemData, 5 StmtData, 13 TypeData, 11 PatternData variants).
  Exports `ast_structurally_equal(src_a, arena_a, root_a, src_b,
  arena_b, root_b) -> bool` plus a `MAX_DEPTH = 2048` recursion cap.
  Covered by 4 unit tests for the obvious shapes (kind mismatch,
  ident-text mismatch, flag mismatch, span-insensitive match).
- `crates/paideia-fmt/src/lib.rs` — register the new `ast_eq` module
  and record it in the pipeline table.
- `crates/paideia-fmt/tests/corpus_parser_equivalence.rs` — new
  (~360 LOC). Houses the opt-in `#[ignore]`-gated sweep plus 8 sanity
  tests that keep the harness itself regression-tested at normal
  `cargo test` tempo. Reuses `paideia_fmt::corpus::walk_pdx` from
  FMT-13 for discovery and shares the `PAIDEIA_FMT_CORPUS` override +
  workspace-root climb logic bit-for-bit.

## Equivalence contract (what the comparator checks)

For every (`id_a`, `id_b`) pair, in lockstep across the two arenas:

1. Same `NodeKind`.
2. Same non-`NodeId` scalar payload — bool flags (`public`, `mutable`,
   `pipe_form`, `no_frame`, `jump_table`), enums (`PrefixOp`, `LoopKind`,
   `LinClass`, `AtomicOrdering`, `CallingConvention`, `SegPrefix`,
   `MacroFragmentKind`, `RepMin`), `Option<u32>`, `Option<String>` for
   `link_section`, decoded literal bytes (`StringLiteral`,
   `ByteStringLiteral`, `InlineBytes`, `InlineStr`), `InterruptAttr`
   (full `PartialEq`), `SharingConstraint::{left_path, right_path}`
   (string paths), `GenericParam::Lifetime::name`, `MatchAttrs`.
3. Every child `NodeId` compares equal under the same relation.
4. For leaf-shaped nodes (`Ident`, `Placeholder`, `GenericParam`,
   `MacroPattern`, `MacroTemplate`) whose identity lives *outside* the
   arena data vectors, source bytes at the node's span are compared
   directly.

Mnemonic IDs on `StmtData::Instruction` are intentionally resolved to
text via `AstArena::mnemonic_str` before comparison — the ID itself is
per-arena so a raw `u32` compare would always fail across the two
parses.

## Fields intentionally NOT compared (known gaps)

- **Side-tables** keyed on `NodeId` and attached to the arena rather
  than the enum payload: `ItemAtomicTable`, `ItemDslParserTable`,
  `ItemFingerprintTable`, `StructFieldAttrTable`, `StructAttrTable`,
  `FunctorAttrTable`, `MacroExpandDirectiveTable`,
  `PatternTypeHints`. These capture sparse attributes like
  `@atomic(Relaxed)`, `@endian(be|le)`, `@packed_struct`,
  `@dsl_parser(...)`. They are deliberately out of scope today because
  (a) the comparator would need per-table equality helpers that the
  side-tables don't expose publicly, and (b) today's byte-identical
  FMT-3 fallback means these entries round-trip trivially. The day a
  structural attribute-rewrite pass lands (e.g., `@atomic(Relaxed)` →
  canonical lowercase), this harness will need a side-table
  comparator; filed as a follow-up (not blocking FMT-14 landing).

Nothing in the AST data vectors is *intentionally* formatter-mutable
today — the style spec §7 / §10 passes rewrite only whitespace,
trailing commas, glyphs, and column alignment. In particular,
attribute ordering (§8.3) is source-preserving: `#[derive(A, B)]`
stays `#[derive(A, B)]`.

## Files intentionally excluded from the sweep

**No path-based blocklist.** The harness classifies `Skipped` on any
file whose initial parse fails — which is exactly the set of
intentionally-malformed fixtures:

- `tests/parser-reject-corpus/corpus/**`
- `tests/effects-corpus/corpus/reject/**`
- `tests/impl-corpus/corpus/reject/**`
- `tests/linearity-regression/reject/**`
- `tests/*/corpus/reject/**` more broadly
- Named `*_malformed_*.pdx`, `parse_error_*.pdx`, `lex_error.pdx`

Keeping the exclusion implicit (via "did it parse?") beats a path list
because any new reject corpus added by future tickets is auto-covered,
and any accidental reject-corpus regression (file that was supposed to
reject but now parses) shows up as a visible participant in the sweep
rather than a silent skip.

## Running

```sh
cargo test --package paideia-fmt --test corpus_parser_equivalence \
    -- --include-ignored --nocapture
```

`--nocapture` surfaces the "scanned / equal / skipped / mismatches /
broke-parse / format-error" summary line, mirroring FMT-13's shape so a
reader comparing the two sweeps sees aligned counts.

Narrow a bisect with `PAIDEIA_FMT_CORPUS=tests/effects-corpus cargo
test ...`.

## FMT-3 fallback interaction

Today's `format()` returns `source.to_string()` for every input (byte-
identical fallback plus the `walker::format_module` whole-file
`Doc::text(source.content())`), so the sweep is a *gate*: every file
either `Skipped` or `Equal`, nothing dirty. The harness will start
earning its keep the moment FMT-4..8 land per-kind structural walkers
that emit non-verbatim output.

## Follow-ups (filed separately, not blocking)

- Side-table comparator (atomic ordering, endian, packed-struct,
  fingerprint, DSL parser, macro-expand directives). Needs new
  `AstArena::*_table().get(NodeId) -> Option<...>` surface or a
  per-table iterator — currently only mutable accessors are public.
- Token-level round-trip harness (FMT-15, paideia-as#1579) will share
  the same `walk_pdx` + workspace-root plumbing — slot into
  `corpus_round_trip.rs` without touching this file.
