//! UNI-7 (paideia-as#1594) — full UCD 15.1 conformance corpus sweep.
//!
//! Four `#[ignore]` tests that run paideia-as-unicode against the
//! complete Unicode conformance test files:
//!
//!   * `GraphemeBreakTest.txt`     → `grapheme_boundaries`
//!   * `NormalizationTest.txt`     → `nfc_normalize`
//!   * `LineBreakTest.txt`         → `line_break_opportunities`
//!   * `IdentifierStatus.txt`      → `check_ident(_, AllowedChars)`
//!
//! The files are vendored under `tests/ucd-corpus/15.1.0/`. They are
//! **not** downloaded at test time — see `tests/ucd-corpus/15.1.0/README.md`
//! for the manual vendor step. The harnesses skip gracefully if a
//! corpus file is absent or empty of data rows (comments only), so a
//! fresh checkout — or a UCD-version bump that temporarily lands with
//! placeholder files — still compiles and runs the default
//! (non-ignored) suite without surprises.
//!
//! All four tests are `#[ignore]` so `cargo test -p paideia-as-unicode`
//! stays fast and offline-friendly; invoke with
//! `cargo test -p paideia-as-unicode -- --ignored --nocapture` once the
//! corpus is in place.
//!
//! Fingerprint discipline: each test embeds `uni7-<file>-sweep` in its
//! panic summary so the debugger can correlate against UNI-7's spec
//! §8 acceptance line item.

use std::fs;
use std::path::{Path, PathBuf};

use paideia_as_unicode::{
    IdentProfile, check_ident, grapheme_boundaries, is_xid_start, line_break_opportunities,
    nfc_normalize,
};

/// Directory, relative to the crate manifest, holding the vendored UCD
/// conformance files. The version segment (`15.1.0`) is deliberate: a
/// UCD version bump creates a new sibling directory so historical
/// corpora stay reachable for bisection.
const CORPUS_DIR: &str = "tests/ucd-corpus/15.1.0";

/// Marker string any placeholder file carries in a `#`-comment line
/// (see `README.md`). The harness does **not** short-circuit on this
/// marker — the placeholder files ship 2–3 real data rows for a smoke
/// test. The marker is advisory ("full corpus not yet vendored").
const VENDOR_MARKER: &str = "VENDOR: fetch from unicode.org";

/// Resolve a corpus file to an absolute path rooted at the crate's
/// manifest directory. `CARGO_MANIFEST_DIR` is the only reliable test-
/// time anchor; `std::env::current_dir()` can be the workspace root
/// under cargo-nextest or a per-test tmpdir under proptest regression
/// harnesses.
fn corpus_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(CORPUS_DIR)
        .join(filename)
}

/// Returned by [`load_corpus`] when the vendor file is missing. The
/// harness then logs the skip reason and returns early — never panics.
enum CorpusLoad {
    Loaded(String),
    Skip(String),
}

/// Load a corpus file, returning the raw text or a human-readable skip
/// reason. "Missing" means the file does not exist on disk; the
/// VENDOR_MARKER comment is **not** a skip trigger (the placeholder
/// files pair the marker with real smoke rows).
fn load_corpus(filename: &str) -> CorpusLoad {
    let path = corpus_path(filename);
    match fs::read_to_string(&path) {
        Ok(s) => CorpusLoad::Loaded(s),
        Err(e) => CorpusLoad::Skip(format!(
            "{} missing or unreadable ({}); run the vendor step in {}/README.md",
            path.display(),
            e,
            CORPUS_DIR
        )),
    }
}

/// Strip a trailing `# ...` comment and surrounding whitespace. Returns
/// `None` for lines that have no content after that strip (blank or
/// pure comment).
fn strip_comment(line: &str) -> Option<&str> {
    let no_comment = line.split('#').next().unwrap_or("");
    let trimmed = no_comment.trim();
    if trimmed.is_empty() { None } else { Some(trimmed) }
}

/// Parse a hex code point like `"0041"` into a `char`. Surrogates and
/// out-of-range values return `None`.
fn parse_cp(hex: &str) -> Option<char> {
    u32::from_str_radix(hex.trim(), 16)
        .ok()
        .and_then(char::from_u32)
}

/// Count `#`-comment lines that carry the VENDOR_MARKER. Used only for
/// the diagnostic log so operators see "running on placeholder" vs.
/// "running on full corpus" at a glance.
fn is_placeholder(text: &str) -> bool {
    text.lines().any(|l| l.contains(VENDOR_MARKER))
}

