//! UNI-11 (paideia-as#1598) — public API freeze audit.
//!
//! This test is the drift gate for the paideia-as-unicode public surface
//! frozen at UNI-11. It reads the on-disk golden snapshot
//! `tests/public_api.golden`, normalizes the content against end-of-line
//! and trailing-whitespace noise, and asserts:
//!
//! 1. The golden file parses as the UNI-11 freeze format
//!    (`<kind>\t<path>[\t<extras>]`, with `#`-prefixed comment lines and
//!    blank lines skipped).
//! 2. The per-kind entry counts match the authoritative inventory in
//!    [`EXPECTED_COUNTS`]. A new pub item in source without a golden
//!    regen reads here as a count-mismatch failure.
//! 3. A representative sentinel from every module is present under its
//!    expected `(kind, path)` key. Catches silent entry-shape drift that
//!    would otherwise pass the raw count check.
//! 4. Every top-level pub item named in the inventory is callable at
//!    compile time via its qualified path — the compile-time reference
//!    block below forces a build failure if a signature changes or an
//!    item is removed from source without regenerating the golden and
//!    updating this test.
//!
//! The golden file itself is the UNI-11 contract; this test is only its
//! enforcement surface. See
//! `design/toolchain/paideia-as-unicode-api-freeze.md` for the companion
//! policy document, and `design/toolchain/paideia-as-unicode-spec.md`
//! §4 for the inventory tables.
//!
//! # Regenerating the golden
//!
//! After a reviewed, UNI-ticket-backed API change, the golden is updated
//! by hand (or by a future scripted path) and this test's
//! [`EXPECTED_COUNTS`] and sentinel list are updated in the same PR.
//! There is no runtime regen mode: the freeze discipline is deliberately
//! two-sided (golden + inventory) so no single-file edit can quietly
//! change the surface.
//!
//! # Determinism
//!
//! No external crate is consulted. Parsing is pure `std` over the
//! `include_str!`-captured golden content. Line endings and trailing
//! whitespace are normalized before parsing so the test behaves
//! identically on any platform / autocrlf configuration.

use std::collections::BTreeMap;

/// Compile-time embedding of the frozen golden file. Any edit to the
/// `.golden` is observable at the next test run; a bulk tool rewriting
/// line endings is normalized away in [`parse_golden`].
const GOLDEN_RAW: &str = include_str!("public_api.golden");

/// Authoritative per-kind entry counts at the UNI-11 freeze.
///
/// Must be kept in sync with `tests/public_api.golden`. Any addition,
/// removal, or re-classification of a pub item requires updating the
/// corresponding row here AND the golden entry — the freeze discipline
/// is a two-sided check.
const EXPECTED_COUNTS: &[(&str, usize)] = &[
    ("mod", 12),
    ("reexport", 66),
    ("use", 4),
    ("struct", 8),
    ("field", 14),
    ("enum", 11),
    ("variant", 405),
    ("fn", 55),
    ("method", 5),
    ("impl", 9),
];

