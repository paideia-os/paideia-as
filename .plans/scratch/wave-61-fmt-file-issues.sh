#!/usr/bin/env bash
# Wave 61 — paideia-fmt completion: issue filer script.
#
# NOT EXECUTABLE by design — review before running. Main runs after review.
# All issues go to paideia-os/paideia-as. Each carries area:fmt + size:*.
# Round label chosen at filing time (defaults to round:v0.33 below;
# bump before running if the next-wave bundle has moved on).
#
# Prereq: gh auth status must be clean, repo paideia-os/paideia-as must exist.

set -euo pipefail

REPO="paideia-os/paideia-as"
ROUND="round:v0.33"   # <-- confirm before running; see paideia-as/labels

# -----------------------------------------------------------------------------
# FMT-0 — Style spec + syntax-reference stub
# -----------------------------------------------------------------------------
gh issue create --repo "$REPO" \
  --title "[FMT-0] fmt style spec + resurrect syntax-reference stub" \
  --label "area:fmt,type:doc,size:S,round:fmt-completion" \
  --body "$(cat <<'EOF'
## Summary
Create the formatter style specification and resurrect the top-level
`design/toolchain/syntax-reference.md` placeholder that several design docs
already link to but which is missing from the mainline tree.

## Deliverable
- `design/toolchain/fmt-style.md` (new) with concrete values for:
  - `max_width = 100`
  - `indent_width = 2`
  - `use_tabs = false`
  - `max_blank_lines = 2`
  - `trailing_comma_in_blocks = always`
  - `align_match_arms = on`
  - `align_unsafe_fields = on`
  - `glyph_style = Preserve` (default; conversion stays a code-action)
  - config file name: `.pdxfmt` (TOML)
  - per-file pragma: `// pdxfmt: ignore-file`
- `design/toolchain/syntax-reference.md` (new) — minimal top-level stub that
  cites §2.1 whitespace, §2.2 comments, §3.4 reserved words sections already
  referenced by other docs, and defers detail to the paideia-as-ast crate.

## Acceptance
- Both files exist and are linked from `design/toolchain/README.md` (or the
  nearest equivalent index).
- No code changes.

## Dependencies
None. First item in Wave 61.
EOF
)"

# -----------------------------------------------------------------------------
# FMT-1a — Trivia-attached CST wrapper (lexer side)
# -----------------------------------------------------------------------------
gh issue create --repo "$REPO" \
  --title "[FMT-1a] trivia harvest: lexer-side wrapper for comment carriage" \
  --label "area:fmt,type:feature,size:M,round:fmt-completion" \
  --body "$(cat <<'EOF'
## Summary
New `paideia-fmt::cst` module that drives `paideia_as_lexer::Lexer` and
captures `Lexer::take_trivia()` after each `next_token`, producing a
`Vec<(TokenIndex, Vec<Trivia>, &str)>` keyed by the token the trivia
precedes.

## Deliverable
- `crates/paideia-fmt/src/cst.rs` (new) with:
  - `pub struct TriviaStream { pub tokens: Vec<Token>, pub leading_trivia: Vec<Vec<Trivia>>, pub trailing_trivia: Vec<Trivia> }`
  - `pub fn tokenize_with_trivia(file: FileId, source: &SourceText, sink: &mut dyn DiagnosticSink) -> TriviaStream`
- No touch to parser or lexer crates.

## Acceptance
- Unit tests over `examples/01_hello.pdx` and `examples/09_effects.pdx`:
  reconstruct the comment text at each attachment point.
- `trailing_trivia` captures the final `\n` after the last top-level item.

## Dependencies
FMT-0 (style spec informs defaults but not blocked on content).
EOF
)"

# -----------------------------------------------------------------------------
# FMT-1b — Trivia-to-node attachment pass
# -----------------------------------------------------------------------------
gh issue create --repo "$REPO" \
  --title "[FMT-1b] trivia-to-node attachment: leading / inner / trailing" \
  --label "area:fmt,type:feature,size:M,round:fmt-completion" \
  --body "$(cat <<'EOF'
