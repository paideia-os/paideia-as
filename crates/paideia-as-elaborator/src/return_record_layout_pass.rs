//! Populator for `IrArena::return_record_layout_table` — the side-table
//! that feeds `Symbol::return_record_layout` on function bindings whose
//! declared return type is a record.
//!
//! PAS-DEBT-B4-002 Slice A (paideia-as#1554). Slice-A wires **demand
//! side pieces (1) and (2)** — parser support for record types at fn
//! return position (already accepted by
//! `parse_type_paren` via `parse_type` → `parse_type_record`) plus this
//! side-table. Slices B and C consume the field to drive
//! `emit_call.rs` / `emit_ret` aggregate-return branches and the record
//! subsystem's pair-unpack lowering.
//!
//! # Pass shape
//!
//! Walks item-level `ItemData::Let { ty: Some(ty_node), value, .. }`
//! nodes. When both hold:
//!
//! 1. The RHS `value` is an `ExprLambda`.
//! 2. The annotation `ty_node` is a `TypeFnPtr { ret, .. }` whose `ret`
//!    is either:
//!    a. An inline anonymous record `TypeData::Record { fields }`, or
//!    b. A named struct `TypeData::Name { name, args: [] }` present in
//!       the `StructRegistry`.
//!
//! …the pass computes a byte-exact `RecordLayout` using the same
//! natural-alignment rules as
//! `emit_pass_state::finalise_record_layouts` (u8/u16/u32/u64 + `*T` at
//! 8 B; the size code drives both `size` and `align`) and inserts it
//! into `IrArena::return_record_layout_table` keyed by the outer Let's
//! `IrNodeId`.
//!
//! # Silent-drop policy
//!
//! Any shape this pass cannot fold is silently skipped: the field type
//! is not one of the recognised scalars, the annotation is not a
//! `TypeFnPtr`, or a named-struct return type isn't in the registry.
//! `Symbol::return_record_layout` then stays `None` on that symbol and
//! Slice B / C code falls back to the historical scalar-return
//! codepath. `T0552` (unsupported struct field type) still fires at
//! `build_struct_registry` for named structs and at
//! `populate_record_layout_table` for `RecordCons` sites; this pass is
//! read-only for diagnostics.

use paideia_as_ast::{AstArena, ItemData, NodeId, NodeKind, TypeData};
use paideia_as_diagnostics::SourceMap;
use paideia_as_ir::record_layout::{FieldLayout, RecordLayout};
use paideia_as_ir::{IrArena, IrNodeId};
use std::collections::HashMap;

use crate::struct_registry::{StructRegistry, decode_field_type};

/// Populate `IrArena::return_record_layout_table` from item-level Let
/// bindings whose declared return type is a record.
///
/// See the module docblock for the pass shape, silent-drop policy, and
/// cross-slice consumer contract. Runs after `populate_let_meta_ty`
/// (from which we inherit the invariant that any type annotation the
/// user typed has been lowered and interned) and before
/// `emit_walker::walk` (which reads back the side-table when
/// constructing the `Symbol`).
pub fn populate_return_record_layouts(
    ast: &AstArena,
    ir: &mut IrArena,
    ast_to_ir: &HashMap<NodeId, IrNodeId>,
    source_map: &SourceMap,
    registry: &StructRegistry,
) {
    for i in 1..=ast.len() {
        let Some(ast_id) = NodeId::new(i as u32) else { continue };
        let Some(node) = ast.get(ast_id) else { continue };
        if node.kind != NodeKind::Let {
            continue;
        }
        let Some(ItemData::Let { ty: Some(ty_node), value, .. }) = ast.item_data(ast_id) else {
            continue;
        };

        // Slice A only wires the Lambda-RHS case: `pub let f : (…) ->
        // Record = fn (…) → …`. A non-lambda RHS with a fn-typed
        // annotation is a fn-pointer alias, not a function definition —
        // there is no callee body to attach an sret shape to.
        let Some(value_node) = ast.get(*value) else { continue };
        if value_node.kind != NodeKind::ExprLambda {
            continue;
        }

        // Peel the annotation. Only `TypeFnPtr` carries a return type.
        let Some(TypeData::FnPtr { ret, .. }) = ast.type_data(*ty_node) else { continue };

        // Compute a layout from the return type (inline record OR
        // named-struct-in-registry). If neither shape matches, silently
        // skip — Symbol::return_record_layout stays None.
        let Some(layout) = layout_for_return_type(ast, source_map, registry, *ret) else {
            continue;
        };

        let Some(let_ir_id) = ast_to_ir.get(&ast_id) else { continue };
        ir.return_record_layout_table_mut().insert(*let_ir_id, layout);
    }
}