/// Sentinel entries — one per module (plus a crate-root re-export and
/// one representative impl / variant / field). If any of these is
/// missing from the golden under its (kind, path) key, the surface has
/// drifted or an entry has been reclassified. Catches drift that
/// coincidentally preserves per-kind counts.
const SENTINEL_ENTRIES: &[(&str, &str)] = &[
    // Modules (one per module file).
    ("mod", "paideia_as_unicode::case"),
    ("mod", "paideia_as_unicode::error"),
    ("mod", "paideia_as_unicode::fingerprint"),
    ("mod", "paideia_as_unicode::grapheme"),
    ("mod", "paideia_as_unicode::ident"),
    ("mod", "paideia_as_unicode::lsp_bridge"),
    ("mod", "paideia_as_unicode::nfc"),
    ("mod", "paideia_as_unicode::norm"),
    ("mod", "paideia_as_unicode::segment"),
    ("mod", "paideia_as_unicode::ucd"),
    ("mod", "paideia_as_unicode::utf8"),
    ("mod", "paideia_as_unicode::width"),
    // Crate-root re-exports — one from each originating module.
    ("reexport", "paideia_as_unicode::CaseFoldLocale"),
    ("reexport", "paideia_as_unicode::Utf8Error"),
    ("reexport", "paideia_as_unicode::GraphemeBoundaries"),
    ("reexport", "paideia_as_unicode::IdentProfile"),
    ("reexport", "paideia_as_unicode::nfc_normalize"),
    ("reexport", "paideia_as_unicode::Form"),
    ("reexport", "paideia_as_unicode::LineBreak"),
    ("reexport", "paideia_as_unicode::GeneralCategory"),
    ("reexport", "paideia_as_unicode::Utf8Decoder"),
    ("reexport", "paideia_as_unicode::WidthMode"),
    // Flagship types — one struct and one enum per concerned module.
    ("enum", "paideia_as_unicode::case::CaseFoldLocale"),
    ("struct", "paideia_as_unicode::error::Utf8Error"),
    ("enum", "paideia_as_unicode::error::Utf8ErrorKind"),
    ("struct", "paideia_as_unicode::fingerprint::FingerprintCase"),
    ("struct", "paideia_as_unicode::fingerprint::FingerprintRecord"),
    ("struct", "paideia_as_unicode::grapheme::GraphemeBoundaries"),
    ("enum", "paideia_as_unicode::ident::IdentProfile"),
    ("enum", "paideia_as_unicode::ident::IdentErrorKind"),
    ("struct", "paideia_as_unicode::ident::IdentError"),
    ("enum", "paideia_as_unicode::norm::Form"),
    ("enum", "paideia_as_unicode::segment::LineBreak"),
    ("struct", "paideia_as_unicode::segment::LineBreakOpportunity"),
    ("struct", "paideia_as_unicode::segment::SegmentError"),
    ("enum", "paideia_as_unicode::segment::SegmentErrorKind"),
    ("enum", "paideia_as_unicode::ucd::GeneralCategory"),
    ("enum", "paideia_as_unicode::ucd::Block"),
    ("struct", "paideia_as_unicode::utf8::Utf8Decoder"),
    ("enum", "paideia_as_unicode::width::WidthMode"),
    ("enum", "paideia_as_unicode::width::Width"),
    // Representative functions / methods / variants / fields / impls.
    ("fn", "paideia_as_unicode::case::fold"),
    ("fn", "paideia_as_unicode::norm::nfc_normalize"),
    ("fn", "paideia_as_unicode::segment::wrap_at_width"),
    ("fn", "paideia_as_unicode::ucd::general_category"),
    ("fn", "paideia_as_unicode::ucd::is_letter"),
    ("fn", "paideia_as_unicode::ident::check_ident"),
    ("fn", "paideia_as_unicode::fingerprint::fingerprint_golden_path"),
    ("fn", "paideia_as_unicode::lsp_bridge::utf8_offset_to_utf16"),
    ("fn", "paideia_as_unicode::lsp_bridge::stream_is_nfc"),
    ("method", "paideia_as_unicode::utf8::Utf8Decoder::new"),
    ("method", "paideia_as_unicode::utf8::Utf8Decoder::position"),
    ("method", "paideia_as_unicode::utf8::Utf8Decoder::is_exhausted"),
    ("method", "paideia_as_unicode::width::Width::columns_latin"),
    ("method", "paideia_as_unicode::width::Width::columns_east_asian"),
    ("variant", "paideia_as_unicode::case::CaseFoldLocale::Default"),
    ("variant", "paideia_as_unicode::case::CaseFoldLocale::Turkic"),
    ("variant", "paideia_as_unicode::norm::Form::Nfc"),
    ("variant", "paideia_as_unicode::ucd::Block::BasicLatin"),
    ("variant", "paideia_as_unicode::ucd::Block::Unassigned"),
    ("variant", "paideia_as_unicode::ucd::GeneralCategory::Lu"),
    ("variant", "paideia_as_unicode::ident::IdentErrorKind::MixedScript"),
    ("field", "paideia_as_unicode::error::Utf8Error::offset"),
    ("field", "paideia_as_unicode::error::Utf8Error::reason"),
    ("field", "paideia_as_unicode::segment::LineBreakOpportunity::byte_offset"),
    ("field", "paideia_as_unicode::segment::LineBreakOpportunity::kind"),
    ("field", "paideia_as_unicode::ident::IdentError::offset"),
    ("field", "paideia_as_unicode::ident::IdentError::kind"),
    ("field", "paideia_as_unicode::fingerprint::FingerprintCase::tag"),
    ("field", "paideia_as_unicode::fingerprint::FingerprintRecord::tag"),
    ("impl", "paideia_as_unicode::utf8::Utf8Decoder"),
    ("impl", "paideia_as_unicode::grapheme::GraphemeBoundaries"),
    ("impl", "paideia_as_unicode::error::Utf8Error"),
    ("impl", "paideia_as_unicode::error::Utf8ErrorKind"),
    ("impl", "paideia_as_unicode::segment::SegmentError"),
    ("impl", "paideia_as_unicode::segment::SegmentErrorKind"),
    ("impl", "paideia_as_unicode::ucd::GeneralCategory"),
    // Intra-module re-exports (the ::nfc shim and the ::ucd::Script bridge).
    ("use", "paideia_as_unicode::nfc::nfc_normalize"),
    ("use", "paideia_as_unicode::nfc::is_nfc"),
    ("use", "paideia_as_unicode::nfc::is_ascii_fast_path_eligible"),
    ("use", "paideia_as_unicode::ucd::Script"),
];

