//! Issue #1313 — `compute_bss_size_from_type` silently emitted 8 bytes for any
//! `[T; N]` array length that wasn't a bare integer literal.
//!
//! `pub let mut _task_pool : [u64; MAX_PIDS * TASK_SLOT_QWORDS] = uninit` in
//! paideia-os (paideia-os#1594) linked at 8 bytes instead of 142336, so pids
//! past the first wrote complete task structures into the adjacent
//! kernel-stack region with no diagnostic anywhere.
//!
//! These tests pin both directions of the fix:
//! - a single-identifier length that names a resolvable module-level
//!   constant sizes correctly (verified via the linked symbol's `st_size`,
//!   not merely a successful build — the whole reason #1313 and its sibling
//!   #1308/#1309 survived is that nothing checked the size);
//! - any length that isn't a literal or a resolvable constant (an
//!   expression, a `let mut`) is a hard T0577 error instead of an 8-byte
//!   guess.

use object::{Object, ObjectSymbol};

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn identifier_array_length_resolves_module_constant() {
    // `pub let SOME_CONST : u64 = 12` then `pub let mut arr : [u64; SOME_CONST] = uninit`.
    let out = run_build(build_emit("pa1313_bss_const_len.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");
    let sym = file
        .symbols()
        .find(|s| s.name().unwrap_or("") == "arr")
        .expect("symbol `arr` not found in ELF");

    assert_eq!(
        sym.size(),
        12 * 8,
        "[u64; SOME_CONST] with SOME_CONST = 12 must link at 96 bytes, not 8"
    );
}

#[test]
fn expression_array_length_is_rejected_with_t0577() {
    // `[u64; MAX_PIDS * SLOT_QWORDS] = uninit` — a binary expression is not a
    // single resolvable identifier; this must fail loudly, not link at 8 bytes.
    let out = run_build(build_emit("pa1313_bss_unresolvable_len.pdx"));
    out.assert_diag("T0577");
}
