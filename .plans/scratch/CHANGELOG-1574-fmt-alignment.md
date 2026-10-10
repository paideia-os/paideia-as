# CHANGELOG — FMT-10 alignment polish (paideia-as#1574)

## Summary

Resolves the §11.1 ambiguity flagged in FMT-5b: adopts **OPTION C
adaptive** for unsafe-block field alignment (pad all four keys by
default, demote `block:` to single-space-after-colon only when its
body is single-line AND padding would overflow `max_width`). Lifts the
previously hard-coded alignment-column gate in match-arm `=>`
alignment to use `ctx.opts.max_width`, and surfaces both alignment
rules on new `FormatOptions` flags so the forthcoming `.pdxfmt`
`[fmt] align = "off"` key (FMT-11) can toggle them.

## Changed files

- `crates/paideia-fmt/src/lib.rs`
  - `FormatOptions` grows two bool fields:
    - `align_match_arms` (default `true`) — gate for the §8.2 `=>`
      padder.
    - `align_unsafe_fields` (default `true`) — gate for the §11.1
      four-key padder.
- `crates/paideia-fmt/src/format_expr.rs`
  - `format_match`: alignment-off gate reads `ctx.opts.max_width`
    instead of hard-coded `100`, and short-circuits to the pad-less
    branch when `!ctx.opts.align_match_arms`.
  - `format_unsafe`: `pad_for` becomes `pad_for_key(key, align)`.
    `block:` now participates in alignment with the other three keys
    by default (OPTION C). An adaptive demote branch — triggered when
    the block body is single-line AND the padded first line would
    exceed `max_width` (`BLOCK_INDENT_EST = 10` conservative outer
    indent) — falls back to single-space-after-colon for `block:`
    only.
  - New tests (`#[cfg(test)]`):
    - `unsafe_option_c_pads_block_key_with_multi_line_body`
    - `unsafe_option_c_block_single_space_when_align_off`
    - `unsafe_option_c_demotes_block_when_single_line_body_would_overflow`
    - `match_align_match_arms_off_single_space`
    - `match_align_adapts_to_longest_arm_width`
    - `match_align_drops_when_longest_pattern_exceeds_budget`
    - `match_arm_body_indent_uniform_across_arms`
    - `unsafe_option_c_both_flags_default_true`
  - Test helper `fmt_expr_opts(src, &opts)` added; `fmt_expr_width`
    now routes through it so a single parse/attach pipeline serves
    both width-override and alignment-override tests.
- `design/toolchain/paideia-fmt-style-spec.md`
  - §11.1 rewritten: OPTION C adopted explicitly, with worked example
    for the padded form and the demote fallback. Cross-references
    the two new `FormatOptions` flags.

## Variants NOT covered

- `.pdxfmt` round-trip of `[fmt] align = "off"` lands in FMT-11
  (paideia-as#1575). The flags are wired into `FormatOptions` today
  so FMT-11 is a pure config-parser diff.
- OPTION C demote budget uses a conservative `BLOCK_INDENT_EST = 10`
  for the outer indent estimate (we don't thread column position
  through Doc-build). A future tightening (route current column into
  `FormatContext`) would let the formatter demote only when the
  real line would overflow, not when the pessimistic estimate would.

## Additional alignment opportunities discovered

- **Macro rules (§12.1 cross-reference §8.2)**: `MacroDecl` rule
  arrows also warrant `=>` alignment. Not covered here — macro
  dispatch is still on the walker fallback (bucket-B spec called this
  out). File as FMT-10a follow-up.
- **Handler-value `op`/`finally` arms**: `format_handler_value` emits
  `op name => body` and `finally => cleanup` but does not align the
  `=>` across arms. Could reuse the `measure_doc_flat` → pad idiom
  from `format_match`. Deferred — niche surface, no corpus pressure.
- **Record field initialisers**: `RecordCons` (not yet structural,
  on `fallback_slice`) will want `name: value` alignment analogous
  to unsafe-field alignment when it lands. Flag it when FMT-9 or a
  dedicated ticket claims `RecordCons`.

## Approx LOC

- `lib.rs`: +~22 (two struct fields + default + docs)
- `format_expr.rs`: +~160 (format_unsafe rewrite + 8 new tests + test
  helper split)
- `paideia-fmt-style-spec.md`: +~30

## Build

Not run — main should invoke `bash tools/build.sh` and re-invoke me
with the error tail if it fails.
