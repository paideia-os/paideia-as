# UNI-12 — paideia-as-unicode consumer wire-up audit

Wave: UNI-W5. Issue: paideia-as#1599.

Scope: survey the three consumer crates — `paideia-as-shell-lex`,
`paideia-as-shell-ast`, `paideia-as-shell-completion` — for existing
`paideia_as_unicode::` traffic and for hand-rolled string handling that
pre-dates the UNI-W1..W4 surfaces (UNI-1 norm, UNI-2 case, UNI-3
segment, UNI-4 ident). Adopt the new surfaces where correctness or
clarity improves. Do NOT refactor pub API.

---

## crates/paideia-as-shell-lex

### Current usage

* `src/lexer.rs:28`  — `use paideia_as_unicode::Utf8Decoder;`
* `src/lexer.rs:102` — `Utf8Decoder::new(&bytes[self.pos..])` (decode_at)
* `src/lexer.rs:133` — `Utf8Decoder::new(&bytes[abs..])` (peek2)

The lexer runs its state machine over `Utf8Decoder`. UTF-8 handling is
already centralised through the substrate; no churn needed there.

### Hand-rolled surfaces

* `src/lexer.rs:254-263`  — `is_ident_start` / `is_ident_continue`:

  ```rust
  // UAX#31 ID_Start (approximated): ASCII letter or `_`, or any
  // non-ASCII alphabetic char. Precise UAX#31 tables are `unicode-ident`
  // crate territory; we defer to `char::is_alphabetic` here to keep the
  // dep list small and rely on R220's `unicode-ident` for the strict
  // check at R221.M5.
  fn is_ident_start(c: char) -> bool {
      c == '_' || c.is_ascii_alphabetic() || (!c.is_ascii() && c.is_alphabetic())
  }
  fn is_ident_continue(c: char) -> bool {
      is_ident_start(c) || c.is_ascii_digit()
          || (!c.is_ascii() && c.is_alphanumeric())
  }
  ```

  The comment explicitly flags this as a UAX #31 approximation deferred
  to a dedicated layer. UNI-4 landed that layer — `is_xid_start` /
  `is_xid_continue` from `unicode-ident` with the same `#![forbid(unsafe_code)]`
  discipline, backing UAX #31 verbatim.

### Wire-up

* Replace both predicates with the UNI-4 surface.
  * `is_ident_start`  →  `c == '_' || paideia_as_unicode::is_xid_start(c)`
    (XID_Start does not include `_`, so the manual `_` branch stays).
  * `is_ident_continue` → `paideia_as_unicode::is_xid_continue(c)` alone
    (XID_Continue includes `_` and ASCII digits).

### Test added

* `tests/uni_w5_xid_ident.rs` — three fixtures:
  * `uni-w5-lex-01` — a Catalan ell with middle-dot ident
    (`l·l` + ASCII suffix). `is_alphanumeric('·')` is false, so the old
    predicate stops at `l`, truncating the ident; UAX #31 XID_Continue
    includes U+00B7 so the new predicate accepts the whole run.
  * `uni-w5-lex-02` — Greek letter ident (`αβγ`). Both predicates
    accept this; regression guard against overshoot.
  * `uni-w5-lex-03` — digit-first non-ident (`1abc`). Both predicates
    reject the leading digit; regression guard against admitting
    numeric starts.

---

## crates/paideia-as-shell-ast

### Current usage

* `src/parser/mod.rs:122` — `paideia_as_unicode::nfc_normalize(nfc_src)`
  (debug-only sanity check that the caller handed a pre-normalised
  source).
* `src/nfc_map.rs:35` — `use unicode_normalization::UnicodeNormalization;`
  (direct dep for `.nfc()` iterator access and future
  canonical-combining-class lookup).
* `src/nfc_map.rs:100` — `src.nfc().collect::<String>()` for the
  non-ASCII NFC path.

### Hand-rolled surfaces

None of the hand-rolled surfaces identified in UNI-W1..W4's surface set
appear in shell-ast:

