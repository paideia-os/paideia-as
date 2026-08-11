//! PA-v0.21-009 (#1285): TlbOps lowering round-trips.
//!
//! Coverage:
//!   invlpg_single(va)        → invlpg [rdi + 0]     (SysVRegs; va in RDI)
//!   flush_cache_writeback()  → wbinvd               (Literal; nullary)
//!
//! invpcid_single / invpcid_all_nonglobal deliberately NOT tested here —
//! their lowering waits on the INVPCID mnemonic landing across the
//! exhaustive Mnemonic-match sites (encoder + IR passes). The recipe
//! registry currently returns None for those two, and the emit path
//! therefore falls through to normal call emission (a paideia-os call
//! site would see an unresolved-intrinsic diagnostic — no silent
//! success). Adding placeholder tests today would burn the harness on a
//! shape we know is going to change.

use paideia_as_ir::{
    InstrMode, IrArena, IrNodeId, abi,
    instruction::{Mnemonic, Operand, Scale},
};
use paideia_as_elaborator::stdlib_lowering::{ArgConvention, lower_stdlib_method};

#[test]
fn tlb_ops_invlpg_single_lowers_to_invlpg_mem_rdi() {
    let arena = IrArena::new();
    let va_id = IrNodeId::new(1).expect("valid node id");

    let recipe =
        lower_stdlib_method("TlbOps", "invlpg_single", InstrMode::Mode64, &[va_id], &arena)
            .expect("TlbOps::invlpg_single must be registered")
            .expect("invlpg_single lowering must succeed");

    assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
    assert!(recipe.labels.is_empty());
    assert_eq!(recipe.instructions.len(), 1);

    let inst = &recipe.instructions[0];
    assert_eq!(inst.mnemonic, Mnemonic::Invlpg);
    assert_eq!(inst.operands.len(), 1);
    assert_eq!(inst.mode, InstrMode::Mode64);

    match &inst.operands[0] {
        Operand::MemSib {
            base,
            index,
            scale,
            disp,
        } => {
            assert_eq!(*base, abi::RDI, "va arrives in RDI (SysV arg 0)");
            assert!(index.is_none(), "invlpg has no index register");
            assert!(matches!(scale, Scale::X1));
            assert_eq!(*disp, 0);
        }
        _ => panic!("invlpg operand must be MemSib{{RDI, None, X1, 0}}"),
    }
}

#[test]
fn tlb_ops_flush_cache_writeback_lowers_to_wbinvd() {
    let arena = IrArena::new();
    let recipe = lower_stdlib_method(
        "TlbOps",
        "flush_cache_writeback",
        InstrMode::Mode64,
        &[],
        &arena,
    )
    .expect("TlbOps::flush_cache_writeback must be registered")
    .expect("flush_cache_writeback lowering must succeed");

    assert_eq!(recipe.arg_convention, ArgConvention::Literal);
    assert!(recipe.labels.is_empty());
    assert_eq!(recipe.instructions.len(), 1);
    assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Wbinvd);
    assert!(recipe.instructions[0].operands.is_empty());
}

#[test]
fn tlb_ops_invpcid_single_is_deferred_returns_none_for_now() {
    // Until INVPCID lands, TlbOps::invpcid_single is unregistered. This test
    // documents the intentional gap so it flips green (with an updated
    // assertion) the moment the recipe lands.
    let arena = IrArena::new();
    let pcid_id = IrNodeId::new(1).expect("valid node id");
    let va_id = IrNodeId::new(2).expect("valid node id");
    let result = lower_stdlib_method(
        "TlbOps",
        "invpcid_single",
        InstrMode::Mode64,
        &[pcid_id, va_id],
        &arena,
    );
    assert!(
        result.is_none(),
        "invpcid_single is a documented deferred recipe (see stdlib_lowering.rs \
         v0.21-009 note). If this assertion fails, INVPCID has landed — update \
         this test to assert the recipe shape."
    );
}

#[test]
fn tlb_ops_invpcid_all_nonglobal_is_deferred_returns_none_for_now() {
    let arena = IrArena::new();
    let pcid_id = IrNodeId::new(1).expect("valid node id");
    let result = lower_stdlib_method(
        "TlbOps",
        "invpcid_all_nonglobal",
        InstrMode::Mode64,
        &[pcid_id],
        &arena,
    );
    assert!(
        result.is_none(),
        "invpcid_all_nonglobal is a documented deferred recipe"
    );
}

#[test]
fn unknown_tlb_method_returns_none() {
    let arena = IrArena::new();
    let result = lower_stdlib_method(
        "TlbOps",
        "invlpg_all", // typo
        InstrMode::Mode64,
        &[],
        &arena,
    );
    assert!(result.is_none());
}
