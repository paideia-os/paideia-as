//! PA-v0.21-007 (#1283): CpuidOps lowering round-trips.
//!
//! Coverage:
//!   cpuid_leaf_ad(leaf, subleaf) -> u64      spliced as:
//!     push rbx                    ; preserve callee-saved RBX
//!     mov  rax, rdi               ; leaf → EAX
//!     mov  rcx, rsi               ; subleaf → ECX
//!     cpuid                       ; EAX/EBX/ECX/EDX ← CPUID
//!     pop  rbx                    ; restore callee-saved RBX
//!     shl  rdx, 32                ; RDX high half in position
//!     or   rax, rdx               ; RAX = (EDX << 32) | EAX (SysV return)
//!
//!   cpuid_leaf_bc(leaf, subleaf) -> u64      spliced as:
//!     push rbx                    ; preserve callee-saved RBX
//!     mov  rax, rdi               ; leaf → EAX
//!     mov  rcx, rsi               ; subleaf → ECX
//!     cpuid                       ; EAX/EBX/ECX/EDX ← CPUID
//!     mov  rax, rbx               ; capture EBX BEFORE the pop restores RBX
//!     pop  rbx                    ; restore callee-saved RBX
//!     shl  rcx, 32                ; ECX high half in position
//!     or   rax, rcx               ; RAX = (ECX << 32) | EBX (SysV return)
//!
//! These tests only assert the recipe shape (mnemonic + operand structure);
//! byte-exact verification for each individual mnemonic lives with the
//! encoder tests (mov reg-reg, cpuid, shl/shr r64 imm, or r64 r64,
//! push/pop r64), which the recipe merely composes.
//!
//! The recipe is spliced INLINE in place of the CALL+RET in the caller's
//! function body (see the SysVRegs branch in emit_call.rs), so RBX must
//! be bracketed by push/pop within the recipe itself — the surrounding
//! elaborator caller-save spill set is [RCX, RDX, R8, R9] and does not
//! cover callee-saved registers.

use paideia_as_ir::{
    InstrMode, Instruction, IrArena, IrNodeId, RegId, abi,
    instruction::{Mnemonic, Operand},
};
use paideia_as_elaborator::stdlib_lowering::{ArgConvention, lower_stdlib_method};

/// Expect a `Mnemonic::Mov` of the shape `mov dst, src` at
/// `recipe.instructions[i]` with reg-reg operands.
fn assert_mov_reg_reg(
    inst: &Instruction,
    expected_dst: RegId,
    expected_src: RegId,
    context: &str,
) {
    assert_eq!(inst.mnemonic, Mnemonic::Mov, "{}: mnemonic", context);
    assert_eq!(inst.operands.len(), 2, "{}: operand count", context);
    match (&inst.operands[0], &inst.operands[1]) {
        (Operand::Reg(dst), Operand::Reg(src)) => {
            assert_eq!(*dst, expected_dst, "{}: dst", context);
            assert_eq!(*src, expected_src, "{}: src", context);
        }
        _ => panic!("{}: expected reg-reg operands", context),
    }
}

/// Expect a `Mnemonic::Push` or `Pop` of a single register at `inst`.
fn assert_push_or_pop_reg(
    inst: &Instruction,
    expected_mnem: Mnemonic,
    expected_reg: RegId,
    context: &str,
) {
    assert_eq!(inst.mnemonic, expected_mnem, "{}: mnemonic", context);
    assert_eq!(inst.operands.len(), 1, "{}: operand count", context);
    match &inst.operands[0] {
        Operand::Reg(r) => assert_eq!(*r, expected_reg, "{}: reg", context),
        _ => panic!("{}: expected Reg operand", context),
    }
}

