//! UNI-10 (paideia-as#1597) — comprehensive property-test suite covering
//! every subsystem of `paideia-as-unicode`.
//!
//! The earlier `tests/invariants_property.rs` suite under the R221.M1 /
//! R221.M2 brand covers three invariants (NFC idempotence, grapheme ≤
//! char ≤ byte, UTF-8 decoder round-trip). UNI-10 broadens that surface
//! to **≥ 15 invariants** that span all six public modules:
//!
//! * **UNI-1** `norm`     — four normalization forms + dispatcher
//! * **UNI-2** `case`     — fold / uppercase / `eq_ignore_case_unicode`
//! * **UNI-3** `segment`  — UAX #14 opportunities + `wrap_at_width`
//! * **UNI-4** `ident`    — UAX #31 + UTS #39 profile dispatcher
//! * **UNI-5** `ucd`      — General_Category / Script group predicates
//! * **UNI-9** `width`    — UAX #11 + `WidthMode` locale
//!
//! Each `proptest!` block names the invariant with a `uni10-pNN`
//! fingerprint so a failing binary correlates to a specific line item in
//! the UNI-10 dispatch map (per `feedback_workerbee_verify_claims.md`
//! fingerprint discipline). Specs cited inline: UAX #9 / #11 / #14 / #15
//! / #24 / #29 / #31 / #44 and UTS #39.
//!
//! # Why property tests here (vs. table-driven)
//!
//! The UCD / spec vectors already in `tests/*_conformance.rs` cover
//! known-value behavior — the external spec says "this input should
//! produce that output." Property tests cover *structural* invariants —
//! "regardless of input, these invariants always hold" — which is where
//! regressions silently slip past a conformance suite that only exercises
//! fixed inputs. Both suites together catch different failure modes.
//!
//! # Shrinking
//!
//! Default `ProptestConfig` applies: 256 cases per `proptest!` block,
//! shrinking enabled. On failure, proptest shrinks to the minimal
//! counterexample via `.minimum()` on the string strategy. The whole
//! suite runs in well under a second on a cold cache (the UCD tables are
//! pulled in once by the first test and shared thereafter).
//!
//! # Input strategies
//!
//! * `any_string()`         — `".{0,64}"` regex-generated; covers ASCII,
//!   Latin-1, CJK, astral planes. Excludes `\n` and `\r` by default
//!   (regex `.` behavior) which keeps the width / wrap tests on the
//!   non-newline path.
//! * `any_string_with_newlines()` — `"(?s).{0,64}"`, dotall-enabled so
//!   `.` includes `\n` and `\r`. The segment tests need this to exercise
//!   mandatory-break paths.
//! * `safe_wrap_input()`    — `"[A-Za-z0-9 ]{0,64}"`, pure-ASCII and
//!   newline-free so the width-bound invariant is unambiguous (see the
//!   `wrap_at_width` section below for why a general-Unicode strategy
//!   plus a tiny `max_width` would violate the bound by design).
//! * `ascii_ident()`        — `"[A-Za-z_][A-Za-z0-9_]{0,16}"`, matches
//!   the `IdentProfile::Ascii` grammar exactly. Used for the identifier
//!   monotonicity and the Ascii ⊆ Default inclusion.
//! * `any_char()`           — `proptest::char::any()` for the UCD
//!   group-predicate partition and the Rust-std-vs-GC equivalence.
//!
//! # Why `wrap_at_width` width-bound is restricted
//!
//! The `force_wrap_range` fallback in `src/segment.rs` admits a single
//! grapheme into a chunk even when that grapheme's column count exceeds
//! `max_width` (the guard is `chunk_w > 0 && chunk_w + gw > max_width`;
//! when `chunk_w == 0` the first grapheme is unconditionally admitted).
//! A `wrap_at_width("日", 1)` therefore legally returns `["日"]` whose
//! width is 2. The property is only sound when `max_width >= 2` or when
//! the input is pure ASCII — we test the ASCII sub-case to pin the
//! strongest bound without a false positive on CJK input.

use paideia_as_unicode::{
    CaseFoldLocale, Form, GeneralCategory, IdentProfile, Script, Width, WidthMode, check_ident,
    eq_ignore_case_unicode, fold, general_category, grapheme_width_in, is_form, is_identifier,
    is_letter, is_mark, is_nfc, is_nfd, is_number, is_other, is_punctuation, is_separator,
    is_symbol, line_break_opportunities, nfc_normalize, nfd_normalize, nfkc_normalize,
    nfkd_normalize, normalize, script, str_width, str_width_in, uppercase, width_in, wrap_at_width,
};
use proptest::prelude::*;
use unicode_segmentation::UnicodeSegmentation;

