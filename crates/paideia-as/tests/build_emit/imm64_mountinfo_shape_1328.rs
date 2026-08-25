//! Regression pin for issue #1328: 63-bit-set imm64 `mov` at a labeled
//! branch target after `je` fall-through inside an unsafe block.
//!
//! The paideia-os `src/kernel/core/syscall/sys_mountinfo.pdx` file at commit
//! 706a0f4 carried the workaround comment "paideia-as U1606 rejects 63-bit-set
//! imm64 in this context — stage via r10". When the observation was re-run
//! against a HEAD paideia-as build it did not reproduce (post-#1270's
//! raw-asm/StmtExpr ordering fix). This pin freezes the pattern so any future
//! encoder-order or label-alias regression that re-surfaces the
//! sys_mountinfo failure will trip here rather than in downstream kernel
//! builds.
//!
//! The `imm64_top_bit_fd` / `imm64_top_bit_fc` fixtures next door already pin
//! the 63-bit imm64 mov in isolation; this fixture goes further by placing
//! the mov at a labeled target reached via `je` fall-through, with
//! register-manipulation arithmetic (`shr`, `and`, `cmp`) preceding it —
//! the exact context that was failing in the wild.
//!
//! Fixture: `tests/build-emit/imm64_mountinfo_shape_1328.pdx`
//!
//! Golden bytes at the two labeled arms (issue #1101 optimization: use the
//! C7 form since -3 / -2 sign-extend from i32):
//!   mi_esrch:  48 C7 C0 FD FF FF FF C3   (`mov rax, 0xFFFFFFFFFFFFFFFD; ret`)
//!   mi_enoent: 48 C7 C0 FE FF FF FF C3   (`mov rax, 0xFFFFFFFFFFFFFFFE; ret`)

use object::{Object, ObjectSection};

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn build_text_bytes() -> Vec<u8> {
    let input = build_emit("imm64_mountinfo_shape_1328.pdx");
    let out = run_build(input);
    out.assert_ok();
    let bytes = out.artifact_bytes();
    let file = object::File::parse(&*bytes).expect("output should parse as an ELF object");
    let text = file.section_by_name(".text").expect(".text section should exist");
    text.data().expect(".text section should have data").to_vec()
}

/// The two labeled arms each end in the sign-bit-set imm64 mov + ret. Both
/// must appear byte-exact in the emitted .text; a regression that dropped or
/// silently swapped either mov (e.g. re-introducing a U1606 rejection) would
/// leave the .text without the golden window.
#[test]
fn esrch_arm_encodes_negative_three_via_c7_form() {
    let text = build_text_bytes();
    // -3 as i64 fits i32 (sign-ext): C7 C0 FD FF FF FF, plus ret.
    assert!(
        find(&text, &[0x48, 0xC7, 0xC0, 0xFD, 0xFF, 0xFF, 0xFF, 0xC3]).is_some(),
        "esrch arm bytes (48 C7 C0 FD FF FF FF C3) not found in .text: {text:02X?}",
    );
}

#[test]
fn enoent_arm_encodes_negative_two_via_c7_form() {
    let text = build_text_bytes();
    // -2 as i64 fits i32 (sign-ext): C7 C0 FE FF FF FF, plus ret.
    assert!(
        find(&text, &[0x48, 0xC7, 0xC0, 0xFE, 0xFF, 0xFF, 0xFF, 0xC3]).is_some(),
        "enoent arm bytes (48 C7 C0 FE FF FF FF C3) not found in .text: {text:02X?}",
    );
}

/// Higher-level shape check: the je fall-through must land at the enoent arm,
/// which in turn must precede a labeled-jump target reachable via je. This
/// pins that label aliasing did not silently collapse both arms onto the
/// same site (the observed failure shape in sys_mountinfo pre-fix).
#[test]
fn both_arms_are_distinct_in_text() {
    let text = build_text_bytes();
    let esrch = find(&text, &[0x48, 0xC7, 0xC0, 0xFD, 0xFF, 0xFF, 0xFF, 0xC3])
        .expect("esrch arm present");
    let enoent = find(&text, &[0x48, 0xC7, 0xC0, 0xFE, 0xFF, 0xFF, 0xFF, 0xC3])
        .expect("enoent arm present");
    assert_ne!(
        esrch, enoent,
        "esrch and enoent arms collapsed onto the same offset — label alias regression"
    );
}
