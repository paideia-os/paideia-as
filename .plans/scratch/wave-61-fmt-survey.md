# Wave 61 — paideia-fmt completion survey

**Target crate:** `crates/paideia-fmt/` in paideia-as.
**Goal:** take the formatter from the current whitespace-only normalizer to a
full `.pdx` auto-formatter with round-trip stability, comment preservation, and
parser-equivalence over the paideia-as `.pdx` corpus.

**Scope boundary.** This survey covers only `paideia-fmt` (plus the trivia /
CST plumbing it will depend on in `paideia-as-lexer` and
`paideia-as-parser`). CLI driver (`paideia-as fmt …` in
`crates/paideia-as/src/cmd_fmt.rs`) and LSP wiring
(`crates/paideia-lsp/src/server.rs`) already exist and will be re-pointed at
the new API during the final wave rather than rewritten.

---

## 1. What is implemented today

### 1.1 Crate layout

```
crates/paideia-fmt/
├── Cargo.toml             # zero runtime deps (just workspace metadata)
├── src/
│   ├── lib.rs             # 134 LOC
│   └── main.rs            # 23 LOC
└── tests/
    └── cli.rs             # 28 LOC (one stdin round-trip test)
```

Total LOC: **~185** (library + binary + tests).

### 1.2 Public API (`paideia_fmt` crate)

| Item | Purpose |
|---|---|
| `pub struct FormatOptions { tab_width, use_tabs, max_blank_lines, glyph_style }` | Configuration struct. |
| `pub enum GlyphStyle { Preserve, Ascii, Unicode }` | ASCII↔Unicode arrow/lambda policy. |
| `pub fn format(source: &str, opts: &FormatOptions) -> String` | **Only** formatter entry point. |
| `pub fn ascii_to_unicode(source: &str) -> String` | Scalar glyph rewrite. |
| `pub fn unicode_to_ascii(source: &str) -> String` | Reverse direction. |

### 1.3 Formatting rules actually enforced

The current `format` function is a **line-at-a-time text pass**. It does NOT
tokenize, lex, or parse. It performs exactly four passes:

1. **Trailing-whitespace strip.** `line.trim_end()` on every line.
2. **Tab/space normalization.** Replace `\t` with `tab_width` spaces; if
   `use_tabs`, re-tab leading indent only.
3. **Blank-line cap.** At most `max_blank_lines` consecutive blank lines
   (default 2).
4. **Glyph pair substitution.** Scalar `str::replace` of
   `->/→`, `=>/⇒`, `/=/≠`, `forall/∀`, `exists/∃`, `\\/λ`.
5. **Trailing newline.** Always ends with exactly one `\n`.

### 1.4 Downstream consumers

- `crates/paideia-as/src/cmd_fmt.rs` — `paideia-as fmt [files…]` CLI subcommand
  (`--check`, `--diff`, `--stdin`, multi-file). Delegates to
  `paideia_fmt::format(_, &FormatOptions::default())`.
- `crates/paideia-lsp/src/server.rs:151` — `textDocument/formatting` handler
  reuses the same entry point.
- `crates/paideia-lsp/src/code_action.rs:150,183` — code-action wrappers for
  ASCII↔Unicode glyph conversion only.

### 1.5 Tests

- 9 unit tests in `lib.rs` covering: glyph substitution (both directions),
  trailing-whitespace strip, blank-line cap, no-op on already-formatted input,
  idempotence over a single formatted/unformatted pair, glyph flag plumbing.
- 1 integration test (`tests/cli.rs`) exercising `--stdin`.
- 11 tests in `cmd_fmt.rs` covering CLI behaviour (check, diff, multi-file,
  stdin, exit codes).

No corpus-based testing. No comment-preservation testing. No parser-round-trip
testing. The idempotence test is a single hardcoded pair.

---

## 2. What is missing

The current implementation **cannot** be extended incrementally into a real
formatter: it is a text-only pass with no awareness of what a `.pdx` program
contains. Everything below the "whitespace rules" line is net-new code.

