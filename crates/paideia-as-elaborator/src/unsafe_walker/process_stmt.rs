//! `UnsafeWalker::process_instruction_stmt` — resolves the mnemonic, parses
//! operands, applies width-recovery / CPU-feature / clobber checks, and inserts
//! the `Instruction` into the arena.
//! Split out of `unsafe_walker.rs` (paideia-as #1403).

use std::collections::{HashMap, HashSet};

use paideia_as_ast::{AstArena, NodeId, StmtData};
use paideia_as_diagnostics::{
    Category, Diagnostic, DiagnosticCode, DiagnosticSink, Severity,
};
use paideia_as_ir::instruction::{
    CpuFeature, EncodingHint, InstrMode, Instruction, IntWidth, Mnemonic, Operand, RegId,
};
use paideia_as_ir::record_layout::{RecordLayout, RecordTypeId};
use paideia_as_ir::{IrNodeId, SmallVec};

use crate::LocalBindingTable;

use super::diag::{
    u_code, U_MALFORMED_OPERAND, U_SYMBOLREF_NOT_SUPPORTED, U_UNEXPECTED_OPERANDS,
    U_UNKNOWN_LABEL, U_UNKNOWN_MNEMONIC, U_UNRESOLVED_FIELD_OFFSET,
};
use super::mnemonic_table::resolve_mnemonic;
use super::operand::{parse_operand_from_ast, OperandError};
use super::register::{get_register_name, register_name_width};
use super::symbol_ref::supports_symbol_ref;
use super::walker::UnsafeWalker;