## Summary
Post-parse pass that walks the AST and attaches the `TriviaStream` from
FMT-1a onto `NodeId`s via byte-range comparison against node spans.

## Deliverable
- `crates/paideia-fmt/src/trivia_map.rs` (new) with:
  - `pub struct TriviaMap { pub leading: HashMap<NodeId, Vec<Trivia>>, pub inner: HashMap<NodeId, Vec<Trivia>>, pub trailing: Vec<Trivia> }`
  - `pub fn attach_trivia(arena: &AstArena, root: NodeId, stream: &TriviaStream) -> TriviaMap`
- Deterministic iteration (sorted-key wrapper if needed; no raw HashMap in
  output).

## Acceptance
- For every file in `tests/accept/`, every lexer `Trivia` ends up in exactly
  one of `leading[node]`, `inner[node]`, or `trailing`.
- Unit tests verify leading-doc-comment attachment to the following item
  across all 21 files in `examples/`.

## Dependencies
FMT-1a.
EOF
)"

# -----------------------------------------------------------------------------
# FMT-2 — Doc IR + layout engine
# -----------------------------------------------------------------------------
gh issue create --repo "$REPO" \
  --title "[FMT-2] Doc IR + deterministic layout engine" \
  --label "area:fmt,type:feature,size:M,round:fmt-completion" \
  --body "$(cat <<'EOF'
## Summary
Introduce a Wadler-style `Doc` IR and a layout engine that fits it into a
`max_width` budget. Hand-rolled (no `pretty` crate) to keep paideia-as supply-
chain minimal and output deterministic.

## Deliverable
- `crates/paideia-fmt/src/doc.rs` (new) with:
  - `pub enum Doc { Nil, Text(String), Line, SoftLine, HardLine, Nest(usize, Box<Doc>), Group(Box<Doc>), Concat(Vec<Doc>) }`
  - `pub fn layout(doc: &Doc, max_width: usize) -> String`
  - Helpers: `text`, `line`, `softline`, `hardline`, `nest`, `group`, `concat`.
- Unit tests for break-at-max-width, nested groups, soft vs hard lines,
  determinism.

## Acceptance
- `layout(doc, 100)` is deterministic across three runs.
- Group with soft lines fits on one line when under width, breaks when over.
- Hard line always breaks regardless of width.

## Dependencies
FMT-0 (confirms `max_width` default).
EOF
)"

# -----------------------------------------------------------------------------
# FMT-3 — AST walker scaffold + text-level fallback
# -----------------------------------------------------------------------------
gh issue create --repo "$REPO" \
  --title "[FMT-3] walker scaffold + parse-fail fallback" \
  --label "area:fmt,type:feature,size:S,round:fmt-completion" \
  --body "$(cat <<'EOF'
## Summary
`paideia_fmt::format_ast(source, opts) -> String` wires FMT-1b and FMT-2
together. Dispatches per AST variant to a `unimplemented!(variant)` stub so
FMT-4..10 can light up variants individually.

## Deliverable
- `crates/paideia-fmt/src/walk.rs` (new).
- `pub fn format_ast(source: &str, opts: &FormatOptions) -> String` that:
  1. Tokenizes with trivia (FMT-1a).
  2. Parses with `paideia_as_parser::Parser::parse_source_file()`.
  3. If parse fails, falls back to the existing line-level `format()`.
  4. Else attaches trivia (FMT-1b) and walks the tree, dispatching per
     `ItemData` / `ExprData` / `TypeData` / `PatternData` variant to
     `unimplemented!()` stubs for now.

## Acceptance
- On a parse-failing input, output is identical to the current
  `paideia_fmt::format` text pass.
- On a parse-succeeding input, walker reaches every top-level item and
  surfaces the stub `unimplemented!()` with the variant name.

