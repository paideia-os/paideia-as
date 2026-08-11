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
fn tlb_ops_invpcid_single_lowers_to_stack_descriptor_and_invpcid() {
    // v0.21-009-followup (#1297): INVPCID mnemonic landed; TlbOps::invpcid_single
    // now emits the descriptor-on-stack recipe.
    //
    // Expected sequence (SysVRegs; pcid in RDI, va in RSI):
    //   sub rsp, 16
    //   mov [rsp + 0], rdi        ; descriptor low  = pcid
    //   mov [rsp + 8], rsi        ; descriptor high = va
    //   mov rax, 0                ; type = 0 (individual-address)
    //   invpcid rax, [rsp]
    //   add rsp, 16
    let arena = IrArena::new();
    let pcid_id = IrNodeId::new(1).expect("valid node id");
    let va_id = IrNodeId::new(2).expect("valid node id");
    let recipe = lower_stdlib_method(
        "TlbOps",
        "invpcid_single",
        InstrMode::Mode64,
        &[pcid_id, va_id],
        &arena,
    )
    .expect("TlbOps::invpcid_single must be registered")
    .expect("invpcid_single lowering must succeed");

    assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
    assert!(recipe.labels.is_empty());
    assert_eq!(recipe.instructions.len(), 6, "sub/store-lo/store-hi/mov-type/invpcid/add");

    let mnemonics: Vec<Mnemonic> =
        recipe.instructions.iter().map(|i| i.mnemonic).collect();
    assert_eq!(
        mnemonics,
        vec![
            Mnemonic::Sub,
            Mnemonic::Mov,
            Mnemonic::Mov,
            Mnemonic::Mov,
            Mnemonic::Invpcid,
            Mnemonic::Add,
        ],
    );

    // Spot-check the INVPCID operand shape: [Reg(RAX), MemSib{RSP, ..., disp=0}].
    let invpcid = &recipe.instructions[4];
    assert_eq!(invpcid.operands.len(), 2);
    match (&invpcid.operands[0], &invpcid.operands[1]) {
        (Operand::Reg(r), Operand::MemSib { base, index, scale, disp }) => {
            assert_eq!(*r, abi::RAX, "type register is RAX");
            assert_eq!(*base, abi::RSP, "descriptor address is [rsp]");
            assert!(index.is_none());
            assert!(matches!(scale, Scale::X1));
            assert_eq!(*disp, 0);
        }
        _ => panic!("invpcid operand shape must be (Reg(RAX), [rsp])"),
    }

    // Type-load instruction preceding INVPCID must set RAX to 0 (type = 0).
    let mov_type = &recipe.instructions[3];
    assert_eq!(mov_type.mnemonic, Mnemonic::Mov);
    match (&mov_type.operands[0], &mov_type.operands[1]) {
        (Operand::Reg(r), Operand::Imm64(v)) => {
            assert_eq!(*r, abi::RAX);
            assert_eq!(*v, 0, "invpcid_single uses type 0 (individual-address)");
        }
        _ => panic!("type-load must be `mov rax, 0`"),
    }
}

#[test]
fn tlb_ops_invpcid_all_nonglobal_uses_type_1_with_zeroed_va_slot() {
    // v0.21-009-followup (#1297): Type-1 recipe zeros the linear-addr descriptor
    // slot (SDM: reserved bits must be 0 even when the type ignores the field).
    //
    // Expected sequence:
    //   sub rsp, 16
    //   mov [rsp + 0], rdi      ; low  = pcid
    //   xor rax, rax            ; scratch zero
    //   mov [rsp + 8], rax      ; high = 0
    //   mov rax, 1              ; type = 1 (single-context)
    //   invpcid rax, [rsp]
    //   add rsp, 16
    let arena = IrArena::new();
    let pcid_id = IrNodeId::new(1).expect("valid node id");
    let recipe = lower_stdlib_method(
        "TlbOps",
        "invpcid_all_nonglobal",
        InstrMode::Mode64,
        &[pcid_id],
        &arena,
    )
    .expect("TlbOps::invpcid_all_nonglobal must be registered")
    .expect("invpcid_all_nonglobal lowering must succeed");

    assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
    assert!(recipe.labels.is_empty());
    assert_eq!(recipe.instructions.len(), 7);

    let mnemonics: Vec<Mnemonic> =
        recipe.instructions.iter().map(|i| i.mnemonic).collect();
    assert_eq!(
        mnemonics,
        vec![
            Mnemonic::Sub,
            Mnemonic::Mov,
            Mnemonic::Xor,
            Mnemonic::Mov,
            Mnemonic::Mov,
            Mnemonic::Invpcid,
            Mnemonic::Add,
        ],
    );

    // Type-load must be `mov rax, 1` (type = 1 selects single-context /
    // all-nonglobal per SDM Vol 3A §4.10.4.1).
    let mov_type = &recipe.instructions[4];
    match (&mov_type.operands[0], &mov_type.operands[1]) {
        (Operand::Reg(r), Operand::Imm64(v)) => {
            assert_eq!(*r, abi::RAX);
            assert_eq!(*v, 1, "invpcid_all_nonglobal uses type 1");
        }
        _ => panic!("type-load must be `mov rax, 1`"),
    }
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