### 2.1 Infrastructure gaps (foundational — must land first)

| # | Gap | Why it blocks |
|---|---|---|
| F1 | **No CST / trivia-attached syntax tree.** The lexer emits `Trivia` (whitespace, newline, line/block/doc comments) but `paideia-as-parser` discards it — the AST (`paideia-as-ast`) has no comment-carrying node. A formatter that re-emits from the AST will **silently delete every comment** in the source. | Comment preservation (hard requirement) and round-trip stability are impossible without this. |
| F2 | **No formatting IR.** No Wadler/Oppen-style `Doc` / `Group` / `Nest` / `Break` builder — just string concatenation. | Rich line-breaking, max-line-width wrap, consistent-indent nesting, and alignment rules all require an IR. |
| F3 | **No max-line-width policy.** `FormatOptions` has no `max_width` field. There is also no style document in `design/toolchain/` prescribing it. | Every per-syntax rule depends on knowing when to wrap. |
| F4 | **No AST walker.** `paideia-as-ast::visit` exists for other crates but no `format_item(arena, id, &opts)` / `format_expr(…)` / etc. tree-walker is wired. | Every per-syntax rule hangs off a walker. |

### 2.2 Per-syntax rule gaps (one rule per AST variant)

Variant counts from `grep -c "^    [A-Z]" crates/paideia-as-ast/src/*.rs`:

| AST enum | Variants with zero formatter rule |
|---|---|
| `ItemData` | **17** (Module, Structure, Signature, Let, Fn, Enum, Struct, Trait, Impl, Functor, PackedStruct, GpuContext, Timeline, EndianAttr, MacroExpandDirective, AtomicItem, FunctorAttr) |
| `ExprData` | **39** (literals, call, binop, prefix, match, if, loop, while, for, let-in, block, lambda, record, tuple, index, field, cast, ref, deref, unsafe, handler, pack, unpack, quote, splice, segment-prefix, pattern-match, …) |
| `TypeData` | **16** (Path, Fn, Tuple, Array, Slice, Ref, Ptr, Memref, Record, Enum, Trait, Lifetime, LinearClass, Effect, Capability, Generic) |
| `PatternData` | **11** (Wildcard, Literal, Binding, Tuple, Record, Enum, Range, Or, Guard, Ref, Rest) |
| **Total** | **83 AST shapes** — zero formatter rules today. |

Plus the surface forms that are not single variants but *compound shapes*:

- Generic parameter + bound lists (`<T: Trait<Item=Foo>>`).
- Effect rows (`!{RawMem, IO}`).
- Capability rows (`@{paideia.raw_mem}`).
- Attribute syntax (`@endian(be)`, `@interrupt(x86_64)`, `@abi(sysv)`, …).
- Doc-comment attachment (`///` / `/** */` → preceding item).
- Match-arm alignment (optional, high-value for readability).
- Unsafe-block field grouping (`effects:`, `capabilities:`,
  `justification:`, `block:` — see `examples/15_unsafe.pdx`).
- `impl<T> Trait for (Type (T))` surface — generic header on both sides.
- `enum Opt<T> { Some(T), None }` single-line vs multi-line.
- Multi-line let with trailing `=` on signature line
  (`let f : (…) -> R !{…} @{…} =\n  fn (x) -> …`).

### 2.3 TODO / FIXME / unimplemented markers

```
$ grep -rnE "TODO|FIXME|unimplemented!" crates/paideia-fmt/
(no hits)
```

The crate is "clean" because it is intentionally minimal; the gap is the
missing surface area, not latent markers.

### 2.4 Configuration gaps

- No config-file discovery (no `.pdxfmt`, no `[fmt]` block in a workspace
  manifest).
- No per-file override pragma (comparable to `// rustfmt-ignore`).
- No stable-vs-unstable options split (prettier/rustfmt pattern).
- No `--emit <files|stdout|check-diff>` menu beyond the existing boolean
  flags on the CLI.