## Dependencies
FMT-1b, FMT-2.
EOF
)"

# -----------------------------------------------------------------------------
# FMT-4a — ItemData top-level items
# -----------------------------------------------------------------------------
gh issue create --repo "$REPO" \
  --title "[FMT-4a] item formatters: Module, Structure, Signature, Let, Fn, Enum, Struct, Trait, Impl" \
  --label "area:fmt,type:feature,size:M,round:fmt-completion" \
  --body "$(cat <<'EOF'
## Summary
Nine top-level `ItemData` variants. Each gets a `Doc`-returning formatter and
2-5 unit tests over hand-crafted snippets lifted from `examples/`.

## Deliverable
- Entries in `crates/paideia-fmt/src/walk.rs` for each variant.
- Unit tests inlined per variant.

## Acceptance
- `format_ast` returns a non-panicking `Doc` for every file in `examples/`
  (even if later-missing variants in bodies still panic — those are FMT-4b
  and FMT-5*).
- Doc comments at item sites preserved verbatim (uses FMT-9's result once
  FMT-9 lands; stub `attach_doc_comment` placeholder until then).

## Dependencies
FMT-3.
EOF
)"

# -----------------------------------------------------------------------------
# FMT-4b — ItemData remaining items
# -----------------------------------------------------------------------------
gh issue create --repo "$REPO" \
  --title "[FMT-4b] item formatters: Functor, PackedStruct, GpuContext, Timeline, EndianAttr, MacroExpandDirective, AtomicItem, FunctorAttr" \
  --label "area:fmt,type:feature,size:M,round:fmt-completion" \
  --body "$(cat <<'EOF'
## Summary
The remaining eight `ItemData` variants (specialty surfaces).

## Deliverable
- Entries in `walk.rs` per variant.
- Fixtures from `tests/accept/` and `tests/pdx/` covering each specialty
  item surface.

## Acceptance
- Every `ItemData` variant has a formatter — zero `unimplemented!` at the
  item level after this lands.

## Dependencies
FMT-3. Parallel-safe with FMT-4a.
EOF
)"

# -----------------------------------------------------------------------------
# FMT-5a — ExprData primary / postfix / call / binop / prefix
# -----------------------------------------------------------------------------
gh issue create --repo "$REPO" \
  --title "[FMT-5a] expr formatters: primary / postfix / call / binop / prefix" \
  --label "area:fmt,type:feature,size:M,round:fmt-completion" \
  --body "$(cat <<'EOF'
## Summary
About 20 `ExprData` variants covering literals, path refs, calls, binary ops
(with precedence-driven parentheses), cast, field access, index, ref/deref,
tuple, record, segment-prefix.

## Deliverable
- Variant-by-variant entries in `walk.rs`.
- Precedence table lifted from `paideia_as_parser::precedence`.

## Acceptance
- Round-trip stability (`format(format(src)) == format(src)`) over all
  files in `tests/accept/` that use only these variants.

## Dependencies
FMT-3. Parallel-safe with FMT-5b.
EOF
)"

# -----------------------------------------------------------------------------
# FMT-5b — ExprData control flow + binders
# -----------------------------------------------------------------------------
gh issue create --repo "$REPO" \
  --title "[FMT-5b] expr formatters: match / if / loop / while / for / let-in / block / lambda / handler / unsafe / pack / unpack / quote / splice / pattern-match / memref" \
  --label "area:fmt,type:feature,size:M,round:fmt-completion" \
  --body "$(cat <<'EOF'
## Summary
~19 `ExprData` variants covering control flow and binders. Includes match-arm
block layout (alignment is deferred to FMT-10), unsafe-block field layout,
let-in, lambda `fn (…) -> …`.

## Deliverable
- Variant-by-variant entries in `walk.rs`.
- Unsafe-block field order enforced: `effects`, `capabilities`,
  `justification`, `block`.