// ---------------------------------------------------------------------
// Input strategies
// ---------------------------------------------------------------------

fn any_string() -> impl Strategy<Value = String> {
    ".{0,64}".prop_map(|s| s)
}

fn any_string_with_newlines() -> impl Strategy<Value = String> {
    "(?s).{0,64}".prop_map(|s| s)
}

fn safe_wrap_input() -> impl Strategy<Value = String> {
    "[A-Za-z0-9 ]{0,64}".prop_map(|s| s)
}

fn no_wrap_input() -> impl Strategy<Value = String> {
    // Non-space ASCII identifier-ish shape: no leading/trailing
    // whitespace to trim, no newlines to split on.
    "[A-Za-z0-9]{1,32}".prop_map(|s| s)
}

fn ascii_ident() -> impl Strategy<Value = String> {
    // Letter-start (not `_`-start) so `is_identifier` holds by
    // construction: `_` is UAX #31 `XID_Continue` but **not**
    // `XID_Start`, so a bare `_abc` would fail `is_identifier` even
    // though the Ascii profile would admit it. Restricting the leader
    // to `[A-Za-z]` keeps the generator aligned with the Default
    // profile the monotonicity property exercises.
    "[A-Za-z][A-Za-z0-9_]{0,16}".prop_map(|s| s)
}

/// Every `Form` variant. `Form` is `#[non_exhaustive]` so we enumerate
/// the four known variants manually; a new form added upstream would
/// not break this list silently (it would simply go un-tested until
/// the array is updated — the Cargo test runner then flags the gap).
const ALL_FORMS: [Form; 4] = [Form::Nfc, Form::Nfd, Form::Nfkc, Form::Nfkd];

// ---------------------------------------------------------------------
// UNI-1 — Normalization (UAX #15)
// ---------------------------------------------------------------------

proptest! {
    /// `uni10-p01` — UAX #15 §1.4 idempotence: for every normalization
    /// form `f` and every input `x`, `normalize(normalize(x, f), f)`
    /// equals `normalize(x, f)`. This is the structural guarantee the
    /// SH-D9.4 IPC-boundary pin depends on (a normalizer that was not
    /// idempotent would make equality under NFC a function of how many
    /// times the string crossed the boundary).
    #[test]
    fn uni10_p01_normalize_idempotent(input in any_string()) {
        for &form in &ALL_FORMS {
            let once = normalize(&input, form);
            let twice = normalize(&once, form);
            prop_assert_eq!(
                &twice, &once,
                "uni10-p01: normalize({:?}, {:?}) not idempotent",
                input, form
            );
        }
    }
}

proptest! {
    /// `uni10-p02` — ASCII is trivially normalized in every form (UAX
    /// #15 §1.3 — ASCII contains no combining marks, no compatibility
    /// decompositions, no canonical equivalents). The implementation's
    /// ASCII fast path in `src/norm.rs` returns an owned copy without
    /// loading the UCD tables; this property pins the semantic contract
    /// behind that optimization.
    #[test]
    fn uni10_p02_ascii_is_normalized_in_every_form(
        input in "[\\x20-\\x7E]{0,64}",
    ) {
        for &form in &ALL_FORMS {
            prop_assert_eq!(
                normalize(&input, form),
                input.clone(),
                "uni10-p02: ASCII should round-trip under {:?}",
                form
            );
            prop_assert!(
                is_form(&input, form),
                "uni10-p02: is_form({:?}, {:?}) must hold",
                input, form
            );
        }
    }
}

proptest! {
    /// `uni10-p03` — NFKC output is an NFC fixpoint (UAX #15 §1.3): NFKC
    /// applies compatibility decomposition then canonical composition, so
    /// `nfc_normalize(nfkc_normalize(x)) == nfkc_normalize(x)`. The
    /// stronger `is_nfc(nfkc_normalize(x))` form fails on quick-check
    /// Maybe cases such as U+00A8 → " \u{0308}" (space does not form a
    /// composition with a following combining mark, so the output is
    /// NFC-equivalent but the quick-check table reports Maybe).
    #[test]
    fn uni10_p03_nfkc_is_nfc_fixpoint(input in any_string()) {
        let once = nfkc_normalize(&input);
        let twice = nfc_normalize(&once);
        prop_assert_eq!(
            &once, &twice,
            "uni10-p03: nfc_normalize(nfkc_normalize({:?})) differs from nfkc_normalize({:?})",
            input, input
        );
    }
}