/// Parse the golden file into a `(kind, path)` → `extras` map.
///
/// Normalization applied per line:
/// * strip trailing `\r` (handle any autocrlf-rewritten checkout);
/// * strip trailing whitespace;
/// * drop blank lines and `#`-prefixed comment lines.
///
/// A duplicate `(kind, path, extras)` key is a parse error: the freeze
/// requires a one-to-one map from (pub item, trait impl) to golden entry.
/// Multiple impl rows for the same type share `(kind, path)` but differ
/// in `extras` (which trait is implemented), so the extras participate
/// in the dedup key.
fn parse_golden() -> BTreeMap<(String, String, String), ()> {
    let mut map: BTreeMap<(String, String, String), ()> = BTreeMap::new();
    for (lineno0, raw) in GOLDEN_RAW.lines().enumerate() {
        let lineno = lineno0 + 1;
        // Strip CR (autocrlf) + any trailing whitespace. Line split by
        // `str::lines()` already drops the terminator, but a `\r\n` on
        // disk leaves a stray `\r`.
        let line = raw.trim_end_matches('\r').trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(3, '\t');
        let kind = parts.next().unwrap_or("").to_string();
        let path = parts.next().unwrap_or("").to_string();
        let extras = parts.next().unwrap_or("").to_string();
        assert!(
            !kind.is_empty() && !path.is_empty(),
            "public_api.golden:{}: expected `<kind>\\t<path>[\\t<extras>]`, got {:?}",
            lineno,
            line
        );
        let key = (kind, path, extras);
        if map.insert(key.clone(), ()).is_some() {
            panic!(
                "public_api.golden:{}: duplicate entry {:?}",
                lineno, key
            );
        }
    }
    map
}

#[test]
fn public_api_golden_parses_cleanly() {
    let entries = parse_golden();
    assert!(
        !entries.is_empty(),
        "public_api.golden parsed to zero entries; file is probably empty or all-comments"
    );
}