## Acceptance
- Every `ExprData` variant has a formatter — zero `unimplemented!` at the
  expr level after this + FMT-5a land.

## Dependencies
FMT-3. Parallel-safe with FMT-5a.
EOF
)"

# -----------------------------------------------------------------------------
# FMT-6 — TypeData formatters
# -----------------------------------------------------------------------------
gh issue create --repo "$REPO" \
  --title "[FMT-6] type formatters: 16 TypeData variants + effect rows + capability rows" \
  --label "area:fmt,type:feature,size:M,round:fmt-completion" \
  --body "$(cat <<'EOF'
## Summary
All 16 `TypeData` variants, plus the `!{Eff1, Eff2}` and `@{Cap1, Cap2}`
row-type surface.

## Deliverable
- Entries in `walk.rs` for each variant.
- Dedicated helpers for effect rows and capability rows (used by Fn type
  and by Fn item signature).

## Acceptance
- Zero `unimplemented!` at the type level.
- Round-trip stability for `examples/09_effects.pdx` and
  `examples/15_unsafe.pdx`.

## Dependencies
FMT-3. Parallel-safe with FMT-5*/FMT-7.
EOF
)"

# -----------------------------------------------------------------------------
# FMT-7 — PatternData formatters
# -----------------------------------------------------------------------------
gh issue create --repo "$REPO" \
  --title "[FMT-7] pattern formatters: 11 PatternData variants" \
  --label "area:fmt,type:feature,size:M,round:fmt-completion" \
  --body "$(cat <<'EOF'
## Summary
All 11 `PatternData` variants: Wildcard, Literal, Binding (incl. `mut`),
Tuple, Record, Enum, Range, Or, Guard, Ref, Rest.

## Deliverable
- Entries in `walk.rs` for each variant.
- Record-destructuring layout: single-line if ≤ max_width, else one field per
  line with trailing comma.

## Acceptance
- Zero `unimplemented!` at the pattern level.
- Round-trip stability for `examples/05_patterns.pdx`.

## Dependencies
FMT-3. Parallel-safe with FMT-5*/FMT-6.
EOF
)"

# -----------------------------------------------------------------------------
# FMT-8 — Attribute / annotation formatters
# -----------------------------------------------------------------------------
gh issue create --repo "$REPO" \
  --title "[FMT-8] attribute formatters: @endian, @interrupt, @abi, @link_section, @jump_table, @packed, @repr" \
  --label "area:fmt,type:feature,size:S,round:fmt-completion" \
  --body "$(cat <<'EOF'
## Summary
Attribute block formatters. Multiple attributes stack one-per-line in a
deterministic order (lexical order of attribute name).

## Deliverable
- `crates/paideia-fmt/src/attrs.rs` (new).
- Covers every variant of `ItemAttribute` enum from `paideia-as-ast`.

## Acceptance
- Round-trip over `tests/accept/` samples that carry attributes.

## Dependencies
FMT-4a.
EOF
)"

# -----------------------------------------------------------------------------
# FMT-9 — Doc-comment attachment
# -----------------------------------------------------------------------------
gh issue create --repo "$REPO" \
  --title "[FMT-9] doc-comment attachment pass" \
  --label "area:fmt,type:feature,size:S,round:fmt-completion" \
  --body "$(cat <<'EOF'
## Summary
Promote leading `///` / `/** */` trivia on an item node to a formal doc slot
emitted immediately above the item.

## Deliverable
- Helper `fn emit_doc_comments(trivia_map: &TriviaMap, node: NodeId, buf: &mut Doc)`.

## Acceptance
- Doc text preserved verbatim.
- No blank line between a doc block and its item.
- Non-doc comments (`//`, `/* */`) stay inline where they were.

## Dependencies
FMT-1b, FMT-4a.
EOF
)"