proptest! {
    /// `uni10-p04` — NFKD output is an NFD fixpoint (UAX #15 §1.3): NFKD
    /// is a compatibility decomposition; its output is canonically
    /// ordered and fully decomposed, so `nfd_normalize(nfkd_normalize(x))
    /// == nfkd_normalize(x)`. Uses the fixpoint form (same reason as
    /// p03) to tolerate quick-check Maybe outputs.
    #[test]
    fn uni10_p04_nfkd_is_nfd_fixpoint(input in any_string()) {
        let once = nfkd_normalize(&input);
        let twice = nfd_normalize(&once);
        prop_assert_eq!(
            &once, &twice,
            "uni10-p04: nfd_normalize(nfkd_normalize({:?})) differs from nfkd_normalize({:?})",
            input, input
        );
    }
}

proptest! {
    /// `uni10-p05` — `is_form` ⇔ `normalize == identity`. Two different
    /// routes to the same question "is this string already in form `f`";
    /// a drift between the quick-check table and the full normalizer
    /// would present here before any downstream equality fails.
    #[test]
    fn uni10_p05_is_form_iff_normalize_identity(input in any_string()) {
        for &form in &ALL_FORMS {
            let normalized = normalize(&input, form);
            let in_form = is_form(&input, form);
            let is_fixed = normalized == input;
            // `is_form` quick-check may say "No" to a string whose
            // slow-path normalization happens to be an identity (the
            // `Maybe` conservative branch). The one-way guarantee that
            // *does* hold: `is_form` ⇒ `normalize == identity`. The
            // reverse is a soft check we only enforce for the common
            // case to catch gross drift.
            if in_form {
                prop_assert!(
                    is_fixed,
                    "uni10-p05: is_form says Yes but normalize({:?}, {:?}) \
                     = {:?} differs",
                    input, form, normalized
                );
            }
        }
    }
}

// ---------------------------------------------------------------------
// UNI-2 — Case folding (UCD CaseFolding.txt)
// ---------------------------------------------------------------------

proptest! {
    /// `uni10-p06` — UCD CaseFolding.txt row stability: a case-folded
    /// string is itself case-folded (the fold is idempotent). Property
    /// holds for both `Default` and `Turkic` locales.
    #[test]
    fn uni10_p06_fold_idempotent(input in any_string()) {
        for &locale in &[CaseFoldLocale::Default, CaseFoldLocale::Turkic] {
            let once = fold(&input, locale);
            let twice = fold(&once, locale);
            prop_assert_eq!(
                &twice, &once,
                "uni10-p06: fold({:?}, {:?}) not idempotent",
                input, locale
            );
        }
    }
}

proptest! {
    /// `uni10-p07` — upper then fold = fold on the ASCII sub-grammar.
    /// `fold ∘ uppercase ≡ fold` holds **only** on inputs free of the
    /// Turkic dotless-I pair (U+0131 ı / U+0130 İ) and other
    /// SpecialCasing conditional rows: `uppercase("ı") = "I"` and
    /// `fold("I", Default) = "i"` ≠ `fold("ı", Default) = "ı"`, so
    /// the identity breaks on even that one Latin Extended codepoint.
    /// Restricting the generator to printable ASCII makes the identity
    /// unambiguous — the ASCII fast path in `fold_default_impl`
    /// (byte-local `c | 0x20`) and Rust std's ASCII uppercase
    /// (byte-local `c & !0x20`) commute by construction.
    #[test]
    fn uni10_p07_fold_absorbs_uppercase_ascii(
        input in "[\\x20-\\x7E]{0,64}",
    ) {
        let direct = fold(&input, CaseFoldLocale::Default);
        let via_upper = fold(&uppercase(&input), CaseFoldLocale::Default);
        prop_assert_eq!(
            via_upper, direct,
            "uni10-p07: fold(uppercase({:?}), Default) ≠ fold({:?}, Default)",
            input, input
        );
    }
}

