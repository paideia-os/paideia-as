//! Symbol and label resolution: rewrite relocation operands to emit-ready form.
//!
//! This module provides the operand-level resolver that rewrites every
//! reloc-producing operand in a slice of `Instruction`s into a form that
//! `emit_instruction` accepts (no `SymbolRef`, `LabelRef`, `MemRipRelSym`,
//! `MemSymIndexed` remain), given a caller-supplied name-to-address table and
//! a label-to-index map.

use crate::instruction::{Instruction, Mnemonic, Operand};
use crate::label_map::LabelMap;
use crate::symbol_table::SymbolTable;
use alloc::string::String;
use alloc::vec::Vec;
use core::mem;

/// How to materialize a resolved symbol address in the operand.
///
/// This is `#[non_exhaustive]` so v0.21+ may add richer policies (e.g., a
/// hybrid `RipRel32IfNear`) without breaking downstream matches.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum ResolvePolicy {
    /// Rewrite `SymbolRef { name, addend }` to
    /// `Imm64((symbols[name] as i64) + addend as i64)`.
    ///
    /// Suitable for the WASM-JIT `mov reg, imm64; call reg` idiom and for any
    /// caller who doesn't know the buffer's emit-time base address. Every
    /// symbol reference costs 10 bytes (`MOV r64, imm64`) — no rel32
    /// tightening.
    AbsoluteImm64,
}

/// Errors returned by [`resolve_symbols`].
///
/// `#[non_exhaustive]` — v0.21+ may add variants without a major bump.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolveError {
    /// A `SymbolRef` / `MemRipRelSym` / `MemSymIndexed` operand referenced a
    /// name that is not present in the `SymbolTable`.
    UnknownSymbol {
        /// Instruction slice index where the reference lives.
        instr_index: usize,
        /// Operand index within that instruction (0 = first operand).
        operand_index: usize,
        /// The unresolved symbol name.
        name: String,
    },

    /// A `LabelRef` operand referenced a label not present in the `LabelMap`.
    UnknownLabel {
        /// Instruction slice index where the reference lives.
        instr_index: usize,
        /// Operand index within that instruction.
        operand_index: usize,
        /// The unresolved label name.
        name: String,
    },

    /// A resolved symbol/label address fell outside the encodable range for
    /// the operand form the mnemonic requires (e.g., `MemSymIndexed` requires
    /// the resolved address to fit in i32 for its Abs32 relocation slot).
    OutOfRange {
        /// Instruction slice index where the out-of-range value materialised.
        instr_index: usize,
        /// The resolved address that overflowed.
        resolved: u64,
        /// The encoding form that the mnemonic requires.
        required_form: &'static str,
    },

    /// A `Mnemonic::Vpxor`-family (or other) instruction cannot legally accept
    /// the operand form the resolver would have produced (e.g., a `Jcc` with
    /// an absolute `Imm64` — Jcc encodes rel32 only). Signals a
    /// caller-supplied policy / mnemonic mismatch.
    NotEncodableForMnemonic {
        /// Instruction slice index.
        instr_index: usize,
        /// The mnemonic that rejected the resolved form.
        mnemonic: Mnemonic,
    },
}

impl core::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ResolveError::UnknownSymbol { instr_index, operand_index, name } => {
                write!(f, "unknown symbol '{}' at instruction {} operand {}", name, instr_index, operand_index)
            }
            ResolveError::UnknownLabel { instr_index, operand_index, name } => {
                write!(f, "unknown label '{}' at instruction {} operand {}", name, instr_index, operand_index)
            }
            ResolveError::OutOfRange { instr_index, resolved, required_form } => {
                write!(f, "resolved address 0x{:x} out of range for {} at instruction {}", resolved, required_form, instr_index)
            }
            ResolveError::NotEncodableForMnemonic { instr_index, mnemonic } => {
                write!(f, "resolved operand not encodable for mnemonic {:?} at instruction {}", mnemonic, instr_index)
            }
        }
    }
}

