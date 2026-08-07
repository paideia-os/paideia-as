# klog-migrate — Tokenizer-Driven UART→klog Rewriter

**Status**: v0.20 (post-SELF-HOST, unreleased)

**Issue**: [paideia-as#1272](https://github.com/paideia-os/paideia-as/issues/1272)

**Cross-repo prereq**: [paideia-os#717](https://github.com/paideia-os/paideia-os/issues/717) — unblocks [paideia-os#704](https://github.com/paideia-os/paideia-os/issues/704).

## Overview

`klog-migrate` is a workspace-member binary under `tools/klog-migrate/` (peer of `tools/ddc` and `tools/pax-introspect`). It rewrites the direct-UART fingerprint

```
      lea rdi, [rip + <MSG_SYM>];
      call uart_puts;
```

into the structured-log fingerprint

```
      mov rdi, <LEVEL>;
      lea rsi, [rip + <SUBSYS_SYM>];
      lea rdx, [rip + <MSG_SYM>];
      call klog_s1;
```

across `.pdx` source files, reusing each `<MSG_SYM>` (already a NUL-terminated `.rodata` byte blob from `boot_stub*.S` or `.pdx` array-literal declarations) as the klog tag pointer. No new `.pdx` rodata is emitted; no byte counts are fabricated.

## Why a tool, not `sed`

paideia-os#699 saw workerbee produce a 2/5 byte-count-fabrication rate on the smaller `uart_put_hex` migration. Regex-driven substitutions over `.pdx` failed in two mechanical ways:

1. **Comment leakage**: `// call uart_puts inline description` matched and got rewritten inside comments.
2. **String-literal leakage**: paideia-as `justification: "…klog_s1 replaces call uart_puts…"` strings matched the pattern.

Both are impossible under a tokenizer-driven rewriter: the paideia-as lexer strips `//` line comments and `/* */` block comments as `Trivia`, and lexes string literals as a single `StringLit` token whose interior is never visited. Any `Ident("uart_puts")` token the tool observes is real code.

## Design

### Pipeline

```
.pdx source
  │
  ▼
paideia_as_lexer::Lexer::collect_tokens()      // strips trivia + strings
  │
  ▼
scan_pattern(tokens) → Vec<Match>              // 10..=12-token variable-width walk
  │
  ▼
Match::render(source, opts) → Replacement      // byte-range + new_text
  │
  ▼
apply_replacements(source, reps)               // right-to-left splice
  │
  ▼
migrated .pdx source
```

### Target pattern

10 mandatory tokens (positions 0–7, 9–10) plus 2 optional semicolons (8, 11),
in order, with any trivia (whitespace/comment) between adjacent tokens:

| # | Kind      | Text (case-sensitive)     | Required |
|---|-----------|---------------------------|----------|
| 0 | `Ident`   | `lea`                     | yes      |
| 1 | `Ident`   | `rdi`                     | yes      |
| 2 | `Comma`   | —                         | yes      |
| 3 | `LBracket`| `[`                       | yes      |
| 4 | `Ident`   | `rip`                     | yes      |
| 5 | `Plus`    | `+`                       | yes      |
| 6 | `Ident`   | *(msg-symbol, captured)*  | yes      |
| 7 | `RBracket`| `]`                       | yes      |
| 8 | `Semicolon`| `;`                      | **no** (#1273) |
| 9 | `Ident`   | `call`                    | yes      |
|10 | `Ident`   | `uart_puts`               | yes      |
|11 | `Semicolon`| `;`                      | **no** (#1273) |

Between 10 and 12 tokens total. The captured msg-symbol from position 6
becomes the tag pointer in the rewrite.

The two `Semicolon` positions are **optional** because `.pdx` accepts
newline-terminated statements alongside semicolon-terminated ones, and real
kernel sources (paideia-os/src/kernel/boot/kernel_main.pdx) mix both styles
freely. `#1273` fixed the original hard-required semicolons: they had been
silently skipping 54 valid migration targets in kernel_main.pdx. Fixtures
`no_semi_lea.pdx`, `no_semi_call.pdx`, `no_semi_both.pdx` (each with a
`.expected.pdx` peer) pin the three semicolon-optional variants.

Match extent: `tokens[0].span.byte_start()` .. last-consumed-token
`.span.byte_end()`. When both semicolons are present, the last token is
position 11 (`;`); when only the trailing one is elided, it is position 10
(`uart_puts`); when only the middle one is elided, it is position 11 (`;`);
when both are elided, it is position 10 again. This range does **not**
include the trailing newline; the tool re-inserts newlines, per-line
indentation, and normalises the output to the semicolon-terminated form in
the rendered replacement (so a semicolon-optional site becomes
semicolon-terminated after migration).

### Rewrite rendering

Given a match with captured symbol `SYM`, the rendered replacement is a four-line block:

```
{IND}mov rdi, {LEVEL};
{IND}lea rsi, [rip + {SUBSYS}];
{IND}lea rdx, [rip + {SYM}];
{IND}call klog_s1;
```

`{IND}` is the whitespace prefix of the source line containing `tokens[0]` (extracted by walking back from `tokens[0].span.byte_start()` to the preceding `\n` or start-of-file). This preserves the surrounding block's indentation exactly.

`{LEVEL}` is `--fail-level` if `SYM` matches `--fail-pattern` (default `(?i)(fail|err)`); otherwise `--level` (default `3` = `LEVEL_INFO`).

`{SUBSYS}` comes from `--subsys` (default `SUBSYS_BOOT`).

The tool never emits a trailing newline: the source range being replaced ended at the `;` of the `call uart_puts;` line, and the newline (if any) that followed remains part of the surrounding buffer.

### Splice discipline

All `Match`es are computed in a single forward pass, then applied **right-to-left** so that each replacement's byte offsets remain valid regardless of the length delta of later replacements. The tool bails with exit code 2 if any two matches overlap (invariant: a match ends at position `p`, and the next match must start ≥ `p`).

## CLI

```
paideia-as-klog-migrate <FILE> [OPTIONS]

Options:
  --level N              Level literal for non-fail messages    [default: 3]
  --subsys SYM           Symbol name for the SUBSYS argument    [default: SUBSYS_BOOT]
  --fail-level N         Level literal for fail messages        [default: 5]
  --fail-pattern REGEX   Regex against msg symbol name          [default: (?i)(fail|err)]
  --in-place             Write migrated source back to FILE
  --check                Exit 1 if FILE would change; 0 clean
  --diff                 Print unified diff to stderr
  -h, --help             Print help
  -V, --version          Print version
```

### Modes

- **Default** (`--in-place` false, `--check` false): print migrated source to stdout.
- **`--in-place`**: overwrite `FILE` iff there is at least one match. Exit 0. Idempotent on re-run.
- **`--check`**: exit 1 iff there is at least one match (i.e., FILE would change); exit 0 otherwise. Complements `paideia-as fmt --check`.
- **`--diff`**: additionally emits a unified diff to stderr. Composable with any of the above modes.

### Exit codes

| Code | Meaning |
|------|---------|
| 0    | Success (stdout printed, in-place write completed, or `--check` clean). |
| 1    | `--check` mode and FILE would change. |
| 2    | I/O error, invalid regex, malformed source, or overlapping matches (bug). |

### Warnings

The rewritten byte range (`tokens[0].byte_start()` .. last-consumed-token `.byte_end()`) covers the entire two-line pattern including any `//` trailing comment that sits **between** the two matched lines. Comments in that gap are silently absorbed by the rewrite. The tool detects this by scanning the replaced substring for `//` or `/*` and, for each hit, emits a stderr line of the form

```
paideia-as-klog-migrate: <FILE>:<LINE>: dropped trailing comment while migrating `<MSG_SYM>` — review by hand
```

The migration still completes; the warning is advisory. Callers that treat any warning as fatal can grep stderr for `dropped trailing comment`.

## Test surface

Layered from smallest to largest:

1. **Unit** (`tools/klog-migrate/src/scan.rs`):
   - Empty token stream → no matches.
   - Isolated `lea rdi, [rip + foo];` (no following call) → no matches.
   - Isolated `call uart_puts;` (no preceding lea) → no matches.
   - The full 12-token pattern → 1 match with captured symbol.
   - The 10-token semicolon-optional pattern (both `;` elided) → 1 match (#1273).
   - The 11-token variants (either `;` elided) → 1 match each (#1273).
   - Two adjacent patterns, mixed-terminator styles → 2 matches (#1273).

2. **Comment / string safety** (`tools/klog-migrate/src/scan.rs`):
   - `// lea rdi, [rip + foo]; call uart_puts;` — 0 matches.
   - `justification: "call uart_puts here"` — 0 matches.
   - Pattern immediately after a comment on the previous line — 1 match, indent preserved.

3. **Render** (`tools/klog-migrate/src/render.rs`):
   - Default level → `mov rdi, 3;`.
   - Fail-pattern hit (`some_fail_msg`) → `mov rdi, 5;`.
   - Fail-pattern miss (`_err_msg` with case-sensitive pattern) → default level.
   - Custom subsys → `lea rsi, [rip + SUBSYS_INT_];`.
   - Indent preserved: 6-space, tab, mixed.

4. **Splice** (`tools/klog-migrate/src/splice.rs`):
   - Single match, byte-exact substitution.
   - Two matches in same file, right-to-left ordering verified.
   - Overlapping matches → error.

5. **CLI** (`tools/klog-migrate/tests/cli.rs`, `assert_cmd`):
   - Stdout mode: default → migrated to stdout, source unchanged.
   - `--in-place`: file overwritten, second run is a no-op.
   - `--check`: exit 1 if would change, 0 if idempotent.
   - `--diff`: unified diff visible on stderr, primary output unaffected.

6. **Golden `.pdx` corpus** (`tools/klog-migrate/tests/fixtures/`):
   - `basic.pdx` / `basic.expected.pdx`: canonical two-line pattern.
   - `fail_pattern.pdx` / `fail_pattern.expected.pdx`: mixed OK / FAIL symbols.
   - `comment_safe.pdx`: pattern inside `//` — no rewrite (round-trip: input == output).
   - `trailing_comment.pdx`: rewrite absorbs a trailing `//` on the `lea` line — stderr warning fires.
   - `no_semi_lea.pdx` / `no_semi_lea.expected.pdx` (#1273): `;` after `]` omitted.
   - `no_semi_call.pdx` / `no_semi_call.expected.pdx` (#1273): `;` after `uart_puts` omitted.
   - `no_semi_both.pdx` / `no_semi_both.expected.pdx` (#1273): both `;` omitted.

## Non-goals (v0.20)

- **Multi-file batch mode**: v0.20 handles one file per invocation. Batch orchestration lives in the caller's `Makefile` / shell.
- **New rodata declaration**: the tool never emits `.pdx` `let` items. Callers who need brand-new tag symbols declare them by hand (rare; msg symbols already exist for every witness).
- **Higher-arity klog wrappers**: v0.20 targets `klog_s1` only. `klog_s1_x1` / `_x2` / `_x3` / `_x4` migrations (which fold `uart_put_hex` sequences) are deferred to v0.21; the token-scan module is designed so each new pattern is one additional `PatternKind` variant + one render function.
- **AST-level rewriter**: paideia-as parses `unsafe { block: { ... } }` payloads as opaque `IrKind::RawInstruction` / `IrKind::Unsafe` node sequences (Phase 7 m2-001), so the AST does not carry per-instruction navigation a rewriter would benefit from. When self-hosting adds an assembly-block AST (Tier 2), this tool becomes the reference for the AST-level pass.

## Extension model

Adding a new pattern:

```rust
// tools/klog-migrate/src/patterns.rs
pub enum PatternKind {
    UartPutsToKlogS1,           // v0.20
    UartPutHexToKlogS1X1,       // v0.21 (planned)
    // ...
}
```

Each `PatternKind` provides:

- `fn matches(window: &[Token], source: &str) -> Option<Match>`
- `fn render(m: &Match, opts: &RenderOpts, source: &str) -> String`

The scan driver iterates patterns in declaration order and picks the longest match at each cursor position (LR-style disambiguation). Overlaps between different pattern kinds are still caught by the splice-time invariant.
