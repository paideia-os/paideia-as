# Wave 61 — paideia-fmt issue plan

Companion to `.plans/scratch/wave-61-fmt-survey.md`. Each row is **one landable
wave** (atomic, size S or M; the two L entries from §4 are split into A/B
sub-issues here).

Issue titles use the prefix **`[FMT-N]`** where N is the handle. All issues
file to `paideia-os/paideia-as`. Each issue carries `area:fmt` + a `size:*`
label + a `round:v0.*` round label chosen by main at filing time.

## Issue table

| # | Handle | Title | Size | Depends on | Deliverable |
|---|---|---|---|---|---|
| 1 | FMT-0 | Style spec + resurrect syntax-reference stub | S | — | `design/toolchain/fmt-style.md` + top-level `design/toolchain/syntax-reference.md` placeholder referencing fmt-style; defines max-width (100), indent (2 spaces), blank-line policy, trailing-comma policy, alignment policy, glyph policy, config file name `.pdxfmt` |
| 2 | FMT-1a | Trivia-attached CST wrapper (lexer-side take + spans) | M | FMT-0 | New `paideia-fmt::cst` module. Harvests `Lexer::take_trivia()` after each `next_token`, builds `Vec<(Span, TriviaKind, &str)>` keyed by preceding token index. **Does not touch the parser.** Pure wrapper around `Lexer` + source text. Unit tests over `examples/01_hello.pdx` and `examples/09_effects.pdx` |
| 3 | FMT-1b | Trivia-to-node attachment pass | M | FMT-1a | Walk the AST post-parse; for every `NodeId` with a known span, bind the trivia whose range falls strictly before the node's start to that node as **leading trivia**; trivia strictly inside the node's range becomes **inner trivia**; the final trailing trivia (after the last top-level item) becomes `source_file_trailing`. New `TriviaMap { leading: HashMap<NodeId, Vec<Trivia>>, inner: HashMap<NodeId, Vec<Trivia>>, trailing: Vec<Trivia> }` with deterministic iteration |
| 4 | FMT-2 | `Doc` IR + layout engine | M | FMT-0 | New `paideia-fmt::doc` module with `Doc::{Nil, Text, Line, SoftLine, HardLine, Nest(usize, Doc), Group(Doc), Concat(Vec<Doc>)}` and `layout(doc, max_width) -> String`. Deterministic. No external pretty crate. Unit tests for break-at-max-width and nested groups |
| 5 | FMT-3 | AST walker scaffold + text-level fallback | S | FMT-1b, FMT-2 | New `paideia-fmt::walk` module. `pub fn format_ast(source: &str, opts: &FormatOptions) -> String` — tokenize, parse, walk, layout. On parse error, fall back to the existing line-level pass (preserving comment preservation at least by file contents). Does not implement any variant yet — just dispatch stubs that `panic!("unimplemented {variant}")` so FMT-4..10 light up individually |
| 6 | FMT-4a | ItemData formatters — top-level items (Module, Structure, Signature, Let, Fn, Enum, Struct, Trait, Impl) | M | FMT-3 | 9 variants. Each lands with 2-5 doctest-style unit tests per variant |
| 7 | FMT-4b | ItemData formatters — remaining items (Functor, PackedStruct, GpuContext, Timeline, EndianAttr, MacroExpandDirective, AtomicItem, FunctorAttr) | M | FMT-3 | 8 variants |
| 8 | FMT-5a | ExprData formatters — primary / postfix / call / binop / prefix (literals, path refs, calls, binary ops, cast, field, index, ref, deref, tuple, record, segment-prefix) | M | FMT-3 | ~20 variants |
| 9 | FMT-5b | ExprData formatters — control flow + binders (match, if, loop, while, for, let-in, block, lambda, handler, unsafe, pack, unpack, quote, splice, pattern-match, memref) | M | FMT-3 | ~19 variants. Includes match-arm layout decision (block-level, not aligned yet — alignment is FMT-10) |
| 10 | FMT-6 | TypeData formatters (16 variants) incl. effect rows + capability rows + memref | M | FMT-3 | Covers `!{Eff1, Eff2}` and `@{Cap1}` row syntax; function-type arrow, tuple, array, slice, ptr/ref types, record/enum/trait types, lifetime params, linear-class marker |
| 11 | FMT-7 | PatternData formatters (11 variants) | M | FMT-3 | Wildcard, literal, binding (incl. `mut`), tuple, record, enum, range, or-pattern, guard, ref, rest. Covers irrefutable let-bindings and refutable match arms |
| 12 | FMT-8 | Attribute / annotation formatters | S | FMT-4a | `@endian(be\|le)`, `@interrupt(x86_64)`, `@abi(sysv\|ms\|…)`, `@link_section("…")`, `@jump_table`, `@packed`, `@repr(…)`. Deterministic ordering; multi-attr stacks one-per-line |
| 13 | FMT-9 | Doc-comment attachment | S | FMT-1b, FMT-4a | Lift `///` / `/** */` from leading trivia onto the following item's doc slot; preserve content verbatim; collapse trailing blank line between doc and item |
| 14 | FMT-10 | Match-arm + unsafe-block field alignment | S | FMT-5b, FMT-4a | Match arms whose patterns are single-line get `=>` column-aligned within an arm group; unsafe-block fields (`effects:`, `capabilities:`, `justification:`, `block:`) get `:` column-aligned. Toggleable; default on |
| 15 | FMT-11 | `FormatOptions` v2 + `.pdxfmt` config file | M | FMT-4a | TOML config: `max_width`, `indent_width`, `use_tabs`, `max_blank_lines`, `glyph_style`, `align_match_arms`, `trailing_comma_in_blocks`. Discovery: walk up from file to nearest `.pdxfmt`, fall through to defaults. Per-file ignore pragma: `// pdxfmt: ignore-file` top-of-file |
| 16 | FMT-12 | CLI: glob + list-different | S | FMT-11 | `paideia-as fmt crates/**/*.pdx` expansion via `glob` crate (reuse workspace dep); `--list-different` prints newline-separated changed paths and exits 0 (clean) or 1 (changes present); workspace-root default `paideia-as fmt` → `**/*.pdx` under git-tracked files |
| 17 | FMT-13 | Idempotence corpus harness | M | FMT-4b, FMT-5b, FMT-6, FMT-7, FMT-8, FMT-9, FMT-10 | New `crates/paideia-fmt/tests/corpus_idempotence.rs` sweeping `tests/accept/`, `examples/`, `tests/pdx/`. Asserts `format(format(f)) == format(f)` for every `.pdx` file. Reports any failing path by name |
| 18 | FMT-14 | Parser-equivalence corpus harness | M | FMT-13 | New `crates/paideia-fmt/tests/corpus_parse_eq.rs` sweeping `tests/accept/`. Asserts the AST of `parse(f)` and `parse(format(f))` are **structurally** equal (ignore `NodeId` identity, compare the variant tree). Reuses `paideia-as-ast::visit` or hand-rolled walker |
| 19 | FMT-15 | Comment-preservation corpus harness | M | FMT-14 | For every file in the harness, every lexer `Trivia` span text in the input must appear in the output (set equality on comment text; order equality on attachment). Fails loudly if any comment vanishes |
| 20 | FMT-16 | LSP delta: AST path + range formatting | S | FMT-15 | Switch `textDocument/formatting` to `paideia_fmt::format_ast` (fallback to text pass on parse error). Add `textDocument/rangeFormatting` that formats the smallest spanning item at that range |
| 21 | FMT-17 | Final acceptance | M | FMT-13, FMT-14, FMT-15, FMT-16 | Green across full corpus; `tools/build.sh` clean; CHANGELOG entry; version bump; `design/toolchain/fmt-style.md` upgraded from draft to v1 with the final defaults; declare paideia-fmt v1-ready for paideia-os adoption |