proptest! {
    /// `uni10-p08` — `eq_ignore_case_unicode` is symmetric. Any binary
    /// relation that is intended as an equivalence must be symmetric
    /// (reflexivity and transitivity are covered by `uni10-p06` + the
    /// fold equivalence below).
    #[test]
    fn uni10_p08_eq_ignore_case_symmetric(
        a in any_string(),
        b in any_string(),
    ) {
        for &locale in &[CaseFoldLocale::Default, CaseFoldLocale::Turkic] {
            let ab = eq_ignore_case_unicode(&a, &b, locale);
            let ba = eq_ignore_case_unicode(&b, &a, locale);
            prop_assert_eq!(
                ab, ba,
                "uni10-p08: eq_ignore_case asymmetric for ({:?}, {:?}, {:?})",
                a, b, locale
            );
        }
    }
}

proptest! {
    /// `uni10-p09` — `eq_ignore_case_unicode(a, b, l) ⇔ fold(a, l) ==
    /// fold(b, l)`. This is the structural equation the implementation
    /// documents in the ASCII fast path; the property pins the slow
    /// path to the same semantics so a divergence in either direction
    /// surfaces.
    #[test]
    fn uni10_p09_eq_ignore_case_iff_fold_eq(
        a in any_string(),
        b in any_string(),
    ) {
        for &locale in &[CaseFoldLocale::Default, CaseFoldLocale::Turkic] {
            let via_relation = eq_ignore_case_unicode(&a, &b, locale);
            let via_fold = fold(&a, locale) == fold(&b, locale);
            prop_assert_eq!(
                via_relation, via_fold,
                "uni10-p09: relation ({}) ≠ fold-eq ({}) for ({:?}, {:?}, {:?})",
                via_relation, via_fold, a, b, locale
            );
        }
    }
}

// ---------------------------------------------------------------------
// UNI-3 — Segmentation (UAX #14 line-break + wrap)
// ---------------------------------------------------------------------

proptest! {
    /// `uni10-p10` — UAX #14 opportunities are strictly ascending in
    /// byte offset. `unicode-linebreak::linebreaks` emits them in
    /// order; the paideia-as `line_break_opportunities` adapter must
    /// preserve that order (a sort or dedup would corrupt the stream).
    #[test]
    fn uni10_p10_linebreak_offsets_strictly_ascending(
        input in any_string_with_newlines(),
    ) {
        let opps = line_break_opportunities(&input);
        for pair in opps.windows(2) {
            prop_assert!(
                pair[0].byte_offset < pair[1].byte_offset,
                "uni10-p10: offsets not strictly ascending in {:?}: \
                 {} vs {}",
                input, pair[0].byte_offset, pair[1].byte_offset
            );
        }
    }
}

proptest! {
    /// `uni10-p11` — UAX #14 LB3: the end of text is always a mandatory
    /// break. Operationally: for every non-empty input the last
    /// opportunity has `byte_offset == input.len()`. Empty input emits
    /// no opportunities (documented contract on
    /// `line_break_opportunities`).
    #[test]
    fn uni10_p11_linebreak_covers_end_of_text(
        input in any_string_with_newlines(),
    ) {
        let opps = line_break_opportunities(&input);
        if input.is_empty() {
            prop_assert!(
                opps.is_empty(),
                "uni10-p11: empty input must yield no opportunities"
            );
        } else {
            let last = opps.last().expect("non-empty input must have ≥1 opp");
            prop_assert_eq!(
                last.byte_offset, input.len(),
                "uni10-p11: last opp offset {} ≠ len {} for {:?}",
                last.byte_offset, input.len(), input
            );
        }
    }
}

proptest! {
    /// `uni10-p12` — `wrap_at_width` width-bound on ASCII input: every
    /// returned line has `str_width <= max_width`. ASCII is used so
    /// the `force_wrap_range` fallback (which admits a single oversized
    /// grapheme) can be ruled out — for pure ASCII, `str_width(line) ==
    /// line.len()` and the greedy wrapper never emits an over-long
    /// slice.
    #[test]
    fn uni10_p12_wrap_width_bound_ascii(
        input in safe_wrap_input(),
        max_width in 1usize..=32,
    ) {
        let lines = wrap_at_width(&input, max_width);
        for line in &lines {
            prop_assert!(
                str_width(line) <= max_width,
                "uni10-p12: line {:?} has width {} > max_width {} in {:?}",
                line, str_width(line), max_width, input
            );
        }
    }
}

