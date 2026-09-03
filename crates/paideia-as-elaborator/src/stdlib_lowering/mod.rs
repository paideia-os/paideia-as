//! Stdlib trait method → mnemonic sequence lowering.
//!
//! PA-r16-007-backtrack (#1036): a hardcoded registry that maps
//! `(trait_name, method_name)` pairs to the IR instruction sequences
//! they should lower to. Consulted by emit_call before its normal SysV
//! call-marshalling.
//!
//! PA-r16-007-followup (#1056): PerCpuOps::percpu_inc / percpu_add lowering.
//! Extends signature to accept arg_ids and arena so recipes can extract
//! integer-literal arguments at compile time (required for absolute-displacement
//! encoding).
//!
//! PA-r16-007-registry-runtime-args (#1062): ArgConvention enum distinguishes
//! Literal recipes (args baked into operands at compile time) from SysVRegs
//! recipes (args pre-marshalled into RDI/RSI/RDX/RCX/R8/R9).
//!
//! Scope: PauseOps::spin_hint(), PerCpuOps::percpu_inc/percpu_add,
//! MmioOps::mmio_read_u32/mmio_write_u32 in v0.16.
//! Follow-up issues track BytesOps, ChecksumOps retrofits.
//!
//! v0.21-008 (#1284): MsrOps::rdmsr/wrmsr — typed wrappers over the
//! zero-arity Rdmsr/Wrmsr mnemonics; args arrive in SysV regs (idx in RDI,
//! val in RSI for wrmsr) and the recipe marshals ECX + EDX:EAX, then packs
//! the RDMSR result into RAX per the SysV return convention. Hardware
//! zero-extends RAX / RDX after rdmsr in 64-bit mode (SDM Vol 2B RDMSR),
//! so no explicit masking is needed before the shl-or pack.
//!
//! v0.21-009 (#1285): TlbOps::invlpg_single / flush_cache_writeback —
//! typed wrappers over Invlpg / Wbinvd. invpcid_* remains a follow-up
//! (the INVPCID mnemonic hasn't landed across all exhaustive
//! Mnemonic-match sites yet); calls to it fall through to normal call
//! emission and emit an unresolved-intrinsic diagnostic — no silent
//! success.
//!
//! v0.21-007 (#1283): CpuidOps::cpuid_leaf_ad / cpuid_leaf_bc — a pair
//! of SysVRegs recipes over the zero-arity Cpuid mnemonic that together
//! surface every register CPUID writes as SysV scalar u64 returns
//! (RAX=EDX:EAX, RAX=ECX:EBX). RBX is bracketed with push/pop because
//! CPUID clobbers it and RBX is callee-saved. Typed per-leaf decoders
//! (leaves 0x01, 0x0B, 0x0D, 0x1A, 0x1F) live in
//! `crates/paideia-stdlib/pdx/cpuid.pdx` and compose on top. Full
//! record-return marshalling (a single `cpuid_leaf(...) -> CpuidRegs`
//! intrinsic) is tracked in #1298 and does not need to land before
//! consumers can call the primitives here.
//!
//! v0.21-013 (#1289): MmioOps volatile load/store widening to u8/u16/u64
//! — mirrors the existing mmio_read_u32/mmio_write_u32 shape with
//! MovSized{width=W8/W16/W64}. MovSized is emitted as a raw asm
//! instruction, not a typed load, so CSE never collapses two adjacent
//! reads at the same address (the "two distinct MOVs" fixture in
//! #1289's acceptance criteria is satisfied by construction). Volatile
//! ordering across MMIO writes and unrelated WB stores is a caller
//! discipline: use BarrierOps::barrier_full / barrier_store between
//! critical MMIO pairs. We do NOT embed a fence in every recipe —
//! doubling instruction count for every device-register access when
//! most callsites do not need it is worse than the discipline of
//! naming the fence.
//!
//! # Internal structure
//!
//! Refactored 2026-08-11 from a single 3364-line file into a directory
//! module with one submodule per stdlib trait family (pauseops,
//! percpuops, refcountops, bitmapops, mmioops, bytesops, bulkmemops,
//! checksumops, barrierops, testloopops, msrops, tlbops, bitfieldops).
//! Each submodule exposes a `try_lower(method_name, ...)` fn scoped
//! `pub(super)`; nothing beyond this module changed. Shared types
//! (`StdlibLoweringError`, `ArgConvention`, `LoweringRecipe`) remain
//! here so the family submodules import them via `super::…`.
//!
//! The top-level `lower_stdlib_method` shrank from a 2200-line
//! `match (trait, method)` to a 14-arm `match trait_name` that
//! delegates to each family's `try_lower` — cpuidops arrived last
//! (v0.21-007 / #1283).

use paideia_as_ir::{
    IrArena, IrNodeId,
    instruction::InstrMode,
};

// Re-exported for `#[cfg(test)] mod tests { use super::*; }` below — the
// tests reference every IR-instruction type that the family submodules
// use internally. Public consumers reach these types through
// paideia_as_ir; nothing new is exported at crate-root.
#[cfg(test)]
#[allow(unused_imports)]
use paideia_as_ir::{
    SmallVec, abi,
    instruction::{Instruction, IntWidth, Mnemonic, Operand, SegPrefix},
};

mod barrierops;
mod bitfieldops;
mod bitmapops;
mod bulkmemops;
mod bytesops;
mod checksumops;
mod cpuidops;
mod cryptoops;
mod mldsaops;
mod mmioops;
mod msrops;
mod pauseops;
mod percpuops;
mod refcountops;
mod testloopops;
mod tlbops;

/// Error returned by lower_stdlib_method when recipe matching succeeds
/// but argument extraction fails.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StdlibLoweringError {
    /// A required argument is not an integer literal.
    NonLiteralArg {
        /// 0-based index of the failing argument.
        arg_index: usize,
        /// Qualified name like "PerCpuOps::percpu_inc".
        method: &'static str,
    },
}

/// Argument-passing convention for a lowering recipe.
///
/// PA-r16-007 (#1062): distinguishes recipes that bake args into their
/// operands at compile time (Literal) from those that expect args
/// pre-marshalled into SysV argument registers (SysVRegs).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ArgConvention {
    /// Recipe uses arg values as literals baked into instruction operands.
    /// emit_call skips SysV arg-marshalling entirely.
    Literal,
    /// Recipe references SysV argument registers (RDI, RSI, RDX, RCX, R8, R9)
    /// which emit_call must populate via normal SysV arg-marshalling BEFORE
    /// splicing the recipe.
    SysVRegs,
}