#[test]
fn cpuid_ops_leaf_ad_lowers_to_push_mov_cpuid_pop_shl_or() {
    let mut arena = IrArena::new();
    let leaf_id = IrNodeId::new(1).expect("valid node id");
    let sub_id = IrNodeId::new(2).expect("valid node id");
    // CpuidOps recipes are SysVRegs, so the arena literal is irrelevant to
    // recipe matching — populate for parity with the MsrOps test.
    arena.literal_values_mut().insert(leaf_id, 0x01);
    arena.literal_values_mut().insert(sub_id, 0x00);

    let recipe = lower_stdlib_method(
        "CpuidOps",
        "cpuid_leaf_ad",
        InstrMode::Mode64,
        &[leaf_id, sub_id],
        &arena,
    )
    .expect("CpuidOps::cpuid_leaf_ad must be registered")
    .expect("cpuid_leaf_ad lowering must succeed");

    assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
    assert!(recipe.labels.is_empty());
    assert_eq!(
        recipe.instructions.len(),
        7,
        "cpuid_leaf_ad recipe = push + 2*mov + cpuid + pop + shl + or"
    );

    // 0: push rbx — preserve callee-saved RBX.
    assert_push_or_pop_reg(&recipe.instructions[0], Mnemonic::Push, abi::RBX, "push rbx");

    // 1: mov rax, rdi — leaf → EAX (via 64-bit mov; upper 32 of RDI zeroed
    //    by SysV for a u32 arg).
    assert_mov_reg_reg(&recipe.instructions[1], abi::RAX, abi::RDI, "mov rax, rdi");

    // 2: mov rcx, rsi — subleaf → ECX.
    assert_mov_reg_reg(&recipe.instructions[2], abi::RCX, abi::RSI, "mov rcx, rsi");

    // 3: cpuid — nullary.
    assert_eq!(recipe.instructions[3].mnemonic, Mnemonic::Cpuid);
    assert!(recipe.instructions[3].operands.is_empty());
    assert_eq!(recipe.instructions[3].mode, InstrMode::Mode64);

    // 4: pop rbx — restore callee-saved RBX.
    assert_push_or_pop_reg(&recipe.instructions[4], Mnemonic::Pop, abi::RBX, "pop rbx");

    // 5: shl rdx, 32 — shift EDX result into the high 32 of RAX's pack.
    assert_eq!(recipe.instructions[5].mnemonic, Mnemonic::Shl);
    match (&recipe.instructions[5].operands[0], &recipe.instructions[5].operands[1]) {
        (Operand::Reg(r), Operand::Imm64(imm)) => {
            assert_eq!(*r, abi::RDX);
            assert_eq!(*imm, 32);
        }
        _ => panic!("shl rdx, 32 operands wrong shape"),
    }

    // 6: or rax, rdx — pack EDX into the high 32 of RAX (EAX already there).
    assert_eq!(recipe.instructions[6].mnemonic, Mnemonic::Or);
    match (&recipe.instructions[6].operands[0], &recipe.instructions[6].operands[1]) {
        (Operand::Reg(dst), Operand::Reg(src)) => {
            assert_eq!(*dst, abi::RAX);
            assert_eq!(*src, abi::RDX);
        }
        _ => panic!("or rax, rdx operands wrong shape"),
    }
}

#[test]
fn cpuid_ops_leaf_bc_lowers_to_push_mov_cpuid_mov_pop_shl_or() {
    let arena = IrArena::new();
    let leaf_id = IrNodeId::new(1).expect("valid node id");
    let sub_id = IrNodeId::new(2).expect("valid node id");

    let recipe = lower_stdlib_method(
        "CpuidOps",
        "cpuid_leaf_bc",
        InstrMode::Mode64,
        &[leaf_id, sub_id],
        &arena,
    )
    .expect("CpuidOps::cpuid_leaf_bc must be registered")
    .expect("cpuid_leaf_bc lowering must succeed");

    assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
    assert!(recipe.labels.is_empty());
    assert_eq!(
        recipe.instructions.len(),
        8,
        "cpuid_leaf_bc recipe = push + 2*mov + cpuid + mov + pop + shl + or"
    );

    // 0: push rbx.
    assert_push_or_pop_reg(&recipe.instructions[0], Mnemonic::Push, abi::RBX, "push rbx");

    // 1: mov rax, rdi.
    assert_mov_reg_reg(&recipe.instructions[1], abi::RAX, abi::RDI, "mov rax, rdi");

    // 2: mov rcx, rsi.
    assert_mov_reg_reg(&recipe.instructions[2], abi::RCX, abi::RSI, "mov rcx, rsi");

    // 3: cpuid.
    assert_eq!(recipe.instructions[3].mnemonic, Mnemonic::Cpuid);
    assert!(recipe.instructions[3].operands.is_empty());

    // 4: mov rax, rbx — capture EBX BEFORE the pop restores RBX. Ordering
    //    is load-bearing: swapping instructions 4 and 5 would lose EBX
    //    and read back the caller's saved RBX instead.
    assert_mov_reg_reg(&recipe.instructions[4], abi::RAX, abi::RBX, "mov rax, rbx");

    // 5: pop rbx — restore callee-saved RBX.
    assert_push_or_pop_reg(&recipe.instructions[5], Mnemonic::Pop, abi::RBX, "pop rbx");

    // 6: shl rcx, 32.
    assert_eq!(recipe.instructions[6].mnemonic, Mnemonic::Shl);
    match (&recipe.instructions[6].operands[0], &recipe.instructions[6].operands[1]) {
        (Operand::Reg(r), Operand::Imm64(imm)) => {
            assert_eq!(*r, abi::RCX);
            assert_eq!(*imm, 32);
        }
        _ => panic!("shl rcx, 32 operands wrong shape"),
    }

    // 7: or rax, rcx — pack ECX into the high 32 (EBX now in low via mov #4).
    assert_eq!(recipe.instructions[7].mnemonic, Mnemonic::Or);
    match (&recipe.instructions[7].operands[0], &recipe.instructions[7].operands[1]) {
        (Operand::Reg(dst), Operand::Reg(src)) => {
            assert_eq!(*dst, abi::RAX);
            assert_eq!(*src, abi::RCX);
        }
        _ => panic!("or rax, rcx operands wrong shape"),
    }
}