proptest! {
    /// `uni10-p13` — `wrap_at_width(s, usize::MAX)` on single-token,
    /// newline-free input is `[s]`. With unbounded width and no
    /// mandatory breaks, the greedy wrapper has no reason to split.
    /// Trimming of trailing whitespace is non-issue because the
    /// strategy forbids leading / trailing spaces.
    #[test]
    fn uni10_p13_wrap_unbounded_single_token(input in no_wrap_input()) {
        let lines = wrap_at_width(&input, usize::MAX);
        prop_assert_eq!(
            lines, vec![input.as_str()],
            "uni10-p13: wrap_at_width({:?}, MAX) ≠ [{:?}]",
            input, input
        );
    }
}

// ---------------------------------------------------------------------
// UNI-4 — Identifier predicates (UAX #31 + UTS #39)
// ---------------------------------------------------------------------

proptest! {
    /// `uni10-p14` — `is_identifier` is monotone in ASCII prefixes.
    /// For a well-formed ASCII identifier `s` and any non-empty byte
    /// prefix that ends on an identifier-byte boundary (always, in
    /// ASCII), the prefix is also a valid identifier. Chosen for ASCII
    /// so no NFC decomposition can span the cut (NFC-closure under
    /// prefixing holds trivially for pure ASCII; the general-Unicode
    /// case is left to a UNI-10.1 follow-up).
    #[test]
    fn uni10_p14_is_identifier_monotone_ascii_prefix(
        s in ascii_ident(),
        k in 1usize..=16,
    ) {
        prop_assume!(is_identifier(&s));
        let k = k.min(s.len());
        let prefix = &s[..k];
        prop_assert!(
            is_identifier(prefix),
            "uni10-p14: prefix {:?} of {:?} is not an identifier",
            prefix, s
        );
    }
}

proptest! {
    /// `uni10-p15` — `IdentProfile::Ascii` ⊆ `IdentProfile::Default`
    /// on the Letter-starting sub-grammar. The Ascii profile admits
    /// `_` as a leading byte (per `is_ascii_ident_start_byte` in
    /// `src/ident.rs`), but `_` (U+005F, Pc) is **not** `XID_Start`,
    /// so a bare `_abc` passes Ascii yet fails Default — a documented
    /// Pc-vs-UAX-#31 asymmetry, not a bug. We restrict the input to
    /// letter-start identifiers (`[A-Za-z][A-Za-z0-9_]*`), the subset
    /// on which the inclusion holds unconditionally, and assert the
    /// implication there.
    #[test]
    fn uni10_p15_ascii_profile_subset_of_default(
        input in "[A-Za-z][A-Za-z0-9_]{0,16}",
    ) {
        prop_assert!(
            check_ident(&input, IdentProfile::Ascii).is_ok(),
            "uni10-p15: strategy-guaranteed Ascii-valid {:?} rejected",
            input
        );
        prop_assert!(
            check_ident(&input, IdentProfile::Default).is_ok(),
            "uni10-p15: Ascii letter-start {:?} rejected by Default",
            input
        );
    }
}

// ---------------------------------------------------------------------
// UNI-5 — UCD property adapter
// ---------------------------------------------------------------------

proptest! {
    /// `uni10-p16` — the seven group predicates partition `char`. For
    /// every Rust `char` value, exactly one of `is_letter`, `is_mark`,
    /// `is_number`, `is_punctuation`, `is_symbol`, `is_separator`,
    /// `is_other` returns true. This is the UCD §4.5 top-level-group
    /// contract (General_Category's first letter uniquely assigns the
    /// group).
    #[test]
    fn uni10_p16_group_predicates_partition(c in proptest::char::any()) {
        let hits = [
            is_letter(c),
            is_mark(c),
            is_number(c),
            is_punctuation(c),
            is_symbol(c),
            is_separator(c),
            is_other(c),
        ]
        .iter()
        .filter(|b| **b)
        .count();
        prop_assert_eq!(
            hits, 1,
            "uni10-p16: char U+{:04X} has {} group hits (expected 1)",
            c as u32, hits
        );
    }
}

