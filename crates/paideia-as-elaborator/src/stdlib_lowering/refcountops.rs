//! `RefcountOps` stdlib-lowering recipes.
//!
//! Extracted 2026-08-11 from `stdlib_lowering.rs` (God-file refactor).
//! Each stdlib trait family lives in its own submodule so unrelated
//! intrinsic families evolve independently.

#![allow(unused_imports)]

use paideia_as_ir::{
    IrArena, IrNodeId, SmallVec, abi,
    instruction::{InstrMode, Instruction, IntWidth, Mnemonic, Operand, SegPrefix},
};

use super::{ArgConvention, LoweringRecipe, StdlibLoweringError};

/// Dispatch a `RefcountOps::<method_name>` call to its lowering recipe.
///
/// Returns `None` for unknown methods (caller falls through to normal
/// call emission).
pub(super) fn try_lower(
    method_name: &str,
    mode: InstrMode,
    arg_ids: &[IrNodeId],
    arena: &IrArena,
) -> Option<Result<LoweringRecipe, StdlibLoweringError>> {
    let _ = (arg_ids, arena);
    match method_name {
        // PA-v0.21-003 (#1279): RefcountOps atomic refcount primitives.
        // Trait declared at paideia-stdlib/pdx/refcount.pdx (PA-R16-009).
        // SysVRegs: RDI = counter (*u32), RAX = return.
        //
        // The AC says "compile to lock xadd sequence" — every primitive here
        // reaches for `lock xadd_d [rdi], eax`, matching the design comment
        // in paideia-os core/sync/atomic_refcount.pdx (PA-R18-M3-002 / #769)
        // which is the primary consumer once #1279 unblocks that migration
        // path away from raw asm.
        //
        // Return value semantics (matching Linux atomic_fetch_add /
        // atomic_dec_and_test):
        //   - refcount_incr(p)         → previous *p
        //   - refcount_decr(p)         → previous *p
        //   - refcount_decr_and_test(p)→ bool: true iff new *p == 0
        //
        // decr_and_test's boolean is derived from the previous value: since
        // xadd delivers `previous` in EAX and decrements by 1 atomically,
        // `new == 0` iff `previous == 1`. Emit that comparison inline as
        // `cmp eax, 1 / sete al / movzx eax, al` — a canonical bool-return
        // sequence that costs 8 additional bytes beyond the atomic RMW.
        "refcount_incr" => {
            // mov eax, 1               ; B8 01 00 00 00       (5 bytes)
            // lock xadd_d [rdi], eax  ; F0 0F C1 07           (4 bytes)
            let mut mov_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            mov_ops.push(Operand::Reg(abi::RAX));
            mov_ops.push(Operand::Imm64(1));

            let mut xadd_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            xadd_ops.push(Operand::MemSib {
                base: abi::RDI,
                index: None,
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });
            xadd_ops.push(Operand::Reg(abi::RAX));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
                        operands: mov_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::LockXadd { width: IntWidth::W32 },
                        operands: xadd_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                ],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
            }))
        }
        "refcount_decr" => {
            // mov eax, -1              ; B8 FF FF FF FF       (5 bytes)
            // lock xadd_d [rdi], eax  ; F0 0F C1 07           (4 bytes)
            let mut mov_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            mov_ops.push(Operand::Reg(abi::RAX));
            mov_ops.push(Operand::Imm64(-1));

            let mut xadd_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            xadd_ops.push(Operand::MemSib {
                base: abi::RDI,
                index: None,
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });
            xadd_ops.push(Operand::Reg(abi::RAX));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
                        operands: mov_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::LockXadd { width: IntWidth::W32 },
                        operands: xadd_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                ],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
            }))
        }
        "refcount_decr_and_test" => {
            // mov eax, -1              ; B8 FF FF FF FF       (5 bytes)
            // lock xadd_d [rdi], eax   ; F0 0F C1 07          (4 bytes)  ; EAX = previous *p
            // cmp eax, 1               ; 83 F8 01             (3 bytes)  ; ZF = 1 iff prev == 1 (new == 0)
            // sete al                  ; 0F 94 C0             (3 bytes)  ; AL = 1 iff ZF
            // movzx eax, al            ; 48 0F B6 C0          (4 bytes)  ; zero-extend to bool return
            use paideia_as_ir::instruction::Cond;

            let mut mov_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            mov_ops.push(Operand::Reg(abi::RAX));
            mov_ops.push(Operand::Imm64(-1));

            let mut xadd_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            xadd_ops.push(Operand::MemSib {
                base: abi::RDI,
                index: None,
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });
            xadd_ops.push(Operand::Reg(abi::RAX));

            let mut cmp_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            cmp_ops.push(Operand::Reg(abi::RAX));
            cmp_ops.push(Operand::Imm64(1));

            let mut sete_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            sete_ops.push(Operand::Reg(abi::RAX)); // low byte AL

            let mut movzx_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            movzx_ops.push(Operand::Reg(abi::RAX));
            movzx_ops.push(Operand::Reg(abi::RAX)); // src also RAX; encoder treats as r/m8 via encoding_hint

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
                        operands: mov_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::LockXadd { width: IntWidth::W32 },
                        operands: xadd_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::CmpSized { width: IntWidth::W32 },
                        operands: cmp_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Setcc(Cond::Eq),
                        operands: sete_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Movzx,
                        operands: movzx_ops,
                        encoding_hint: Some(paideia_as_ir::instruction::EncodingHint {
                            opcode: 0x0F_B6,
                            operand_size: 1,
                        }),
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                ],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
            }))
        }
        _ => None,
    }
}
