//! PA-v0.21-012 (#1288): BitfieldOps::get_bits / set_bits lowering round-trips.
//!
//! get_bits(word: u64, start: u32, width: u32) -> u64 spliced as:
//!   mov  rax, rdi        ; rax  = word
//!   mov  rcx, rsi        ; cl   = start
//!   shr  rax, cl         ; rax >>= start
//!   mov  r9,  1
//!   mov  rcx, rdx        ; cl   = width
//!   shl  r9,  cl         ; r9   = 1 << width
//!   sub  r9,  1          ; r9   = mask
//!   and  rax, r9         ; rax  = extracted field
//!
//! set_bits(word, start, width, val) -> u64 spliced as:
//!   mov  r8,  rcx        ; save val
//!   mov  r9,  1
//!   mov  rcx, rdx        ; cl   = width
//!   shl  r9,  cl         ; r9   = 1 << width
//!   sub  r9,  1          ; r9   = raw_mask
//!   and  r8,  r9         ; r8   = val & raw_mask
//!   mov  rcx, rsi        ; cl   = start
//!   shl  r8,  cl         ; r8   = payload in place
//!   shl  r9,  cl         ; r9   = mask in place
//!   not  r9              ; r9   = ~mask
//!   and  rdi, r9         ; rdi  = cleared slot
//!   or   rdi, r8         ; rdi |= payload
//!   mov  rax, rdi        ; return
//!
//! These tests assert the recipe shape only (mnemonic + operand structure);
//! byte-exact verification for each individual mnemonic lives with the
//! encoder tests (mov, shl/shr r64 CL / imm8, and, or, not, sub), which the
//! recipe merely composes.

use paideia_as_ir::{
    InstrMode, IrArena, IrNodeId, abi,
    instruction::{Mnemonic, Operand},
};
use paideia_as_elaborator::stdlib_lowering::{ArgConvention, lower_stdlib_method};

#[test]
fn bitfield_ops_get_bits_lowers_to_eight_instruction_shift_mask_recipe() {
    let arena = IrArena::new();
    let word_id = IrNodeId::new(1).expect("valid node id");
    let start_id = IrNodeId::new(2).expect("valid node id");
    let width_id = IrNodeId::new(3).expect("valid node id");

    let recipe = lower_stdlib_method(
        "BitfieldOps",
        "get_bits",
        InstrMode::Mode64,
        &[word_id, start_id, width_id],
        &arena,
    )
    .expect("BitfieldOps::get_bits must be registered")
    .expect("BitfieldOps::get_bits lowering must succeed");

    assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
    assert!(recipe.labels.is_empty());
    assert_eq!(
        recipe.instructions.len(),
        8,
        "get_bits recipe = mov rax,rdi + mov rcx,rsi + shr rax,cl + mov r9,1 + mov rcx,rdx + shl r9,cl + sub r9,1 + and rax,r9"
    );

    // 1: mov rax, rdi
    assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Mov);
    match (&recipe.instructions[0].operands[0], &recipe.instructions[0].operands[1]) {
        (Operand::Reg(d), Operand::Reg(s)) => {
            assert_eq!(*d, abi::RAX, "mov #1 dst = RAX");
            assert_eq!(*s, abi::RDI, "mov #1 src = RDI (word)");
        }
        _ => panic!("mov #1 must be reg-reg"),
    }

    // 2: mov rcx, rsi
    assert_eq!(recipe.instructions[1].mnemonic, Mnemonic::Mov);
    match (&recipe.instructions[1].operands[0], &recipe.instructions[1].operands[1]) {
        (Operand::Reg(d), Operand::Reg(s)) => {
            assert_eq!(*d, abi::RCX, "mov #2 dst = RCX");
            assert_eq!(*s, abi::RSI, "mov #2 src = RSI (start)");
        }
        _ => panic!("mov #2 must be reg-reg"),
    }

    // 3: shr rax, cl (encoded as shr rax, rcx — encoder validates second operand is RCX)
    assert_eq!(recipe.instructions[2].mnemonic, Mnemonic::Shr);
    match (&recipe.instructions[2].operands[0], &recipe.instructions[2].operands[1]) {
        (Operand::Reg(d), Operand::Reg(s)) => {
            assert_eq!(*d, abi::RAX);
            assert_eq!(*s, abi::RCX, "shr count reg must be RCX (only CL is used)");
        }
        _ => panic!("shr #3 must be reg-reg (RAX, RCX)"),
    }

    // 4: mov r9, 1
    assert_eq!(recipe.instructions[3].mnemonic, Mnemonic::Mov);
    match (&recipe.instructions[3].operands[0], &recipe.instructions[3].operands[1]) {
        (Operand::Reg(d), Operand::Imm64(imm)) => {
            assert_eq!(*d, abi::R9);
            assert_eq!(*imm, 1);
        }
        _ => panic!("mov #4 must be reg-imm (R9, 1)"),
    }

    // 5: mov rcx, rdx
    assert_eq!(recipe.instructions[4].mnemonic, Mnemonic::Mov);
    match (&recipe.instructions[4].operands[0], &recipe.instructions[4].operands[1]) {
        (Operand::Reg(d), Operand::Reg(s)) => {
            assert_eq!(*d, abi::RCX);
            assert_eq!(*s, abi::RDX, "mov #5 src = RDX (width)");
        }
        _ => panic!("mov #5 must be reg-reg"),
    }

    // 6: shl r9, cl
    assert_eq!(recipe.instructions[5].mnemonic, Mnemonic::Shl);
    match (&recipe.instructions[5].operands[0], &recipe.instructions[5].operands[1]) {
        (Operand::Reg(d), Operand::Reg(s)) => {
            assert_eq!(*d, abi::R9);
            assert_eq!(*s, abi::RCX);
        }
        _ => panic!("shl #6 must be reg-reg (R9, RCX)"),
    }

    // 7: sub r9, 1
    assert_eq!(recipe.instructions[6].mnemonic, Mnemonic::Sub);
    match (&recipe.instructions[6].operands[0], &recipe.instructions[6].operands[1]) {
        (Operand::Reg(d), Operand::Imm64(imm)) => {
            assert_eq!(*d, abi::R9);
            assert_eq!(*imm, 1);
        }
        _ => panic!("sub #7 must be reg-imm (R9, 1)"),
    }

    // 8: and rax, r9
    assert_eq!(recipe.instructions[7].mnemonic, Mnemonic::And);
    match (&recipe.instructions[7].operands[0], &recipe.instructions[7].operands[1]) {
        (Operand::Reg(d), Operand::Reg(s)) => {
            assert_eq!(*d, abi::RAX);
            assert_eq!(*s, abi::R9);
        }
        _ => panic!("and #8 must be reg-reg (RAX, R9)"),
    }
}