impl UnsafeWalker {
    /// Process a single StmtInstruction node.
    ///
    /// Resolves the mnemonic, parses all operands, and inserts an Instruction
    /// into the arena's side-table. Emits diagnostics on error. Also validates
    /// label references against the collected labels map (Phase 6 m4-002) and
    /// CPU feature requirements (PA-r16-004-backtrack-a #1033).
    ///
    /// Issue #1244: Thread next_emission_order counter to ensure raw asm statements
    /// interleave with call statements at their true source positions.
    pub(super) fn process_instruction_stmt(
        arena: &mut paideia_as_ir::IrArena,
        ast: &AstArena,
        stmt_id: NodeId,
        diags: &mut Vec<Diagnostic>,
        sink: &mut dyn DiagnosticSink,
        source_map: &paideia_as_diagnostics::SourceMap,
        record_layouts: &HashMap<RecordTypeId, RecordLayout>,
        labels: &HashMap<String, u32>,
        local_bindings: &LocalBindingTable,
        instr_mode: InstrMode,
        enabled_features: &HashSet<CpuFeature>,
        owning_lambda_id: Option<u32>,
        instr_to_lambda: &mut HashMap<IrNodeId, u32>,
        next_emission_order: &mut u32,
    ) -> Option<paideia_as_ir::IrNodeId> {
        // Get the statement data.
        let stmt_data = match ast.stmt_data(stmt_id) {
            Some(StmtData::Instruction { mnemonic, operands }) => (mnemonic, operands),
            _ => return None,
        };

        let mnemonic_id = stmt_data.0;
        let operand_ids = stmt_data.1;

        // Get the mnemonic string from the arena's interned table.
        let mnemonic_str = ast.mnemonic_str(*mnemonic_id);

        // Resolve the mnemonic to a Mnemonic enum variant.
        let mut mnemonic = match resolve_mnemonic(mnemonic_str) {
            Some(m) => m,
            None => {
                // U1605: Unknown mnemonic
                let span = ast.get(stmt_id).map(|n| n.span).unwrap_or_else(|| {
                    paideia_as_diagnostics::Span::new(
                        paideia_as_diagnostics::FileId::new(1).unwrap(),
                        0,
                        1,
                    )
                });
                let diag = Diagnostic::error(u_code(U_UNKNOWN_MNEMONIC))
                    .message(format!("unknown mnemonic: {}", mnemonic_str))
                    .with_span(span)
                    .finish();
                let _ = sink.emit(diag.clone());
                diags.push(diag);
                return None;
            }
        };

        // PA-r16-004-backtrack-a (#1033): Check if this mnemonic requires a CPU feature.
        // If required, verify it's declared in #![target_features = "..."].
        if let Some(required_feature) = mnemonic.required_feature() {
            if !enabled_features.contains(&required_feature) {
                // U1612: Instruction requires CPU feature but it is not declared
                let instr_span = ast.get(stmt_id).map(|n| n.span).unwrap_or_else(|| {
                    paideia_as_diagnostics::Span::new(
                        paideia_as_diagnostics::FileId::new(1).unwrap(),
                        0,
                        1,
                    )
                });
                let diag = Diagnostic::error(
                    DiagnosticCode::new(Category::U, Severity::Error, 1612)
                        .expect("valid U1612 code"),
                )
                .message(format!(
                    "instruction '{}' requires CPU feature '{}' but it is not declared; add `#![target_features = \"{}\"]` at the module root",
                    mnemonic_str,
                    required_feature.as_str(),
                    required_feature.as_str()
                ))
                .with_span(instr_span)
                .finish();
                let _ = sink.emit(diag.clone());
                diags.push(diag);
                return None; // Skip this instruction, same fail-mode as U1605/U1606
            }
        }

        // Phase 6 m1-005: Check if this is a zero-arity instruction with operands.
        // If mnemonic.arity() == 0 and operand_ids is non-empty, emit U1607 and proceed with empty operands.
        let mut parsed_operands: SmallVec<[Operand; 3]> = SmallVec::new();

        let expected_arity = mnemonic.arity();
        if expected_arity == 0 && !operand_ids.is_empty() {
            // Emit U1607 with span of the first operand
            if let Some(&first_operand_id) = operand_ids.first() {
                let operand_span = ast
                    .get(first_operand_id)
                    .map(|n| n.span)
                    .unwrap_or_else(|| {
                        paideia_as_diagnostics::Span::new(
                            paideia_as_diagnostics::FileId::new(1).unwrap(),
                            0,
                            1,
                        )
                    });
                let diag = Diagnostic::error(u_code(U_UNEXPECTED_OPERANDS))
                    .message(format!(
                        "unexpected operands for zero-arity instruction: {}",
                        mnemonic_str
                    ))
                    .with_span(operand_span)
                    .finish();
                let _ = sink.emit(diag.clone());
                diags.push(diag);
            }
            // Continue with empty operands (recovery posture)
        } else {
            // Parse all operands normally.
            let mut operand_error = false;

            for &operand_id in operand_ids {
                match parse_operand_from_ast(
                    ast,
                    operand_id,
                    source_map,
                    record_layouts,
                    mnemonic,
                    local_bindings,
                    labels,
                ) {
                    Ok(operand) => {
                        parsed_operands.push(operand);
                    }
                    Err(OperandError::UnknownRegister(_name, span)) => {
                        // U1606: Malformed operand (register name not recognized)
                        let diag = Diagnostic::error(u_code(U_MALFORMED_OPERAND))
                            .message(
                                "malformed operand in unsafe block: unknown register".to_string(),
                            )
                            .with_span(span)
                            .finish();
                        let _ = sink.emit(diag.clone());
                        diags.push(diag);
                        operand_error = true;
                        break;
                    }
                    Err(OperandError::MalformedOperand(span)) => {
                        // U1606: Malformed operand (shape error)
                        let diag = Diagnostic::error(u_code(U_MALFORMED_OPERAND))
                            .message("malformed operand in unsafe block".to_string())
                            .with_span(span)
                            .finish();
                        let _ = sink.emit(diag.clone());
                        diags.push(diag);
                        operand_error = true;
                        break;
                    }
                    Err(OperandError::UnresolvedFieldOffset(span)) => {
                        // U1608: Unresolved field offset in unsafe block
                        let diag = Diagnostic::error(u_code(U_UNRESOLVED_FIELD_OFFSET))
                            .message(
                                "field offset not resolved; declare struct before use".to_string(),
                            )
                            .with_span(span)
                            .finish();
                        let _ = sink.emit(diag.clone());
                        diags.push(diag);
                        operand_error = true;
                        break;
                    }
                }
            }

            // If any operand parsing failed, skip this instruction.
            if operand_error {
                return None;
            }
        }

        // Phase 6 m4-002: Validate label references.
        // Check each operand to see if it's a LabelRef and verify it exists in the labels map.
        for operand in &parsed_operands {
            if let Operand::LabelRef { name, .. } = operand {
                if !labels.contains_key(name) {
                    // U1610: Unknown label reference
                    let stmt_span = ast.get(stmt_id).map(|n| n.span).unwrap_or_else(|| {
                        paideia_as_diagnostics::Span::new(
                            paideia_as_diagnostics::FileId::new(1).unwrap(),
                            0,
                            1,
                        )
                    });
                    let diag = Diagnostic::error(u_code(U_UNKNOWN_LABEL))
                        .message(format!("unknown label reference: {}", name))
                        .with_span(stmt_span)
                        .finish();
                    let _ = sink.emit(diag.clone());
                    diags.push(diag);
                    return None;
                }
            }
        }

        // Phase-N #1248 + #1254 completion: Convert Cmp to CmpSized when a
        // width-carrying sub-register appears in the operand shape.
        //
        // Register-name-to-RegId collapses sub-register spellings onto the
        // 64-bit RegId (al/ax/eax/rax all → RegId(0)); the generic Cmp
        // encoder then always emits the 64-bit REX.W form. Recover the
        // narrow width from the register *name* and retarget to
        // Mnemonic::CmpSized { width } so `cmp al, imm` / `cmp ax, imm` /
        // `cmp eax, imm` (and their reg-reg / [mem],reg peers) reach the
        // dedicated narrow encoder.
        //
        // Supported shapes (matching the encoder's cmp support):
        //   [Reg, Imm64]  — width from operand 0
        //   [Reg, Reg]    — width from operand 0
        //   [MemSib, Reg] — width from operand 1 (store form)
        // Peephole cmp→test rewrite (peephole.rs:265) deliberately does
        // NOT match CmpSized — the rewrite widens to REX.W Test and would
        // reintroduce the same class of miscompile; a sibling TestSized
        // variant is a separate follow-up.
        if mnemonic == Mnemonic::Cmp {
            let is_reg_imm = matches!(
                parsed_operands.as_slice(),
                [Operand::Reg(_), Operand::Imm64(_)],
            );
            let is_reg_reg = matches!(
                parsed_operands.as_slice(),
                [Operand::Reg(_), Operand::Reg(_)],
            );
            let is_mem_reg = matches!(
                parsed_operands.as_slice(),
                [Operand::MemSib { .. }, Operand::Reg(_)],
            );

            // Width-carrying operand index: dst for reg-imm / reg-reg,
            // src for the store shape.
            let width_op_idx = if is_mem_reg { 1 } else { 0 };

            if is_reg_imm || is_reg_reg || is_mem_reg {
                if let Some(width) = operand_ids
                    .get(width_op_idx)
                    .and_then(|&id| get_register_name(ast, id, source_map))
                    .and_then(|name| register_name_width(&name))
                    .filter(|w| matches!(w, IntWidth::W8 | IntWidth::W16 | IntWidth::W32))
                {
                    mnemonic = Mnemonic::CmpSized { width };
                }
            }
        }

        // Phase R68 (paideia-os #1861, paideia-as #1329): movzx/movsx
        // reg-to-reg source-width recovery.
        //
        // Mnemonic::Movzx/Movsx are two-operand (dst64, src) forms whose
        // encoders (encode_movzx/encode_movsx) read the source width from
        // `Instruction::encoding_hint.operand_size`, defaulting to 1
        // (movzx) or 4 (movsx) when absent. Register-name-to-RegId
        // collapses sub-register spellings onto the 64-bit RegId (al/ax/
        // eax/rax all -> RegId(0)), so — exactly as CmpSized/MovSized do
        // above — the true source width has to be recovered from the
        // register *name*, not the collapsed RegId.
        //
        // Only the register-to-register shape is handled: a memory source
        // (`movzx rax, [mem]`) has no register name to recover width from
        // and would need a width-suffixed mnemonic (mirroring mov_b/
        // mov_w) as a separate follow-up. Widths outside each mnemonic's
        // real repertoire (movzx: 8/16-bit source only — a 32-bit source
        // zero-extends for free via a plain `mov r32, r32`, per
        // encode.rs's movzx_reg64 docs; movsx: 8/16/32-bit source, never
        // 64) are left unset, falling back to the encoder's existing
        // default rather than mis-encoding.
        let mut movx_encoding_hint: Option<EncodingHint> = None;
        if matches!(mnemonic, Mnemonic::Movzx | Mnemonic::Movsx) {
            if let [Operand::Reg(_), Operand::Reg(_)] = parsed_operands.as_slice() {
                if let Some(width) = operand_ids
                    .get(1)
                    .and_then(|&id| get_register_name(ast, id, source_map))
                    .and_then(|name| register_name_width(&name))
                {
                    let valid = match mnemonic {
                        Mnemonic::Movzx => matches!(width, IntWidth::W8 | IntWidth::W16),
                        Mnemonic::Movsx => {
                            matches!(width, IntWidth::W8 | IntWidth::W16 | IntWidth::W32)
                        }
                        _ => false,
                    };
                    if valid {
                        let operand_size = match width {
                            IntWidth::W8 => 1,
                            IntWidth::W16 => 2,
                            IntWidth::W32 => 4,
                            IntWidth::W64 => 8,
                        };
                        movx_encoding_hint = Some(EncodingHint { opcode: 0, operand_size });
                    }
                }
            }
        }

        // Phase 6 m4-005: Validate SymbolRef operands.
        // SymbolRef is only supported for call/jmp mnemonics. If a bare-identifier symbol
        // was parsed as SymbolRef for a different mnemonic, emit U1611.
        for (idx, operand) in parsed_operands.iter().enumerate() {
            if let Operand::SymbolRef { name, .. } = operand {
                if !supports_symbol_ref(mnemonic) {
                    // U1611: SymbolRef not supported for this mnemonic
                    let operand_span = if let Some(&operand_id) = operand_ids.get(idx) {
                        ast.get(operand_id).map(|n| n.span).unwrap_or_else(|| {
                            paideia_as_diagnostics::Span::new(
                                paideia_as_diagnostics::FileId::new(1).unwrap(),
                                0,
                                1,
                            )
                        })
                    } else {
                        ast.get(stmt_id).map(|n| n.span).unwrap_or_else(|| {
                            paideia_as_diagnostics::Span::new(
                                paideia_as_diagnostics::FileId::new(1).unwrap(),
                                0,
                                1,
                            )
                        })
                    };
                    let diag = Diagnostic::error(u_code(U_SYMBOLREF_NOT_SUPPORTED))
                        .message(format!(
                            "SymbolRef operand '{}' not supported for mnemonic {} in Phase 6; \
                             only call and jmp support symbol references",
                            name, mnemonic_str
                        ))
                        .with_span(operand_span)
                        .finish();
                    let _ = sink.emit(diag.clone());
                    diags.push(diag);
                    return None;
                }
            }
        }

        // PA-r16-004-backtrack-b (#1034): Check for implicit-clobber warnings.
        // Detect when a LOCK-prefixed mnemonic's implicit writes overlap explicit operands.
        let clobbered = mnemonic.implicit_writes();
        if !clobbered.is_empty() {
            // Walk explicit operands. For each Reg/MemSib base/MemSib index that
            // matches a clobbered register, emit U1613 (a warning, not an error).
            for op in &parsed_operands {
                let touched: Vec<RegId> = match op {
                    Operand::Reg(r) => vec![*r],
                    Operand::MemSib { base, index, .. } => {
                        let mut v = vec![*base];
                        if let Some(idx) = index {
                            v.push(*idx);
                        }
                        v
                    }
                    _ => vec![],
                };
                for r in touched {
                    if clobbered.contains(&r) {
                        let instr_span = ast.get(stmt_id).map(|n| n.span).unwrap_or_else(|| {
                            paideia_as_diagnostics::Span::new(
                                paideia_as_diagnostics::FileId::new(1).unwrap(),
                                0,
                                1,
                            )
                        });
                        let diag = Diagnostic::warning(
                            DiagnosticCode::new(Category::U, Severity::Warning, 1613)
                                .expect("valid U1613 code"),
                        )
                        .message(format!(
                            "instruction '{}' implicitly writes register {:?}, which appears in an explicit operand — value will be silently overwritten",
                            mnemonic_str, r
                        ))
                        .with_span(instr_span)
                        .finish();
                        let _ = sink.emit(diag.clone());
                        diags.push(diag);
                        break; // one diagnostic per instruction is enough
                    }
                }
            }
        }

        // Create the Instruction and insert it into the arena.
        // Phase-5-m3-004: Allocate a fresh IrNodeId for this instruction statement.
        // Each unsafe block instruction gets its own IR node in the instruction side-table,
        // enabling correct byte-level emission via emit_text_from_instructions.
        let stmt_span = ast.get(stmt_id).map(|n| n.span).unwrap_or_else(|| {
            paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
        });

        // Allocate a fresh IrNodeId for this instruction.
        // Use IrKind::Placeholder as a generic container for the instruction side-table entry.
        let ir_node_id = arena.alloc(paideia_as_ir::IrKind::Placeholder, stmt_span);

        // PA8 m3-003 (#827): width-aware `mov reg, imm` retarget.
        //
        // `register_name_to_regid` collapses sub-register spellings onto their
        // 64-bit `RegId`, so `mov al, 5` and `mov eax, 5` and `mov rax, 5` reach
        // the encoder as the same width-agnostic `Mnemonic::Mov`. The encoder's
        // generic `mov reg, imm` path always emits the 10-byte 64-bit form. Here
        // we recover the destination width from the register *name* (before the
        // collapse) and retarget to `Mnemonic::MovSized { width }`, whose encoder
        // path already emits the narrow `B0+rb imm8` / `66 B8 imm16` / `B8+rd imm32` forms.
        //
        // PA10-006d: Support W8, W16, and W32 immediate forms. The r64 imm32/imm64 forms
        // remain in the generic `mov` path (its existing `48 B8 imm64` behavior is preserved).
        //
        // PA13-001 (#930): Also retarget narrow-width load forms `[Reg, MemSib]` where the
        // destination register is al/cl/dl/bl/ah/ch/dh/bh/r8b–r15b (W8), ax–di/r8w–r15w (W16),
        // or eax–edi/r8d–r15d (W32). Width is inferred from the destination register name.
        //
        // #1251: Also retarget narrow-width store forms `[MemSib, Reg]` where the source
        // register is al/cl/dl/bl/ah/ch/dh/bh/r8b–r15b (W8), ax–di/r8w–r15w (W16),
        // or eax–edi/r8d–r15d (W32). Width is inferred from the source register name (operand 1).
        let mnemonic = if matches!(mnemonic, Mnemonic::Mov) {
            let is_imm = matches!(parsed_operands.as_slice(), [Operand::Reg(_), Operand::Imm64(_)]);
            let is_load = matches!(parsed_operands.as_slice(), [Operand::Reg(_), Operand::MemSib { .. }]);
            let is_store = matches!(parsed_operands.as_slice(), [Operand::MemSib { .. }, Operand::Reg(_)]);

            // Width-carrying operand index: dst for imm/load, src for store.
            let width_op_idx = if is_store { 1 } else { 0 };

            if is_imm || is_load || is_store {
                operand_ids
                    .get(width_op_idx)
                    .and_then(|&id| get_register_name(ast, id, source_map))
                    .and_then(|name| register_name_width(&name))
                    .filter(|w| matches!(w, IntWidth::W8 | IntWidth::W16 | IntWidth::W32))
                    .map_or(mnemonic, |width| Mnemonic::MovSized { width })
            } else {
                mnemonic
            }
        } else {
            mnemonic
        };

        // PA-R13-010: Bitwise operation with 64-bit immediate expansion.
        // Check if this is or/and/xor with imm64 that needs expansion.
        let final_ir_node_id = if matches!(mnemonic, Mnemonic::Or | Mnemonic::And | Mnemonic::Xor)
            && matches!(parsed_operands.as_slice(), [Operand::Reg(_), Operand::Imm64(_)])
            && instr_mode == InstrMode::Mode64
        {
            if let [Operand::Reg(dst), Operand::Imm64(imm)] = parsed_operands.as_slice() {
                if crate::imm64_expand::needs_expansion(*imm) {
                    // Attempt expansion
                    match crate::imm64_expand::expand_bitop_imm64(arena, stmt_span, mnemonic, *dst, *imm, instr_mode, next_emission_order) {
                        Some((mov_id, op_id)) => {
                            // Expansion succeeded. Both synthesized instructions
                            // (movabs + the bitwise op) already carry real
                            // emission_order values assigned from the shared
                            // counter (see imm64_expand.rs). Register both with
                            // the owning lambda, matching the normal path below.
                            if let Some(lambda_id) = owning_lambda_id {
                                instr_to_lambda.insert(mov_id, lambda_id);
                                instr_to_lambda.insert(op_id, lambda_id);
                            }
                            // Use the movabs head for label aliasing.
                            mov_id
                        }
                        None => {
                            // Collision: dst is r11
                            crate::imm64_expand::emit_r11_collision_diagnostic(stmt_span, sink);
                            return None;
                        }
                    }
                } else {
                    // No expansion needed; use the allocated node
                    let emission_order = {
                        let order = *next_emission_order;
                        *next_emission_order += 1;
                        order
                    };
                    let inst = Instruction {
                        mnemonic,
                        operands: parsed_operands,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode: instr_mode,
                        emission_order,
                    };
                    arena.instructions_mut().insert(ir_node_id, inst);
                    // #1139: Record which lambda owns this instruction.
                    if let Some(lambda_id) = owning_lambda_id {
                        instr_to_lambda.insert(ir_node_id, lambda_id);
                    }
                    ir_node_id
                }
            } else {
                // Shouldn't reach here due to pattern guard, but fallback to normal path
                let emission_order = {
                    let order = *next_emission_order;
                    *next_emission_order += 1;
                    order
                };
                let inst = Instruction {
                    mnemonic,
                    operands: parsed_operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: instr_mode,
                    emission_order,
                };
                arena.instructions_mut().insert(ir_node_id, inst);
                // #1139: Record which lambda owns this instruction.
                if let Some(lambda_id) = owning_lambda_id {
                    instr_to_lambda.insert(ir_node_id, lambda_id);
                }
                ir_node_id
            }
        } else {
            // Normal path: not a bitwise op or doesn't need expansion
            let emission_order = {
                let order = *next_emission_order;
                *next_emission_order += 1;
                order
            };
            let inst = Instruction {
                mnemonic,
                operands: parsed_operands,
                encoding_hint: movx_encoding_hint,
                byte_offset_in_text: None,
                mode: instr_mode,
                emission_order,
            };
            arena.instructions_mut().insert(ir_node_id, inst);
            // #1139: Record which lambda owns this instruction.
            if let Some(lambda_id) = owning_lambda_id {
                instr_to_lambda.insert(ir_node_id, lambda_id);
            }
            ir_node_id
        };

        Some(final_ir_node_id)
    }
}