# -----------------------------------------------------------------------------
# FMT-10 — Match-arm + unsafe-block alignment
# -----------------------------------------------------------------------------
gh issue create --repo "$REPO" \
  --title "[FMT-10] alignment: match arms + unsafe-block fields" \
  --label "area:fmt,type:feature,size:S,round:fmt-completion" \
  --body "$(cat <<'EOF'
## Summary
Column-align `=>` in match arms when every arm pattern+body fits on one line.
Column-align `:` in unsafe-block field block.

## Deliverable
- Alignment post-pass over the match-arm `Doc` list and the unsafe-block
  field `Doc` list.
- Toggleable via `FormatOptions::align_match_arms` / `align_unsafe_fields`.

## Acceptance
- `examples/05_patterns.pdx` match arms align.
- `examples/15_unsafe.pdx` unsafe-field colons align.
- Multi-line arms disable alignment for that arm group (no partial align).

## Dependencies
FMT-5b, FMT-4a.
EOF
)"

# -----------------------------------------------------------------------------
# FMT-11 — FormatOptions v2 + .pdxfmt config file
# -----------------------------------------------------------------------------
gh issue create --repo "$REPO" \
  --title "[FMT-11] FormatOptions v2 + .pdxfmt TOML config discovery" \
  --label "area:fmt,type:feature,size:M,round:fmt-completion" \
  --body "$(cat <<'EOF'
## Summary
Expand `FormatOptions` with every knob from `fmt-style.md` and add a
TOML-based config-file loader with workspace-up discovery.

## Deliverable
- `FormatOptions { max_width, indent_width, use_tabs, max_blank_lines, glyph_style, align_match_arms, align_unsafe_fields, trailing_comma_in_blocks }`.
- `pub fn load_options_for(path: &Path) -> FormatOptions` — walks upward
  from the file's directory to the first `.pdxfmt`, parses TOML, falls back
  to defaults on absence.
- `// pdxfmt: ignore-file` pragma at the top of a file disables formatting
  for that file (identity pass).

## Acceptance
- Unit tests for discovery, parse, pragma.
- CLI respects discovered config.

## Dependencies
FMT-4a (FormatOptions type is live by then).
EOF
)"

# -----------------------------------------------------------------------------
# FMT-12 — CLI: glob + list-different
# -----------------------------------------------------------------------------
gh issue create --repo "$REPO" \
  --title "[FMT-12] CLI: glob expansion + --list-different + workspace default" \
  --label "area:fmt,type:feature,size:S,round:fmt-completion" \
  --body "$(cat <<'EOF'
## Summary
Make the CLI usable at project scale: glob patterns, list-different, and a
no-arg workspace-wide default.

## Deliverable
- `paideia-as fmt crates/**/*.pdx` works (via `glob` crate — reuse workspace
  dep).
- `--list-different` prints newline-separated changed paths; exit 1 if any.
- Zero-arg `paideia-as fmt` formats every `.pdx` tracked by git in the
  current workspace.

## Acceptance
- Three new integration tests in `crates/paideia-as/tests/`.

## Dependencies
FMT-11.
EOF
)"

# -----------------------------------------------------------------------------
# FMT-13 — Idempotence corpus harness
# -----------------------------------------------------------------------------
gh issue create --repo "$REPO" \
  --title "[FMT-13] corpus idempotence harness" \
  --label "area:fmt,type:test,size:M,round:fmt-completion" \
  --body "$(cat <<'EOF'
## Summary
Sweep the `.pdx` corpus and assert `format(format(src)) == format(src)` for
every file.

## Deliverable
- `crates/paideia-fmt/tests/corpus_idempotence.rs`.
- Scans `tests/accept/`, `examples/`, `tests/pdx/` relative to workspace root.
- Prints every failing path with a unified diff.

## Acceptance
- All currently-accepted corpus files pass.
- Any new corpus file auto-included on next run.

## Dependencies
FMT-4b, FMT-5b, FMT-6, FMT-7, FMT-8, FMT-9, FMT-10.
EOF
)"