/// A lowering recipe: the instructions to splice + how args reach them.
#[derive(Debug, Clone)]
pub struct LoweringRecipe {
    /// Instructions to splice in place of the function call.
    pub instructions: Vec<paideia_as_ir::instruction::Instruction>,
    /// Argument-passing convention: whether args are baked into operands (Literal)
    /// or pre-marshalled into SysV registers (SysVRegs).
    pub arg_convention: ArgConvention,
    /// Local-label declarations for backward/forward jumps inside the recipe.
    /// Each entry is (label_name, index_into_instructions) — the label aliases
    /// the IrNodeId assigned to `instructions[index]` at splice time.
    ///
    /// PA-r16-007 (#1066): enables loop-shaped recipes like ipv4_checksum.
    /// Labels are per-recipe and get mangled with the caller's lambda_node_id
    /// at splice time to prevent collisions across recipe invocations.
    pub labels: Vec<(&'static str, usize)>,
    /// Optional extern-C call target (paideia-as#1305).
    ///
    /// When `Some(sym)`, the recipe's `instructions` are spliced as usual
    /// AFTER SysV arg marshalling; then `emit_call_args_and_call` continues
    /// down the normal path — emitting `call <sym>` with a symbol
    /// relocation, restoring caller-save scratch, and running the SysV / MS
    /// postlude. Only meaningful with `ArgConvention::SysVRegs`.
    ///
    /// Used by `stdlib_lowering::cryptoops` to route `Argon2id::derive`
    /// (etc.) to the extern-C thunks in `paideia-as-crypto::ffi`. Downstream
    /// consumers (paideia-os or host tooling) satisfy the symbol at link
    /// time by depending on `paideia-as-crypto` as a static rlib — or by
    /// substituting a paideia-native implementation under the same name
    /// once one lands (Phase 6+).
    ///
    /// Recipes not routing to an extern call MUST leave this `None`; the
    /// existing SysVRegs early-return path handles them.
    pub extern_target: Option<String>,
}

/// Look up the lowering recipe for `(trait_name, method_name)`.
/// Returns:
/// - `None` if the pair is not a known stdlib trait method, signalling
///   emit_call should fall through to normal call emission.
/// - `Some(Ok(recipe))` if the method matched and args were successfully
///   extracted (for Literal recipes) or matched (for SysVRegs recipes).
///   Recipe is spliced in place of the call, with arg_convention determining
///   whether arg-marshalling occurs before splicing.
/// - `Some(Err(NonLiteralArg))` if the method matched but at least one arg
///   is not an integer literal (only for Literal-convention recipes).
///   Caller should emit diagnostic and skip lowering.
///
/// The returned LoweringRecipe (on Ok) indicates both the instructions to splice
/// and whether they expect pre-marshalled SysV registers (SysVRegs) or have args
/// baked into operands (Literal). No `call target` is ever emitted; `ret` behavior
/// depends on the caller's function structure.
#[must_use]
pub fn lower_stdlib_method(
    trait_name: &str,
    method_name: &str,
    mode: InstrMode,
    arg_ids: &[IrNodeId],
    arena: &IrArena,
) -> Option<Result<LoweringRecipe, StdlibLoweringError>> {
    match trait_name {
        "PauseOps" => pauseops::try_lower(method_name, mode, arg_ids, arena),
        "PerCpuOps" => percpuops::try_lower(method_name, mode, arg_ids, arena),
        "RefcountOps" => refcountops::try_lower(method_name, mode, arg_ids, arena),
        "BitmapOps" => bitmapops::try_lower(method_name, mode, arg_ids, arena),
        "MmioOps" => mmioops::try_lower(method_name, mode, arg_ids, arena),
        "BytesOps" => bytesops::try_lower(method_name, mode, arg_ids, arena),
        "BulkMemOps" => bulkmemops::try_lower(method_name, mode, arg_ids, arena),
        "ChecksumOps" => checksumops::try_lower(method_name, mode, arg_ids, arena),
        "BarrierOps" => barrierops::try_lower(method_name, mode, arg_ids, arena),
        "TestLoopOps" => testloopops::try_lower(method_name, mode, arg_ids, arena),
        "MsrOps" => msrops::try_lower(method_name, mode, arg_ids, arena),
        "TlbOps" => tlbops::try_lower(method_name, mode, arg_ids, arena),
        "BitfieldOps" => bitfieldops::try_lower(method_name, mode, arg_ids, arena),
        "CpuidOps" => cpuidops::try_lower(method_name, mode, arg_ids, arena),
        // paideia-as#1305 — trait names match the Rust type names in
        // `paideia-as-crypto` (Argon2id, ChaCha20Poly1305) so a call
        // site's spelling matches the trait's implementation.
        "Argon2id" => cryptoops::try_lower_argon2id(method_name, mode, arg_ids, arena),
        "ChaCha20Poly1305" => {
            cryptoops::try_lower_chacha20_poly1305(method_name, mode, arg_ids, arena)
        }
        // paideia-as#1352 — MlKem768::{keygen, encaps, decaps} route
        // to the extern-C ML-KEM-768 KEM thunks in
        // `paideia-as-crypto::ffi`. Same shape as Argon2id /
        // ChaCha20Poly1305: no preamble instructions, SysVRegs
        // argument convention, extern_target names the FFI symbol.
        "MlKem768" => cryptoops::try_lower_ml_kem_768(method_name, mode, arg_ids, arena),
        // paideia-as#1330 — MlDsa65::sign routes to the extern-C
        // ML-DSA-65 signing thunk in `paideia-pq-sign::ffi`.
        "MlDsa65" => mldsaops::try_lower(method_name, mode, arg_ids, arena),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ir::{IrArena, IrNodeId};

    #[test]
    fn pause_ops_spin_hint_returns_pause_mnemonic() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("PauseOps", "spin_hint", InstrMode::Mode64, &[], &arena)
            .expect("pause recipe should exist")
            .expect("pause lowering should succeed");
        assert_eq!(recipe.instructions.len(), 1);
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Pause);
        assert!(recipe.instructions[0].operands.is_empty());
        assert_eq!(recipe.arg_convention, ArgConvention::Literal);
    }