#[test]
fn public_api_golden_matches_expected_counts() {
    let entries = parse_golden();
    let mut actual: BTreeMap<&str, usize> = BTreeMap::new();
    for ((kind, _, _), _) in &entries {
        *actual.entry(kind.as_str()).or_insert(0) += 1;
    }
    let mut mismatches = Vec::new();
    for (kind, expected) in EXPECTED_COUNTS {
        let got = actual.get(*kind).copied().unwrap_or(0);
        if got != *expected {
            mismatches.push(format!(
                "kind {:?}: expected {} entries, found {}",
                kind, expected, got
            ));
        }
    }
    // Also fail if an unexpected kind shows up (future entry types must
    // be added to EXPECTED_COUNTS explicitly).
    for kind in actual.keys() {
        if !EXPECTED_COUNTS.iter().any(|(k, _)| k == kind) {
            mismatches.push(format!(
                "kind {:?} is not listed in EXPECTED_COUNTS; add it (and update \
                 the sentinel list / freeze doc) if the UNI-11 freeze intentionally \
                 widens the API-entry vocabulary",
                kind
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "public_api.golden drift detected:\n  {}\n\n\
         If the drift is intentional, regenerate the golden under a UNI follow-up \
         ticket, update EXPECTED_COUNTS, and update \
         design/toolchain/paideia-as-unicode-api-freeze.md.",
        mismatches.join("\n  ")
    );
}

#[test]
fn public_api_golden_contains_every_sentinel() {
    let entries = parse_golden();
    let mut missing = Vec::new();
    for (kind, path) in SENTINEL_ENTRIES {
        let found = entries
            .keys()
            .any(|(k, p, _)| k == kind && p == path);
        if !found {
            missing.push(format!("missing: {}\t{}", kind, path));
        }
    }
    assert!(
        missing.is_empty(),
        "public_api.golden sentinel drift detected:\n  {}\n\n\
         Each sentinel maps one-to-one to a pub item the UNI-11 freeze pins. \
         If a sentinel is intentionally gone, update SENTINEL_ENTRIES AND \
         design/toolchain/paideia-as-unicode-api-freeze.md.",
        missing.join("\n  ")
    );
}

#[test]
fn public_api_golden_rows_use_fully_qualified_crate_paths() {
    let entries = parse_golden();
    for (kind, path, _) in entries.keys() {
        assert!(
            path.starts_with("paideia_as_unicode::") || path == "paideia_as_unicode",
            "public_api.golden row with non-crate-local path: kind={:?} path={:?}",
            kind,
            path,
        );
    }
}

// --------------------------------------------------------------------
// Compile-time touches — every top-level pub item is referenced here,
// so any removal / signature mismatch surfaces as a build error even
// when a would-be author of a drifting change forgets to regenerate the
// golden file. The references are type-coerced function pointers and
// `_` bindings on constants / types so the compiler enforces the shape
// without any runtime effect.
// --------------------------------------------------------------------
#[allow(dead_code)]
mod compile_time_touches {
    use paideia_as_unicode::{
        Block, CaseFoldLocale, Form, GeneralCategory, GraphemeBoundaries, IdentError,
        IdentErrorKind, IdentProfile, LineBreak, LineBreakOpportunity, Script, SegmentError,
        SegmentErrorKind, Utf8Decoder, Utf8Error, Width, WidthMode, block,
        check_ident, eq_ignore_case_unicode, fold, fold_default, general_category,
        grapheme_advance, grapheme_boundaries, grapheme_count, grapheme_retreat,
        grapheme_width, grapheme_width_in, is_ascii_fast_path_eligible, is_form,
        is_identifier, is_letter, is_mark, is_nfc, is_nfd, is_nfkc, is_nfkd,
        is_number, is_other, is_punctuation, is_separator, is_symbol, is_xid_continue,
        is_xid_start, line_break_opportunities, lowercase, mixed_script, next_line_break,
        nfc_normalize, nfd_normalize, nfkc_normalize, nfkd_normalize, normalize,
        script, str_width, str_width_in, uppercase, width, width_in, wrap_at_width,
    };

    // Function-pointer coercions pin signature shape. If the parameter
    // list or the return type on any pub fn changes, the coercion fails
    // to compile.
    #[allow(clippy::type_complexity)]
    pub fn signatures_pin() {
        let _: fn(&str, CaseFoldLocale) -> String = fold;
        let _: fn(&str) -> String = fold_default;
        let _: fn(&str, &str, CaseFoldLocale) -> bool = eq_ignore_case_unicode;
        let _: fn(&str) -> String = uppercase;
        let _: fn(&str) -> String = lowercase;

        let _: fn(&str) -> bool = is_ascii_fast_path_eligible;
        let _: fn(&str) -> bool = is_nfc;
        let _: fn(&str) -> bool = is_nfd;
        let _: fn(&str) -> bool = is_nfkc;
        let _: fn(&str) -> bool = is_nfkd;
        let _: fn(&str, Form) -> bool = is_form;
        let _: fn(&str) -> String = nfc_normalize;
        let _: fn(&str) -> String = nfd_normalize;
        let _: fn(&str) -> String = nfkc_normalize;
        let _: fn(&str) -> String = nfkd_normalize;
        let _: fn(&str, Form) -> String = normalize;

        let _: for<'a> fn(&'a str) -> GraphemeBoundaries<'a> = grapheme_boundaries;
        let _: fn(&str) -> usize = grapheme_count;
        let _: fn(&str, usize) -> usize = grapheme_advance;
        let _: fn(&str, usize) -> usize = grapheme_retreat;

        let _: fn(char) -> bool = is_xid_start;
        let _: fn(char) -> bool = is_xid_continue;
        let _: fn(&str) -> bool = is_identifier;
        let _: fn(&str, IdentProfile) -> Result<(), IdentError> = check_ident;
        let _: fn(&str) -> Vec<Script> = mixed_script;

        let _: fn(&str) -> Vec<LineBreakOpportunity> = line_break_opportunities;
        let _: fn(&str, usize) -> Option<(usize, LineBreak)> = next_line_break;
        // `wrap_at_width` returns `Vec<&str>` tied to the input lifetime,
        // which is a higher-ranked signature; coerce it under a bounded
        // lifetime closure instead of a bare fn pointer.
        let _wrap: for<'a> fn(&'a str, usize) -> Vec<&'a str> = wrap_at_width;

        let _: fn(char) -> GeneralCategory = general_category;
        let _: fn(char) -> Script = script;
        let _: fn(char) -> Block = block;
        let _: fn(char) -> bool = is_letter;
        let _: fn(char) -> bool = is_mark;
        let _: fn(char) -> bool = is_number;
        let _: fn(char) -> bool = is_punctuation;
        let _: fn(char) -> bool = is_symbol;
        let _: fn(char) -> bool = is_separator;
        let _: fn(char) -> bool = is_other;

        let _: fn(char) -> Width = width;
        let _: fn(char, WidthMode) -> Width = width_in;
        let _: fn(&str) -> usize = str_width;
        let _: fn(&str, WidthMode) -> usize = str_width_in;
        let _: fn(&str) -> usize = grapheme_width;
        let _: fn(&str, WidthMode) -> usize = grapheme_width_in;

        // UNI-13 LSP position-encoding bridge.
        let _: fn(&str, usize) -> usize = paideia_as_unicode::utf8_offset_to_utf16;
        let _: fn(&str, usize) -> usize = paideia_as_unicode::utf16_offset_to_utf8;
        let _: fn(&str, usize) -> usize = paideia_as_unicode::utf8_offset_to_utf32;
        let _: fn(&str, usize) -> usize = paideia_as_unicode::utf32_offset_to_utf8;
        // `stream_is_nfc` is generic over `I: Iterator<Item = char>`;
        // bind one monomorphization (over `std::str::Chars<'_>`) to
        // pin the signature at the freeze.
        fn pin_stream_is_nfc(iter: std::str::Chars<'_>) -> bool {
            paideia_as_unicode::stream_is_nfc(iter)
        }
        let _ = pin_stream_is_nfc as fn(std::str::Chars<'_>) -> bool;

        // Methods: pin via explicit trampolines that force the method
        // name + receiver + return shape to resolve at compile time.
        // Fn-pointer coercion of an inherent method on a lifetime-
        // parameterized type runs into HRTB inference quirks; the
        // trampoline sidesteps that while still catching every drift.
        fn pin_decoder_new<'a>(bytes: &'a [u8]) -> Utf8Decoder<'a> {
            Utf8Decoder::new(bytes)
        }
        fn pin_decoder_position(d: &Utf8Decoder<'_>) -> usize {
            d.position()
        }
        fn pin_decoder_exhausted(d: &Utf8Decoder<'_>) -> bool {
            d.is_exhausted()
        }
        fn pin_width_latin(w: Width) -> usize {
            w.columns_latin()
        }
        fn pin_width_east_asian(w: Width) -> usize {
            w.columns_east_asian()
        }
        let _ = pin_decoder_new as for<'a> fn(&'a [u8]) -> Utf8Decoder<'a>;
        let _ = pin_decoder_position as fn(&Utf8Decoder<'_>) -> usize;
        let _ = pin_decoder_exhausted as fn(&Utf8Decoder<'_>) -> bool;
        let _ = pin_width_latin as fn(Width) -> usize;
        let _ = pin_width_east_asian as fn(Width) -> usize;
    }

    // Type-presence touches. Reference every enum, every struct, and
    // the critical variants / fields the freeze pins. Any rename of a
    // variant or a field spelling trips the compiler here.
    pub fn type_presence() {
        let _ = CaseFoldLocale::Default;
        let _ = CaseFoldLocale::Turkic;

        let _ = Form::Nfc;
        let _ = Form::Nfd;
        let _ = Form::Nfkc;
        let _ = Form::Nfkd;

        let _ = IdentProfile::Ascii;
        let _ = IdentProfile::Default;
        let _ = IdentProfile::AllowedChars;

        let _ = IdentErrorKind::BadStart;
        let _ = IdentErrorKind::BadContinue;
        let _ = IdentErrorKind::NotNfc;
        let _ = IdentErrorKind::MixedScript { scripts: Vec::new() };
        let _ = IdentErrorKind::Confusable;

        let _ = LineBreak::Mandatory;
        let _ = LineBreak::Allowed;
        let _ = LineBreak::Prohibited;

        let _ = SegmentErrorKind::WidthTooSmall { minimum: 0 };

        let _ = Width::Zero;
        let _ = Width::Narrow;
        let _ = Width::Ambiguous;
        let _ = Width::Wide;

        let _ = WidthMode::Latin;
        let _ = WidthMode::EastAsian;

        let _ = GeneralCategory::Lu;
        let _ = GeneralCategory::Cn;

        let _ = Block::BasicLatin;
        let _ = Block::Unassigned;

        // Struct field spellings. The structs `Utf8Error` and
        // `IdentError` are constructable from outside the crate (not
        // `#[non_exhaustive]`); the structs `LineBreakOpportunity` and
        // `SegmentError` are `#[non_exhaustive]` and must only be
        // *inspected* through borrowed access. We take borrowed-field
        // accessor fns that force the compiler to prove the field
        // names + types exist on the frozen surface.
        let err = Utf8Error {
            offset: 0,
            reason: paideia_as_unicode::error::Utf8ErrorKind::InvalidLeadByte,
        };
        let _ = err.offset;
        let _ = err.reason;

        let ident_err = IdentError { offset: 0, kind: IdentErrorKind::BadStart };
        let _ = ident_err.offset;
        let _ = ident_err.kind;

        fn access_lbo_byte_offset(o: &LineBreakOpportunity) -> &usize { &o.byte_offset }
        fn access_lbo_kind(o: &LineBreakOpportunity) -> &LineBreak { &o.kind }
        fn access_seg_offset(e: &SegmentError) -> &usize { &e.offset }
        fn access_seg_kind(e: &SegmentError) -> &SegmentErrorKind { &e.kind }
        let _ = access_lbo_byte_offset as fn(&LineBreakOpportunity) -> &usize;
        let _ = access_lbo_kind as fn(&LineBreakOpportunity) -> &LineBreak;
        let _ = access_seg_offset as fn(&SegmentError) -> &usize;
        let _ = access_seg_kind as fn(&SegmentError) -> &SegmentErrorKind;

        let fc = paideia_as_unicode::fingerprint::FingerprintCase {
            tag: "",
            input: "",
            expected: "",
        };
        let _ = fc.tag;
        let _ = fc.input;
        let _ = fc.expected;

        let fr = paideia_as_unicode::fingerprint::FingerprintRecord {
            tag: String::new(),
            input: String::new(),
            expected: String::new(),
        };
        let _ = fr.tag;
        let _ = fr.input;
        let _ = fr.expected;
    }

    // Fingerprint-helper function pointers so the fingerprint module's
    // surface is touched by this test too.
    pub fn fingerprint_signatures_pin() {
        use paideia_as_unicode::fingerprint::{
            FingerprintCase, FingerprintRecord, fingerprint_golden_path,
            maybe_regen_fingerprints, read_fingerprint_golden, verify_fingerprint,
            write_fingerprint_golden, write_fingerprint_golden_records,
        };
        let _: fn(&str) -> std::path::PathBuf = fingerprint_golden_path;
        let _: fn(&FingerprintCase, &str) -> Result<(), String> = verify_fingerprint;
        let _: fn(&str, &[FingerprintCase]) -> std::io::Result<()> =
            write_fingerprint_golden;
        let _: fn(&str, &[FingerprintRecord]) -> std::io::Result<()> =
            write_fingerprint_golden_records;
        let _: fn(&str) -> std::io::Result<Vec<FingerprintRecord>> =
            read_fingerprint_golden;
        let _: fn() -> bool = maybe_regen_fingerprints;
    }
}
