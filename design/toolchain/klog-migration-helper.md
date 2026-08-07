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
scan_pattern(tokens) → Vec<Match>              // 11-token window walk
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

11 tokens, in order, with any trivia (whitespace/comment) between adjacent tokens:

| # | Kind      | Text (case-sensitive) |
|---|-----------|-----------------------|
| 0 | `Ident`   | `lea`                 |
| 1 | `Ident`   | `rdi`                 |
| 2 | `Comma`   | —                     |
| 3 | `LBracket`| `[`                   |
| 4 | `Ident`   | `rip`                 |
| 5 | `Plus`    | `+`                   |
| 6 | `Ident`   | *(msg-symbol, captured)* |
| 7 | `RBracket`| `]`                   |
| 8 | `Semicolon`| `;`                  |
| 9 | `Ident`   | `call`                |
|10 | `Ident`   | `uart_puts`           |
|11 | `Semicolon`| `;`                  |

12 tokens total. The captured msg-symbol from position 6 becomes the tag pointer in the rewrite.

Match extent: `tokens[0].span.byte_start()` .. `tokens[11].span.byte_end()`. This range does **not** include the trailing newline; the tool re-inserts newlines and per-line indentation in the rendered replacement.

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

The rewritten byte range (`tokens[0].byte_start()` .. `tokens[11].byte_end()`) covers the entire two-line pattern including any `//` trailing comment that sits **between** the two matched lines. Comments in that gap are silently absorbed by the rewrite. The tool detects this by scanning the replaced substring for `//` or `/*` and, for each hit, emits a stderr line of the form

```
paideia-as-klog-migrate: <FILE>:<LINE>: dropped trailing comment while migrating `<MSG_SYM>` — review by hand
```

The migration still completes; the warning is advisory. Callers that treat any warning as fatal can grep stderr for `dropped trailing comment`.

## Test surface

Layered from smallest to largest:

1. **Unit** (`crates/klog-migrate/src/scan.rs`):
   - Empty token stream → no matches.
   - Isolated `lea rdi, [rip + foo];` (no following call) → no matches.
   - Isolated `call uart_puts;` (no preceding lea) → no matches.
   - Exactly the 12-token pattern → 1 match with captured symbol.
   - Two adjacent patterns → 2 matches with correct spans.

2. **Comment / string safety** (`crates/klog-migrate/src/scan.rs`):
   - `// lea rdi, [rip + foo]; call uart_puts;` — 0 matches.
   - `justification: "call uart_puts here"` — 0 matches.
   - Pattern immediately after a comment on the previous line — 1 match, indent preserved.

3. **Render** (`crates/klog-migrate/src/render.rs`):
   - Default level → `mov rdi, 3;`.
   - Fail-pattern hit (`some_fail_msg`) → `mov rdi, 5;`.
   - Fail-pattern miss (`_err_msg` with case-sensitive pattern) → default level.
   - Custom subsys → `lea rsi, [rip + SUBSYS_INT_];`.
   - Indent preserved: 6-space, tab, mixed.

4. **Splice** (`crates/klog-migrate/src/splice.rs`):
   - Single match, byte-exact substitution.
   - Two matches in same file, right-to-left ordering verified.
   - Overlapping matches → error.

5. **CLI** (`crates/klog-migrate/tests/cli.rs`, `assert_cmd`):
   - Stdout mode: default → migrated to stdout, source unchanged.
   - `--in-place`: file overwritten, second run is a no-op.
   - `--check`: exit 1 if would change, 0 if idempotent.
   - `--diff`: unified diff visible on stderr, primary output unaffected.

6. **Golden `.pdx` corpus** (`tools/klog-migrate/tests/fixtures/`):
   - `basic.pdx` / `basic.expected.pdx`: single-line pattern.
   - `same_line.pdx` / `same_line.expected.pdx`: `lea ...; call uart_puts;` on one source line.
   - `fail_pattern.pdx` / `fail_pattern.expected.pdx`: mixed OK / FAIL symbols.
   - `comment_safe.pdx` / `comment_safe.expected.pdx`: pattern inside `//` — no rewrite.
   - `string_safe.pdx` / `string_safe.expected.pdx`: pattern inside `justification:` string — no rewrite.

## Non-goals (v0.20)

- **Multi-file batch mode**: v0.20 handles one file per invocation. Batch orchestration lives in the caller's `Makefile` / shell.
- **New rodata declaration**: the tool never emits `.pdx` `let` items. Callers who need brand-new tag symbols declare them by hand (rare; msg symbols already exist for every witness).
- **Higher-arity klog wrappers**: v0.20 targets `klog_s1` only. `klog_s1_x1` / `_x2` / `_x3` / `_x4` migrations (which fold `uart_put_hex` sequences) are deferred to v0.21; the token-scan module is designed so each new pattern is one additional `PatternKind` variant + one render function.
- **AST-level rewriter**: paideia-as parses `unsafe { block: { ... } }` payloads as opaque `IrKind::RawInstruction` / `IrKind::Unsafe` node sequences (Phase 7 m2-001), so the AST does not carry per-instruction navigation a rewriter would benefit from. When self-hosting adds an assembly-block AST (Tier 2), this tool becomes the reference for the AST-level pass.

## Extension model

Adding a new pattern:

```rust
// crates/klog-migrate/src/patterns.rs
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