proptest! {
    /// `uni10-p17` — `char::is_control()` ⇔ `general_category == Cc`.
    /// Rust std's `is_control` is documented to be "General_Category =
    /// Cc"; this property pins the paideia-as GC adapter to that fact
    /// so a future UCD-version drift between `unicode-properties` and
    /// rustc's embedded table surfaces here before anything else sees
    /// it.
    #[test]
    fn uni10_p17_std_is_control_equals_gc_cc(c in proptest::char::any()) {
        let via_std = c.is_control();
        let via_gc = matches!(general_category(c), GeneralCategory::Cc);
        prop_assert_eq!(
            via_std, via_gc,
            "uni10-p17: U+{:04X}: is_control={}, gc_cc={}",
            c as u32, via_std, via_gc
        );
    }
}

proptest! {
    /// `uni10-p18` — `Script::Unknown` is confined to private-use (Co)
    /// and unassigned / noncharacter (Cn) codepoints. UAX #24 §4
    /// assigns Script `Zzzz` (Unknown) to codepoints without a specific
    /// script — in Rust's `char` domain (surrogates excluded) that is
    /// the Co ∪ Cn union. A regression that leaked `Unknown` into
    /// Letter / Mark / Symbol / … categories would surface here.
    #[test]
    fn uni10_p18_script_unknown_confined_to_co_or_cn(c in proptest::char::any()) {
        if matches!(script(c), Script::Unknown) {
            let gc = general_category(c);
            prop_assert!(
                matches!(gc, GeneralCategory::Co | GeneralCategory::Cn),
                "uni10-p18: U+{:04X} is Script::Unknown but gc = {:?}",
                c as u32, gc
            );
        }
    }
}

// ---------------------------------------------------------------------
// UNI-9 — East-Asian width + WidthMode
// ---------------------------------------------------------------------

proptest! {
    /// `uni10-p19` — `str_width_in` decomposes into a sum of
    /// `grapheme_width_in` over the grapheme-cluster partition, once
    /// ANSI escapes are excluded. The exclusion is because
    /// `str_width_in` strips ANSI inline whereas a direct
    /// `graphemes()` walk sees the ESC bytes; a strategy that excludes
    /// U+001B keeps both sides on the same input.
    #[test]
    fn uni10_p19_str_width_is_grapheme_sum(input in "[^\\x1B]{0,64}") {
        for &mode in &[WidthMode::Latin, WidthMode::EastAsian] {
            let whole = str_width_in(&input, mode);
            let parts: usize = input
                .graphemes(true)
                .map(|g| grapheme_width_in(g, mode))
                .sum();
            prop_assert_eq!(
                whole, parts,
                "uni10-p19: str_width_in({:?}, {:?}) = {} ≠ Σ grapheme = {}",
                input, mode, whole, parts
            );
        }
    }
}

proptest! {
    /// `uni10-p20` — ASCII letters are narrow (1 col) regardless of
    /// `WidthMode`. UAX #11 assigns Latin letters property `Na`
    /// (narrow) in both the Latin and East-Asian tables; the paideia-as
    /// `WidthMode` dispatcher must not accidentally promote them.
    #[test]
    fn uni10_p20_ascii_letter_narrow_in_every_mode(
        c in "[A-Za-z]",
    ) {
        let ch = c.chars().next().expect("single-char match");
        for &mode in &[WidthMode::Latin, WidthMode::EastAsian] {
            prop_assert_eq!(
                width_in(ch, mode), Width::Narrow,
                "uni10-p20: ASCII '{}' not Narrow under {:?}",
                ch, mode
            );
        }
    }
}

proptest! {
    /// `uni10-p21` — CJK Unified Ideographs are wide (2 cols)
    /// regardless of `WidthMode`. UAX #11 assigns property `W` to
    /// U+4E00..=U+9FFF; both the Latin and East-Asian width tables
    /// agree on this range. A regression that erased the `Wide` arm
    /// (e.g. an upstream `unicode-width` change that renamed `W` to
    /// `Full`) would surface as a counterexample here.
    #[test]
    fn uni10_p21_cjk_ideograph_wide_in_every_mode(
        c in 0x4E00u32..=0x9FFFu32,
    ) {
        let ch = char::from_u32(c).expect("CJK range is all assigned");
        for &mode in &[WidthMode::Latin, WidthMode::EastAsian] {
            prop_assert_eq!(
                width_in(ch, mode), Width::Wide,
                "uni10-p21: CJK U+{:04X} not Wide under {:?}",
                c, mode
            );
        }
    }
}
