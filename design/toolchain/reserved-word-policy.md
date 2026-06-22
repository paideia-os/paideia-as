# Reserved-Word Policy

This document describes paideia-as's policy for reserved words, contextual keywords, and the
decision criteria for adding or removing words from reservation.

## Overview

paideia-as maintains a set of **71 reserved words** (as of phase 1) that cannot be used as
identifiers. Reserved words are declared once in `crates/paideia-as-lexer/src/token.rs` and
used by:

- The lexer: to classify tokens as `TokenKind::KwXxx` during lexical analysis.
- The parser: to expect specific keywords in grammar productions.
- Documentation and tools: exposed via `paideia_as_lexer::reserved::{RESERVED_WORDS, is_reserved()}`.

## Current Reserved Words (71 total)

Organized by category:

- **Item declarations** (12): `let`, `fn`, `module`, `signature`, `structure`, `functor`,
  `effect`, `capability`, `extern`, `import`, `export`, `pub`.
- **Control flow** (13): `if`, `else`, `match`, `when`, `do`, `with`, `loop`, `while`,
  `for`, `break`, `continue`, `return`, `yield`.
- **Effect system (action blocks)** (1): `action`.
- **Type system** (12): `type`, `enum`, `struct`, `record`, `trait`, `impl`, `where`,
  `forall`, `ordered`, `linear`, `affine`, `unrestricted`.
- **Effect system (handlers)** (3): `perform`, `resume`, `finally`.
- **Substructural / unsafe** (7): `unsafe`, `move`, `borrow`, `consume`, `drop`, `own`,
  `mut`.
- **Literals and constants** (5): `true`, `false`, `null`, `Self`, `self`.
- **Memory and addressing** (4): `sizeof`, `alignof`, `offsetof`, `asm`.
- **Module operations** (3): `in`, `as`, `use`.
- **Future reservations** (11): `abstract`, `async`, `await`, `coroutine`, `deriving`,
  `dyn`, `implicit`, `lemma`, `proof`, `reflect`, `virtual`.

## Contextual Keywords

A **contextual keyword** is an identifier that carries semantic meaning in a specific
parsing context but does not occupy a dedicated token kind. Contextual keywords allow
user code to reclaim common words for use as variable or function names in non-semantic
contexts.

### Current Contextual Keywords

1. **`macro`** — used to introduce macro declarations; detected via `expect_contextual("macro")`
   in `parse_macro.rs:35`.
2. **`quote`** — used to introduce quote (anti-quotation) expressions; detected via
   contextual peek in `parse_primary.rs`.
3. **`uninit`** — used in type annotations for uninitialized values; detected in type-parsing
   contexts.
4. **`op`** — used to introduce handler operation arms; detected in `parse_handler.rs`.
5. **`handle`** — used to bind effect handlers and introduce handler-value expressions;
   detected via `expect_contextual("handle")` in `parse_handler.rs` and contextual peek
   in `parse_primary.rs`.

## Cost of Reservation

Reserving a word imposes costs:

- **User friction**: the word cannot be used as a variable, function, or field name without
  quoting (e.g., `r#handle`).
- **Documentation burden**: every reserved word must be listed in `RESERVED_WORDS`, documented
  in syntax specs, and maintained across tools.
- **Downstream tooling**: LSP servers, editor syntax highlighters (helix, emacs, tree-sitter),
  and test frameworks must all be updated to recognize the new keyword.

Therefore, **reservation is preferred only for words that truly govern grammar structure**
(e.g., `if`, `fn`, `let`). Words that appear in specific contexts where disambiguation is
possible should be contextual keywords instead.

## Decision Protocol: Reservation vs. Contextual

When deciding whether to reserve a new word:

1. **Is the word a necessary grammar point?**
   - If **yes**: reserve it. Example: `if`, `fn`, `let` are unavoidable in expressions and
     declarations; they must be reserved.
   - If **no**: proceed to step 2.

2. **Is the word always preceded or followed by unambiguous context?**
   - If **yes**: make it contextual. Example: `handle` appears after `with` or at the start
     of a block-like expression, where an identifier peek + text check is sufficient.
   - If **no**: consider reservation.

3. **Is there a lightweight way to detect the word in isolation?**
   - If **yes** (context-free lookahead via span text): make it contextual. Example: `macro`
     is always followed by a name identifier, allowing a simple text comparison.
   - If **no**: consider reservation.

## Adding or Removing Contextual Keywords

### Adding a Contextual Keyword

1. Identify the parsing function(s) where the keyword is used.
2. Add a helper like `Parser::expect_contextual("keyword")` (or use an existing one) that:
   - Expects a `TokenKind::Ident`.
   - Extracts the token's text from the source using its span.
   - Compares it to the expected string.
   - Emits `P0100` ("unexpected token") on mismatch.
3. Update all call sites to use the contextual check instead of expecting a reserved keyword.
4. Remove the reserved keyword from `RESERVED_WORDS`, the `TokenKind` enum, and `keyword_kind()`.
5. Update documentation (this file, syntax-reference.md, etc.).
6. Add parser tests for both the positive case (correct context) and error cases (wrong context).

### Removing a Contextual Keyword (Promoting to Reserved)

1. Add a new `TokenKind::KwXxx` variant and update `keyword_kind()`.
2. Add the word to `RESERVED_WORDS` and update counts.
3. Update the lexer and parser to use the reserved keyword.
4. Notify downstream tooling maintainers (LSP, editor configs, tree-sitter).
5. Update documentation.

## Downstream Synchronization

When a reserved-word or contextual-keyword change is made:

- **Helix**: update `runtime/queries/paideia-as/highlights.scm` to recognize new keywords.
- **Emacs**: update `modes/paideia-as-mode.el` (if maintained) to include the word in
  `paideia-as-keywords`.
- **tree-sitter**: update `tree-sitter-paideia-as` grammar rules and keyword lists.
- **LSP**: update `crates/paideia-as-lsp/src/` to handle the new keyword in semantic analysis.

These updates should accompany the PR that introduces the keyword change and should be
tested in CI before merging.

## References

- `crates/paideia-as-lexer/src/token.rs`: canonical enum and `keyword_kind()` function.
- `crates/paideia-as-lexer/src/reserved.rs`: public `is_reserved()` predicate.
- `crates/paideia-as-parser/src/parser.rs`: `expect_contextual()` helper.
- `crates/paideia-as-parser/src/parse_macro.rs`: example contextual-keyword detection.
- `design/syntax-reference.md` §3.4: user-facing reserved-word spec.