/// Compute a `RecordLayout` for a return-position type node.
///
/// Returns `Some(layout)` when the node is either an inline
/// `TypeData::Record { fields }` whose fields all fold via
/// `decode_field_type`, or a `TypeData::Name { name, args: [] }` whose
/// name is present in the `StructRegistry`. Returns `None` on any
/// other shape (scalar, tuple, fn-ptr, unresolved name, unrecognised
/// field type) — the pass then leaves the side-table entry absent.
///
/// Field-type decoding mirrors `emit_pass_state::finalise_record_layouts`
/// exactly: `size_code = byte_code & 0x0F`, `is_signed = (byte_code &
/// 0x10) != 0`, `field_align == field_size`, struct alignment is the
/// max of field alignments, and the struct is tail-padded to that
/// alignment. Fields marked as unsupported abort the whole layout —
/// returning `None` — so the caller does not stamp a half-formed
/// descriptor onto the side-table.
fn layout_for_return_type(
    ast: &AstArena,
    source_map: &SourceMap,
    registry: &StructRegistry,
    ret_type: NodeId,
) -> Option<RecordLayout> {
    match ast.type_data(ret_type)? {
        TypeData::Record { fields } => layout_from_record_fields(ast, source_map, fields),
        TypeData::Name { name, args } if args.is_empty() => {
            let name_text = extract_source_text(ast, source_map, *name)?;
            let type_id = registry.get_by_name(&name_text)?;
            let field_descriptors = registry.get_fields(type_id)?;
            layout_from_byte_codes(field_descriptors)
        }
        _ => None,
    }
}

/// Compute a `RecordLayout` from an inline record type's
/// `(field_name_id, field_type_id)` list. Each field's source text is
/// decoded via `decode_field_type` (u8/i8/u16/i16/u32/i32/u64/i64 and
/// any `*T`); an unrecognised entry aborts the whole layout by
/// returning `None`.
fn layout_from_record_fields(
    ast: &AstArena,
    source_map: &SourceMap,
    fields: &[(NodeId, NodeId)],
) -> Option<RecordLayout> {
    let mut descriptors: Vec<(String, u8)> = Vec::with_capacity(fields.len());
    for (field_name_id, field_type_id) in fields {
        let field_name = extract_source_text(ast, source_map, *field_name_id)?;
        // A fn-pointer field is 8 B unsigned. Non-pointer, non-scalar
        // shapes are refused (return None) to keep this pass byte-
        // identical with the historical named-struct path.
        let byte_code = if let Some(td) = ast.type_data(*field_type_id) {
            if matches!(td, TypeData::FnPtr { .. }) {
                0x08
            } else {
                let text = extract_source_text(ast, source_map, *field_type_id)?;
                decode_field_type(&text)?
            }
        } else {
            let text = extract_source_text(ast, source_map, *field_type_id)?;
            decode_field_type(&text)?
        };
        descriptors.push((field_name, byte_code));
    }
    layout_from_byte_codes(&descriptors)
}