#[test]
fn bitfield_ops_set_bits_lowers_to_thirteen_instruction_mask_and_or_recipe() {
    let arena = IrArena::new();
    let word_id  = IrNodeId::new(1).expect("valid node id");
    let start_id = IrNodeId::new(2).expect("valid node id");
    let width_id = IrNodeId::new(3).expect("valid node id");
    let val_id   = IrNodeId::new(4).expect("valid node id");

    let recipe = lower_stdlib_method(
        "BitfieldOps",
        "set_bits",
        InstrMode::Mode64,
        &[word_id, start_id, width_id, val_id],
        &arena,
    )
    .expect("BitfieldOps::set_bits must be registered")
    .expect("BitfieldOps::set_bits lowering must succeed");

    assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
    assert!(recipe.labels.is_empty());
    assert_eq!(
        recipe.instructions.len(),
        13,
        "set_bits recipe = 13 (mov r8,rcx + mov r9,1 + mov rcx,rdx + shl r9,cl + sub r9,1 + and r8,r9 + mov rcx,rsi + shl r8,cl + shl r9,cl + not r9 + and rdi,r9 + or rdi,r8 + mov rax,rdi)"
    );

    // Verify the last instruction returns via RAX = RDI (SysV return).
    let last = &recipe.instructions[12];
    assert_eq!(last.mnemonic, Mnemonic::Mov);
    match (&last.operands[0], &last.operands[1]) {
        (Operand::Reg(d), Operand::Reg(s)) => {
            assert_eq!(*d, abi::RAX, "final mov dst must be RAX (SysV return)");
            assert_eq!(*s, abi::RDI, "final mov src must be RDI (composed word)");
        }
        _ => panic!("final mov must be reg-reg"),
    }

    // Verify the val-rescue mov (r8, rcx) is instruction #0 — must happen
    // BEFORE any shift-count marshalling clobbers RCX.
    assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Mov);
    match (&recipe.instructions[0].operands[0], &recipe.instructions[0].operands[1]) {
        (Operand::Reg(d), Operand::Reg(s)) => {
            assert_eq!(*d, abi::R8, "val must be rescued into R8");
            assert_eq!(*s, abi::RCX, "val arrives in RCX per SysV arg 3");
        }
        _ => panic!("val-rescue mov must be reg-reg"),
    }

    // Verify NOT r9 lands (mask inversion is the semantic hinge — no NOT means
    // set_bits smears rather than replaces).
    let not_positions: Vec<_> = recipe.instructions
        .iter()
        .enumerate()
        .filter(|(_, i)| i.mnemonic == Mnemonic::Not)
        .collect();
    assert_eq!(not_positions.len(), 1, "exactly one NOT (mask inversion)");
    let (not_idx, not_inst) = not_positions[0];
    assert_eq!(not_idx, 9, "NOT must land at position 9 (after mask is placed)");
    match &not_inst.operands[0] {
        Operand::Reg(r) => assert_eq!(*r, abi::R9, "NOT operates on R9 (mask reg)"),
        _ => panic!("NOT operand must be reg"),
    }

    // Verify OR rdi, r8 (payload merge).
    let or_positions: Vec<_> = recipe.instructions
        .iter()
        .enumerate()
        .filter(|(_, i)| i.mnemonic == Mnemonic::Or)
        .collect();
    assert_eq!(or_positions.len(), 1, "exactly one OR (payload merge)");
    let (_, or_inst) = or_positions[0];
    match (&or_inst.operands[0], &or_inst.operands[1]) {
        (Operand::Reg(d), Operand::Reg(s)) => {
            assert_eq!(*d, abi::RDI);
            assert_eq!(*s, abi::R8);
        }
        _ => panic!("OR must be reg-reg (RDI, R8)"),
    }
}

#[test]
fn unknown_bitfield_method_returns_none() {
    // Guard against typos silently matching.
    let arena = IrArena::new();
    let word_id = IrNodeId::new(1).expect("valid node id");
    assert!(lower_stdlib_method(
        "BitfieldOps",
        "get_bits_maybe", // typo
        InstrMode::Mode64,
        &[word_id],
        &arena,
    )
    .is_none());
}