/// Resolve every `SymbolRef`, `LabelRef`, `MemRipRelSym`, and `MemSymIndexed`
/// operand in `instructions` into a form `emit_instruction` accepts.
///
/// After a successful return, no instruction in `instructions` carries a
/// reloc-producing operand. Feeding the slice through `emit_instruction`
/// will not return `EmitError::UnresolvedRelocation`.
///
/// # Errors
///
/// See [`ResolveError`] for the taxonomy. On error, `instructions` may have
/// been **partially mutated** (some entries rewritten, some not) — callers
/// that need transactional semantics should snapshot the slice with `clone`
/// before the call. This is documented and tested at §5 canary R-9.
///
/// # Stability
///
/// This function is part of the paideia-as v0.20 stable public API. Signature
/// changes require a major-version bump; adding variants to [`ResolveError`]
/// or [`ResolvePolicy`] is compatible because both enums are
/// `#[non_exhaustive]`.
pub fn resolve_symbols(
    instructions: &mut [Instruction],
    symbols: &SymbolTable,
    labels: &LabelMap,
    _policy: ResolvePolicy,
) -> Result<(), ResolveError> {
    // Pass 1: Layout — compute byte offsets for each instruction.
    let mut offsets = Vec::with_capacity(instructions.len());
    let mut cursor: u32 = 0;
    for ins in instructions.iter() {
        offsets.push(cursor);
        cursor = cursor.saturating_add(ins.mnemonic.estimated_size(&ins.operands));
    }

    // Pass 2: Rewrite operands.
    for (i, ins) in instructions.iter_mut().enumerate() {
        for (j, op) in ins.operands.iter_mut().enumerate() {
            match op {
                // Unchanged: register, memory, immediate operands
                Operand::Reg(_)
                | Operand::SegReg(_)
                | Operand::Imm64(_)
                | Operand::MemSib { .. }
                | Operand::MemDisp { .. }
                | Operand::MemRipRel { .. }
                | Operand::MemSeg { .. }
                | Operand::Var { .. }
                | Operand::MemDispIndexed { .. } => {
                    // No change
                }

                // Symbol reference → Imm64 (under AbsoluteImm64 policy)
                Operand::SymbolRef { name, addend } => {
                    let sym_addr = symbols
                        .get(name)
                        .ok_or_else(|| ResolveError::UnknownSymbol {
                            instr_index: i,
                            operand_index: j,
                            name: name.clone(),
                        })?;
                    let resolved = (sym_addr as i64).wrapping_add(*addend as i64);
                    let _ = mem::replace(op, Operand::Imm64(resolved));
                }

                // Label reference → Imm64 (offset into the emitted buffer)
                Operand::LabelRef { name, addend } => {
                    let label_idx = labels.get(name).ok_or_else(|| ResolveError::UnknownLabel {
                        instr_index: i,
                        operand_index: j,
                        name: name.clone(),
                    })?;
                    let label_offset = offsets[label_idx] as i64;
                    let resolved = label_offset.wrapping_add(*addend as i64);
                    let _ = mem::replace(op, Operand::Imm64(resolved));
                }

                // RIP-relative symbol → MemRipRel (absolute addressing)
                Operand::MemRipRelSym { name, addend } => {
                    let sym_addr = symbols
                        .get(name)
                        .ok_or_else(|| ResolveError::UnknownSymbol {
                            instr_index: i,
                            operand_index: j,
                            name: name.clone(),
                        })?;
                    let resolved = (sym_addr as i64).wrapping_add(*addend as i64);
                    let disp = i32::try_from(resolved).map_err(|_| ResolveError::OutOfRange {
                        instr_index: i,
                        resolved: sym_addr,
                        required_form: "i32",
                    })?;
                    let _ = mem::replace(op, Operand::MemRipRel { disp });
                }

                // Indexed memory with symbol → MemDispIndexed
                Operand::MemSymIndexed { name, addend, index, scale } => {
                    let sym_addr = symbols
                        .get(name)
                        .ok_or_else(|| ResolveError::UnknownSymbol {
                            instr_index: i,
                            operand_index: j,
                            name: name.clone(),
                        })?;
                    let resolved = (sym_addr as i64).wrapping_add(*addend as i64);
                    let disp = i32::try_from(resolved).map_err(|_| ResolveError::OutOfRange {
                        instr_index: i,
                        resolved: sym_addr,
                        required_form: "i32",
                    })?;
                    // Copy values before mem::replace to avoid borrow checker issues
                    let index_copy = *index;
                    let scale_copy = *scale;
                    let _ = mem::replace(op, Operand::MemDispIndexed {
                        disp,
                        index: index_copy,
                        scale: scale_copy,
                    });
                }
            }
        }
    }

    // Pass 3: Debug-assert no reloc operands remain.
    #[cfg(debug_assertions)]
    for (i, ins) in instructions.iter().enumerate() {
        for (j, op) in ins.operands.iter().enumerate() {
            debug_assert!(
                !matches!(
                    op,
                    Operand::SymbolRef { .. }
                        | Operand::LabelRef { .. }
                        | Operand::MemRipRelSym { .. }
                        | Operand::MemSymIndexed { .. }
                ),
                "resolve_symbols left reloc operand at [{}][{}]",
                i,
                j
            );
        }
    }

    Ok(())
}