/// Compute a `RecordLayout` from a `[(field_name, byte_code)]` list.
///
/// The `byte_code` shape is the same encoding
/// `struct_registry::build_struct_registry` uses (see
/// `struct_registry.rs` docblock): low nibble is the size code (1, 2,
/// 4, 8) and bit 0x10 is the signed flag. Alignment == size for every
/// current field type; struct alignment is the max; the struct is
/// tail-padded to that alignment.
///
/// A field with an unrecognised size code aborts the layout via
/// `None`, matching `finalise_record_layouts`.
fn layout_from_byte_codes(fields: &[(String, u8)]) -> Option<RecordLayout> {
    if fields.is_empty() {
        return Some(RecordLayout::with_field_names(0, 1, Vec::new(), Vec::new()));
    }

    let mut struct_align: u8 = 1;
    let mut current_offset: u64 = 0;
    let mut finalised_fields: Vec<FieldLayout> = Vec::with_capacity(fields.len());
    let mut field_names: Vec<String> = Vec::with_capacity(fields.len());

    for (name, byte_code) in fields {
        let size_code = byte_code & 0x0F;
        let is_signed = (byte_code & 0x10) != 0;

        let (field_align, field_size) = match size_code {
            1 => (1u8, 1u8),
            2 => (2u8, 2u8),
            4 => (4u8, 4u8),
            8 => (8u8, 8u8),
            _ => return None, // Unsupported size code — abort layout.
        };

        struct_align = struct_align.max(field_align);
        let field_align_u64 = field_align as u64;
        current_offset =
            ((current_offset + field_align_u64 - 1) / field_align_u64) * field_align_u64;

        finalised_fields.push(FieldLayout {
            offset: current_offset,
            size: field_size,
            signed: is_signed,
            is_float: false,
        });
        field_names.push(name.clone());
        current_offset += field_size as u64;
    }

    let align_u64 = struct_align as u64;
    let struct_size = ((current_offset + align_u64 - 1) / align_u64) * align_u64;

    Some(RecordLayout::with_field_names(
        struct_size,
        struct_align,
        finalised_fields,
        field_names,
    ))
}