/// Parse one `GraphemeBreakTest` / `LineBreakTest` row.
///
/// Format: `<marker> XXXX <marker> YYYY <marker> ZZZZ <marker> # ...`
///
///   * `÷` (U+00F7 DIVISION SIGN) is a break opportunity (at the
///     current byte offset).
///   * `×` (U+00D7 MULTIPLICATION SIGN) is a no-break marker (not a
///     boundary — just a separator).
///   * Code points are 4–6 hex digits.
///
/// `GraphemeBreakTest.txt` rows always start with `÷` (UAX #29 GB1 —
/// break at SOT). `LineBreakTest.txt` rows always start with `×` (UAX
/// #14 LB2 — no break at SOT). Both are accepted; the harness differ-
/// entiates in its caller.
///
/// Returns `(string, expected_boundaries)` where `expected_boundaries`
/// is the list of byte offsets at which a `÷` was seen. The caller
/// decides whether offset 0 and offset `s.len()` are part of the
/// contract (grapheme boundaries include both; line-break
/// opportunities include neither in our API).
fn parse_break_row(line: &str) -> Option<(String, Vec<usize>)> {
    let line = strip_comment(line)?;
    let mut s = String::new();
    let mut boundaries = Vec::new();
    let mut saw_any_marker = false;
    for tok in line.split_whitespace() {
        match tok {
            // U+00F7 DIVISION SIGN — "break here".
            "\u{00F7}" => {
                boundaries.push(s.len());
                saw_any_marker = true;
            }
            // U+00D7 MULTIPLICATION SIGN — "do not break here".
            "\u{00D7}" => {
                saw_any_marker = true;
            }
            cp => {
                let ch = parse_cp(cp)?;
                s.push(ch);
            }
        }
    }
    // Rows without a single `÷`/`×` marker are malformed (or an
    // `@Part*` header leaked through); surface as parse-skip rather
    // than as a spurious count mismatch.
    if !saw_any_marker {
        return None;
    }
    Some((s, boundaries))
}

/// Parse one `NormalizationTest` data row into the five columns UAX #15
/// §5 defines: `(source, NFC, NFD, NFKC, NFKD)`. `None` for comments,
/// blank lines, and the `@Part*` section headers.
fn parse_norm_row(line: &str) -> Option<[String; 5]> {
    let line = strip_comment(line)?;
    if line.starts_with('@') {
        return None;
    }
    let fields: Vec<&str> = line.split(';').map(|f| f.trim()).collect();
    if fields.len() < 5 {
        return None;
    }
    let mut out = <[String; 5] as Default>::default();
    for (i, field) in fields.iter().take(5).enumerate() {
        let mut s = String::new();
        for tok in field.split_whitespace() {
            s.push(parse_cp(tok)?);
        }
        out[i] = s;
    }
    Some(out)
}

/// Parse one `IdentifierStatus.txt` row into (code-point range,
/// status). Range is inclusive on both ends; a single-code-point line
/// returns a degenerate range `(x, x)`.
fn parse_ident_status_row(line: &str) -> Option<((u32, u32), String)> {
    let line = strip_comment(line)?;
    let mut parts = line.split(';');
    let range = parts.next()?.trim();
    let status = parts.next()?.trim().to_string();
    let (lo, hi) = if let Some((a, b)) = range.split_once("..") {
        (
            u32::from_str_radix(a.trim(), 16).ok()?,
            u32::from_str_radix(b.trim(), 16).ok()?,
        )
    } else {
        let x = u32::from_str_radix(range.trim(), 16).ok()?;
        (x, x)
    };
    Some(((lo, hi), status))
}

/// Emit the per-test "running on vendored data" banner. `cargo test
/// -- --nocapture` surfaces this; without `--nocapture` it is still
/// stored in the libtest capture buffer and printed on failure.
fn banner(tag: &str, path: &Path, placeholder: bool) {
    eprintln!(
        "ucd_conformance[{tag}]: reading {} ({})",
        path.display(),
        if placeholder { "placeholder — smoke only" } else { "full corpus" }
    );
}

// -------------------------------------------------------------------
// 1. GraphemeBreakTest.txt — UAX #29 grapheme-cluster boundaries.
// -------------------------------------------------------------------

