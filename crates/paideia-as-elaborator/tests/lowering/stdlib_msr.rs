//! PA-v0.21-008 (#1284): MsrOps::rdmsr / wrmsr lowering round-trips.
//!
//! rdmsr(idx: u32) -> u64  spliced as:
//!   mov  rcx, rdi           ; idx → RCX (upper 32 of RDI = 0 for a u32 arg)
//!   rdmsr                   ; EDX:EAX ← MSR[ECX]; hardware zero-extends RAX/RDX
//!   shl  rdx, 32            ; RDX high half in position
//!   or   rax, rdx           ; RAX = (EDX << 32) | EAX  (SysV return)
//!
//! wrmsr(idx: u32, val: u64) -> () spliced as:
//!   mov  rcx, rdi           ; idx → RCX
//!   mov  rax, rsi           ; val low half naturally in EAX
//!   mov  rdx, rsi           ; copy val to RDX ...
//!   shr  rdx, 32            ; ... then shift down for EDX
//!   wrmsr                   ; MSR[ECX] ← EDX:EAX
//!
//! These tests only assert the recipe shape (mnemonic + operand structure);
//! byte-exact verification for each individual mnemonic lives with the
//! encoder tests (mov reg-reg, rdmsr/wrmsr, shl/shr r64 imm, or r64 r64),
//! which the recipe merely composes.

use paideia_as_ir::{
    InstrMode, IrArena, IrNodeId, abi,
    instruction::{Mnemonic, Operand},
};
use paideia_as_elaborator::stdlib_lowering::{ArgConvention, lower_stdlib_method};

#[test]
fn msr_ops_rdmsr_lowers_to_mov_rdmsr_shl_or() {
    let mut arena = IrArena::new();
    let idx_id = IrNodeId::new(1).expect("valid node id");
    // MsrOps recipes are SysVRegs, so the arena literal value is irrelevant to
    // recipe matching — but we insert one to prove the recipe does not consume it.
    arena.literal_values_mut().insert(idx_id, 0x1B); // IA32_APIC_BASE, illustrative

    let recipe = lower_stdlib_method("MsrOps", "rdmsr", InstrMode::Mode64, &[idx_id], &arena)
        .expect("MsrOps::rdmsr must be registered")
        .expect("MsrOps::rdmsr lowering must succeed");

    assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
    assert!(recipe.labels.is_empty());
    assert_eq!(
        recipe.instructions.len(),
        4,
        "rdmsr recipe = mov + rdmsr + shl + or"
    );

    // 1: mov rcx, rdi
    assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Mov);
    let ops0 = &recipe.instructions[0].operands;
    assert_eq!(ops0.len(), 2);
    match (&ops0[0], &ops0[1]) {
        (Operand::Reg(dst), Operand::Reg(src)) => {
            assert_eq!(*dst, abi::RCX, "dst of mov #1 = RCX");
            assert_eq!(*src, abi::RDI, "src of mov #1 = RDI (SysV arg 0)");
        }
        _ => panic!("mov #1 must be reg-reg"),
    }

    // 2: rdmsr — nullary
    assert_eq!(recipe.instructions[1].mnemonic, Mnemonic::Rdmsr);
    assert!(recipe.instructions[1].operands.is_empty());

    // 3: shl rdx, 32
    assert_eq!(recipe.instructions[2].mnemonic, Mnemonic::Shl);
    let ops2 = &recipe.instructions[2].operands;
    assert_eq!(ops2.len(), 2);
    match (&ops2[0], &ops2[1]) {
        (Operand::Reg(r), Operand::Imm64(imm)) => {
            assert_eq!(*r, abi::RDX);
            assert_eq!(*imm, 32);
        }
        _ => panic!("shl operands wrong shape"),
    }

    // 4: or rax, rdx
    assert_eq!(recipe.instructions[3].mnemonic, Mnemonic::Or);
    let ops3 = &recipe.instructions[3].operands;
    assert_eq!(ops3.len(), 2);
    match (&ops3[0], &ops3[1]) {
        (Operand::Reg(dst), Operand::Reg(src)) => {
            assert_eq!(*dst, abi::RAX);
            assert_eq!(*src, abi::RDX);
        }
        _ => panic!("or operands wrong shape"),
    }
}

#[test]
fn msr_ops_wrmsr_lowers_to_mov_rcx_mov_rax_mov_rdx_shr_wrmsr() {
    let arena = IrArena::new();
    let idx_id = IrNodeId::new(1).expect("valid node id");
    let val_id = IrNodeId::new(2).expect("valid node id");

    let recipe = lower_stdlib_method(
        "MsrOps",
        "wrmsr",
        InstrMode::Mode64,
        &[idx_id, val_id],
        &arena,
    )
    .expect("MsrOps::wrmsr must be registered")
    .expect("MsrOps::wrmsr lowering must succeed");

    assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
    assert!(recipe.labels.is_empty());
    assert_eq!(
        recipe.instructions.len(),
        5,
        "wrmsr recipe = mov rcx,rdi + mov rax,rsi + mov rdx,rsi + shr rdx,32 + wrmsr"
    );

    // Verify wrmsr is the final mnemonic and nullary
    let last = &recipe.instructions[4];
    assert_eq!(last.mnemonic, Mnemonic::Wrmsr);
    assert!(last.operands.is_empty());

    // Verify shr rdx, 32 is instruction 3
    let shr = &recipe.instructions[3];
    assert_eq!(shr.mnemonic, Mnemonic::Shr);
    match (&shr.operands[0], &shr.operands[1]) {
        (Operand::Reg(r), Operand::Imm64(imm)) => {
            assert_eq!(*r, abi::RDX);
            assert_eq!(*imm, 32);
        }
        _ => panic!("shr operands wrong shape"),
    }

    // Verify the three MOVs are reg-reg and target RCX, RAX, RDX from RDI/RSI/RSI
    for (i, (dst, src)) in [
        (abi::RCX, abi::RDI),
        (abi::RAX, abi::RSI),
        (abi::RDX, abi::RSI),
    ]
    .iter()
    .enumerate()
    {
        let inst = &recipe.instructions[i];
        assert_eq!(inst.mnemonic, Mnemonic::Mov, "instruction {} should be MOV", i);
        match (&inst.operands[0], &inst.operands[1]) {
            (Operand::Reg(d), Operand::Reg(s)) => {
                assert_eq!(*d, *dst, "MOV #{i} dst");
                assert_eq!(*s, *src, "MOV #{i} src");
            }
            _ => panic!("MOV {i} must be reg-reg"),
        }
    }
}

#[test]
fn unknown_msr_method_returns_none() {
    // Guard against typos silently matching.
    let arena = IrArena::new();
    let idx_id = IrNodeId::new(1).expect("valid node id");
    assert!(lower_stdlib_method(
        "MsrOps",
        "rdmsr_maybe", // typo
        InstrMode::Mode64,
        &[idx_id],
        &arena,
    )
    .is_none());
}