# -----------------------------------------------------------------------------
# FMT-14 — Parser-equivalence corpus harness
# -----------------------------------------------------------------------------
gh issue create --repo "$REPO" \
  --title "[FMT-14] corpus parser-equivalence harness" \
  --label "area:fmt,type:test,size:M,round:fmt-completion" \
  --body "$(cat <<'EOF'
## Summary
For every file in `tests/accept/`, assert that `parse(src)` and
`parse(format(src))` produce AST trees that are **structurally equal**
(ignoring `NodeId` identity, comparing variant tree + payload fields).

## Deliverable
- `crates/paideia-fmt/tests/corpus_parse_eq.rs`.
- Structural-eq walker over `AstArena` (compare variants + leaf payloads;
  compare children by recursive call, not by NodeId value).

## Acceptance
- All `tests/accept/` files pass.
- Mismatches surface with a human-readable path + first-differing node id
  + variant on both sides.

## Dependencies
FMT-13.
EOF
)"

# -----------------------------------------------------------------------------
# FMT-15 — Comment-preservation corpus harness
# -----------------------------------------------------------------------------
gh issue create --repo "$REPO" \
  --title "[FMT-15] corpus comment-preservation harness" \
  --label "area:fmt,type:test,size:M,round:fmt-completion" \
  --body "$(cat <<'EOF'
## Summary
For every file in `tests/accept/` + `examples/`, assert that every `Trivia`
with a comment kind present in the input is present in the output, and that
consecutive comment runs preserve their relative order.

## Deliverable
- `crates/paideia-fmt/tests/corpus_comments.rs`.

## Acceptance
- Every comment text survives round-trip verbatim.
- Fails with a clear diff when a comment is dropped or reordered.

## Dependencies
FMT-14.
EOF
)"

# -----------------------------------------------------------------------------
# FMT-16 — LSP delta
# -----------------------------------------------------------------------------
gh issue create --repo "$REPO" \
  --title "[FMT-16] LSP: switch textDocument/formatting to AST path + add rangeFormatting" \
  --label "area:fmt,area:lsp,type:feature,size:S,round:fmt-completion" \
  --body "$(cat <<'EOF'
## Summary
Repoint the LSP formatting handlers at the new `format_ast` entry point
(with text-level fallback preserved for parse failures) and add
`textDocument/rangeFormatting` that formats the smallest spanning item.

## Deliverable
- `crates/paideia-lsp/src/server.rs` — switch handler.
- New `textDocument/rangeFormatting` capability declaration.

## Acceptance
- Existing LSP integration tests pass unchanged.
- New rangeFormatting test formats a single `fn` item without touching the
  rest of the file.

## Dependencies
FMT-15.
EOF
)"

# -----------------------------------------------------------------------------
# FMT-17 — Final acceptance
# -----------------------------------------------------------------------------
gh issue create --repo "$REPO" \
  --title "[FMT-17] final acceptance: round-trip + comment-preservation + parser-equivalence green" \
  --label "area:fmt,type:test,size:M,round:fmt-completion,tracking" \
  --body "$(cat <<'EOF'
## Summary
Close the paideia-fmt completion wave. Confirm all three corpus harnesses
green, build clean, and declare v1-ready.

## Acceptance
- `cargo test -p paideia-fmt` green.
- `tools/build.sh` clean on main after this wave's merge.
- `design/toolchain/fmt-style.md` promoted from draft to v1.
- CHANGELOG entry naming every FMT-* issue closed in this wave.
- Workspace version bumped (per `feedback_paideia_as_version_discipline.md`).

## Dependencies
FMT-13, FMT-14, FMT-15, FMT-16.
EOF
)"

echo "All 21 Wave 61 fmt issues drafted (FMT-0, 1a, 1b, 2, 3, 4a, 4b, 5a, 5b, 6..17). Review the printed URLs before resuming the loop."
