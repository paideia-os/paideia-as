//! Nested pattern binding decomposition for match arms and stack-form
//! enum let-bindings.
//!
//! Public entry points (called from stack-form arm binding and
//! register-form arm binding respectively):
//! - `lower_pattern` (PA-r17-009 m9-009): stack-form. Emits width-correct
//!   memory loads relative to `base_reg + base_offset` for each Simple
//!   leaf; recurses on `Record` and `EnumVariant { payload: Some }`.
//! - `lower_pattern_from_reg` (#1084 follow-up): register-form. Extracts
//!   fields from a single source register via shift + narrowing move,
//!   using bit offsets computed from record field layouts.
//!
//! Both delegate to `*_inner` recursion helpers that thread a
//! live-reg-filtered scratch pool through the traversal (#1216).
//!
//! Extracted verbatim from the pre-split `emit_enum_match.rs` (issue #1409).

use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use paideia_as_ir::{abi, IrArena, IrNodeId, SmallVec};

use crate::emit_walker::EmitWalker;

use super::diagnostics::{t0566_code, u1652_code, u1653_code, u1654_code};

impl EmitWalker {
    /// Phase 17 m9-009 (pa-r17-009): Lower nested pattern bindings.
    ///
    /// Recursively decomposes a PatternBinding tree, emitting load instructions
    /// to extract nested record fields and enum payloads. Bindings are recorded
    /// in LocalBindingTable for later variable resolution.
    ///
    /// Algorithm:
    /// - `Wildcard`: no-op
    /// - `Simple(name)`: emit width-correct load; insert into LocalBindingTable
    /// - `EnumVariant { payload: Some(inner) }`: recurse with payload offset (base_offset + 8)
    /// - `Record { type_id, fields }`: for each field, compute sub_offset and recurse
    ///
    /// Register allocation from scratch pool: [RCX(1), RDX(2), R8(8), R10(10), R11(11)]
    /// Exhaustion emits diagnostic; no spill.
    ///
    /// # Panics
    /// None under normal operation; may emit diagnostics on register exhaustion or
    /// missing layout information.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn lower_pattern(
        &mut self,
        pattern: &paideia_as_ir::PatternBinding,
        base_reg: RegId,
        base_offset: i32,
        arm_id: IrNodeId,
        slot: &mut u32,
        arena: &IrArena,
        default_size_signed: (u8, bool),
    ) {
        // #1216: pool snapshot at outer entry; all sibling binders share one pool.
        let base_pool = [abi::RCX, abi::RDX, abi::R8, abi::R10, abi::R11];
        let live = self.state.local_bindings.live_regs();
        let pool: SmallVec<[RegId; 5]> =
            base_pool.iter().copied().filter(|r| !live.contains(r)).collect();
        self.lower_pattern_inner(pattern, base_reg, base_offset, arm_id, slot, arena, default_size_signed, &pool);
    }

    /// Private inner method for lower_pattern that threads the register pool through recursion.
    /// Pool is snapshot once at outer entry and shared across all sibling binders.
    #[allow(clippy::too_many_arguments)]
    fn lower_pattern_inner(
        &mut self,
        pattern: &paideia_as_ir::PatternBinding,
        base_reg: RegId,
        base_offset: i32,
        arm_id: IrNodeId,
        slot: &mut u32,
        arena: &IrArena,
        default_size_signed: (u8, bool),
        pool: &[RegId],
    ) {
        use paideia_as_ir::PatternBinding;

        match pattern {
            PatternBinding::Wildcard => {
                // No-op: wildcard matches anything without binding
            }

            PatternBinding::Simple(name) => {
                let idx = *slot as usize;
                if idx >= pool.len() {
                    // U1654 — pool exhausted after outer live-reg filter
                    self.push_typed_diag(u1654_code(), format!(
                        "Nested pattern binding exhaustion: pool={} slot={} in arm {}",
                        pool.len(), *slot, arm_id.get()));
                    return;
                }
                let dest_reg = pool[idx];
                let load_id = IrNodeId::new(arm_id.get() * 1000 + *slot + 1).unwrap_or(arm_id);
                *slot += 1;

                // Emit width-correct load via the unified dispatch.
                let (size, signed) = default_size_signed;
                let before = self.diagnostics.len();
                self.emit_widening_load(load_id, base_offset, dest_reg, size, signed);
                if self.diagnostics.len() > before {
                    // Unsupported size — the helper already pushed a diagnostic.
                    return;
                }

                // Insert binding into LocalBindingTable
                self.state.local_bindings.insert(name.clone(), dest_reg);
            }

            PatternBinding::EnumVariant {
                variant_index: _,
                payload_type,
                payload: Some(inner),
            } => {
                // Payload at offset 8 (enum layout standard)
                let sub_offset = base_offset + 8;

                let (sub_size, sub_signed) = if let Some(payload_type_id) = payload_type {
                    // If payload_type is a record, look up its layout
                    if let Some(rec_layout) = self.state.record_layout(*payload_type_id) {
                        // Use first field's size/signed as default for nested pattern
                        if let Some(first_field) = rec_layout.fields.first() {
                            (first_field.size, first_field.signed)
                        } else {
                            default_size_signed
                        }
                    } else {
                        // Layout not found; use default
                        default_size_signed
                    }
                } else {
                    default_size_signed
                };

                // Recurse with payload pattern
                self.lower_pattern_inner(inner, base_reg, sub_offset, arm_id, slot, arena, (sub_size, sub_signed), pool);
            }

            PatternBinding::EnumVariant {
                payload: None,
                ..
            } => {
                // Unit variant; no payload to extract
            }

            PatternBinding::Record {
                type_id,
                fields,
            } => {
                // Look up record layout
                let rec_layout = match self.state.record_layout(*type_id) {
                    Some(l) => l.clone(),
                    None => {
                        self.push_typed_diag(u1652_code(), format!(
                            "No record layout found for nested pattern type {}",
                            type_id.0
                        ));
                        return;
                    }
                };

                // For each field, compute offset and recurse
                for (field_name, sub_pattern) in fields {
                    let field_idx = match rec_layout.field_index_by_name(field_name) {
                        Some(idx) => idx,
                        None => {
                            self.push_typed_diag(u1653_code(), format!(
                                "Field '{}' not found in record layout type {}",
                                field_name, type_id.0
                            ));
                            continue;
                        }
                    };

                    let field_layout = &rec_layout.fields[field_idx];
                    let sub_offset = base_offset + field_layout.offset as i32;
                    let sub_size_signed = (field_layout.size, field_layout.signed);

                    self.lower_pattern_inner(
                        sub_pattern,
                        base_reg,
                        sub_offset,
                        arm_id,
                        slot,
                        arena,
                        sub_size_signed,
                        pool,
                    );
                }
            }
        }
    }

    /// #1084 (follow-up): Extract pattern fields from a register (register-form enums).
    ///
    /// Mirrors `lower_pattern` but works from RDX (register source) instead of memory
    /// via RDI. Used for register-form enums (layout.size <= 16) where the payload
    /// is already in RDX after scrutinee load.
    ///
    /// For `Simple` leaves:
    /// - Emit `mov <dest>, <source_reg>`
    /// - If bit_offset > 0 and size < 8: emit `shr <dest>, bit_offset*8` (or `sar` if signed)
    /// - If size < 8: emit narrowing move (movzx, movsx, or plain mov reg32)
    /// - Insert `local_bindings[name] = dest`
    ///
    /// For nested patterns (Record, EnumVariant with payload):
    /// - Compute sub_offset = bit_offset + field.offset*8 (but stay within bit boundaries)
    /// - Recurse with adjusted bit_offset
    ///
    /// Scratch pool: [RCX, R8, R10, R11] (4 regs, excludes RDX which is source).
    /// Exhaustion → diagnostic (same as `lower_pattern`).
    pub(crate) fn lower_pattern_from_reg(
        &mut self,
        pattern: &paideia_as_ir::PatternBinding,
        source_reg: RegId,
        bit_offset: u32,  // Bit offset into source_reg (0, 32, etc.)
        arm_id: IrNodeId,
        slot: &mut u32,
        arena: &IrArena,
        default_size_signed: (u8, bool),
    ) {
        // #1216: pool snapshot at outer entry; all sibling binders share one pool.
        let base_pool = [abi::RCX, abi::R8, abi::R10, abi::R11];
        let live = self.state.local_bindings.live_regs();
        let pool: SmallVec<[RegId; 4]> =
            base_pool.iter().copied().filter(|r| !live.contains(r)).collect();
        self.lower_pattern_from_reg_inner(pattern, source_reg, bit_offset, arm_id, slot, arena, default_size_signed, &pool);
    }

    /// Private inner method for lower_pattern_from_reg that threads the register pool through recursion.
    /// Pool is snapshot once at outer entry and shared across all sibling binders.
    fn lower_pattern_from_reg_inner(
        &mut self,
        pattern: &paideia_as_ir::PatternBinding,
        source_reg: RegId,
        bit_offset: u32,  // Bit offset into source_reg (0, 32, etc.)
        arm_id: IrNodeId,
        slot: &mut u32,
        arena: &IrArena,
        default_size_signed: (u8, bool),
        pool: &[RegId],
    ) {
        use paideia_as_ir::PatternBinding;

        match pattern {
            PatternBinding::Wildcard => {
                // No-op: wildcard matches anything without binding
            }

            PatternBinding::Simple(name) => {
                let idx = *slot as usize;
                if idx >= pool.len() {
                    // U1654 — pool exhausted after outer live-reg filter
                    self.push_typed_diag(u1654_code(), format!(
                        "Nested pattern binding exhaustion: pool={} slot={} in arm {} (from register)",
                        pool.len(), *slot, arm_id.get()));
                    return;
                }
                let dest_reg = pool[idx];
                *slot += 1;

                let (size, signed) = default_size_signed;

                // Step 1: Emit `mov <dest>, <source_reg>` (always load full 64 bits first)
                let mov_id = self.alloc_synthetic_id();
                let mut mov_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                mov_operands.push(Operand::Reg(dest_reg));
                mov_operands.push(Operand::Reg(source_reg));
                self.emit_inst(mov_id, Instruction {
                    mnemonic: Mnemonic::Mov,
                    operands: mov_operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                });

                // Step 2: If bit_offset > 0, emit shift to align field to LSBs
                // bit_offset is a compile-time constant, so use immediate-form shift encoding
                if bit_offset > 0 {
                    debug_assert!(bit_offset < 64, "shift amount must fit in 6 bits");
                    let shift_id = self.alloc_synthetic_id();
                    let mut shift_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                    shift_operands.push(Operand::Reg(dest_reg));
                    shift_operands.push(Operand::Imm64(bit_offset as i64));
                    let mnemonic = if signed { Mnemonic::Sar } else { Mnemonic::Shr };
                    self.emit_inst(shift_id, Instruction {
                        mnemonic,
                        operands: shift_operands,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode: self.current_mode(),
                        emission_order: 0,
                    });
                }

                // Step 3: Emit narrowing move based on size/signedness
                // For register-to-register narrowing, use the appropriate mnemonic
                // without additional encoding hints (let the encoder handle register-form encoding)
                match (size, signed) {
                    (8, _) => {
                        // Full 64-bit; already in dest_reg, no narrowing needed
                    }
                    (4, false) => {
                        // Zero-extend: mov dest32, dest32 (implicit zero-extend in encoder)
                        let narrow_id = self.alloc_synthetic_id();
                        let mut narrow_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                        narrow_operands.push(Operand::Reg(dest_reg));
                        narrow_operands.push(Operand::Reg(dest_reg));
                        self.emit_inst(narrow_id, Instruction {
                            mnemonic: Mnemonic::Mov,
                            operands: narrow_operands,
                            encoding_hint: None,
                            byte_offset_in_text: None,
                            mode: self.current_mode(),
                            emission_order: 0,
                        });
                    }
                    (4, true) => {
                        // Sign-extend: movsxd dest, dest32 (encoder handles via movsx with width=4)
                        let narrow_id = self.alloc_synthetic_id();
                        let mut narrow_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                        narrow_operands.push(Operand::Reg(dest_reg));
                        narrow_operands.push(Operand::Reg(dest_reg));
                        self.emit_inst(narrow_id, Instruction {
                            mnemonic: Mnemonic::Movsx,
                            operands: narrow_operands,
                            encoding_hint: None,
                            byte_offset_in_text: None,
                            mode: self.current_mode(),
                            emission_order: 0,
                        });
                    }
                    (2, false) => {
                        // Zero-extend: movzx dest, dest16
                        let narrow_id = self.alloc_synthetic_id();
                        let mut narrow_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                        narrow_operands.push(Operand::Reg(dest_reg));
                        narrow_operands.push(Operand::Reg(dest_reg));
                        self.emit_inst(narrow_id, Instruction {
                            mnemonic: Mnemonic::Movzx,
                            operands: narrow_operands,
                            encoding_hint: None,
                            byte_offset_in_text: None,
                            mode: self.current_mode(),
                            emission_order: 0,
                        });
                    }
                    (2, true) => {
                        // Sign-extend: movsx dest, dest16
                        let narrow_id = self.alloc_synthetic_id();
                        let mut narrow_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                        narrow_operands.push(Operand::Reg(dest_reg));
                        narrow_operands.push(Operand::Reg(dest_reg));
                        self.emit_inst(narrow_id, Instruction {
                            mnemonic: Mnemonic::Movsx,
                            operands: narrow_operands,
                            encoding_hint: None,
                            byte_offset_in_text: None,
                            mode: self.current_mode(),
                            emission_order: 0,
                        });
                    }
                    (1, false) => {
                        // Zero-extend: movzx dest, dest8
                        let narrow_id = self.alloc_synthetic_id();
                        let mut narrow_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                        narrow_operands.push(Operand::Reg(dest_reg));
                        narrow_operands.push(Operand::Reg(dest_reg));
                        self.emit_inst(narrow_id, Instruction {
                            mnemonic: Mnemonic::Movzx,
                            operands: narrow_operands,
                            encoding_hint: None,
                            byte_offset_in_text: None,
                            mode: self.current_mode(),
                            emission_order: 0,
                        });
                    }
                    (1, true) => {
                        // Sign-extend: movsx dest, dest8
                        let narrow_id = self.alloc_synthetic_id();
                        let mut narrow_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                        narrow_operands.push(Operand::Reg(dest_reg));
                        narrow_operands.push(Operand::Reg(dest_reg));
                        self.emit_inst(narrow_id, Instruction {
                            mnemonic: Mnemonic::Movsx,
                            operands: narrow_operands,
                            encoding_hint: None,
                            byte_offset_in_text: None,
                            mode: self.current_mode(),
                            emission_order: 0,
                        });
                    }
                    _ => {
                        self.push_typed_diag(t0566_code(), format!("Unsupported field size/signed in register pattern: size={}, signed={}", size, signed));
                    }
                }

                // Insert binding into LocalBindingTable
                self.state.local_bindings.insert(name.clone(), dest_reg);
            }

            PatternBinding::EnumVariant {
                variant_index: _,
                payload_type,
                payload: Some(inner),
            } => {
                // #1084: In register form, payload record fields start at bit_offset (no additional offset).
                // The enum's discriminant is in RAX; payload is already in source_reg (RDX) at bit 0.
                let sub_bit_offset = bit_offset;

                let (sub_size, sub_signed) = if let Some(payload_type_id) = payload_type {
                    // If payload_type is a record, look up its layout
                    if let Some(rec_layout) = self.state.record_layout(*payload_type_id) {
                        // Use first field's size/signed as default for nested pattern
                        if let Some(first_field) = rec_layout.fields.first() {
                            (first_field.size, first_field.signed)
                        } else {
                            default_size_signed
                        }
                    } else {
                        // Layout not found; use default
                        default_size_signed
                    }
                } else {
                    default_size_signed
                };

                // Recurse with payload pattern
                self.lower_pattern_from_reg_inner(
                    inner,
                    source_reg,
                    sub_bit_offset,
                    arm_id,
                    slot,
                    arena,
                    (sub_size, sub_signed),
                    pool,
                );
            }

            PatternBinding::EnumVariant {
                payload: None,
                ..
            } => {
                // Unit variant; no payload to extract
            }

            PatternBinding::Record {
                type_id,
                fields,
            } => {
                // Look up record layout
                let rec_layout = match self.state.record_layout(*type_id) {
                    Some(l) => l.clone(),
                    None => {
                        self.push_typed_diag(u1652_code(), format!(
                            "No record layout found for nested pattern type {}",
                            type_id.0
                        ));
                        return;
                    }
                };

                // For each field, compute bit offset and recurse
                for (field_name, sub_pattern) in fields {
                    let field_idx = match rec_layout.field_index_by_name(field_name) {
                        Some(idx) => idx,
                        None => {
                            self.push_typed_diag(u1653_code(), format!(
                                "Field '{}' not found in record layout type {}",
                                field_name, type_id.0
                            ));
                            continue;
                        }
                    };

                    let field_layout = &rec_layout.fields[field_idx];
                    // Compute bit offset: byte_offset * 8 bits/byte + bit_offset
                    let sub_bit_offset = bit_offset + (field_layout.offset as u32) * 8;
                    let sub_size_signed = (field_layout.size, field_layout.signed);

                    self.lower_pattern_from_reg_inner(
                        sub_pattern,
                        source_reg,
                        sub_bit_offset,
                        arm_id,
                        slot,
                        arena,
                        sub_size_signed,
                        pool,
                    );
                }
            }
        }
    }
}
