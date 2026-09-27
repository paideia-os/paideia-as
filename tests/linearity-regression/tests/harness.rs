//! `cargo test --test harness -p paideia-linearity-regression` runs the
//! seed corpus.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use paideia_linearity_regression::{parse_expect_file, parse_s_codes_from_stderr_public, s_codes_for};

fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn collect_pdx_files(dir: &Path) -> Vec<PathBuf> {
    if !dir.exists() {
        return Vec::new();
    }
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("read corpus dir")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("pdx"))
        .collect();
    out.sort();
    out
}

#[test]
fn accept_corpus_emits_no_s_codes() {
    let dir = corpus_root().join("accept");
    let files = collect_pdx_files(&dir);
    let mut failures = Vec::new();
    for path in &files {
        match s_codes_for(path) {
            Ok(codes) if codes.is_empty() => {}
            Ok(codes) => failures.push(format!(
                "{}: expected no S-codes, got {:?}",
                path.file_name().unwrap().to_string_lossy(),
                codes
            )),
            Err(e) => failures.push(format!(
                "{}: harness error: {e}",
                path.file_name().unwrap().to_string_lossy()
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "{} accept files failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// Reject corpus is documentation-by-example until the IR carries
// structured symbol/binding payloads (m2/m5).
//
// The harness now invokes `paideia-as build` via subprocess (m1-010),
// so the CLI wiring is complete. However, the LinearityWalker (and other
// walkers) runs end-to-end against kind-only IR. Linearity classes and
// effect/capability payloads are empty, so the walkers cannot fire
// S0901/S0903 (overused/wrong-effect) diagnostics on real source yet.
//
// Once m2/m5 inject structured payloads at lowering time, this test
// will light up and catch regressions where the accept corpus stops
// being clean.
//
// ---------------------------------------------------------------------
// paideia-as#1532 (PAS-DEBT-B7-004) — reactivation status (2026-09-27)
//
// The debt-catalog row (`design/paideia-as-debt-catalog.md` §S, row 468)
// summarises the blocker as "awaiting borrow-checker phase-4 driver
// hookup". That is one of two independent gaps — this test cannot be
// re-enabled until BOTH land. Removing `#[ignore]` today would fail on
// all 24 reject fixtures because no walker fires on any of them.
//
// ## Gap 1 — LinearityWalker payload starvation (lowering side)
//
// `LinearityWalker` (`crates/paideia-as-elaborator/src/check_linearity.rs`)
// is registered in `crates/paideia-as/src/cmd_build/walker_pipeline.rs`
// step 1 and its unit tests fire S0900/S0901/S0902/S0903/S0904 via
// direct arena manipulation. End-to-end from `.pdx` source, however,
// every IR node is minted with `LinClass::Unrestricted` (see
// `crates/paideia-as-elaborator/src/lower.rs:137` doc comment).
// `crates/paideia-as-elaborator/src/kind.rs::type_kind` only assigns
// `Linear` / `Affine` when the type is `Type::Ref { mutable }` — no
// path today lowers a bare `let unused_linear = 1` (see
// `reject/s0900_never_used_01.pdx`) into a Linear binding, and the
// surface syntax has no `linear` / `affine` keyword or class annotation
// hook. The walker therefore observes only Unrestricted symbols on the
// reject corpus and stays silent. This gap belongs to phase-3 m2/m5
// (structured payload injection) — no separate issue tracks it yet;
// file one and cite `crates/paideia-as-elaborator/src/kind.rs::type_kind`
// as the extension point.
//
// ## Gap 2 — Borrow-checker walker not wired into the pipeline
//
// `crates/paideia-as-elaborator/src/borrow_walker.rs::BorrowWalker`
// ships with unit tests for its two-code lattice (its own S0906 =
// immut+mut conflict, S0907 = double mut) but has **zero** references
// from `crates/paideia-as/src/`: `grep -rn BorrowWalker crates/paideia-as/src/`
// returns nothing. It is not part of the walker_pipeline.rs sequence
// (LinearityWalker → EffectRowWalker → CapWalker → EmitWalker), so no
// user source ever exercises it. `reject/s0906_*.pdx` and
// `reject/s0907_*.pdx` are placeholders (`let x = 1`) that additionally
// disagree with LinearityWalker's own spec (which numbers S0906 = branch
// mismatch, S0907 = illegal lambda capture — see
// `check_linearity.rs:167`). Wiring BorrowWalker into the pipeline is
// the "borrow-checker phase-4 driver hookup" the catalog cites; that
// hookup also has no tracking issue yet.
//
// ## Expected S-codes and fixture inventory (24 files)
//
// After both gaps close, the test asserts one `.expect` per fixture:
//   * S0900 (never used):                     3 fixtures — s0900_never_used_0[1-3].pdx
//   * S0901 (overused):                       3 fixtures — s0901_overused_0[1-3].pdx
//   * S0902 (let shadows unconsumed linear):  2 fixtures — s0902_reserved_0[1-2].pdx
//   * S0903 (out-of-order use of ordered):    3 fixtures — s0903_out_of_order_0[1-3].pdx
//   * S0904 (affine consumed in multi-arms):  4 fixtures — s0904_match_arms_*, s0904_reserved_0[1-2]
//   * S0905 (handler reorders ordered):       4 fixtures — s0905_*.pdx (linearity-side spec)
//   * S0906 (branch mismatch OR borrow):      3 fixtures — s0906_branch_mismatch_0[1-3]
//   * S0907 (illegal capture OR double mut):  2 fixtures — s0907_illegal_capture_0[1-2]
//
// Note the S0906/S0907 spec collision between `check_linearity.rs`
// (branch mismatch / illegal capture) and `borrow_walker.rs`
// (immut+mut / double mut) — reactivation must first pick a canonical
// numbering. The fixture filenames follow the linearity-side numbering;
// if the borrow-side wins, either the fixtures rename or two disjoint
// S-code bands split (e.g. S0906/S0907 → linearity, S0910/S0911 →
// borrow). This is a design decision that predates reactivation.
//
// ## Recommended reactivation sequence
//
// 1. Land phase-3 m2/m5 payload injection so LinearityWalker sees
//    structured symbols on real `.pdx` source.
// 2. Wire BorrowWalker into `walker_pipeline.rs` after LinearityWalker
//    (region entry/exit hooks come from the IR's scope structure).
// 3. Resolve the S0906/S0907 spec collision (one-line design note).
// 4. Rewrite reject fixtures so their `.pdx` actually exhibits the
//    condition the `.expect` file names (today most are `let x = 1`
//    stubs kept green so the harness compiles).
// 5. Remove this `#[ignore]`.
//
// Steps 1–2 are independent items; steps 3–5 depend on 1+2 landing.
// ---------------------------------------------------------------------
#[test]
#[ignore = "paideia-as#1532 (B7-004): compound-blocked — LinearityWalker sees only LinClass::Unrestricted end-to-end (phase-3 m2/m5 payload injection missing) AND BorrowWalker is not wired into walker_pipeline.rs. See doc-comment above for the reactivation sequence and fixture inventory."]
fn reject_corpus_emits_expected_s_codes() {
    let dir = corpus_root().join("reject");
    let files = collect_pdx_files(&dir);
    let mut failures = Vec::new();
    for path in &files {
        let expect_path = path.with_extension("expect");
        let expected: BTreeSet<String> = match std::fs::read_to_string(&expect_path) {
            Ok(s) => parse_expect_file(&s),
            Err(_) => {
                failures.push(format!(
                    "{}: missing .expect sidecar at {}",
                    path.file_name().unwrap().to_string_lossy(),
                    expect_path.display()
                ));
                continue;
            }
        };
        match s_codes_for(path) {
            Ok(codes) if codes == expected => {}
            Ok(codes) => failures.push(format!(
                "{}: expected {:?}, got {:?}",
                path.file_name().unwrap().to_string_lossy(),
                expected,
                codes
            )),
            Err(e) => failures.push(format!(
                "{}: harness error: {e}",
                path.file_name().unwrap().to_string_lossy()
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "{} reject files failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Issue #1268 regression: an `S`-code embedded in an identifier (e.g., the
/// PascalCase form of a file basename echoed inside an M0305 note like
/// `expected 'PtrCopyNoS0901'`) must NOT be extracted as an S-diagnostic.
/// Before the word-boundary fix, `parse_s_codes_from_stderr` picked up
/// `S0901` from `PtrCopyNoS0901` and reported it, breaking the two accept
/// fixtures whose filenames contained `no_s0901`.
#[test]
fn parse_s_codes_ignores_ident_embedded_code_1268() {
    // The exact fragment that shipped in the M0305 diagnostic note:
    let msg = "expected 'PtrCopyNoS0901'";
    assert!(
        parse_s_codes_from_stderr_public(msg).is_empty(),
        "identifier-embedded S0901 must not register as an S-code"
    );
}

#[test]
fn parse_s_codes_still_extracts_bare_codes_1268() {
    // Sanity: bona-fide diagnostic lines are still picked up.
    let msg = "error[S0901]: linear binding overused";
    let codes = parse_s_codes_from_stderr_public(msg);
    assert_eq!(codes.len(), 1);
    assert!(codes.contains("S0901"));
}

#[test]
fn parse_s_codes_mixed_ident_and_bare_1268() {
    // Both forms in one buffer: only the bare one should be extracted.
    let msg = "note: expected 'PtrCopyNoS0901'\nerror[S0902]: let shadowing";
    let codes = parse_s_codes_from_stderr_public(msg);
    assert_eq!(codes.len(), 1);
    assert!(codes.contains("S0902"));
    assert!(!codes.contains("S0901"));
}

#[test]
fn parse_expect_file_basic() {
    let s = "S0901\n# a comment\nS0903\n\n   S0904   \n";
    let parsed = parse_expect_file(s);
    let expected: BTreeSet<String> = ["S0901", "S0903", "S0904"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(parsed, expected);
}