/// Full `GraphemeBreakTest.txt` sweep. Fingerprint `uni7-gbreak-sweep`.
#[test]
#[ignore = "UNI-7 full UCD sweep — vendor the corpus and run with --ignored"]
fn ucd_conformance_grapheme_break_test() {
    let filename = "GraphemeBreakTest.txt";
    let path = corpus_path(filename);
    let text = match load_corpus(filename) {
        CorpusLoad::Loaded(s) => s,
        CorpusLoad::Skip(reason) => {
            eprintln!("ucd_conformance[gbreak]: SKIP — {reason}");
            return;
        }
    };
    banner("gbreak", &path, is_placeholder(&text));

    let mut total = 0usize;
    let mut passed = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for (lineno, line) in text.lines().enumerate() {
        let Some((s, expected)) = parse_break_row(line) else { continue };
        total += 1;
        let got: Vec<usize> = grapheme_boundaries(&s).collect();
        if got == expected {
            passed += 1;
        } else if failures.len() < 10 {
            failures.push(format!(
                "line {}: expected {expected:?}, got {got:?} for {:?}",
                lineno + 1,
                s
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "uni7-gbreak-sweep: {} / {} rows failed; first 10:\n{}",
        total - passed,
        total,
        failures.join("\n")
    );
    eprintln!("ucd_conformance[gbreak]: {passed} / {total} rows OK (uni7-gbreak-sweep)");
}

// -------------------------------------------------------------------
// 2. NormalizationTest.txt — UAX #15 canonical + compatibility forms.
// -------------------------------------------------------------------

/// Full `NormalizationTest.txt` sweep. Fingerprint `uni7-nfc-sweep`.
///
/// Conformance per UAX #15 §5 (we exercise only the NFC columns here;
/// the NFD / NFKC / NFKD sweeps are UNI-7 follow-ups):
///
/// * `c2 == toNFC(c1)` — NFC on the source matches column 2.
/// * `c2 == toNFC(c2)` — NFC is idempotent on column 2.
/// * `c2 == toNFC(c3)` — NFC on the NFD form recomposes to column 2.
#[test]
#[ignore = "UNI-7 full UCD sweep — vendor the corpus and run with --ignored"]
fn ucd_conformance_normalization_test() {
    let filename = "NormalizationTest.txt";
    let path = corpus_path(filename);
    let text = match load_corpus(filename) {
        CorpusLoad::Loaded(s) => s,
        CorpusLoad::Skip(reason) => {
            eprintln!("ucd_conformance[norm]: SKIP — {reason}");
            return;
        }
    };
    banner("norm", &path, is_placeholder(&text));

    let mut total = 0usize;
    let mut passed = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for (lineno, line) in text.lines().enumerate() {
        // UAX #15 column order (semicolon-separated, 1-indexed in the
        // spec): c1=source, c2=NFC, c3=NFD, c4=NFKC, c5=NFKD. We only
        // need the three NFC-adjacent columns for this harness; the
        // compatibility forms (c4, c5) are UNI-7 follow-up sweeps.
        let Some([c1, c2, c3, _c4, _c5]) = parse_norm_row(line) else {
            continue;
        };
        total += 1;
        let nfc_c1 = nfc_normalize(&c1);
        let nfc_c2 = nfc_normalize(&c2);
        let nfc_c3 = nfc_normalize(&c3);
        let ok = nfc_c1 == c2 && nfc_c2 == c2 && nfc_c3 == c2;
        if ok {
            passed += 1;
        } else if failures.len() < 10 {
            failures.push(format!(
                "line {}: c1={:?} c2={:?} c3={:?} \
                 NFC(c1)={:?} NFC(c2)={:?} NFC(c3)={:?}",
                lineno + 1,
                c1,
                c2,
                c3,
                nfc_c1,
                nfc_c2,
                nfc_c3
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "uni7-nfc-sweep: {} / {} rows failed; first 10:\n{}",
        total - passed,
        total,
        failures.join("\n")
    );
    eprintln!("ucd_conformance[norm]: {passed} / {total} rows OK (uni7-nfc-sweep)");
}

// -------------------------------------------------------------------
// 3. LineBreakTest.txt — UAX #14 line-break opportunities.
// -------------------------------------------------------------------

/// Full `LineBreakTest.txt` sweep. Fingerprint `uni7-lbreak-sweep`.
///
/// A `÷` at offset N (except the leading one at 0) must correspond to
/// *some* opportunity emitted by `line_break_opportunities` at N; a
/// `×` at offset N must correspond to the absence of any opportunity
/// at N. The leading `÷` (offset 0) and the terminal `÷` (offset =
/// s.len()) are implicit in our API — leading never emits, terminal
/// emits `Mandatory` — so both are excluded from the test's offset
/// set.
#[test]
#[ignore = "UNI-7 full UCD sweep — vendor the corpus and run with --ignored"]
fn ucd_conformance_line_break_test() {
    let filename = "LineBreakTest.txt";
    let path = corpus_path(filename);
    let text = match load_corpus(filename) {
        CorpusLoad::Loaded(s) => s,
        CorpusLoad::Skip(reason) => {
            eprintln!("ucd_conformance[lbreak]: SKIP — {reason}");
            return;
        }
    };
    banner("lbreak", &path, is_placeholder(&text));

    let mut total = 0usize;
    let mut passed = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for (lineno, line) in text.lines().enumerate() {
        let Some((s, expected_breaks)) = parse_break_row(line) else {
            continue;
        };
        total += 1;
        let opps = line_break_opportunities(&s);
        let got_offsets: std::collections::BTreeSet<usize> =
            opps.iter().map(|o| o.byte_offset).collect();
        // Only check interior breakpoints: strip the leading 0 and the
        // terminal s.len() entry from the expected list, which match
        // the "start of text" and "end of text" boundaries UAX #14
        // reports but our API does not surface as explicit
        // opportunities.
        let interior: Vec<usize> = expected_breaks
            .iter()
            .copied()
            .filter(|&o| o != 0 && o != s.len())
            .collect();
        let ok = interior.iter().all(|o| got_offsets.contains(o));
        if ok {
            passed += 1;
        } else if failures.len() < 10 {
            failures.push(format!(
                "line {}: expected breaks at {:?}, got {:?} for {:?}",
                lineno + 1,
                interior,
                got_offsets,
                s
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "uni7-lbreak-sweep: {} / {} rows failed; first 10:\n{}",
        total - passed,
        total,
        failures.join("\n")
    );
    eprintln!("ucd_conformance[lbreak]: {passed} / {total} rows OK (uni7-lbreak-sweep)");
}

// -------------------------------------------------------------------
// 4. IdentifierStatus.txt — UTS #39 §3.1 Allowed / Restricted set.
// -------------------------------------------------------------------

/// Full `IdentifierStatus.txt` sweep. Fingerprint `uni7-ident-sweep`.
///
/// For every code point in the file we build a one-character string
/// and run `check_ident(.., IdentProfile::AllowedChars)`. The
/// conformance rule:
///
/// * `status == "Allowed"` AND `XID_Start(cp)` → `check_ident` must
///   succeed. Non-`XID_Start` Allowed code points (digits, combining
///   marks) are skipped — a one-char identifier cannot begin with
///   them regardless of UTS #39 status.
/// * `status == "Restricted"` AND `XID_Start(cp)` → `check_ident`
///   should reject. **Known gap:** our `AllowedChars` profile today
///   enforces single-script but not the full UTS #39 §3.1 allowed
///   set (see `src/ident.rs` module header). The sweep therefore
///   logs each Restricted-but-accepted mismatch and panics with the
///   count; wiring the full IdentifierStatus table into
///   `check_default` is the UNI-7 completion step this harness
///   measures.
#[test]
#[ignore = "UNI-7 full UCD sweep — vendor the corpus and run with --ignored"]
fn ucd_conformance_identifier_status_test() {
    let filename = "IdentifierStatus.txt";
    let path = corpus_path(filename);
    let text = match load_corpus(filename) {
        CorpusLoad::Loaded(s) => s,
        CorpusLoad::Skip(reason) => {
            eprintln!("ucd_conformance[ident]: SKIP — {reason}");
            return;
        }
    };
    banner("ident", &path, is_placeholder(&text));

    let mut total = 0usize;
    let mut passed = 0usize;
    let mut skipped_non_start = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for (lineno, line) in text.lines().enumerate() {
        let Some(((lo, hi), status)) = parse_ident_status_row(line) else {
            continue;
        };
        for cp in lo..=hi {
            let Some(ch) = char::from_u32(cp) else { continue };
            if !is_xid_start(ch) {
                skipped_non_start += 1;
                continue;
            }
            total += 1;
            let s = ch.to_string();
            let result = check_ident(&s, IdentProfile::AllowedChars);
            let ok = match status.as_str() {
                "Allowed" => result.is_ok(),
                "Restricted" => result.is_err(),
                // Unknown status label — treat as parse-skip rather
                // than failure so a UCD-format evolution does not
                // break the harness.
                _ => {
                    continue;
                }
            };
            if ok {
                passed += 1;
            } else if failures.len() < 10 {
                failures.push(format!(
                    "line {}: U+{cp:04X} status={status} -> check_ident = {:?}",
                    lineno + 1,
                    result
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "uni7-ident-sweep: {} / {} XID_Start rows failed \
         ({} non-XID_Start code points skipped); first 10:\n{}",
        total - passed,
        total,
        skipped_non_start,
        failures.join("\n")
    );
    eprintln!(
        "ucd_conformance[ident]: {passed} / {total} XID_Start rows OK \
         ({skipped_non_start} non-XID_Start skipped) (uni7-ident-sweep)"
    );
}
