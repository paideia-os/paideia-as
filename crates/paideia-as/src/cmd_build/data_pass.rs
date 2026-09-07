//! Data-table population, ring-buffer synthesis, and jump-table population.
//!
//! Extracted from `cmd_build.rs` (2026-09-07 refactor, issue #1401).
//!
//! Phase-5-m4-003: Populate data side-table for module-level data bindings.
//! Runs after walker passes and before emit format selection.
//! PA10-007 m1-001: Use actual binding names for data symbols instead of
//! `data_<id>`.
//!
//! Handles every module-level RHS shape:
//! `Literal`, `ArrayLit`, `Placeholder` (uninit BSS), `StringLiteral`,
//! `Borrow` (address-of), `RecordCons`, `EnumCons`, `InlineBytes`.
//!
//! Phase 14 PA14-r14-008: Ring buffer synthesis pass.
//!
//! PA-r15-009b (#1032): Populate jump tables after data table population.

use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, DiagnosticSink, FileId, Severity, SourceMap, VecSink};
use paideia_as_elaborator::{EmitWalker, LoweringResult};
use paideia_as_ir::{IrNodeId, Visibility};

use super::layout::{array_element_byte_width, compute_bss_size_from_type, declared_array_len_from_type};

/// Phase-5-m4-003 + Phase 14 PA14-r14-008: populate data side-table + synthesize ring buffers.
///
/// Iterates once over the IR looking for module-scope `Let` bindings and
/// emits a `DataEntry` for each recognised RHS shape (Literal, ArrayLit,
/// Placeholder, StringLiteral, Borrow, RecordCons, EnumCons, InlineBytes).
/// After the data-table population completes, synthesizes the 4 auxiliary
/// data structures for every `@ring(slots=M, slot_size=K)` binding.
///
/// This is a no-op when `lowering.ir.is_empty()`.
pub(super) fn populate_data_table(
    arena: &AstArena,
    source_map: &SourceMap,
    file: FileId,
    lowering: &mut LoweringResult,
    sink: &mut VecSink,
) {
    if lowering.ir.is_empty() {
        return;
    }

    // Due to Rust borrowing rules, we need to collect the arena state before
    // calling data_mut(). We'll use a temporary struct to hold the necessary data.
    let arena_len = lowering.ir.len();
    let mut data_entries = Vec::new();

    // First pass: collect data entries (using only immutable borrows).
    for i in 1..=arena_len as u32 {
        if let Some(node_id) = IrNodeId::new(i) {
            if let Some(node) = lowering.ir.get(node_id) {
                if node.kind == paideia_as_ir::IrKind::Let {
                    // Issue #1212: Skip statement-scope Lets (function-local bindings).
                    if lowering.ir.is_stmt_let(node_id) {
                        continue;
                    }
                    let children = lowering.ir.children(node_id);

                    // PA10-006s: Look for ArrayLit anywhere in children, not just first.
                    // The IR structure for Let may have multiple children including Var references.
                    // PA-R12-001: Also look for StringLiteral.
                    // PA-r17-010c (#1072): Also look for RecordCons.
                    // PA-r17-007 (#1050): Also look for EnumCons.
                    // Issue #1012: Also look for InlineBytes (@guid, @include_bytes).
                    let mut array_lit_id = None;
                    let mut literal_id = None;
                    let mut string_literal_id = None;
                    let mut record_cons_id = None;
                    let mut enum_cons_id = None;
                    let mut inline_bytes_id = None;

                    for &child_id in children.iter() {
                        if let Some(child_node) = lowering.ir.get(child_id) {
                            if child_node.kind == paideia_as_ir::IrKind::ArrayLit {
                                array_lit_id = Some(child_id);
                            } else if child_node.kind == paideia_as_ir::IrKind::Literal {
                                literal_id = Some(child_id);
                            } else if child_node.kind == paideia_as_ir::IrKind::StringLiteral {
                                string_literal_id = Some(child_id);
                            } else if child_node.kind == paideia_as_ir::IrKind::RecordCons {
                                record_cons_id = Some(child_id);
                            } else if child_node.kind == paideia_as_ir::IrKind::EnumCons {
                                enum_cons_id = Some(child_id);
                            } else if child_node.kind == paideia_as_ir::IrKind::InlineBytes {
                                inline_bytes_id = Some(child_id);
                            }
                        }
                    }

                    // Try ArrayLit first, then Literal, then StringLiteral, then RecordCons, then EnumCons, then InlineBytes, then first child
                    // PA-R12-001: StringLiteral enables `let X : [u8; N] = "string"` patterns
                    // PA-r17-010c (#1072): RecordCons enables `let x : T = T { ... }` patterns
                    // PA-r17-007 (#1050): EnumCons enables `let x : Enum = Enum::Variant(payload)` patterns
                    // Issue #1012: InlineBytes enables `let x : [u8; N] = @guid(...) / @include_bytes(...)` patterns
                    let rhs_id = array_lit_id
                        .or(literal_id)
                        .or(string_literal_id)
                        .or(record_cons_id)
                        .or(enum_cons_id)
                        .or(inline_bytes_id)
                        .or_else(|| children.first().copied());

                    if let Some(rhs_id) = rhs_id {
                        if let Some(rhs_node) = lowering.ir.get(rhs_id) {
                            // PA10-007 m1-001: Use actual binding name from binding_names table
                            let symbol_name = lowering
                                .ir
                                .binding_names()
                                .get(node_id)
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| format!("data_{}", node_id.get()));

                            if rhs_node.kind == paideia_as_ir::IrKind::Literal {
                                // Phase 5: Let with Literal → Rodata (or Data if mutable)
                                if let Some(value) = lowering.ir.literal_values().get(rhs_id) {
                                    let bytes = paideia_as_elaborator::data_encoder::pack_u64_le(value);
                                    let let_info = lowering.ir.let_meta().get(node_id);
                                    let explicit_align = let_info.and_then(|i| i.align);
                                    let is_mutable = let_info
                                        .map(|info| info.mutable).unwrap_or(false);
                                    let link_section = let_info.and_then(|i| i.link_section.clone());
                                    let entry = if is_mutable {
                                        let mut e = paideia_as_ir::DataEntry::new_data(
                                            bytes,
                                            symbol_name,
                                            explicit_align.unwrap_or(8),
                                        );
                                        if let Some(name) = link_section {
                                            e = e.with_section_override(name);
                                        }
                                        e
                                    } else {
                                        let mut e = paideia_as_ir::DataEntry::new_rodata(
                                            bytes,
                                            symbol_name,
                                            explicit_align.unwrap_or(8),
                                        );
                                        if let Some(name) = link_section {
                                            e = e.with_section_override(name);
                                        }
                                        e
                                    };
                                    data_entries.push((node_id, entry));
                                }
                            } else if rhs_node.kind == paideia_as_ir::IrKind::ArrayLit {
                                // Phase 8 m2-002: Let with ArrayLit → pack elements to bytes.
                                // PA10-006s: Use per-element width instead of hardcoded u64.
                                // Walk array element children, pack each element with correct width.
                                // Route to .rodata for immutable, .data for mutable, .bss for uninit.
                                let array_children = lowering.ir.children(rhs_id);
                                let mut packed_bytes = Vec::new();
                                let mut element_count = 0;
                                // #1310: relocations for `&sym` (Borrow) array elements — a
                                // table of symbol addresses. One RelocSpec per element, offset
                                // by its position in the packed byte stream.
                                let mut array_relocs: Vec<paideia_as_ir::RelocSpec> = Vec::new();

                                // PA10-006s: Determine element byte width from AST type
                                let element_width = array_element_byte_width(
                                    node_id,
                                    arena,
                                    source_map,
                                    file,
                                )
                                .unwrap_or(8);

                                for &elem_id in array_children.iter() {
                                    if let Some(elem_node) = lowering.ir.get(elem_id) {
                                        if elem_node.kind == paideia_as_ir::IrKind::Literal {
                                            if let Some(value) =
                                                lowering.ir.literal_values().get(elem_id)
                                            {
                                                let elem_bytes = paideia_as_elaborator::data_encoder::pack_int_le(
                                                    value,
                                                    element_width,
                                                );
                                                packed_bytes.extend(elem_bytes);
                                                element_count += 1;
                                            }
                                        } else if elem_node.kind == paideia_as_ir::IrKind::Borrow
                                            && element_width == 8
                                        {
                                            // #1310: `&sym` element — a symbol address. Only
                                            // supported for 8-byte (u64/pointer-width) elements;
                                            // a pointer cannot be tight-packed into a narrower
                                            // slot. Resolved via the AddrOfSideTable populated
                                            // by the pre-pass above.
                                            if let Some(meta) = lowering.ir.addr_of().get(elem_id) {
                                                let offset = packed_bytes.len() as u64;
                                                packed_bytes.extend(vec![0u8; 8]);
                                                array_relocs.push(paideia_as_ir::RelocSpec::with_width(
                                                    offset,
                                                    meta.symbol.clone(),
                                                    paideia_as_ir::RelocWidth::W64,
                                                    meta.addend,
                                                ));
                                                element_count += 1;
                                            }
                                        }
                                    }
                                }

                                // Issue #1309: reconcile the *emitted* element count against
                                // both the declared arity and the written element list.
                                //
                                // Two ways a symbol used to come out short with no diagnostic:
                                //   1. the initialiser list is shorter than the declared `[T; N]`
                                //      (paideia-os `_frame_meta : [u64; 1024]` was written with
                                //      992 elements and linked at 7936 B instead of 8192 B);
                                //   2. an element is not an encodable integer literal (a negative
                                //      value, a named constant, an expression) and was skipped by
                                //      the loop above without incrementing `element_count`.
                                //
                                // Both are now hard errors. The symbol is *not* emitted, so a
                                // build that survives this pass has byte-exact storage.
                                //
                                // SCOPE: the guard applies only when this branch actually claimed
                                // the array, i.e. at least one element encoded. An array in which
                                // *nothing* encodes is not a partially-emitted symbol — it is a
                                // shape this branch does not own at all (e.g. an element kind
                                // this branch has no encoding for, such as a plain Var reference
                                // to a non-constant binding). Such arrays emit no data entry and
                                // therefore no symbol, so a reference to one fails loudly at link
                                // time as an undefined symbol rather than silently reading short
                                // storage.
                                //
                                // #1310: `[u64; N]` arrays of `&sym` (Borrow) elements — a table
                                // of symbol addresses, e.g. paideia-os
                                // `_klog_files : [u64; 205] = [&name_file_0, &name_file_1, ...]`
                                // — are now encoded above as N relocated pointer slots, so they no
                                // longer fall into this zero-emission gap.
                                let declared_len = declared_array_len_from_type(
                                    node_id, arena, source_map, file,
                                );
                                let written_len = array_children.len();
                                let expected_len = declared_len
                                    .map(|n| n as usize)
                                    .unwrap_or(written_len);
                                let arity_ok = element_count == 0 || element_count == expected_len;

                                if !arity_ok {
                                    let code = paideia_as_diagnostics::DiagnosticCode::new(
                                        paideia_as_diagnostics::Category::T,
                                        paideia_as_diagnostics::Severity::Error,
                                        576,
                                    ).expect("T0576 is valid");
                                    let message = if declared_len.is_some()
                                        && element_count == written_len
                                    {
                                        format!(
                                            "array initialiser has {element_count} elements but the declared type is [_; {expected_len}]"
                                        )
                                    } else {
                                        format!(
                                            "array initialiser has {written_len} elements but only {element_count} could be encoded as constants (expected {expected_len})"
                                        )
                                    };
                                    let diag = paideia_as_diagnostics::Diagnostic::error(code)
                                        .message(message)
                                        .with_span(rhs_node.span)
                                        .finish();
                                    let _ = sink.emit(diag);
                                }

                                if arity_ok && element_count > 0 {
                                    let let_info = lowering.ir.let_meta().get(node_id);
                                    let explicit_align = let_info.and_then(|i| i.align);
                                    let is_mutable = let_info
                                        .map(|info| info.mutable).unwrap_or(false);
                                    let link_section = let_info.and_then(|i| i.link_section.clone());
                                    let entry = if is_mutable {
                                        let mut e = if array_relocs.is_empty() {
                                            paideia_as_ir::DataEntry::new_data(
                                                packed_bytes,
                                                symbol_name,
                                                explicit_align.unwrap_or(8),
                                            )
                                        } else {
                                            paideia_as_ir::DataEntry::new_data_with_relocs(
                                                packed_bytes,
                                                symbol_name,
                                                explicit_align.unwrap_or(8),
                                                array_relocs,
                                            )
                                        };
                                        if let Some(name) = link_section {
                                            e = e.with_section_override(name);
                                        }
                                        e
                                    } else {
                                        let mut e = if array_relocs.is_empty() {
                                            paideia_as_ir::DataEntry::new_rodata(
                                                packed_bytes,
                                                symbol_name,
                                                explicit_align.unwrap_or(8),
                                            )
                                        } else {
                                            paideia_as_ir::DataEntry::new_rodata_with_relocs(
                                                packed_bytes,
                                                symbol_name,
                                                explicit_align.unwrap_or(8),
                                                array_relocs,
                                            )
                                        };
                                        if let Some(name) = link_section {
                                            e = e.with_section_override(name);
                                        }
                                        e
                                    };
                                    data_entries.push((node_id, entry));
                                }
                            } else if rhs_node.kind == paideia_as_ir::IrKind::Placeholder {
                                // Phase 6 m5-005: Let with Placeholder (uninit) → Bss
                                // Route all uninit to .bss regardless of mutability.
                                // Phase 6 m5-005: Compute size from array type annotation if present.
                                //
                                // Issue #1313: a non-literal array length that can't be
                                // resolved is a hard error, never a silent 8-byte guess —
                                // emit T0577 and skip the symbol entirely (like the arity
                                // mismatch above, this makes the build fail via `preview`
                                // rather than link an undersized object).
                                match compute_bss_size_from_type(node_id, arena, source_map, file) {
                                    Ok(size) => {
                                        let let_info = lowering.ir.let_meta().get(node_id);
                                        let explicit_align = let_info.and_then(|i| i.align);
                                        let link_section = let_info.and_then(|i| i.link_section.clone());
                                        let mut entry = paideia_as_ir::DataEntry::new_bss(
                                            symbol_name,
                                            explicit_align.unwrap_or(8),
                                            size,
                                        );
                                        if let Some(name) = link_section {
                                            entry = entry.with_section_override(name);
                                        }
                                        data_entries.push((node_id, entry));
                                    }
                                    Err(unresolved) => {
                                        let code = paideia_as_diagnostics::DiagnosticCode::new(
                                            paideia_as_diagnostics::Category::T,
                                            paideia_as_diagnostics::Severity::Error,
                                            577,
                                        ).expect("T0577 is valid");
                                        let diag = paideia_as_diagnostics::Diagnostic::error(code)
                                            .message(unresolved.message)
                                            .with_span(unresolved.span)
                                            .finish();
                                        let _ = sink.emit(diag);
                                    }
                                }
                            } else if rhs_node.kind == paideia_as_ir::IrKind::StringLiteral {
                                // PA-R12-001 (issue #910): Let with StringLiteral RHS →
                                // inline the byte payload directly into .rodata (or .data if mutable).
                                //
                                // Handles `let X : [u8; N] = "..."` where the declared array type
                                // gives the symbol shape. N bytes are laid down as the symbol body; no
                                // relocation needed — the payload is self-contained.
                                //
                                // If N < literal length, truncate; if N > literal length, zero-pad.
                                // If no [u8; N] annotation, default to the literal's byte length.
                                if let Some(bytes) = lowering.ir.literal_bytes().get(rhs_id) {
                                    let is_mutable = lowering.ir.let_meta().get(node_id)
                                        .map(|info| info.mutable).unwrap_or(false);

                                    let declared_len = declared_array_len_from_type(
                                        node_id, arena, source_map, file,
                                    );
                                    let final_bytes: Vec<u8> = match declared_len {
                                        Some(n) => {
                                            let n = n as usize;
                                            let mut v = Vec::with_capacity(n);
                                            v.extend_from_slice(&bytes[..bytes.len().min(n)]);
                                            v.resize(n, 0);
                                            v
                                        }
                                        None => bytes.clone(),
                                    };

                                    let let_info = lowering.ir.let_meta().get(node_id);
                                    let explicit_align = let_info.and_then(|i| i.align);
                                    let link_section = let_info.and_then(|i| i.link_section.clone());
                                    let entry = if is_mutable {
                                        let mut e = paideia_as_ir::DataEntry::new_data(final_bytes, symbol_name, explicit_align.unwrap_or(1));
                                        if let Some(name) = link_section {
                                            e = e.with_section_override(name);
                                        }
                                        e
                                    } else {
                                        let mut e = paideia_as_ir::DataEntry::new_rodata(final_bytes, symbol_name, explicit_align.unwrap_or(1));
                                        if let Some(name) = link_section {
                                            e = e.with_section_override(name);
                                        }
                                        e
                                    };
                                    data_entries.push((node_id, entry));
                                }
                            } else if rhs_node.kind == paideia_as_ir::IrKind::Borrow {
                                // PA-R17-003 (issue #981): Let with Borrow (address-of) → 8-byte relocation slot
                                // Consult the AddrOfSideTable populated by the pre-emit pass above.
                                // #988 v2: Access by rhs_id (Borrow node) not node_id (Let node)
                                if let Some(meta) = lowering.ir.addr_of().get(rhs_id) {
                                    let bytes = vec![0u8; 8];  // 8 zero bytes (placeholder for linker)
                                    let reloc = paideia_as_ir::RelocSpec::with_width(
                                        0,  // offset 0: entire 8 bytes hold the pointer
                                        meta.symbol.clone(),
                                        paideia_as_ir::RelocWidth::W64,
                                        meta.addend,
                                    );
                                    let let_info = lowering.ir.let_meta().get(node_id);
                                    let is_mutable = let_info
                                        .map(|info| info.mutable).unwrap_or(false);
                                    let explicit_align = let_info.and_then(|i| i.align);
                                    let link_section = let_info.and_then(|i| i.link_section.clone());
                                    let entry = if is_mutable {
                                        let mut e = paideia_as_ir::DataEntry::new_data_with_relocs(
                                            bytes,
                                            symbol_name,
                                            explicit_align.unwrap_or(8),
                                            vec![reloc],
                                        );
                                        if let Some(name) = link_section {
                                            e = e.with_section_override(name);
                                        }
                                        e
                                    } else {
                                        let mut e = paideia_as_ir::DataEntry::new_rodata_with_relocs(
                                            bytes,
                                            symbol_name,
                                            explicit_align.unwrap_or(8),
                                            vec![reloc],
                                        );
                                        if let Some(name) = link_section {
                                            e = e.with_section_override(name);
                                        }
                                        e
                                    };
                                    data_entries.push((node_id, entry));
                                }
                            } else if rhs_node.kind == paideia_as_ir::IrKind::RecordCons {
                                // Issue #1157: RecordCons emit via data_encoder delegation.
                                // Delegates to encode_record_cons for tight-pack field encoding,
                                // then separately walks fields for fnptr Borrow relocations.
                                // #1159: emit T0536 for unsupported field kinds before delegating.
                                if let Some(field_id) = paideia_as_elaborator::data_encoder::first_unencodable_field(&lowering.ir, rhs_id) {
                                    if let Some(field_node) = lowering.ir.get(field_id) {
                                        let code = paideia_as_diagnostics::DiagnosticCode::new(
                                            paideia_as_diagnostics::Category::T,
                                            paideia_as_diagnostics::Severity::Error,
                                            536,
                                        ).expect("T0536 is valid");
                                        let diag = paideia_as_diagnostics::Diagnostic::error(code)
                                            .message("record field must be a literal or function pointer")
                                            .with_span(field_node.span)
                                            .finish();
                                        let _ = sink.emit(diag);
                                    }
                                } else {
                                    match paideia_as_elaborator::data_encoder::encode_record_cons(&lowering.ir, rhs_id) {
                                    Some(bytes) => {
                                    let record_type_id = lowering.ir.record_layout_table().get(rhs_id);
                                    let mut relocs = Vec::new();

                                    // Walk fields to collect Borrow relocations (only Borrow nodes need relocs)
                                    if let Some(type_id) = record_type_id {
                                        if let Some(layout) = lowering.ir.finalised_record_layouts().get(*type_id) {
                                            let field_children = lowering.ir.children(rhs_id);
                                            for (i, &field_id) in field_children[1..].iter().enumerate() {
                                                if let Some(field_node) = lowering.ir.get(field_id) {
                                                    if field_node.kind == paideia_as_ir::IrKind::Borrow {
                                                        // Borrow field: use tight-pack offset from layout
                                                        if i < layout.fields.len() {
                                                            let offset = layout.fields[i].offset;
                                                            if let Some(meta) = lowering.ir.addr_of().get(field_id) {
                                                                let reloc = paideia_as_ir::RelocSpec::with_width(
                                                                    offset,
                                                                    meta.symbol.clone(),
                                                                    paideia_as_ir::RelocWidth::W64,
                                                                    meta.addend,
                                                                );
                                                                relocs.push(reloc);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    let let_info = lowering.ir.let_meta().get(node_id);
                                    let explicit_align = let_info.and_then(|i| i.align);
                                    let is_mutable = let_info
                                        .map(|info| info.mutable).unwrap_or(false);
                                    let link_section = let_info.and_then(|i| i.link_section.clone());
                                    let mut entry = if is_mutable {
                                        if relocs.is_empty() {
                                            paideia_as_ir::DataEntry::new_data(
                                                bytes, symbol_name, explicit_align.unwrap_or(8),
                                            )
                                        } else {
                                            paideia_as_ir::DataEntry::new_data_with_relocs(
                                                bytes, symbol_name, explicit_align.unwrap_or(8), relocs,
                                            )
                                        }
                                    } else {
                                        if relocs.is_empty() {
                                            paideia_as_ir::DataEntry::new_rodata(
                                                bytes, symbol_name, explicit_align.unwrap_or(8),
                                            )
                                        } else {
                                            paideia_as_ir::DataEntry::new_rodata_with_relocs(
                                                bytes, symbol_name, explicit_align.unwrap_or(8), relocs,
                                            )
                                        }
                                    };
                                    if let Some(name) = link_section {
                                        entry = entry.with_section_override(name);
                                    }
                                    data_entries.push((node_id, entry));
                                    }
                                    None => {
                                        // encode_record_cons returned None: either layout is not available or
                                        // a field is not encodable. For now, silently skip (diagnostics should
                                        // have been emitted during elaboration if there were type errors).
                                    }
                                    }
                                }
                            } else if rhs_node.kind == paideia_as_ir::IrKind::EnumCons {
                                // Issue #1091 (#PA-r17-008): EnumCons emit via data_encoder delegation.
                                // Delegates to encode_enum_cons which handles discriminant + recursive payload encoding
                                // (including nested records), then wraps the result in DataEntry.
                                match paideia_as_elaborator::data_encoder::encode_enum_cons(&lowering.ir, rhs_id) {
                                    Some(bytes) => {
                                        let let_info = lowering.ir.let_meta().get(node_id);
                                        let explicit_align = let_info.and_then(|i| i.align);
                                        let is_mutable = let_info
                                            .map(|info| info.mutable).unwrap_or(false);
                                        let link_section = let_info.and_then(|i| i.link_section.clone());
                                        let mut entry = if is_mutable {
                                            paideia_as_ir::DataEntry::new_data(
                                                bytes,
                                                symbol_name,
                                                explicit_align.unwrap_or(8),
                                            )
                                        } else {
                                            paideia_as_ir::DataEntry::new_rodata(
                                                bytes,
                                                symbol_name,
                                                explicit_align.unwrap_or(8),
                                            )
                                        };
                                        if let Some(name) = link_section {
                                            entry = entry.with_section_override(name);
                                        }
                                        data_entries.push((node_id, entry));
                                    }
                                    None => {
                                        // encode_enum_cons returned None: either layout is not available or
                                        // a payload child is not encodable. Walk payload children to find
                                        // the first non-encodable one and emit a T0555 diagnostic with its span.
                                        let payload_children = lowering.ir.children(rhs_id);
                                        let mut bad_child_span = rhs_node.span;
                                        for &payload_id in payload_children {
                                            if paideia_as_elaborator::data_encoder::encode_ir_value(&lowering.ir, payload_id).is_none() {
                                                if let Some(payload_node) = lowering.ir.get(payload_id) {
                                                    bad_child_span = payload_node.span;
                                                }
                                                break;
                                            }
                                        }
                                        let diag = Diagnostic::error(
                                            DiagnosticCode::new(
                                                Category::T,
                                                Severity::Error,
                                                0555, // T0555: enum payload must be encodable
                                            ).expect("T0555 is valid")
                                        )
                                        .message("enum variant payload must be a literal or record literal")
                                        .with_span(bad_child_span)
                                        .finish();
                                        let _ = sink.emit(diag);
                                    }
                                }
                            } else if rhs_node.kind == paideia_as_ir::IrKind::InlineBytes {
                                // Issue #1012: InlineBytes (@guid, @include_bytes) emit.
                                // The bytes are already in the literal_bytes side-table, keyed by the
                                // InlineBytes node ID. Look them up and emit directly to .rodata/.data
                                // with 1-byte alignment (the bytes are the payload as-is).
                                // T0558: Also check size agreement between declared [u8; N] and actual bytes.
                                if let Some(bytes) = lowering.ir.literal_bytes().get(rhs_id) {
                                    // T0558 retroactive size guard: if declared_array_len is Some,
                                    // it must equal bytes.len(). If not, emit T0558 and skip.
                                    let declared_len = declared_array_len_from_type(
                                        node_id, arena, source_map, file,
                                    );
                                    if let Some(n) = declared_len {
                                        if (n as usize) != bytes.len() {
                                            let span = lowering.ir.get(node_id)
                                                .map(|n| n.span)
                                                .unwrap_or_else(|| paideia_as_diagnostics::Span::new(file, 0, 0));
                                            let diag = Diagnostic::error(
                                                DiagnosticCode::new(Category::T, Severity::Error, 558)
                                                    .expect("valid T code"),
                                            )
                                            .message(format!(
                                                "size mismatch: declared [u8; {}] but got {} bytes",
                                                n, bytes.len()
                                            ))
                                            .with_span(span)
                                            .finish();
                                            let _ = sink.emit(diag);
                                            continue;  // Skip this entry
                                        }
                                    }

                                    let let_info = lowering.ir.let_meta().get(node_id);
                                    let is_mutable = let_info
                                        .map(|info| info.mutable).unwrap_or(false);
                                    let explicit_align = let_info.and_then(|i| i.align);
                                    let link_section = let_info.and_then(|i| i.link_section.clone());
                                    let mut entry = if is_mutable {
                                        paideia_as_ir::DataEntry::new_data(
                                            bytes.clone(),
                                            symbol_name,
                                            explicit_align.unwrap_or(1),
                                        )
                                    } else {
                                        paideia_as_ir::DataEntry::new_rodata(
                                            bytes.clone(),
                                            symbol_name,
                                            explicit_align.unwrap_or(1),
                                        )
                                    };
                                    if let Some(name) = link_section {
                                        entry = entry.with_section_override(name);
                                    }
                                    data_entries.push((node_id, entry));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Second pass: populate the data table (using mutable borrow).
    for (node_id, entry) in data_entries {
        lowering.ir.data_mut().insert(node_id, entry);
    }

    // Phase 14 PA14-r14-008: Ring buffer synthesis pass.
    // For each Let with @ring(slots=M, slot_size=K), synthesize 4 data structures:
    // - <name>_slots (BSS, size=M*K, align=64)
    // - <name>_head (DATA, size=8, value=0, align=8)
    // - <name>_tail (DATA, size=8, value=0, align=8)
    // - <name>_mask (RODATA, size=8, value=M-1, align=8)
    {
        let mut ring_entries = Vec::new();

        // Collect ring bindings and their metadata.
        for i in 1..=arena_len as u32 {
            if let Some(node_id) = IrNodeId::new(i) {
                if let Some(node) = lowering.ir.get(node_id) {
                    if node.kind == paideia_as_ir::IrKind::Let {
                        if let Some(let_info) = lowering.ir.let_meta().get(node_id) {
                            if let Some((slots, slot_size)) = let_info.ring {
                                let symbol_name = lowering
                                    .ir
                                    .binding_names()
                                    .get(node_id)
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| format!("ring_{}", node_id.get()));

                                ring_entries.push((node_id, symbol_name, slots, slot_size));
                            }
                        }
                    }
                }
            }
        }

        // For each ring entry, allocate fresh IrNodeIds and create data structures.
        for (orig_id, base_name, slots, slot_size) in ring_entries {
            // Allocate 3 fresh IrNodeIds for head, tail, mask.
            // We reuse orig_id for the slots structure.
            let span = lowering.ir.get(orig_id).map(|n| n.span)
                .expect("ring binding should have valid span");

            let head_id = lowering.ir.alloc(paideia_as_ir::IrKind::Placeholder, span);
            let tail_id = lowering.ir.alloc(paideia_as_ir::IrKind::Placeholder, span);
            let mask_id = lowering.ir.alloc(paideia_as_ir::IrKind::Placeholder, span);

            // Create the 4 data structures.
            let slots_size = (slots as u64) * (slot_size as u64);
            let slots_entry = paideia_as_ir::DataEntry::new_bss(
                format!("{}_slots", base_name),
                64,  // Ring slots always aligned to 64 bytes
                slots_size,
            );

            let head_entry = paideia_as_ir::DataEntry::new_data(
                vec![0, 0, 0, 0, 0, 0, 0, 0],  // 8 zero bytes
                format!("{}_head", base_name),
                8,
            );

            let tail_entry = paideia_as_ir::DataEntry::new_data(
                vec![0, 0, 0, 0, 0, 0, 0, 0],  // 8 zero bytes
                format!("{}_tail", base_name),
                8,
            );

            let mask_value = (slots - 1) as i64;
            let mask_bytes = paideia_as_elaborator::data_encoder::pack_u64_le(mask_value);
            let mask_entry = paideia_as_ir::DataEntry::new_rodata(
                mask_bytes,
                format!("{}_mask", base_name),
                8,
            );

            // Register the 4 symbols (all are objects, not functions).
            let slots_sym = paideia_as_ir::Symbol::new_with_visibility(
                format!("{}_slots", base_name),
                paideia_as_ir::SymbolKind::Object,
                orig_id,
                Visibility::Global,
            );
            let head_sym = paideia_as_ir::Symbol::new_with_visibility(
                format!("{}_head", base_name),
                paideia_as_ir::SymbolKind::Object,
                head_id,
                Visibility::Global,
            );
            let tail_sym = paideia_as_ir::Symbol::new_with_visibility(
                format!("{}_tail", base_name),
                paideia_as_ir::SymbolKind::Object,
                tail_id,
                Visibility::Global,
            );
            let mask_sym = paideia_as_ir::Symbol::new_with_visibility(
                format!("{}_mask", base_name),
                paideia_as_ir::SymbolKind::Object,
                mask_id,
                Visibility::Global,
            );

            // Insert symbols, replacing any existing symbol for orig_id.
            lowering.ir.symbols_mut().insert(slots_sym);
            lowering.ir.symbols_mut().insert(head_sym);
            lowering.ir.symbols_mut().insert(tail_sym);
            lowering.ir.symbols_mut().insert(mask_sym);

            // Insert data entries.
            lowering.ir.data_mut().insert(orig_id, slots_entry);
            lowering.ir.data_mut().insert(head_id, head_entry);
            lowering.ir.data_mut().insert(tail_id, tail_entry);
            lowering.ir.data_mut().insert(mask_id, mask_entry);
        }
    }
}

/// PA-r15-009b (#1032): Populate jump tables after data table population.
/// This synthesizes rodata entries for @jump_table dense match dispatches.
/// Called after `populate_data_table` to get mutable access to the arena.
///
/// This is a no-op when `lowering.ir.is_empty()`.
pub(super) fn populate_jump_tables(lowering: &mut LoweringResult) {
    if lowering.ir.is_empty() {
        return;
    }
    EmitWalker::populate_jump_tables_from_arena(&mut lowering.ir);
}