### 2.5 Testing gaps

- **No corpus harness.** The repo has **1,233 `.pdx` files** on disk
  (see §3), none exercised by the formatter.
- **No idempotence sweep.** The single `format_idempotent` unit test uses one
  hardcoded pair; it does not sweep the corpus.
- **No parser-equivalence sweep.** Nothing checks that
  `parse(format(src)) ≡ parse(src)` across the corpus — the single hardest
  invariant to maintain.
- **No comment-preservation suite.** Nothing checks that comment text and its
  attachment position survive a round-trip.
- **No golden-file suite.** No `fixtures/<name>.pdx.in` → `.pdx.out`.

### 2.6 CLI / tool-surface gaps (lower priority — infrastructure exists)

- No globbing (`paideia-as fmt crates/**/*.pdx`). Current CLI takes a literal
  path list.
- No `--list-different` (print only names of files that would change) — useful
  for CI gating in paideia-os once we add formatter CI there.
- No `--emit json` (structured per-file-report, for editor integrations).
- No workspace-wide "format everything tracked by git" convenience.

### 2.7 Documentation gaps

- `design/toolchain/syntax-reference.md` is referenced from
  `pretty.rs`, `reserved-word-policy.md`, `phase-3-plan.md`, and
  `pointer-types-phase3.md` but **does not exist in the current working
  tree** — only in agent worktrees. This document is the natural home for
  the formatter style spec; its absence from the mainline tree is a dependency
  for FMT-0.
- No `design/toolchain/fmt-style.md`. A formatter without a style spec is a
  formatter whose output nobody can review.

---

## 3. Dependency inventory

### 3.1 `.pdx` corpus (test-input material)

```
Total .pdx files in-tree:                            1,233
├── crates/paideia-as-emit/tests/fixtures/build-emit:    362  (round-trip emit)
├── codes/ (diagnostic negative/positive samples):      251
├── pdx/ (unit-level fixtures):                         189
├── corpus/ (reflection snapshots):                      86
├── accept/ (parser accepts):                            77
├── reject/ (parser rejects):                            56
├── speculative/ (parser forward-looking):               53
├── fixtures/ (misc fixtures):                           39
├── examples/ (user-facing worked examples):             21
├── other misc dirs:                                   ~100
```

**Verdict.** We have ample corpus. The 77-file **`accept/` corpus** is the
natural seed for round-trip and parser-equivalence testing (every file in
`accept/` parses cleanly, by definition). The 21-file **`examples/` corpus**
is the natural seed for idempotence testing (user-facing shapes). The 56-file
**`reject/` corpus** can be excluded from the formatter's parser-driven path
because the formatter has nothing to do with ill-formed input (text-level
fallback should still be lossless — covered by F2).

### 3.2 Lexer trivia API — already present

`paideia-as-lexer::Lexer::take_trivia()` returns `Vec<Trivia>` of all trivia
accumulated since the last token. Each `Trivia` carries a `Span` (file +
byte range). This is the raw material the CST wrapper (FMT-1) will consume.

`TriviaKind` enum variants: `Whitespace`, `Newline`, `LineComment`,
`BlockComment`, `DocLineComment`, `DocBlockComment`.

### 3.3 Parser entry — already present

`paideia_as_parser::Parser::parse_source_file() -> Result<NodeId, ParseError>`
returns a `NodeId` into the `AstArena`. The parser does not currently
associate trivia with any node — that is the FMT-1 wiring.

### 3.4 Pretty-printer precedent — none to reuse

`crates/paideia-as-ast/src/pretty.rs` (1,223 LOC) exists but is a
**debug-dump** facility, not a source-reconstructing printer. Its output is
of the shape `Module { name: n1, sig: None, body: n2 }` — it is not a
starting point for the formatter.

### 3.5 External crate dependencies

The formatter will need:

- **`pretty` (0.12) or `pretty_assertions`** — one of the Oppen/Wadler
  pretty-print crates for the Doc IR. Alternative: hand-roll ~500 LOC. Hand-
  rolling is my recommendation for determinism, zero-surprise behaviour, and
  to keep paideia-as supply-chain minimal.
- **`similar`** — already in `cmd_fmt.rs` for `--diff`. Reuse.
- **`walkdir` or `glob`** — needed for FMT-18 (CLI glob). Already present in
  `crates/paideia-as` for other subcommands — reuse.

**No new mandatory external crates.** The one plausible addition (`pretty`) is
not required; I recommend writing the IR inline for determinism.

---

## 4. Per-gap sizing

Scale: **S** ≤ 200 LOC net diff, 1 wave. **M** = 200–500 LOC, 1–2 waves.
**L** > 500 LOC, 2–3 waves.

| Handle | Gap | Size | Waves |
|---|---|---|---|
| FMT-0 | Style spec (`design/toolchain/fmt-style.md`) + resurrect `design/toolchain/syntax-reference.md` top-level link | S | 1 |
| FMT-1 | Trivia-attached CST layer (new module in `paideia-fmt`; consumes `Lexer::take_trivia` + parser spans) | L | 2 |
| FMT-2 | `Doc` IR (Nil / Text / Line / Nest / Group / Concat) + layout engine with `max_width` | M | 1 |
| FMT-3 | AST walker scaffold (`fn format_source_file` → `Doc`), wires FMT-1 and FMT-2 | S | 1 |
| FMT-4 | `ItemData` formatters (17 variants) | L | 2 |
| FMT-5 | `ExprData` formatters (39 variants) — split into two atoms: EXPR-A primary/postfix/call/binop/prefix, EXPR-B control-flow (match/if/loop/while/for/let-in/block/lambda/handler/unsafe/pack/unpack/quote/splice) | L | 2 (one per sub-atom) |
| FMT-6 | `TypeData` formatters (16 variants) incl. effect rows, capability rows, memref | M | 1 |
| FMT-7 | `PatternData` formatters (11 variants) incl. or-patterns and record-destructuring | M | 1 |
| FMT-8 | Attribute / annotation formatters (`@endian`, `@interrupt`, `@abi`, `@link_section`, `@jump_table`, …) | S | 1 |
| FMT-9 | Doc-comment attachment pass (lift leading `///` / `/** */` onto the following item, with preserved column 0 anchoring) | S | 1 |
| FMT-10 | Match-arm alignment + unsafe-block field alignment | S | 1 |
| FMT-11 | `FormatOptions` v2 + `.pdxfmt` TOML config-file discovery + per-file ignore pragma | M | 1 |
| FMT-12 | CLI: glob expansion (`crates/**/*.pdx`), `--list-different`, workspace-wide default | S | 1 |
| FMT-13 | Corpus **idempotence** harness: `format(format(src)) == format(src)` sweeping `accept/` + `examples/` + `pdx/` | M | 1 |
| FMT-14 | Corpus **parser-equivalence** harness: `parse(format(src)) ≡ parse(src)` (AST-structural eq) sweeping `accept/` | M | 1 |
| FMT-15 | Corpus **comment-preservation** harness: every `Trivia` span text present in output, in attachment order | M | 1 |
| FMT-16 | LSP delta: switch `textDocument/formatting` to AST path + add `textDocument/rangeFormatting` | S | 1 |
| FMT-17 | Final acceptance: round-trip stability + comment preservation + parser-equivalence green on full corpus, with CI note (local-only per feedback_paideia_os_no_cicd — but paideia-as runs via `tools/build.sh` + `cargo test -p paideia-fmt`) | M | 1 |

**Total: 18 atomic issues, ~22 waves** (FMT-1, FMT-4, FMT-5 each span 2 waves).

---

## 5. Dependency chain