**Count: 21 issues (because FMT-1, FMT-4, FMT-5 split into A/B), ~22 waves.**

## Acceptance criteria — FMT-17 (final)

- **Round-trip stability**: for every `.pdx` file in `tests/accept/`,
  `tests/pdx/`, and `examples/`:
  `format(format(src), opts) == format(src, opts)`.
- **Parser equivalence**: for every file in `tests/accept/`:
  `ast_structural_eq(parse(src), parse(format(src)))`.
- **Comment preservation**: every `Trivia` text present in the input source is
  present in `format(src)`, and the ordering of consecutive comment runs is
  preserved.
- **Determinism**: three successive runs on the same input produce
  byte-identical output.
- **Graceful fallback**: files that fail to parse are passed through the
  text-level fallback (whitespace + glyph normalization) without raising.
- **No regression** in `cmd_fmt` CLI behaviour: all existing CLI tests pass
  unchanged.
- **No regression** in LSP: all existing LSP integration tests pass unchanged.

## Risks / open design decisions (reserved to FMT-0)

1. Max line width (recommend **100**).
2. Indent width (recommend **2 spaces** — matches every `.pdx` example in-tree).
3. Alignment default (recommend **on** for match arms + unsafe-block fields).
4. Trailing-comma policy in multi-line block/record/tuple (recommend **always
   add trailing comma**; makes multi-line diffs minimal).
5. Glyph style default (recommend **Preserve** — do not autocorrect user
   choice; conversion stays a code-action in the LSP).

## Rollout notes

- Dispatch pattern: solo softarch for FMT-0, FMT-1a, FMT-1b, FMT-2, FMT-3
  (chain). Then **parallel softarch** (per `feedback_paideia_as_parallel_
  changelog.md`) for FMT-4a + FMT-4b + FMT-5a + FMT-5b + FMT-6 + FMT-7 + FMT-8
  + FMT-9 + FMT-10 — these touch disjoint variants and can all merge in one
  batch with a consolidated CHANGELOG. Then solo for FMT-11, FMT-12, and the
  three corpus harnesses, then FMT-16, FMT-17.
- Debugger runs after every softarch per
  `feedback_paideia_os_loop_shape.md` and `feedback_debugger_every_iteration.md`.
- Builds remain main-only per `feedback_no_background_builds.md`.
