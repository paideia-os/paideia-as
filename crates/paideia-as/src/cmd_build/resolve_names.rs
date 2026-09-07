//! Symbol name resolution + `let_meta` finalisation.
//!
//! Extracted from `cmd_build.rs` (2026-09-07 refactor, issue #1401).
//! Walks the AST to find Let bindings with their real names, then rebuilds
//! the IR symbol table so downstream emitters see actual names (e.g.
//! `_start`, `long_mode_entry`) instead of the synthetic `_let_<id>` names
//! produced by the elaborator. Also seeds each `LetInfo` with mutability,
//! alignment, ring, link_section, abi, and no_frame from the AST.
//!
//! Runs after the walker pipeline and the optimization pass.

use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{FileId, SourceMap};
use paideia_as_elaborator::LoweringResult;

/// Phase-5-m6-005 / B3-004 / PA19-r19-010: symbol name resolution + let_meta seeding.
///
/// Walk the AST to find Let bindings with actual names, then update the symbol table
/// to use the real binding names instead of "_let_<id>". Also extracts the `pub`
/// flag from each Let binding to set STB_GLOBAL, and seeds `LetInfo` with the
/// binding's mutability, alignment, ring, link_section, abi, and no_frame metadata.
pub(super) fn resolve_symbol_names_and_let_meta(
    arena: &AstArena,
    source_map: &SourceMap,
    file: FileId,
    lowering: &mut LoweringResult,
) {
    let mut name_map: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
    let mut visibility_map: std::collections::HashMap<u32, bool> =
        std::collections::HashMap::new();
    let content_ref = source_map.content(file);

    // Walk AST to find all Let bindings and extract their names, visibility, mutability, and alignment
    for i in 0..arena.len() {
        if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
            if let Some(node) = arena.get(ast_id) {
                if node.kind == paideia_as_ast::NodeKind::Let {
                    if let Some(paideia_as_ast::ItemData::Let {
                        public,
                        mutable,
                        name: name_id,
                        value: value_id,
                        align,
                        ring,
                        link_section,
                        abi,
                        no_frame,
                        ..
                    }) = arena.item_data(ast_id)
                    {
                        // Get the name string from source content
                        if let Some(name_node) = arena.get(*name_id) {
                            let span = name_node.span;
                            let start = span.byte_start() as usize;
                            let len = span.byte_len() as usize;
                            if start + len <= content_ref.len() {
                                let name_str = content_ref[start..start + len].to_string();
                                // Map the lambda/value's IR node ID to its binding name
                                // Since 1-to-1 mapping: ast value_id maps to IR node with same numeric id
                                name_map.insert(value_id.get(), name_str);
                                // Also record the public flag for visibility control
                                visibility_map.insert(value_id.get(), *public);

                                // Phase 19 PA19-r19-010: Seed let_meta with mutability, alignment, ring, link_section, and abi
                                // Phase 19 PA19-r19-001: Convert AST CallingConvention to IR CallingConvention
                                // paideia-as#1276 phase 1: also seed no_frame (inert this landing; consulted from emit in a later phase)
                                let ir_abi = abi.map(|cc| match cc {
                                    paideia_as_ast::CallingConvention::Ms => paideia_as_ir::CallingConvention::Ms,
                                    paideia_as_ast::CallingConvention::Sysv => paideia_as_ir::CallingConvention::Sysv,
                                });
                                if let Some(ir_id) = paideia_as_ir::IrNodeId::new(ast_id.get()) {
                                    // Issue #1219: Use read-modify-write to preserve any ty already set by populate_let_meta_ty
                                    let mut info = lowering.ir.let_meta()
                                        .get(ir_id)
                                        .cloned()
                                        .unwrap_or_else(paideia_as_ir::LetInfo::immutable);
                                    info.mutable = *mutable;
                                    info.align = *align;
                                    info.ring = *ring;
                                    info.link_section = link_section.clone();
                                    info.abi = ir_abi;
                                    info.no_frame = *no_frame;
                                    lowering.ir.let_meta_mut().insert(ir_id, info);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Now rebuild the symbol table with updated names and visibility
    if !name_map.is_empty() {
        let old_symbols: Vec<_> = lowering.ir.symbols().iter().cloned().collect();
        lowering.ir.symbols_mut().clear();

        for sym in old_symbols {
            // Check if this symbol's ir_node.get() is in name_map (i.e., it's a function symbol for a named binding)
            if let Some(real_name) = name_map.get(&sym.ir_node.get()) {
                // Extract visibility from the map (default to false if not found)
                let is_public = visibility_map
                    .get(&sym.ir_node.get())
                    .copied()
                    .unwrap_or(false);
                // Preserve PA10-013 auto-global rule for _start + long_mode_entry.
                let auto_global = real_name == "_start" || real_name == "long_mode_entry";
                let visibility = if is_public || auto_global {
                    paideia_as_ir::Visibility::Global
                } else {
                    paideia_as_ir::Visibility::Local
                };
                // Re-insert the symbol with the real name and correct visibility
                let updated_sym = paideia_as_ir::Symbol::new_with_visibility(
                    real_name.clone(),
                    sym.kind,
                    sym.ir_node,
                    visibility,
                );
                lowering.ir.symbols_mut().insert(updated_sym);
            } else {
                // Symbol has no real name mapping, keep the original
                lowering.ir.symbols_mut().insert(sym);
            }
        }
    }
}