/// Extract the source text of an AST node from its span.
///
/// Duplicated (not re-exported) from `struct_registry.rs` to keep this
/// pass a leaf module that any downstream consumer can depend on
/// without inheriting the whole struct-registry compilation surface.
/// A future refactor can lift both copies into a shared
/// `source_extract` helper.
fn extract_source_text(ast: &AstArena, source_map: &SourceMap, node_id: NodeId) -> Option<String> {
    let node = ast.get(node_id)?;
    let span = node.span;
    let source = source_map.content(span.file());
    let start = span.byte_start() as usize;
    let len = span.byte_len() as usize;
    if start + len > source.len() {
        return None;
    }
    Some(source[start..start + len].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::{AstArena, ItemData, NodeKind, TypeData};
    use paideia_as_diagnostics::{FileId, Span, SourceMap};
    use paideia_as_ir::{IrArena, IrKind, IrNodeId};

    // ---- Unit tests for the layout-computation helpers ----

    #[test]
    fn layout_from_byte_codes_empty_returns_zero_size() {
        let layout = layout_from_byte_codes(&[]).expect("empty layout");
        assert_eq!(layout.size, 0);
        assert_eq!(layout.align, 1);
        assert!(layout.fields.is_empty());
        assert!(layout.field_names.is_empty());
    }

    #[test]
    fn layout_from_byte_codes_four_u32_matches_cpuid_regs() {
        // CpuidRegs = 4 × u32, offsets 0/4/8/12, size 16, align 4.
        let fields = vec![
            ("eax".to_string(), 0x04u8),
            ("ebx".to_string(), 0x04u8),
            ("ecx".to_string(), 0x04u8),
            ("edx".to_string(), 0x04u8),
        ];
        let layout = layout_from_byte_codes(&fields).expect("cpuid layout");
        assert_eq!(layout.size, 16);
        assert_eq!(layout.align, 4);
        assert_eq!(layout.fields.len(), 4);
        assert_eq!(layout.fields[0].offset, 0);
        assert_eq!(layout.fields[1].offset, 4);
        assert_eq!(layout.fields[2].offset, 8);
        assert_eq!(layout.fields[3].offset, 12);
        assert!(layout.fields.iter().all(|f| f.size == 4 && !f.signed));
        assert_eq!(layout.field_names, vec!["eax", "ebx", "ecx", "edx"]);
    }

    #[test]
    fn layout_from_byte_codes_mixed_widths_applies_natural_alignment() {
        // { u8; u32; u64 } → offsets 0, 4 (align-up from 1 to 4), 8. Size 16.
        let fields = vec![
            ("a".to_string(), 0x01u8),
            ("b".to_string(), 0x04u8),
            ("c".to_string(), 0x08u8),
        ];
        let layout = layout_from_byte_codes(&fields).expect("mixed layout");
        assert_eq!(layout.align, 8);
        assert_eq!(layout.fields.iter().map(|f| f.offset).collect::<Vec<_>>(), vec![0, 4, 8]);
        assert_eq!(layout.size, 16);
    }

    #[test]
    fn layout_from_byte_codes_rejects_unknown_size_code() {
        let fields = vec![
            ("a".to_string(), 0x04u8),
            ("b".to_string(), 0x03u8), // 3 is not a valid size code — abort.
        ];
        assert!(layout_from_byte_codes(&fields).is_none());
    }

    #[test]
    fn layout_from_byte_codes_signed_flag_survives() {
        let fields = vec![
            ("a".to_string(), 0x14u8), // i32 = 0x14 (signed | size4)
            ("b".to_string(), 0x18u8), // i64 = 0x18 (signed | size8)
        ];
        let layout = layout_from_byte_codes(&fields).expect("signed layout");
        assert!(layout.fields[0].signed);
        assert!(layout.fields[1].signed);
        assert_eq!(layout.align, 8);
        assert_eq!(layout.size, 16);
    }

    // ---- Integration test: the whole pass end-to-end (no lowering
    // pipeline). We hand-build a minimal AST + IR + ast_to_ir + registry
    // shaped as `pub let cpuid : (u32, u32) -> CpuidRegs = fn (…) → 0`
    // and prove that after populate_return_record_layouts the Let's
    // side-table entry describes the 4×u32 layout. ----

    fn test_span(source_len: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), 0, source_len)
    }

    fn empty_span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 0)
    }

    /// Build a source map whose first file has `content` at offset 0.
    /// The FileId comes back as 1 by construction (first-add gives
    /// FileId(1)); every AST node in these tests spans that file, and
    /// `extract_source_text` reads bytes from it verbatim.
    fn make_source_map(content: &str) -> SourceMap {
        let mut sm = SourceMap::new();
        let id = sm.add_file(std::path::PathBuf::from("<inline>"), content.to_string());
        assert_eq!(id, FileId::new(1).unwrap(), "test relies on first add returning FileId(1)");
        sm
    }

    #[test]
    fn pass_populates_layout_for_named_struct_return() {
        // Simulate parser output for `pub let cpuid : (u32, u32) ->
        // CpuidRegs = fn (…) → 0`, minimal shape. The registry has been
        // pre-populated with a CpuidRegs entry (4 × u32).
        let mut ast = AstArena::new();

        // Source slice used only by `extract_source_text` for the
        // named-struct span. Layout: `CpuidRegs` at bytes 0..9.
        let content = "CpuidRegs";
        let source_map = make_source_map(content);

        // Named type node covering the whole `CpuidRegs` name.
        let name_ident = ast.alloc(NodeKind::Ident, test_span(9));
        let ret_ty = ast.alloc_type(
            NodeKind::TypeName,
            test_span(9),
            TypeData::Name { name: name_ident, args: Vec::new() },
        );

        // FnPtr `(u32, u32) -> CpuidRegs` — the params list isn't
        // inspected by the pass, so give it an empty vec.
        let fn_ptr = ast.alloc_type(
            NodeKind::TypeFnPtr,
            empty_span(),
            TypeData::FnPtr {
                params: Vec::new(),
                param_names: Vec::new(),
                ret: ret_ty,
                effects: None,
                capabilities: None,
            },
        );

        // Bare Lambda body — the pass gates on `value.kind ==
        // ExprLambda`, nothing more.
        let lambda = ast.alloc(NodeKind::ExprLambda, empty_span());
        let let_name = ast.alloc(NodeKind::Ident, empty_span());
        let let_id = ast.alloc_item(
            NodeKind::Let,
            empty_span(),
            ItemData::Let {
                public: true,
                mutable: false,
                name: let_name,
                generic_params: Vec::new(),
                ty: Some(fn_ptr),
                value: lambda,
                align: None,
                ring: None,
                link_section: None,
                abi: None,
                no_frame: false,
                interrupt: None,
                doc: None,
            },
        );

        // Build a StructRegistry that knows about CpuidRegs (4 × u32).
        let mut registry = StructRegistry::empty();
        let type_id = paideia_as_ir::record_layout::RecordTypeId(1);
        registry.by_name.insert("CpuidRegs".to_string(), type_id);
        registry.fields.insert(
            type_id,
            vec![
                ("eax".to_string(), 0x04),
                ("ebx".to_string(), 0x04),
                ("ecx".to_string(), 0x04),
                ("edx".to_string(), 0x04),
            ],
        );

        // Hand-wire an IR arena with a single Let-kinded node whose id
        // matches the AST let id (any positive u32 works — the pass
        // just needs the ast_to_ir map to resolve).
        let mut ir = IrArena::new();
        let ir_let_id = ir.alloc(IrKind::Let, empty_span());
        let mut ast_to_ir = HashMap::new();
        ast_to_ir.insert(let_id, ir_let_id);

        populate_return_record_layouts(&ast, &mut ir, &ast_to_ir, &source_map, &registry);

        let layout = ir
            .return_record_layout_table()
            .get(ir_let_id)
            .expect("layout populated for CpuidRegs return");
        assert_eq!(layout.size, 16);
        assert_eq!(layout.align, 4);
        assert_eq!(layout.fields.len(), 4);
        assert_eq!(
            layout.field_names,
            vec!["eax".to_string(), "ebx".to_string(), "ecx".to_string(), "edx".to_string()]
        );

        // Sanity: nodes without a fn-return-record annotation must not
        // receive a side-table entry. Re-run against a fresh IR whose
        // Let has NO type annotation.
        let mut ast2 = AstArena::new();
        let lam2 = ast2.alloc(NodeKind::ExprLambda, empty_span());
        let name2 = ast2.alloc(NodeKind::Ident, empty_span());
        let let2 = ast2.alloc_item(
            NodeKind::Let,
            empty_span(),
            ItemData::Let {
                public: false, mutable: false, name: name2, generic_params: Vec::new(),
                ty: None, value: lam2, align: None, ring: None, link_section: None,
                abi: None, no_frame: false, interrupt: None, doc: None,
            },
        );
        let mut ir2 = IrArena::new();
        let ir2_let = ir2.alloc(IrKind::Let, empty_span());
        let mut m2 = HashMap::new();
        m2.insert(let2, ir2_let);
        populate_return_record_layouts(&ast2, &mut ir2, &m2, &source_map, &registry);
        assert!(ir2.return_record_layout_table().get(ir2_let).is_none());
    }

    #[test]
    fn pass_ignores_non_lambda_rhs_even_with_fn_type_annotation() {
        // `pub let alias : (u32) -> CpuidRegs = other_symbol` — the
        // annotation is a FnPtr with a record return, but the RHS is
        // NOT a Lambda, so no callee body exists to attach the sret
        // shape to. The pass must skip.
        let mut ast = AstArena::new();
        let name_ident = ast.alloc(NodeKind::Ident, test_span(9));
        let ret_ty = ast.alloc_type(
            NodeKind::TypeName,
            test_span(9),
            TypeData::Name { name: name_ident, args: Vec::new() },
        );
        let fn_ptr = ast.alloc_type(
            NodeKind::TypeFnPtr,
            empty_span(),
            TypeData::FnPtr {
                params: Vec::new(),
                param_names: Vec::new(),
                ret: ret_ty,
                effects: None,
                capabilities: None,
            },
        );
        // RHS is a bare Ident (a Var/Path in the elaborator's eyes),
        // NOT an ExprLambda.
        let rhs = ast.alloc(NodeKind::Ident, empty_span());
        let let_name = ast.alloc(NodeKind::Ident, empty_span());
        let let_id = ast.alloc_item(
            NodeKind::Let,
            empty_span(),
            ItemData::Let {
                public: false, mutable: false, name: let_name, generic_params: Vec::new(),
                ty: Some(fn_ptr), value: rhs, align: None, ring: None, link_section: None,
                abi: None, no_frame: false, interrupt: None, doc: None,
            },
        );

        let mut registry = StructRegistry::empty();
        let type_id = paideia_as_ir::record_layout::RecordTypeId(1);
        registry.by_name.insert("CpuidRegs".to_string(), type_id);
        registry.fields.insert(
            type_id,
            vec![("eax".to_string(), 0x04), ("ebx".to_string(), 0x04)],
        );

        let source_map = make_source_map("CpuidRegs");

        let mut ir = IrArena::new();
        let ir_let = ir.alloc(IrKind::Let, empty_span());
        let mut m = HashMap::new();
        m.insert(let_id, ir_let);
        populate_return_record_layouts(&ast, &mut ir, &m, &source_map, &registry);
        assert!(
            ir.return_record_layout_table().get(ir_let).is_none(),
            "non-Lambda RHS must not populate the return-record side-table"
        );
    }
}
