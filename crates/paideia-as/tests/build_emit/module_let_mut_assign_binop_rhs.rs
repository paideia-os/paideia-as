//! #1181: module-let-mut assignment with BinOp RHS (expression tree lowering).

use object::{Object, ObjectSection, ObjectSymbol};

use crate::common::elf::symbol_bytes;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn bitmap_set_no_t0540() {
    let out = run_build(build_emit("module_let_mut_assign_binop_rhs.pdx"));
    out.assert_ok();
    assert!(!out.stderr.contains("T0540"), "T0540 must not fire on BinOp RHS after #1181");
    // Also check that bitmap_clear (with BitNot) doesn't fire T0540
    assert!(!out.stderr.contains("bitmap_clear"), "bitmap_clear must compile without errors");
}

#[test]
fn bitmap_set_emits_load_op_store() {
    let out = run_build(build_emit("module_let_mut_assign_binop_rhs.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let set_bytes = symbol_bytes(&bytes, "bitmap_set").expect("bitmap_set symbol");

    // Fix signature 1: arg1 of shift lands in R11 (49 89 FB = mov r11, rdi).
    assert!(
        set_bytes.windows(3).any(|w| w == [0x49, 0x89, 0xFB]),
        "bitmap_set must contain `mov r11, rdi` (49 89 FB) — proves arg1 does NOT clobber R10"
    );

    // Fix signature 2: shift count comes from R11 (4C 89 D9 = mov rcx, r11).
    assert!(
        set_bytes.windows(3).any(|w| w == [0x4C, 0x89, 0xD9]),
        "bitmap_set must contain `mov rcx, r11` (4C 89 D9) — proves shift count from R11"
    );

    // Positive: mov r10, 1 (49 C7 C2 01 00 00 00) preserved as shift arg0.
    assert!(
        set_bytes.windows(7).any(|w| w == [0x49, 0xC7, 0xC2, 0x01, 0x00, 0x00, 0x00]),
        "bitmap_set must contain `mov r10, 1` (49 C7 C2 01 00 00 00) — shift arg0 preserved"
    );

    // Positive: shl r10, cl (49 D3 E2).
    assert!(
        set_bytes.windows(3).any(|w| w == [0x49, 0xD3, 0xE2]),
        "bitmap_set must contain `shl r10, cl` (49 D3 E2)"
    );

    // Positive: or rax, r10 (4C 09 D0) — outer combine.
    assert!(
        set_bytes.windows(3).any(|w| w == [0x4C, 0x09, 0xD0]),
        "bitmap_set must contain `or rax, r10` (4C 09 D0)"
    );

    // RIP-relative load + store framing.
    assert!(
        set_bytes.windows(3).any(|w| w == [0x48, 0x8B, 0x05]),
        "bitmap_set must contain RIP-relative load prefix (48 8B 05)"
    );
    assert!(
        set_bytes.windows(3).any(|w| w == [0x48, 0x89, 0x05]),
        "bitmap_set must contain RIP-relative store prefix (48 89 05)"
    );

    // REGRESSION TRIP-WIRE: buggy code emitted `mov r10, rdi` (49 89 FA) — must NOT appear.
    assert!(
        !set_bytes.windows(3).any(|w| w == [0x49, 0x89, 0xFA]),
        "bitmap_set must NOT contain `mov r10, rdi` (49 89 FA) — that is the pre-corrective clobber bug"
    );
}

#[test]
fn bitmap_set_has_relocations_to_priority_bitmap() {
    let out = run_build(build_emit("module_let_mut_assign_binop_rhs.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    let mut reloc_count = 0;

    for section in file.sections() {
        for (_offset, relocation) in section.relocations() {
            if let object::RelocationTarget::Symbol(idx) = relocation.target() {
                if let Ok(symbol) = file.symbol_by_index(idx) {
                    if let Ok(name) = symbol.name() {
                        if name == "priority_bitmap" {
                            // Count by addend: -4 is the canonical RIP-relative addend
                            if relocation.addend() == -4 {
                                // We can't distinguish load from store by relocation alone,
                                // but we expect 2 total: one for load, one for store.
                                if matches!(relocation.kind(), object::RelocationKind::Relative) {
                                    reloc_count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // For bitmap_set: 1 load + 1 store = 2 relocations
    assert!(reloc_count >= 2, "must have at least 2 Relative relocations to priority_bitmap (load + store); got {}", reloc_count);
}

#[test]
fn bitmap_clear_emits_bitnot_and_and() {
    let out = run_build(build_emit("module_let_mut_assign_binop_rhs.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let clr_bytes = symbol_bytes(&bytes, "bitmap_clear").expect("bitmap_clear symbol");

    // not r10 (49 F7 D2).
    assert!(
        clr_bytes.windows(3).any(|w| w == [0x49, 0xF7, 0xD2]),
        "bitmap_clear must contain `not r10` (49 F7 D2)"
    );

    // and rax, r10 (4C 21 D0).
    assert!(
        clr_bytes.windows(3).any(|w| w == [0x4C, 0x21, 0xD0]),
        "bitmap_clear must contain `and rax, r10` (4C 21 D0)"
    );

    // Fix signature: mov r11, rdi.
    assert!(
        clr_bytes.windows(3).any(|w| w == [0x49, 0x89, 0xFB]),
        "bitmap_clear must contain `mov r11, rdi` (49 89 FB) — arg1 does not clobber R10"
    );

    // Regression trip-wire.
    assert!(
        !clr_bytes.windows(3).any(|w| w == [0x49, 0x89, 0xFA]),
        "bitmap_clear must NOT contain `mov r10, rdi` (49 89 FA) — pre-corrective clobber bug"
    );
}

#[test]
fn depth_three_binop_rhs_fires_t0540() {
    let out = run_build(build_emit("module_let_mut_assign_binop_rhs_depth_limit.pdx"));
    // Depth-3+ nesting (p1 | (p2 | (p3 | p4))) should be rejected
    let exit_code = out.exit_code().unwrap_or(0);
    assert!(
        exit_code != 0,
        "depth-3 nesting must produce an error; exit code: {}",
        exit_code
    );
    assert!(
        out.stderr.contains("T0540"),
        "depth-3 nesting must fire T0540; got stderr:\n{}",
        out.stderr
    );
}