* No `.to_lowercase()` / `.to_uppercase()`.
* No `is_alphabetic()` / `is_alphanumeric()` identifier gating (the
  parser trusts shell-lex's token kinds).
* No width or grapheme math.

### Wire-up

* `src/nfc_map.rs:100` — centralise the NFC call via the substrate:
  swap `src.nfc().collect::<String>()` for
  `paideia_as_unicode::nfc_normalize(src)`. Same semantics (the
  substrate's non-ASCII path is `UnicodeNormalization::nfc(...).collect()`
  and the ASCII fast path is already handled a few lines above by
  `src.is_ascii()` early-return, so the extra ASCII check inside
  `nfc_normalize` is a dead branch here but zero-cost in release).
  Drops the direct `use` of `unicode_normalization::UnicodeNormalization`
  on the compose path; the crate still keeps the dep because
  nfc_map's planned char-level combining-class work needs it.

### Test added

* `tests/uni_w5_nfc_centralised.rs` — one fixture:
  * `uni-w5-ast-01` — build an `NfcMap` over a decomposed `cafe\u{0301}`
    input and assert that the normalised text equals
    `paideia_as_unicode::nfc_normalize("cafe\u{0301}")` byte-for-byte.
    Guards the delegation (any future divergence in the two paths
    surfaces here immediately).

---

## crates/paideia-as-shell-completion

### Current usage

* `src/lib.rs:61` — `use paideia_as_unicode::nfc_normalize;`
* `src/lib.rs:453,566` — NFC-normalise the pre-cursor prefix before
  tokenising (correctness-critical).

### Hand-rolled surfaces

* `src/matching.rs:88-97` — case-insensitive prefix tier uses
  `.to_lowercase()`:

  ```rust
  let lowered_prefix = prefix.to_lowercase();
  let lowered_candidate = candidate.to_lowercase();
  if lowered_candidate.starts_with(&lowered_prefix) {
      return Some(CASE_INSENSITIVE_BASE - cand_char_len);
  }
  ```

  `.to_lowercase()` is simple-casing-only; UAX #44 full case equivalence
  needs `CaseFolding.txt`'s `C+F` rows (German `ß` → `ss`, Turkish
  dotted/dotless I/i, several Greek letters). UNI-2 landed those rows
  behind `fold_default` / `fold(_, CaseFoldLocale)` /
  `eq_ignore_case_unicode`. The ranker is semantically asking "does
  `prefix` fold-equal a prefix of `candidate`", which is exactly what
  `fold_default` enables.

* `src/matching.rs:73,125` — `candidate.chars().count()` and
  `prefix.chars().count()` as length-penalty inputs. These are
  **character counts**, not display widths; UNI-9 `str_width_in` would
  change scoring on CJK (one char = two cells) in a way that the M3
  ranker explicitly does NOT want — see the module header's
  "character-vs-byte accounting" rationale, which pins character
  counts. Skip.

* `src/lib.rs:508`, `src/lib.rs:998-1009` — `is_path_char` uses byte-level
  ASCII matching. The helper's doc explicitly limits its remit to a
  conservative ASCII subset; replacing with any Unicode-aware
  predicate would broaden path-char beyond the M6 contract. Skip.

### Wire-up

* `src/matching.rs:88-97` — replace Tier 2's `.to_lowercase()`-based
  comparison with `paideia_as_unicode::fold_default`:

  ```rust
  let folded_prefix    = paideia_as_unicode::fold_default(prefix);
  let folded_candidate = paideia_as_unicode::fold_default(candidate);
  if folded_candidate.starts_with(&folded_prefix) {
      return Some(CASE_INSENSITIVE_BASE - cand_char_len);
  }
  ```

  `fold_default` has its own ASCII fast path (byte-local
  `c | 0x20`), so the hot `.to_lowercase()` call-site keeps its
  cheap ASCII behaviour.

### Test added

* `tests/uni_w5_fold_match.rs` — three fixtures:
  * `uni-w5-cmp-01` — `score_match("ß", "SS")` succeeds under UNI-2
    full-case folding (both fold to `ss`); fails under
    `.to_lowercase()` (`"ß".to_lowercase() == "ß"`, no prefix match).
  * `uni-w5-cmp-02` — `score_match("I", "ı")` fails under default
    locale (Default fold: `I` → `i`, `ı` → `ı`, no prefix match);
    guards against accidentally pulling the Turkic fold onto the
    default path.
  * `uni-w5-cmp-03` — `complete()` with an engine whose command list
    is `["straße"]` and a request source `"STRA"` surfaces one
    Tier-2 Candidate. Guards the ranker-to-complete integration.

---

## Opportunities skipped

* **shell-completion `chars().count()` → `str_width_in` (UNI-9).**
  The matcher's length penalty is explicitly character-based (see
  `matching.rs` module header). Switching to UAX #11 display width
  would change the ranker's output on CJK (`str_width_in("日本") == 4`
  vs `"日本".chars().count() == 2`) with no R228.M3 fixture asking
  for that behaviour. Deferred until the REPL popup renderer needs
  display-aligned ranking.
* **shell-completion path-char Unicode-awareness.** `is_path_char` is
  a byte-ASCII subset by design (see M6 docstring); broadening is a
  future milestone, not a wire-up.
* **shell-lex Utf8Decoder "wraps `&str`" — call-site is already using
  the substrate.** No churn.
* **shell-ast parser identifier gating.** The parser consumes
  `TokenKind::Ident(name)` from shell-lex and never re-validates. The
  check lives at the lexer boundary; wiring UNI-4 there (above)
  already captures the semantic.
* **UNI-6 fingerprint, UNI-14 fuzz surfaces.** No consumer currently
  invokes a per-string fingerprint or an adversarial fuzz entry point;
  those surfaces land into the shell at R229.M8 / fuzz infrastructure
  wave, which is out of scope here.