```
FMT-0  (style spec)        ─┐
                            ├─▶ FMT-2  (Doc IR) ──┐
FMT-1  (CST + trivia) ──────┤                     ├─▶ FMT-3 (walker scaffold)
                            │                     │        │
                            │                     │        ├─▶ FMT-4  (items)
                            │                     │        ├─▶ FMT-5  (exprs)
                            │                     │        ├─▶ FMT-6  (types)
                            │                     │        ├─▶ FMT-7  (patterns)
                            │                     │        ├─▶ FMT-8  (attributes)
                            │                     │        ├─▶ FMT-9  (doc-comments)
                            │                     │        └─▶ FMT-10 (alignment)
                            │                     │
                            │                     └─▶ FMT-11 (config)
                            │                              │
                            │                              └─▶ FMT-12 (CLI)
                            │
                            └──────────────────────▶ FMT-13, FMT-14, FMT-15
                                                           │
                                                           └─▶ FMT-16 (LSP delta)
                                                                   │
                                                                   └─▶ FMT-17 (final)
```

**Head of chain (parallel-safe starts):**
- **FMT-0** (design — no code deps).
- **FMT-1** (CST — only lexer + parser deps; can land alongside FMT-0).

**First serial bottleneck:** FMT-2 depends on FMT-0 (style decisions like
`max_width`); FMT-3 depends on both FMT-1 and FMT-2.

**Parallelizable mid-chain:** FMT-4, FMT-5, FMT-6, FMT-7, FMT-8, FMT-9, FMT-10
can all land in parallel once FMT-3 is in. Follows the paideia-as parallel-
softarch-dispatch pattern (per `feedback_paideia_as_parallel_changelog.md`).

**Blocking final:** FMT-13/14/15 are prerequisites for FMT-17's green-light.

---

## 6. Risks and open design decisions

1. **CST vs annotated-AST.** Two viable shapes for FMT-1:
   (a) build a **separate CST** with every token+trivia preserved (rust-analyzer
   style); (b) **annotate the AST** with a sparse `trivia_map: HashMap<NodeId,
   Vec<Trivia>>` + source string. **Recommendation: (b).** Reuses the arena,
   does not duplicate the parser, and the formatter never needs to rewrite
   tokens — only to decide where comments re-attach. This is the "pretty-
   printer + comment anchor" approach from gofmt. Decide in FMT-1.

2. **Max line width.** No prior art in paideia-as. **Recommendation: 100
   columns.** Rust ecosystem default; Linux kernel moved from 80→100; our
   `.pdx` surface has long effect-row + capability-row suffixes
   (`!{RawMem, IO} @{paideia.raw_mem, paideia.io}`) that will routinely blow
   80. Decide in FMT-0 and bake into default `FormatOptions`.

3. **Idempotence vs stability.** `format(format(src)) == format(src)` (second-
   pass stability) is the minimum. `format(parse(fmt(src))) == format(src)`
   (full round-trip) is the real target. Spell out both in acceptance in
   FMT-17.

4. **Macro-expansion directives + quote/splice.** `MacroExpandDirective`,
   `Quote`, and `Splice` nodes have no runtime semantics in the formatter — they
   just need faithful source reconstruction. Low risk; covered in FMT-4/5 as
   plain variants.

5. **Error recovery.** When `parse_source_file` returns `Err`, the formatter
   must fall back to the current text-level pass (do not refuse to format).
   Spelled out in FMT-3 acceptance.

6. **Determinism.** Every pass must be deterministic (no `HashMap` iteration
   order, no system-time dependency). Spelled out in FMT-13 acceptance.

---

## 7. Verdict

The crate is **not a scaffold** (CLI + LSP wiring exists; 185 LOC of real
rules exist) but is **not an incrementally-extensible foundation** either —
the AST walk, trivia carriage, and layout IR are 100% net-new. Treat this as
a **ground-up implementation that keeps the existing text-level pass as an
error-recovery fallback**. 18 atomic issues over roughly 22 waves lands a
complete, corpus-verified `.pdx` formatter.