    #[test]
    fn unknown_trait_returns_none() {
        let arena = IrArena::new();
        assert!(lower_stdlib_method("UnknownTrait", "some_method", InstrMode::Mode64, &[], &arena).is_none());
    }

    #[test]
    fn known_trait_unknown_method_returns_none() {
        let arena = IrArena::new();
        assert!(lower_stdlib_method("PauseOps", "nonexistent", InstrMode::Mode64, &[], &arena).is_none());
    }

    #[test]
    fn percpu_inc_lowers_to_gs_lock_inc() {
        let mut arena = IrArena::new();
        let lit_id = IrNodeId::new(1).expect("valid node id");
        arena.literal_values_mut().insert(lit_id, 0x1000);

        let recipe = lower_stdlib_method(
            "PerCpuOps",
            "percpu_inc",
            InstrMode::Mode64,
            &[lit_id],
            &arena,
        )
        .expect("recipe should exist")
        .expect("lowering should succeed");

        assert_eq!(recipe.instructions.len(), 1);
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::LockInc {
                width: paideia_as_ir::instruction::IntWidth::W64
            }
        );
        assert_eq!(recipe.instructions[0].operands.len(), 1);
        assert_eq!(recipe.arg_convention, ArgConvention::Literal);

        // Verify the operand is MemSeg { Gs, MemDisp { 0x1000 } }
        match &recipe.instructions[0].operands[0] {
            Operand::MemSeg { seg, inner } => {
                assert_eq!(*seg, SegPrefix::Gs);
                match inner.as_ref() {
                    Operand::MemDisp { disp } => {
                        assert_eq!(*disp, 0x1000);
                    }
                    _ => panic!("expected MemDisp inner operand"),
                }
            }
            _ => panic!("expected MemSeg operand"),
        }
    }

    #[test]
    fn percpu_add_lowers_to_gs_lock_add() {
        let mut arena = IrArena::new();
        let disp_id = IrNodeId::new(1).expect("valid node id");
        let val_id = IrNodeId::new(2).expect("valid node id");
        arena.literal_values_mut().insert(disp_id, 0x2000);
        arena.literal_values_mut().insert(val_id, 5);

        let recipe = lower_stdlib_method(
            "PerCpuOps",
            "percpu_add",
            InstrMode::Mode64,
            &[disp_id, val_id],
            &arena,
        )
        .expect("recipe should exist")
        .expect("lowering should succeed");

        assert_eq!(recipe.instructions.len(), 1);
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::LockAdd {
                width: paideia_as_ir::instruction::IntWidth::W64
            }
        );
        assert_eq!(recipe.instructions[0].operands.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::Literal);

        // Verify the first operand is MemSeg { Gs, MemDisp { 0x2000 } }
        match &recipe.instructions[0].operands[0] {
            Operand::MemSeg { seg, inner } => {
                assert_eq!(*seg, SegPrefix::Gs);
                match inner.as_ref() {
                    Operand::MemDisp { disp } => {
                        assert_eq!(*disp, 0x2000);
                    }
                    _ => panic!("expected MemDisp inner operand"),
                }
            }
            _ => panic!("expected MemSeg operand"),
        }

        // Verify the second operand is Imm64(5)
        match &recipe.instructions[0].operands[1] {
            Operand::Imm64(val) => {
                assert_eq!(*val, 5);
            }
            _ => panic!("expected Imm64 operand"),
        }
    }

    #[test]
    fn percpu_inc_non_literal_returns_err() {
        let arena = IrArena::new();
        // Pass an arg_id that's not in the literal_values table
        let missing_id = IrNodeId::new(999).expect("valid node id");

        let result = lower_stdlib_method(
            "PerCpuOps",
            "percpu_inc",
            InstrMode::Mode64,
            &[missing_id],
            &arena,
        )
        .expect("recipe should exist");

        match result {
            Err(StdlibLoweringError::NonLiteralArg { arg_index, method }) => {
                assert_eq!(arg_index, 0);
                assert_eq!(method, "PerCpuOps::percpu_inc");
            }
            Ok(_) => panic!("expected error for non-literal arg"),
        }
    }

    #[test]
    fn percpu_add_non_literal_arg1_returns_err() {
        let mut arena = IrArena::new();
        let disp_id = IrNodeId::new(1).expect("valid node id");
        let missing_id = IrNodeId::new(999).expect("valid node id");
        arena.literal_values_mut().insert(disp_id, 0x2000);

        let result = lower_stdlib_method(
            "PerCpuOps",
            "percpu_add",
            InstrMode::Mode64,
            &[disp_id, missing_id],
            &arena,
        )
        .expect("recipe should exist");

        match result {
            Err(StdlibLoweringError::NonLiteralArg { arg_index, method }) => {
                assert_eq!(arg_index, 1);
                assert_eq!(method, "PerCpuOps::percpu_add");
            }
            Ok(_) => panic!("expected error for non-literal arg"),
        }
    }

    #[test]
    fn mmio_ops_mmio_read_u32_lowers_to_mov_eax_mem_disp32() {
        let mut arena = IrArena::new();
        let addr_id = IrNodeId::new(1).expect("valid node id");
        arena.literal_values_mut().insert(addr_id, 0x1000);

        let recipe = lower_stdlib_method(
            "MmioOps",
            "mmio_read_u32",
            InstrMode::Mode64,
            &[addr_id],
            &arena,
        )
        .expect("recipe should exist")
        .expect("lowering should succeed");

        assert_eq!(recipe.instructions.len(), 1);
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W32
            }
        );
        assert_eq!(recipe.instructions[0].operands.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::Literal);

        // Verify first operand is Reg(RAX)
        match &recipe.instructions[0].operands[0] {
            Operand::Reg(reg) => {
                assert_eq!(*reg, abi::RAX);
            }
            _ => panic!("expected Reg(RAX) operand"),
        }

        // Verify second operand is MemDisp { 0x1000 }
        match &recipe.instructions[0].operands[1] {
            Operand::MemDisp { disp } => {
                assert_eq!(*disp, 0x1000);
            }
            _ => panic!("expected MemDisp operand"),
        }
    }

    #[test]
    fn mmio_ops_mmio_write_u32_lowers_to_mov_mem_disp32_imm32() {
        let mut arena = IrArena::new();
        let addr_id = IrNodeId::new(1).expect("valid node id");
        let val_id = IrNodeId::new(2).expect("valid node id");
        arena.literal_values_mut().insert(addr_id, 0x1000);
        arena.literal_values_mut().insert(val_id, 0x12345678);

        let recipe = lower_stdlib_method(
            "MmioOps",
            "mmio_write_u32",
            InstrMode::Mode64,
            &[addr_id, val_id],
            &arena,
        )
        .expect("recipe should exist")
        .expect("lowering should succeed");

        assert_eq!(recipe.instructions.len(), 1);
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W32
            }
        );
        assert_eq!(recipe.instructions[0].operands.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::Literal);

        // Verify first operand is MemDisp { 0x1000 }
        match &recipe.instructions[0].operands[0] {
            Operand::MemDisp { disp } => {
                assert_eq!(*disp, 0x1000);
            }
            _ => panic!("expected MemDisp operand"),
        }

        // Verify second operand is Imm64(0x12345678)
        match &recipe.instructions[0].operands[1] {
            Operand::Imm64(val) => {
                assert_eq!(*val, 0x12345678);
            }
            _ => panic!("expected Imm64 operand"),
        }
    }

    #[test]
    fn mmio_ops_mmio_read_u32_non_literal_addr_returns_err() {
        let arena = IrArena::new();
        let missing_id = IrNodeId::new(999).expect("valid node id");

        let result = lower_stdlib_method(
            "MmioOps",
            "mmio_read_u32",
            InstrMode::Mode64,
            &[missing_id],
            &arena,
        )
        .expect("recipe should exist");

        match result {
            Err(StdlibLoweringError::NonLiteralArg { arg_index, method }) => {
                assert_eq!(arg_index, 0);
                assert_eq!(method, "MmioOps::mmio_read_u32");
            }
            Ok(_) => panic!("expected error for non-literal arg"),
        }
    }

    #[test]
    fn mmio_ops_mmio_write_u32_non_literal_val_returns_err() {
        let mut arena = IrArena::new();
        let addr_id = IrNodeId::new(1).expect("valid node id");
        let missing_val_id = IrNodeId::new(999).expect("valid node id");
        arena.literal_values_mut().insert(addr_id, 0x1000);

        let result = lower_stdlib_method(
            "MmioOps",
            "mmio_write_u32",
            InstrMode::Mode64,
            &[addr_id, missing_val_id],
            &arena,
        )
        .expect("recipe should exist");

        match result {
            Err(StdlibLoweringError::NonLiteralArg { arg_index, method }) => {
                assert_eq!(arg_index, 1);
                assert_eq!(method, "MmioOps::mmio_write_u32");
            }
            Ok(_) => panic!("expected error for non-literal val"),
        }
    }

    #[test]
    fn test_sysvregs_recipe_with_literal_synthesis() {
        // PA-r16-007 (#1062): synthetic SysVRegs recipe for testing.
        // This test verifies that a recipe can declare SysVRegs arg convention
        // and include instructions that reference SysV argument registers.

        // Manually construct a SysVRegs recipe (simulating what a future
        // SysVRegs-convention recipe would produce).
        let mut operands = SmallVec::new();
        operands.push(Operand::Reg(abi::RAX));
        operands.push(Operand::Reg(abi::RDI));  // Read SysV arg 0

        let recipe = LoweringRecipe {
            instructions: vec![Instruction {
                mnemonic: Mnemonic::Mov,
                operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: InstrMode::Mode64,
                emission_order: 0,
}],
            arg_convention: ArgConvention::SysVRegs,
            labels: vec![],
            extern_target: None,
        };

        // Verify structure
        assert_eq!(recipe.instructions.len(), 1);
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Mov);
        assert_eq!(recipe.instructions[0].operands.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);

        // Verify operands: RAX (dest), RDI (SysV arg0)
        match &recipe.instructions[0].operands[0] {
            Operand::Reg(reg) => assert_eq!(*reg, abi::RAX),
            _ => panic!("expected Reg(RAX)"),
        }
        match &recipe.instructions[0].operands[1] {
            Operand::Reg(reg) => assert_eq!(*reg, abi::RDI),
            _ => panic!("expected Reg(RDI)"),
        }
    }

    // BytesOps getter and setter recipe tests (#1063)

    #[test]
    fn bytes_ops_get_u8_lowers_to_movsized_w8() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "get_u8", InstrMode::Mode64, &[], &arena)
            .expect("get_u8 recipe should exist")
            .expect("get_u8 lowering should succeed");

        assert_eq!(recipe.instructions.len(), 1);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W8
            }
        );
        assert_eq!(recipe.instructions[0].operands.len(), 2);

        // Verify operand 0: Reg(RAX)
        match &recipe.instructions[0].operands[0] {
            Operand::Reg(reg) => assert_eq!(*reg, abi::RAX),
            _ => panic!("expected Reg(RAX)"),
        }

        // Verify operand 1: MemSib{RDI, Some(RSI), X1, 0}
        match &recipe.instructions[0].operands[1] {
            Operand::MemSib { base, index, scale, disp } => {
                assert_eq!(*base, abi::RDI);
                assert_eq!(*index, Some(abi::RSI));
                assert_eq!(*scale, paideia_as_ir::instruction::Scale::X1);
                assert_eq!(*disp, 0);
            }
            _ => panic!("expected MemSib operand"),
        }
    }

    #[test]
    fn bytes_ops_get_u16_le_lowers_to_movsized_w16() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "get_u16_le", InstrMode::Mode64, &[], &arena)
            .expect("get_u16_le recipe should exist")
            .expect("get_u16_le lowering should succeed");

        assert_eq!(recipe.instructions.len(), 1);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W16
            }
        );
    }

    #[test]
    fn bytes_ops_get_u32_le_lowers_to_movsized_w32() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "get_u32_le", InstrMode::Mode64, &[], &arena)
            .expect("get_u32_le recipe should exist")
            .expect("get_u32_le lowering should succeed");

        assert_eq!(recipe.instructions.len(), 1);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W32
            }
        );
    }

    #[test]
    fn bytes_ops_get_u32_be_lowers_to_movsized_w32_plus_bswap32() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "get_u32_be", InstrMode::Mode64, &[], &arena)
            .expect("get_u32_be recipe should exist")
            .expect("get_u32_be lowering should succeed");

        assert_eq!(recipe.instructions.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);

        // First instruction: MovSized W32
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W32
            }
        );

        // Second instruction: Bswap32
        assert_eq!(recipe.instructions[1].mnemonic, Mnemonic::Bswap32);
        assert_eq!(recipe.instructions[1].operands.len(), 1);
        match &recipe.instructions[1].operands[0] {
            Operand::Reg(reg) => assert_eq!(*reg, abi::RAX),
            _ => panic!("expected Reg(RAX)"),
        }
    }

    #[test]
    fn bytes_ops_get_u16_be_lowers_to_movsized_w16_plus_rol_w16() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "get_u16_be", InstrMode::Mode64, &[], &arena)
            .expect("get_u16_be recipe should exist")
            .expect("get_u16_be lowering should succeed");

        assert_eq!(recipe.instructions.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);

        // First instruction: MovSized W16
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W16
            }
        );

        // Second instruction: Rol W16 with imm8=8
        assert_eq!(
            recipe.instructions[1].mnemonic,
            Mnemonic::Rol {
                width: IntWidth::W16
            }
        );
        assert_eq!(recipe.instructions[1].operands.len(), 2);
        match &recipe.instructions[1].operands[1] {
            Operand::Imm64(imm) => assert_eq!(*imm, 8),
            _ => panic!("expected Imm64(8)"),
        }
    }

    #[test]
    fn bytes_ops_get_u64_le_lowers_to_movsized_w64() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "get_u64_le", InstrMode::Mode64, &[], &arena)
            .expect("get_u64_le recipe should exist")
            .expect("get_u64_le lowering should succeed");

        assert_eq!(recipe.instructions.len(), 1);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W64
            }
        );
    }

    #[test]
    fn bytes_ops_get_u64_be_lowers_to_movsized_w64_plus_bswap() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "get_u64_be", InstrMode::Mode64, &[], &arena)
            .expect("get_u64_be recipe should exist")
            .expect("get_u64_be lowering should succeed");

        assert_eq!(recipe.instructions.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);

        // First instruction: MovSized W64
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W64
            }
        );

        // Second instruction: Bswap
        assert_eq!(recipe.instructions[1].mnemonic, Mnemonic::Bswap);
        assert_eq!(recipe.instructions[1].operands.len(), 1);
        match &recipe.instructions[1].operands[0] {
            Operand::Reg(reg) => assert_eq!(*reg, abi::RAX),
            _ => panic!("expected Reg(RAX)"),
        }
    }

    #[test]
    fn bytes_ops_put_u8_lowers_to_add_plus_movsized_w8() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "put_u8", InstrMode::Mode64, &[], &arena)
            .expect("put_u8 recipe should exist")
            .expect("put_u8 lowering should succeed");

        assert_eq!(recipe.instructions.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);

        // First instruction: Add RDI, RSI
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Add);
        assert_eq!(recipe.instructions[0].operands.len(), 2);

        // Second instruction: MovSized W8
        assert_eq!(
            recipe.instructions[1].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W8
            }
        );
        assert_eq!(recipe.instructions[1].operands.len(), 2);
    }

    #[test]
    fn bytes_ops_put_u16_le_lowers_to_add_plus_movsized_w16() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "put_u16_le", InstrMode::Mode64, &[], &arena)
            .expect("put_u16_le recipe should exist")
            .expect("put_u16_le lowering should succeed");

        assert_eq!(recipe.instructions.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Add);
        assert_eq!(
            recipe.instructions[1].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W16
            }
        );
    }

    #[test]
    fn bytes_ops_put_u32_le_lowers_to_add_plus_movsized_w32() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "put_u32_le", InstrMode::Mode64, &[], &arena)
            .expect("put_u32_le recipe should exist")
            .expect("put_u32_le lowering should succeed");

        assert_eq!(recipe.instructions.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Add);
        assert_eq!(
            recipe.instructions[1].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W32
            }
        );
    }

    #[test]
    fn bytes_ops_put_u16_be_lowers_to_rol_w16_plus_add_plus_movsized_w16() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "put_u16_be", InstrMode::Mode64, &[], &arena)
            .expect("put_u16_be recipe should exist")
            .expect("put_u16_be lowering should succeed");

        assert_eq!(recipe.instructions.len(), 3);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);

        // First instruction: Rol W16 with imm8=8
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::Rol {
                width: IntWidth::W16
            }
        );
        assert_eq!(recipe.instructions[0].operands.len(), 2);
        match &recipe.instructions[0].operands[0] {
            Operand::Reg(reg) => assert_eq!(*reg, abi::RDX),
            _ => panic!("expected Reg(RDX)"),
        }
        match &recipe.instructions[0].operands[1] {
            Operand::Imm64(imm) => assert_eq!(*imm, 8),
            _ => panic!("expected Imm64(8)"),
        }

        // Second instruction: Add RDI, RSI
        assert_eq!(recipe.instructions[1].mnemonic, Mnemonic::Add);

        // Third instruction: MovSized W16
        assert_eq!(
            recipe.instructions[2].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W16
            }
        );
    }

    #[test]
    fn bytes_ops_put_u32_be_lowers_to_bswap32_plus_add_plus_movsized_w32() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "put_u32_be", InstrMode::Mode64, &[], &arena)
            .expect("put_u32_be recipe should exist")
            .expect("put_u32_be lowering should succeed");

        assert_eq!(recipe.instructions.len(), 3);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);

        // First instruction: Bswap32 RDX
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Bswap32);
        assert_eq!(recipe.instructions[0].operands.len(), 1);
        match &recipe.instructions[0].operands[0] {
            Operand::Reg(reg) => assert_eq!(*reg, abi::RDX),
            _ => panic!("expected Reg(RDX)"),
        }

        // Second instruction: Add RDI, RSI
        assert_eq!(recipe.instructions[1].mnemonic, Mnemonic::Add);

        // Third instruction: MovSized W32
        assert_eq!(
            recipe.instructions[2].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W32
            }
        );
    }

    #[test]
    fn bytes_ops_put_u64_le_lowers_to_add_plus_movsized_w64() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "put_u64_le", InstrMode::Mode64, &[], &arena)
            .expect("put_u64_le recipe should exist")
            .expect("put_u64_le lowering should succeed");

        assert_eq!(recipe.instructions.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Add);
        assert_eq!(
            recipe.instructions[1].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W64
            }
        );
    }

    #[test]
    fn bytes_ops_put_u64_be_lowers_to_bswap_plus_add_plus_movsized_w64() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "put_u64_be", InstrMode::Mode64, &[], &arena)
            .expect("put_u64_be recipe should exist")
            .expect("put_u64_be lowering should succeed");

        assert_eq!(recipe.instructions.len(), 3);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);

        // First instruction: Bswap RDX
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Bswap);
        assert_eq!(recipe.instructions[0].operands.len(), 1);
        match &recipe.instructions[0].operands[0] {
            Operand::Reg(reg) => assert_eq!(*reg, abi::RDX),
            _ => panic!("expected Reg(RDX)"),
        }

        // Second instruction: Add RDI, RSI
        assert_eq!(recipe.instructions[1].mnemonic, Mnemonic::Add);

        // Third instruction: MovSized W64
        assert_eq!(
            recipe.instructions[2].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W64
            }
        );
    }

    // PA-r16-007 (#1066): Tests for label support in recipes

    #[test]
    fn test_loop_ops_countdown_recipe_has_correct_shape() {
        // Verify the test countdown recipe has the expected structure with labels.
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("TestLoopOps", "test_countdown", InstrMode::Mode64, &[], &arena)
            .expect("test_countdown recipe should exist")
            .expect("test_countdown lowering should succeed");

        // Verify structure: 3 instructions
        assert_eq!(recipe.instructions.len(), 3);

        // Verify mnemonics
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Mov);
        assert_eq!(recipe.instructions[1].mnemonic, Mnemonic::Dec);
        assert_eq!(
            recipe.instructions[2].mnemonic,
            Mnemonic::Jcc(paideia_as_ir::instruction::Cond::Ne)
        );

        // Verify arg_convention is Literal
        assert_eq!(recipe.arg_convention, ArgConvention::Literal);

        // Verify labels: should have one label "loop_top" at index 1
        assert_eq!(recipe.labels.len(), 1);
        assert_eq!(recipe.labels[0].0, "loop_top");
        assert_eq!(recipe.labels[0].1, 1);
    }

    #[test]
    fn test_countdown_recipe_jcc_references_local_label() {
        // Verify the Jcc operand in the test countdown recipe references the local label.
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("TestLoopOps", "test_countdown", InstrMode::Mode64, &[], &arena)
            .expect("test_countdown recipe should exist")
            .expect("test_countdown lowering should succeed");

        // Check the Jcc instruction (index 2) has a LabelRef operand
        let jcc_inst = &recipe.instructions[2];
        assert_eq!(jcc_inst.operands.len(), 1);

        match &jcc_inst.operands[0] {
            Operand::LabelRef { name, addend } => {
                assert_eq!(name, "loop_top");
                assert_eq!(*addend, 0);
            }
            _ => panic!("expected LabelRef operand in Jcc"),
        }
    }

    #[test]
    fn checksum_ops_ipv4_checksum_recipe_has_correct_shape() {
        use paideia_as_ir::instruction::Cond;

        let arena = IrArena::new();
        let recipe = lower_stdlib_method("ChecksumOps", "ipv4_checksum", InstrMode::Mode64, &[], &arena)
            .expect("ipv4_checksum recipe should exist")
            .expect("ipv4_checksum lowering should succeed");

        // Verify instruction count is 26 (double-fold + masked not replaces single fold + adc)
        assert_eq!(recipe.instructions.len(), 26);

        // Verify arg_convention is SysVRegs
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);

        // Verify labels field has three labels at correct indices
        assert_eq!(recipe.labels.len(), 3);
        assert_eq!(recipe.labels[0], ("loop_start", 5));
        assert_eq!(recipe.labels[1], ("odd_check", 11));
        assert_eq!(recipe.labels[2], ("fold", 16));

        // Spot-check mnemonics
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Xor);
        assert_eq!(recipe.instructions[5].mnemonic, Mnemonic::Movzx);
        // Verify fold section: first pass mov, shr, and, add
        assert_eq!(recipe.instructions[16].mnemonic, Mnemonic::Mov);
        assert_eq!(recipe.instructions[17].mnemonic, Mnemonic::Shr);
        assert_eq!(recipe.instructions[18].mnemonic, Mnemonic::And);
        assert_eq!(recipe.instructions[19].mnemonic, Mnemonic::Add);
        // Verify fold section: second pass mov, shr, and, add
        assert_eq!(recipe.instructions[20].mnemonic, Mnemonic::Mov);
        assert_eq!(recipe.instructions[21].mnemonic, Mnemonic::Shr);
        assert_eq!(recipe.instructions[22].mnemonic, Mnemonic::And);
        assert_eq!(recipe.instructions[23].mnemonic, Mnemonic::Add);
        // Verify final not + mask
        assert_eq!(recipe.instructions[24].mnemonic, Mnemonic::Not);
        assert_eq!(recipe.instructions[25].mnemonic, Mnemonic::And);

        // Verify Jcc conditions in loop back edges
        match recipe.instructions[4].mnemonic {
            Mnemonic::Jcc(Cond::Zero) => { /* expected */ }
            _ => panic!("instruction[4] should be Jcc(Zero)"),
        }

        match recipe.instructions[10].mnemonic {
            Mnemonic::Jcc(Cond::NonZero) => { /* expected */ }
            _ => panic!("instruction[10] should be Jcc(NonZero)"),
        }

        match recipe.instructions[12].mnemonic {
            Mnemonic::Jcc(Cond::Zero) => { /* expected */ }
            _ => panic!("instruction[12] should be Jcc(Zero)"),
        }

        // Verify Adc instructions still present in word accumulation loop (before fold)
        match recipe.instructions[7].mnemonic {
            Mnemonic::Adc { width: IntWidth::W64 } => { /* expected */ }
            _ => panic!("instruction[7] should be Adc with W64 width"),
        }

        match recipe.instructions[15].mnemonic {
            Mnemonic::Adc { width: IntWidth::W64 } => { /* expected */ }
            _ => panic!("instruction[15] should be Adc with W64 width"),
        }
    }

    // #1228 (Phase 2 of #1064): BulkMemOps REP-string recipe shape tests.

    #[test]
    fn bulkmem_ops_memcpy_recipe_exists() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BulkMemOps", "memcpy", InstrMode::Mode64, &[], &arena)
            .expect("memcpy recipe should exist")
            .expect("memcpy lowering should succeed");

        assert_eq!(recipe.instructions.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);

        // First instruction: mov rcx, rdx
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Mov);
        assert_eq!(recipe.instructions[0].operands.len(), 2);
        match (&recipe.instructions[0].operands[0], &recipe.instructions[0].operands[1]) {
            (Operand::Reg(dst), Operand::Reg(src)) => {
                assert_eq!(*dst, abi::RCX);
                assert_eq!(*src, abi::RDX);
            }
            _ => panic!("expected mov rcx, rdx"),
        }

        // Terminal instruction: rep movsb (zero-arity)
        assert_eq!(recipe.instructions[1].mnemonic, Mnemonic::RepMovsb);
        assert!(recipe.instructions[1].operands.is_empty());
    }

    #[test]
    fn bulkmem_ops_memset_recipe_exists() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BulkMemOps", "memset", InstrMode::Mode64, &[], &arena)
            .expect("memset recipe should exist")
            .expect("memset lowering should succeed");

        assert_eq!(recipe.instructions.len(), 3);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);

        // First instruction: mov rax, rsi (fill byte into AL)
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Mov);
        match (&recipe.instructions[0].operands[0], &recipe.instructions[0].operands[1]) {
            (Operand::Reg(dst), Operand::Reg(src)) => {
                assert_eq!(*dst, abi::RAX);
                assert_eq!(*src, abi::RSI);
            }
            _ => panic!("expected mov rax, rsi"),
        }

        // Second instruction: mov rcx, rdx (REP implicit counter)
        assert_eq!(recipe.instructions[1].mnemonic, Mnemonic::Mov);
        match (&recipe.instructions[1].operands[0], &recipe.instructions[1].operands[1]) {
            (Operand::Reg(dst), Operand::Reg(src)) => {
                assert_eq!(*dst, abi::RCX);
                assert_eq!(*src, abi::RDX);
            }
            _ => panic!("expected mov rcx, rdx"),
        }

        // Terminal instruction: rep stosb (zero-arity)
        assert_eq!(recipe.instructions[2].mnemonic, Mnemonic::RepStosb);
        assert!(recipe.instructions[2].operands.is_empty());
    }

    #[test]
    fn bulkmem_ops_memcpy_qwords_recipe_exists() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BulkMemOps", "memcpy_qwords", InstrMode::Mode64, &[], &arena)
            .expect("memcpy_qwords recipe should exist")
            .expect("memcpy_qwords lowering should succeed");

        assert_eq!(recipe.instructions.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);

        // First instruction: mov rcx, rdx
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Mov);
        match (&recipe.instructions[0].operands[0], &recipe.instructions[0].operands[1]) {
            (Operand::Reg(dst), Operand::Reg(src)) => {
                assert_eq!(*dst, abi::RCX);
                assert_eq!(*src, abi::RDX);
            }
            _ => panic!("expected mov rcx, rdx"),
        }

        // Terminal instruction: rep movsq (zero-arity)
        assert_eq!(recipe.instructions[1].mnemonic, Mnemonic::RepMovsq);
        assert!(recipe.instructions[1].operands.is_empty());
    }

    #[test]
    fn bulkmem_ops_memset_qwords_recipe_exists() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BulkMemOps", "memset_qwords", InstrMode::Mode64, &[], &arena)
            .expect("memset_qwords recipe should exist")
            .expect("memset_qwords lowering should succeed");

        assert_eq!(recipe.instructions.len(), 3);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);

        // First instruction: mov rax, rsi (fill qword into RAX)
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Mov);
        match (&recipe.instructions[0].operands[0], &recipe.instructions[0].operands[1]) {
            (Operand::Reg(dst), Operand::Reg(src)) => {
                assert_eq!(*dst, abi::RAX);
                assert_eq!(*src, abi::RSI);
            }
            _ => panic!("expected mov rax, rsi"),
        }

        // Second instruction: mov rcx, rdx (REP implicit counter)
        assert_eq!(recipe.instructions[1].mnemonic, Mnemonic::Mov);
        match (&recipe.instructions[1].operands[0], &recipe.instructions[1].operands[1]) {
            (Operand::Reg(dst), Operand::Reg(src)) => {
                assert_eq!(*dst, abi::RCX);
                assert_eq!(*src, abi::RDX);
            }
            _ => panic!("expected mov rcx, rdx"),
        }

        // Terminal instruction: rep stosq (zero-arity)
        assert_eq!(recipe.instructions[2].mnemonic, Mnemonic::RepStosq);
        assert!(recipe.instructions[2].operands.is_empty());
    }

    // ---------- paideia-as#1305: crypto extern-C recipes ----------

    /// `Argon2id::derive` lowers to an empty SysVRegs recipe whose
    /// `extern_target` names the paideia-as-crypto FFI thunk. emit_call
    /// splices nothing, then rewrites the CALL target to
    /// `paideia_crypto_argon2id_derive` and falls through to the normal
    /// CALL / scratch-pop / postlude path. Any mismatch on the symbol
    /// name would produce an unresolvable relocation at link time — the
    /// unit test pins the string exactly.
    #[test]
    fn argon2id_derive_recipe_targets_ffi_thunk() {
        let arena = IrArena::new();
        let recipe =
            lower_stdlib_method("Argon2id", "derive", InstrMode::Mode64, &[], &arena)
                .expect("Argon2id::derive recipe should exist")
                .expect("Argon2id::derive lowering should succeed");

        assert!(
            recipe.instructions.is_empty(),
            "extern-C recipes carry no preamble instructions"
        );
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert!(recipe.labels.is_empty());
        assert_eq!(
            recipe.extern_target.as_deref(),
            Some("paideia_crypto_argon2id_derive")
        );
    }

    /// `ChaCha20Poly1305::seal` lowers to an extern-target recipe.
    #[test]
    fn chacha20_poly1305_seal_recipe_targets_ffi_thunk() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method(
            "ChaCha20Poly1305",
            "seal",
            InstrMode::Mode64,
            &[],
            &arena,
        )
        .expect("ChaCha20Poly1305::seal recipe should exist")
        .expect("ChaCha20Poly1305::seal lowering should succeed");

        assert!(recipe.instructions.is_empty());
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert_eq!(
            recipe.extern_target.as_deref(),
            Some("paideia_crypto_chacha20_poly1305_seal")
        );
    }

    /// `ChaCha20Poly1305::open` lowers to an extern-target recipe.
    #[test]
    fn chacha20_poly1305_open_recipe_targets_ffi_thunk() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method(
            "ChaCha20Poly1305",
            "open",
            InstrMode::Mode64,
            &[],
            &arena,
        )
        .expect("ChaCha20Poly1305::open recipe should exist")
        .expect("ChaCha20Poly1305::open lowering should succeed");

        assert!(recipe.instructions.is_empty());
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert_eq!(
            recipe.extern_target.as_deref(),
            Some("paideia_crypto_chacha20_poly1305_open")
        );
    }

    /// Unknown crypto methods must NOT match — otherwise a typo like
    /// `Argon2id::deriveee` would resolve to the FFI thunk under a
    /// wrong name at link time. The dispatcher returns `None`, so
    /// emit_call falls through to normal call emission and eventually
    /// diagnoses T0553 (undefined identifier).
    #[test]
    fn unknown_argon2id_method_returns_none() {
        let arena = IrArena::new();
        assert!(
            lower_stdlib_method("Argon2id", "no_such_method", InstrMode::Mode64, &[], &arena)
                .is_none()
        );
    }

    #[test]
    fn unknown_chacha20_poly1305_method_returns_none() {
        let arena = IrArena::new();
        assert!(
            lower_stdlib_method(
                "ChaCha20Poly1305",
                "no_such_method",
                InstrMode::Mode64,
                &[],
                &arena
            )
            .is_none()
        );
    }

    // ---------- paideia-as#1352: ML-KEM-768 extern-C recipes ----------

    /// `MlKem768::keygen` lowers to an extern-target recipe whose
    /// symbol name matches the `#[unsafe(no_mangle)]` thunk in
    /// `paideia-as-crypto::ffi`. Any drift on the string would
    /// produce an unresolvable relocation at link time — the unit
    /// test pins the string exactly.
    #[test]
    fn ml_kem_768_keygen_recipe_targets_ffi_thunk() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("MlKem768", "keygen", InstrMode::Mode64, &[], &arena)
            .expect("MlKem768::keygen recipe should exist")
            .expect("MlKem768::keygen lowering should succeed");

        assert!(
            recipe.instructions.is_empty(),
            "extern-C recipes carry no preamble instructions"
        );
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert!(recipe.labels.is_empty());
        assert_eq!(
            recipe.extern_target.as_deref(),
            Some("paideia_crypto_ml_kem_768_keygen")
        );
    }

    /// `MlKem768::encaps` lowers to an extern-target recipe.
    #[test]
    fn ml_kem_768_encaps_recipe_targets_ffi_thunk() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("MlKem768", "encaps", InstrMode::Mode64, &[], &arena)
            .expect("MlKem768::encaps recipe should exist")
            .expect("MlKem768::encaps lowering should succeed");

        assert!(recipe.instructions.is_empty());
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert_eq!(
            recipe.extern_target.as_deref(),
            Some("paideia_crypto_ml_kem_768_encaps")
        );
    }

    /// `MlKem768::decaps` lowers to an extern-target recipe.
    #[test]
    fn ml_kem_768_decaps_recipe_targets_ffi_thunk() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("MlKem768", "decaps", InstrMode::Mode64, &[], &arena)
            .expect("MlKem768::decaps recipe should exist")
            .expect("MlKem768::decaps lowering should succeed");

        assert!(recipe.instructions.is_empty());
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert_eq!(
            recipe.extern_target.as_deref(),
            Some("paideia_crypto_ml_kem_768_decaps")
        );
    }

    /// Unknown ML-KEM-768 methods must NOT match — otherwise a typo
    /// like `MlKem768::keygeen` would resolve to an unresolvable
    /// symbol at link time rather than diagnose T0553 up front.
    #[test]
    fn unknown_ml_kem_768_method_returns_none() {
        let arena = IrArena::new();
        assert!(
            lower_stdlib_method("MlKem768", "no_such_method", InstrMode::Mode64, &[], &arena)
                .is_none()
        );
    }

    /// Existing SysVRegs recipes must continue to carry `extern_target:
    /// None` so emit_call takes the early-return splice path (the
    /// existing behaviour). This test pins that invariant for one
    /// representative recipe from each interesting shape: msr (inline
    /// mnemonics), cpuid (RBX-bracketed), checksum (labelled loop). If
    /// a future refactor sets one of these to `Some(_)` accidentally,
    /// emit_call would emit an unresolved `call` instead of the inline
    /// sequence — a silent miscompile — and this test would catch it.
    #[test]
    fn preexisting_sysvregs_recipes_have_no_extern_target() {
        let arena = IrArena::new();

        let rdmsr = lower_stdlib_method("MsrOps", "rdmsr", InstrMode::Mode64, &[], &arena)
            .expect("rdmsr recipe exists")
            .expect("rdmsr lowering ok");
        assert!(
            rdmsr.extern_target.is_none(),
            "MsrOps::rdmsr must remain a self-contained recipe"
        );

        let cpuid = lower_stdlib_method(
            "CpuidOps",
            "cpuid_leaf_ad",
            InstrMode::Mode64,
            &[],
            &arena,
        )
        .expect("cpuid_leaf_ad recipe exists")
        .expect("cpuid_leaf_ad lowering ok");
        assert!(
            cpuid.extern_target.is_none(),
            "CpuidOps::cpuid_leaf_ad must remain a self-contained recipe"
        );

        let ipv4 = lower_stdlib_method(
            "ChecksumOps",
            "ipv4_checksum",
            InstrMode::Mode64,
            &[],
            &arena,
        )
        .expect("ipv4_checksum recipe exists")
        .expect("ipv4_checksum lowering ok");
        assert!(
            ipv4.extern_target.is_none(),
            "ChecksumOps::ipv4_checksum must remain a self-contained recipe"
        );
    }
}