#[test]
fn cpuid_ops_leaf_bc_pops_rbx_after_capturing_ebx() {
    // Explicit ordering guard: whatever else the recipe evolves into, the
    // read of RBX (mov rax, rbx) must happen BEFORE the pop that restores
    // the caller's RBX. Otherwise the recipe silently returns the caller's
    // RBX contents instead of CPUID's EBX result.
    let arena = IrArena::new();
    let leaf_id = IrNodeId::new(1).expect("valid node id");
    let sub_id = IrNodeId::new(2).expect("valid node id");

    let recipe = lower_stdlib_method(
        "CpuidOps",
        "cpuid_leaf_bc",
        InstrMode::Mode64,
        &[leaf_id, sub_id],
        &arena,
    )
    .expect("recipe registered")
    .expect("lowering succeeds");

    let mut rbx_read_idx: Option<usize> = None;
    let mut pop_idx: Option<usize> = None;
    for (i, inst) in recipe.instructions.iter().enumerate() {
        if inst.mnemonic == Mnemonic::Mov
            && inst.operands.len() == 2
            && matches!(&inst.operands[1], Operand::Reg(r) if *r == abi::RBX)
        {
            rbx_read_idx.get_or_insert(i);
        }
        if inst.mnemonic == Mnemonic::Pop
            && inst.operands.len() == 1
            && matches!(&inst.operands[0], Operand::Reg(r) if *r == abi::RBX)
        {
            pop_idx.get_or_insert(i);
        }
    }

    let rbx_read = rbx_read_idx.expect("recipe must read RBX");
    let pop = pop_idx.expect("recipe must pop RBX");
    assert!(
        rbx_read < pop,
        "capture of EBX must precede pop rbx (rbx_read={}, pop={})",
        rbx_read,
        pop
    );
}

#[test]
fn cpuid_recipe_uniform_across_the_five_target_leaves() {
    // The recipe is oblivious to the concrete leaf/subleaf values (SysV
    // args are marshalled by the emit_call layer, not baked into the
    // recipe operands). Exercise it once per leaf named in the #1283
    // acceptance criteria (0x01 / 0x0B / 0x0D / 0x1A / 0x1F) to prove
    // the same recipe shape drops in for every consumer — no per-leaf
    // fork lurking in the dispatcher.
    for &(leaf, subleaf, label) in &[
        (0x01u64, 0x00u64, "basic feature bits"),
        (0x0Bu64, 0x00u64, "legacy extended topology (level 0)"),
        (0x0Du64, 0x00u64, "processor extended state (XSAVE main)"),
        (0x1Au64, 0x00u64, "hybrid processor type"),
        (0x1Fu64, 0x00u64, "V2 extended topology (level 0)"),
    ] {
        let mut arena = IrArena::new();
        let leaf_id = IrNodeId::new(1).expect("valid node id");
        let sub_id = IrNodeId::new(2).expect("valid node id");
        arena.literal_values_mut().insert(leaf_id, leaf as i64);
        arena.literal_values_mut().insert(sub_id, subleaf as i64);

        let ad = lower_stdlib_method(
            "CpuidOps",
            "cpuid_leaf_ad",
            InstrMode::Mode64,
            &[leaf_id, sub_id],
            &arena,
        )
        .expect("cpuid_leaf_ad recipe must exist")
        .expect("lowering must succeed");
        assert_eq!(
            ad.instructions.len(),
            7,
            "cpuid_leaf_ad shape must not depend on leaf {} ({})",
            leaf,
            label
        );
        assert!(
            ad.instructions.iter().any(|i| i.mnemonic == Mnemonic::Cpuid),
            "cpuid_leaf_ad recipe for leaf {} ({}) must contain a CPUID",
            leaf,
            label
        );

        let bc = lower_stdlib_method(
            "CpuidOps",
            "cpuid_leaf_bc",
            InstrMode::Mode64,
            &[leaf_id, sub_id],
            &arena,
        )
        .expect("cpuid_leaf_bc recipe must exist")
        .expect("lowering must succeed");
        assert_eq!(
            bc.instructions.len(),
            8,
            "cpuid_leaf_bc shape must not depend on leaf {} ({})",
            leaf,
            label
        );
        assert!(
            bc.instructions.iter().any(|i| i.mnemonic == Mnemonic::Cpuid),
            "cpuid_leaf_bc recipe for leaf {} ({}) must contain a CPUID",
            leaf,
            label
        );
    }
}

#[test]
fn unknown_cpuid_method_returns_none() {
    // Guard against typos silently matching.
    let arena = IrArena::new();
    let leaf_id = IrNodeId::new(1).expect("valid node id");
    let sub_id = IrNodeId::new(2).expect("valid node id");
    assert!(lower_stdlib_method(
        "CpuidOps",
        "cpuid_leaf", // no _ad / _bc suffix → not registered
        InstrMode::Mode64,
        &[leaf_id, sub_id],
        &arena,
    )
    .is_none());
}
