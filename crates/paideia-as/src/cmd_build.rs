//! `paideia-as build` — phase-1 placeholder backend.
//!
//! Closes deliverable 4 ("smoke-test elaboration"): the pipeline runs
//! lex → parse → lower → placeholder. The real ELF/PAX/PE emitters
//! arrive at deliverable 8. For now we write a tiny
//! `<input>.placeholder` artifact containing a BLAKE3 hash of the
//! lowered IR's pretty-printed form so the smoke test can verify the
//! pipeline produced something deterministic.
//!
//! # Internal structure
//!
//! Refactored 2026-07-08 from a single 3261-line file. The `run()`
//! orchestrator remains here (with `BuildError`, `EmitFormat`, and
//! `functors_from_modules`); every extracted helper is `pub(super)`-scoped
//! and lives under `cmd_build/`. Nothing new is public.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use crate::cli::Target;
use crate::resolve_var_operands;
use paideia_as_ast::{AstArena, ItemData, NodeId as AstNodeId, StmtData};
use paideia_as_diagnostics::{
    Catalog, Category, Diagnostic, DiagnosticCode, DiagnosticSink, Severity, SourceMap, VecSink,
};
use paideia_as_elaborator::{
    CapWalker, EffectRowWalker, EmitWalker, LinearityWalker, UnsafeWalker, lower_ast_to_ir,
    build_struct_registry, build_enum_registry, finalise_enum_layouts,
    finalise_enum_variant_payloads, placeholder_for, validate_file_module_mapping,
};
use paideia_as_emitter_pax::FunctorsSection;
use paideia_as_types::{TypeInterner, CapSetInterner, Subst};
use paideia_as_effects::EffectInterner;
use paideia_as_ir::{IrKind, IrNodeId, ModuleSideTable, Visibility, walk};
use paideia_as_ir::opt::OptDiagSink;
use paideia_as_ir::opt::dispatch;
use paideia_as_lexer::{Lexer, SourceText};
use paideia_as_parser::Parser;

// --- Internal submodules ---
mod addr_of;
mod diagnostics;
mod elf;
mod fixup;
mod identifier;
mod layout;
mod pax;
mod pe;
mod placeholder;
mod root_attrs;

#[cfg(test)]
mod tests;

use addr_of::extract_var_name_from_operand;
use diagnostics::finish_build_error;
use elf::{build_elf_object, finish_elf};
use identifier::{is_valid_identifier, parse_integer_literal};
use layout::{array_element_byte_width, compute_bss_size_from_type, declared_array_len_from_type};
use pax::{build_pax_object, finish_pax};
use pe::{build_pe_object, finish_pe};
use placeholder::finish_placeholder;
use root_attrs::{extract_root_module_bits, extract_root_module_features};

/// Error type for build operations.
#[derive(Debug, Clone)]
pub enum BuildError {
    /// Instruction encoder failed (e.g., unsupported operand shape).
    Encoder {
        /// IR node ID where the encoder failed.
        node: paideia_as_ir::IrNodeId,
        /// Source span for error reporting.
        source_span: paideia_as_diagnostics::Span,
        /// Encoder error message.
        encoder_message: String,
    },
    /// ELF emitter validation failed (Phase 7 m1-002).
    Emitter {
        /// Diagnostic message from emitter.
        message: String,
    },
}

/// Output format selector for `paideia-as build --emit`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum EmitFormat {
    /// Phase-1 default: write a `<stem>.placeholder` hash next to input.
    Placeholder,
    /// Real ELF64 object via paideia-as-emitter-elf.
    Elf64,
    /// PAX (PaideiaOS Architectural Executable) object via paideia-as-emitter-pax.
    Pax,
    /// PE/COFF (Portable Executable) object via paideia-as-emitter-pe.
    PeCoff,
}

impl EmitFormat {
    /// Parse the `--emit` flag value.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "placeholder" => Ok(Self::Placeholder),
            "elf64" => Ok(Self::Elf64),
            "pax" => Ok(Self::Pax),
            "pe-coff" => Ok(Self::PeCoff),
            other => Err(format!(
                "unknown --emit format `{other}`; expected `placeholder`, `elf64`, `pax`, or `pe-coff`"
            )),
        }
    }
}

/// Resolve a target triplet to an emit format.
fn resolve_target(target: Target) -> EmitFormat {
    match target {
        Target::UefiX86_64 => EmitFormat::PeCoff,
        Target::ElfKernelX86_64 => EmitFormat::Elf64,
        Target::ElfUserX86_64 => EmitFormat::Elf64,
        Target::PaxX86_64 => EmitFormat::Pax,
    }
}

/// Run `paideia-as build <input> [--emit <format>] [-o <output>] [-O <level>] [--encoder-warn] [--sarif <PATH>]`.
pub fn run(input: &Path, output: Option<&Path>, emit: Option<&str>, target: Option<Target>, optimize: u32, encoder_warn: bool, sarif: Option<&Path>) -> ExitCode {
    let format = match (target, emit) {
        (Some(t), None) => resolve_target(t),
        (None, Some(s)) => match EmitFormat::parse(s) {
            Ok(f) => f,
            Err(msg) => {
                eprintln!("paideia-as: {msg}");
                return ExitCode::from(2);
            }
        },
        (None, None) => EmitFormat::Placeholder,
        (Some(_), Some(_)) => unreachable!("clap conflicts_with should prevent both"),
    };
    let bytes = match fs::read(input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("paideia-as: cannot read {}: {e}", input.display());
            return ExitCode::from(2);
        }
    };

    let mut source_map = SourceMap::new();
    let content_string = String::from_utf8_lossy(&bytes).into_owned();
    let file = source_map.add_file(input.to_path_buf(), content_string);

    let mut sink = VecSink::new();
    let catalog = Catalog::embedded();

    let source = match SourceText::from_bytes(file, &bytes) {
        Ok(s) => s,
        Err(diag) => {
            let _ = sink.emit(*diag);
            return finish_placeholder(&source_map, catalog, sink, None, input, output, sarif);
        }
    };

    // Lex.
    let mut lex_sink = VecSink::new();
    let mut lexer = Lexer::new(file, &source);
    let tokens = lexer.collect_tokens(&mut lex_sink);
    for d in lex_sink.into_diagnostics() {
        let _ = sink.emit(d);
    }

    // Parse.
    let mut arena = AstArena::new();
    let root_id;
    {
        let mut parser_sink = VecSink::new();
        let mut p = Parser::new(
            &tokens,
            source.content(),
            file,
            &mut arena,
            &mut parser_sink,
        )
        .with_source_dir(input.parent().map(|p| p.to_path_buf()));
        root_id = p.parse_source_file().ok();
        for d in parser_sink.into_diagnostics() {
            let _ = sink.emit(d);
        }
    }

    // Validate file-to-module mapping (after parse, before lower).
    if let Some(root) = root_id {
        let mut file_module_diags = Vec::new();
        validate_file_module_mapping(
            input,
            root,
            &arena,
            source.content(),
            &mut file_module_diags,
        );
        for d in file_module_diags {
            let _ = sink.emit(d);
        }
    }

    // PA-r17-010a (#1070): Build struct registry before lowering.
    // This enables populate_record_layout_table to look up struct types during RecordCons lowering.
    let registry = build_struct_registry(&arena, &source_map, &mut sink);

    // Phase 7 m4-003 (#1048/#1049): Build enum registry before lowering.
    // This enables populate_enum_cons_info to look up enum types and variants during EnumCons lowering.
    let enum_registry = build_enum_registry(&arena, &source_map, &mut sink);

    // Issue #1054/#1053 (hoisted): Populate enum variant payload types from the enum and struct registries.
    // This enables both type-directed code generation for variant payloads AND nested pattern lowering
    // in populate_match_arm_meta. Must run before lower_ast_to_ir so the payload_map can be threaded through.
    let payload_map = finalise_enum_variant_payloads(&enum_registry, &registry, &arena, &source_map);

    // If there are any errors so far, do not emit anything downstream.
    let mut lowering = lower_ast_to_ir(&arena, &source_map, &mut sink, &registry, &enum_registry, &payload_map);

    // PA-r17-007 (#1050): Populate enum layouts from the enum registry.
    // This enables emit_walker to look up enum layouts during EnumCons and EnumDiscriminant lowering.
    // Issue #1090: Also thread StructRegistry for struct-typed variant payloads fallback.
    let enum_layouts = finalise_enum_layouts(&enum_registry, &registry, &arena, &source_map, &mut sink);
    for (type_id, layout) in enum_layouts {
        lowering.ir.enum_layout_table_mut().insert(type_id, layout);
    }

    // Populate the enum_variant_payload_table from the computed payload_map.
    // Single source of truth pattern: payload_map computed once above,
    // used both for populate_match_arm_meta during lowering and here for the IR side-table.
    for ((enum_id, variant_idx), payload) in payload_map {
        lowering.ir.enum_variant_payload_table_mut()
            .insert(enum_id, variant_idx, payload);
    }

    // Phase-5-m1-001: Extract literal values from AST and populate the IR's literal_values table.
    // This enables emit_walker to look up literal values during lambda lowering.
    {
        let content_ref = source_map.content(file);

        // Walk AST to find all ExprLiteral nodes and extract their numeric values
        for i in 0..arena.len() {
            if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
                if let Some(node) = arena.get(ast_id) {
                    if node.kind == paideia_as_ast::NodeKind::ExprLiteral {
                        if let Some(paideia_as_ast::ExprData::Literal { lit }) =
                            arena.expr_data(ast_id)
                        {
                            // The 'lit' is a Placeholder node that contains the literal's span
                            if let Some(lit_node) = arena.get(*lit) {
                                let span = lit_node.span;
                                let start = span.byte_start() as usize;
                                let len = span.byte_len() as usize;
                                if start + len <= content_ref.len() {
                                    let literal_text = &content_ref[start..start + len];
                                    // Try to parse the literal as u64/i64
                                    // Handle common formats: decimal, hex (0x...), binary (0b...), octal (0o...)
                                    if let Ok(value) = parse_integer_literal(literal_text) {
                                        // Map AST node ID to IR node ID (1-to-1 mapping)
                                        // The KEY is the ExprLiteral node ID (ast_id), not the Placeholder child ID,
                                        // because the IR Literal node ID = ast_id (1-to-1 mapping).
                                        let ir_lit_id = paideia_as_ir::IrNodeId::new(ast_id.get())
                                            .expect("valid ir node id from ast expr literal node");
                                        lowering.ir.literal_values_mut().insert(ir_lit_id, value);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Phase 6 m2-004: Extract binding names from AST Let nodes and populate the IR's binding_names table.
    // This enables emit_walker to use actual binding names (_start, _anchor, etc.) instead of generic _let_<nodeid>.
    {
        let content_ref = source_map.content(file);

        // Walk AST to find all Let nodes and extract their binding names
        for i in 0..arena.len() {
            if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
                if let Some(node) = arena.get(ast_id) {
                    if node.kind == paideia_as_ast::NodeKind::Let {
                        if let Some(paideia_as_ast::ItemData::Let { name: name_id, .. }) =
                            arena.item_data(ast_id)
                        {
                            // Get the Ident node for the binding name
                            if let Some(name_node) = arena.get(*name_id) {
                                let span = name_node.span;
                                let start = span.byte_start() as usize;
                                let len = span.byte_len() as usize;
                                if start + len <= content_ref.len() {
                                    let binding_text = content_ref[start..start + len].to_string();
                                    // Map AST Let node ID to IR Let node ID (1-to-1 mapping)
                                    let ir_let_id = paideia_as_ir::IrNodeId::new(ast_id.get())
                                        .expect("valid ir node id from ast let node");
                                    lowering
                                        .ir
                                        .binding_names_mut()
                                        .insert(ir_let_id, binding_text);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // PA904: Extract public flag from AST Let nodes and populate the IR's public_lets table.
    // This enables the elaborator to mark symbols as global when they have explicit `pub` visibility.
    {
        for i in 0..arena.len() {
            if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
                if let Some(_node) = arena.get(ast_id) {
                    if let Some(paideia_as_ast::ItemData::Let { public, .. }) =
                        arena.item_data(ast_id)
                    {
                        if *public {
                            // Map AST Let node ID to IR Let node ID (1-to-1 mapping)
                            let ir_let_id = paideia_as_ir::IrNodeId::new(ast_id.get())
                                .expect("valid ir node id from ast let node");
                            lowering.ir.public_lets_mut().insert(ir_let_id);
                        }
                    }
                }
            }
        }
    }

    // PA8-m1-001c: Extract lambda parameter binding names from AST Lambda nodes
    // and populate the IR's binding_names and lambda_params tables. This enables
    // emit_walker to use actual parameter names (e.g., "foo", "bar") instead of
    // generic "_param_<index>".
    {
        let content_ref = source_map.content(file);

        // Walk AST to find all Lambda nodes and extract their parameter binding names
        for i in 0..arena.len() {
            if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
                if let Some(_node) = arena.get(ast_id) {
                    if let Some(paideia_as_ast::ExprData::Lambda { params, .. }) =
                        arena.expr_data(ast_id)
                    {
                        // Map Lambda IR node ID to parameter node IDs
                        let ir_lambda_id = paideia_as_ir::IrNodeId::new(ast_id.get())
                            .expect("valid ir node id from ast lambda");
                        let ir_param_ids: Vec<paideia_as_ir::IrNodeId> = params
                            .iter()
                            .filter_map(|param_id| paideia_as_ir::IrNodeId::new(param_id.get()))
                            .collect();
                        lowering
                            .ir
                            .lambda_params_mut()
                            .insert(ir_lambda_id, ir_param_ids);

                        // Each parameter is a Pattern node ID
                        // We need to extract the binding name from each pattern
                        for param_id in params {
                            // Check if this pattern is a simple Ident (most common case)
                            if let Some(paideia_as_ast::PatternData::Ident {
                                name: name_id, ..
                            }) = arena.pattern_data(*param_id)
                            {
                                // Get the Ident node for the parameter binding name
                                if let Some(name_node) = arena.get(*name_id) {
                                    let span = name_node.span;
                                    let start = span.byte_start() as usize;
                                    let len = span.byte_len() as usize;
                                    if start + len <= content_ref.len() {
                                        let binding_text =
                                            content_ref[start..start + len].to_string();
                                        // Map AST pattern node ID to IR param node ID
                                        // (AST and IR use same node IDs per lowering)
                                        let ir_param_id =
                                            paideia_as_ir::IrNodeId::new(param_id.get())
                                                .expect("valid ir node id from ast param");
                                        lowering
                                            .ir
                                            .binding_names_mut()
                                            .insert(ir_param_id, binding_text);
                                    }
                                }
                            }
                            // For non-Ident patterns (e.g., wildcard, destructuring),
                            // we fall back to synthetic _param_<index> in emit_walker.
                        }
                    }
                }
            }
        }
    }

    // PA-r17-004: Populate binding_names for use-site Var IR nodes so that
    // emit_identity_lambda (and future Var-body lowering) can resolve
    // parameter references via LocalBindingTable.
    //
    // Variable references in the source (like `fn(a) -> a`) are represented as ExprPath
    // in the AST. During lowering, these become Var IR nodes. We extract the variable name
    // from the last segment of the ExprPath and populate binding_names for the corresponding
    // IR node with that name.
    {
        let content_ref = source_map.content(file);

        // Walk AST to find all ExprPath nodes (variable references)
        for i in 0..arena.len() {
            if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
                if let Some(node) = arena.get(ast_id) {
                    if node.kind == paideia_as_ast::NodeKind::ExprPath {
                        // Extract the last segment of the path (the identifier)
                        if let Some(paideia_as_ast::ExprData::Path { segments }) = arena.expr_data(ast_id) {
                            if !segments.is_empty() {
                                let last_segment_id = segments[segments.len() - 1];
                                if let Some(segment_node) = arena.get(last_segment_id) {
                                    // The segment should be an Ident node
                                    if segment_node.kind == paideia_as_ast::NodeKind::Ident {
                                        let span = segment_node.span;
                                        let start = span.byte_start() as usize;
                                        let len = span.byte_len() as usize;
                                        if start + len <= content_ref.len() {
                                            let ident_text = content_ref[start..start + len].to_string();
                                            // Map AST ExprPath node ID to IR Var node ID
                                            let ir_id = paideia_as_ir::IrNodeId::new(ast_id.get())
                                                .expect("valid ir node id from ast exprpath");
                                            // Only populate if not already set
                                            if lowering.ir.binding_names().get(ir_id).is_none() {
                                                lowering.ir.binding_names_mut().insert(ir_id, ident_text);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // PA10-002: Extract string and byte string literals from AST nodes
    // and populate the IR's literal_bytes table. This enables the emitter
    // to intern byte sequences and emit .rodata symbols with relocations.
    // Issue #1012: Also extract InlineBytes literals (@guid, @include_bytes).
    {
        let _content_ref = source_map.content(file);

        // Walk AST to find all ExprString, ExprByteString, and ExprInlineBytes nodes
        // and extract their payloads
        for i in 0..arena.len() {
            if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
                if let Some(node) = arena.get(ast_id) {
                    match node.kind {
                        paideia_as_ast::NodeKind::ExprString => {
                            if let Some(paideia_as_ast::ExprData::StringLiteral(s)) =
                                arena.expr_data(ast_id)
                            {
                                let bytes = s.as_bytes().to_vec();
                                let ir_string_id = paideia_as_ir::IrNodeId::new(ast_id.get())
                                    .expect("valid ir node id from ast string literal");
                                lowering.ir.literal_bytes_mut().insert(ir_string_id, bytes);
                            }
                        }
                        paideia_as_ast::NodeKind::ExprByteString => {
                            if let Some(paideia_as_ast::ExprData::ByteStringLiteral(bytes)) =
                                arena.expr_data(ast_id)
                            {
                                let ir_bytestring_id = paideia_as_ir::IrNodeId::new(ast_id.get())
                                    .expect("valid ir node id from ast byte string literal");
                                lowering
                                    .ir
                                    .literal_bytes_mut()
                                    .insert(ir_bytestring_id, bytes.clone());
                            }
                        }
                        paideia_as_ast::NodeKind::ExprInlineBytes => {
                            if let Some(paideia_as_ast::ExprData::InlineBytes(bytes)) =
                                arena.expr_data(ast_id)
                            {
                                let ir_inline_bytes_id = paideia_as_ir::IrNodeId::new(ast_id.get())
                                    .expect("valid ir node id from ast inline bytes literal");
                                lowering
                                    .ir
                                    .literal_bytes_mut()
                                    .insert(ir_inline_bytes_id, bytes.clone());
                            }
                        }
                        paideia_as_ast::NodeKind::ExprInlineStr => {
                            if let Some(paideia_as_ast::ExprData::InlineStr(bytes)) =
                                arena.expr_data(ast_id)
                            {
                                let ir_inline_str_id = paideia_as_ir::IrNodeId::new(ast_id.get())
                                    .expect("valid ir node id from ast inline str literal");
                                lowering
                                    .ir
                                    .literal_bytes_mut()
                                    .insert(ir_inline_str_id, bytes.clone());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Run walkers over the IR to surface S/F/C diagnostics.
    // Phase-2-m1: walkers run with empty injection tables (from CLI), so only
    // diagnostics that depend on kind-only IR will fire (S0900/S0901/S0903).
    // Real effect (F1100, F1101, F1105, F1106) and capability (C1300) diagnostics
    // require per-node payloads that arrive in m3/m5.
    // Phase-5-m1-005: EmitWalker chains into the walker pipeline and populates
    // InstructionSideTable for downstream emit stages.
    let mut emit_walker = EmitWalker::new();
    let mut instruction_table: paideia_as_ir::InstructionSideTable;

    if !lowering.ir.is_empty() {
        // Create a walker sink to accumulate diagnostics from all walkers.
        let mut walker_sink = VecSink::new();

        // PA-R12-004 (#913): control-flow support added in PA-R17-012 (#990)
        // Pure fn bodies can now contain if/match/while/loop; T0532 stub retired.

        // Determine the root node ID for walking. In phase-1 lowering, the parser
        // creates a Module as the first node (NodeId 1 → IrNodeId 1), so we walk
        // from IrNodeId::new(1). If the IR is somehow empty, skip walking.
        if let Some(ir_root_id) = IrNodeId::new(1) {
            // Run each walker with a fresh WalkerCtx to avoid borrow conflicts.
            // Each walker emits diagnostics into walker_sink.

            {
                let mut ctx = paideia_as_ir::WalkerCtx::new(&source_map, &mut walker_sink);
                let mut linearity_walker = LinearityWalker::new();
                walk(&mut linearity_walker, &lowering.ir, ir_root_id, &mut ctx);
            }

            {
                let mut ctx = paideia_as_ir::WalkerCtx::new(&source_map, &mut walker_sink);
                let mut effect_walker = EffectRowWalker::new();
                walk(&mut effect_walker, &lowering.ir, ir_root_id, &mut ctx);
            }

            {
                let mut ctx = paideia_as_ir::WalkerCtx::new(&source_map, &mut walker_sink);
                let mut cap_walker = CapWalker::new();
                walk(&mut cap_walker, &lowering.ir, ir_root_id, &mut ctx);
            }

            // Phase-5-m1-005: Run EmitWalker to populate InstructionSideTable.
            // EmitWalker does not use the walker framework (it uses direct arena iteration),
            // so we call its walk method directly rather than through the walk() driver.

            // Phase 15 m2-002a: Extract root module's #![bits = N] and set initial mode.
            // root_id is the AST root from parsing; it's in scope here.
            let root_mode = extract_root_module_bits(root_id, &arena)
                .map(|bits| {
                    if bits == 32 {
                        paideia_as_ir::instruction::InstrMode::Mode32
                    } else {
                        paideia_as_ir::instruction::InstrMode::Mode64
                    }
                })
                .unwrap_or(paideia_as_ir::instruction::InstrMode::Mode64);
            emit_walker.set_root_mode(root_mode);

            // PA-r16-004-backtrack-a (#1033): Extract root module's #![target_features = "..."]
            // and set enabled CPU features for feature gating.
            let features = extract_root_module_features(root_id, &arena, &source_map, file);
            emit_walker.state_mut().set_enabled_features(features);

            // PA-r17-010c (#1072): populate finalised record layouts from the
            // StructRegistry so visit_record_cons + emit_store_record can consume them.
            emit_walker.state_mut().finalise_record_layouts(&registry.fields);

            // PA-r17-007 (#1050): Mirror enum layouts from IR into walker state.
            // This enables visit_enum_cons + emit_enum_discriminant to consume layouts during emission.
            for (type_id, layout) in lowering.ir.enum_layout_table().iter() {
                emit_walker.state_mut().insert_enum_layout(*type_id, layout.clone());
            }

            // PA-r17-004: Pre-emit pass to populate call_sites metadata for App nodes.
            // Walk IR to find all App nodes and extract callee names, storing metadata
            // in the CallSideTable for later dispatch in emit_walker.
            {
                let content_ref = source_map.content(file);

                // Collect all App node metadata in a separate pass to avoid borrow conflicts
                let mut app_metadata = Vec::new();

                {
                    let ir_arena = &lowering.ir;

                    // Walk all IR nodes to find App nodes
                    for ir_idx in 0..ir_arena.len() {
                        if let Some(ir_id) = IrNodeId::new((ir_idx + 1) as u32) {
                            if let Some(ir_node) = ir_arena.get(ir_id) {
                                if ir_node.kind == IrKind::App {
                                    let app_children = ir_arena.children(ir_id);

                                    // App structure: [callee, arg0, arg1, ...]
                                    if !app_children.is_empty() {
                                        let callee_id = app_children[0];
                                        let arg_count = (app_children.len() - 1) as u32;

                                        // Extract callee name from the callee node's span
                                        if let Some(callee_node) = ir_arena.get(callee_id) {
                                            let span = callee_node.span;
                                            let start = span.byte_start() as usize;
                                            let len = span.byte_len() as usize;
                                            if start + len <= content_ref.len() {
                                                let callee_text = content_ref[start..start + len].to_string();

                                                // Only record if it's a valid identifier
                                                if is_valid_identifier(&callee_text) {
                                                    app_metadata.push((ir_id, callee_text, arg_count));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Now insert all collected metadata into the IR's call_sites table
                for (ir_id, callee_name, arg_count) in app_metadata {
                    let call_meta = paideia_as_ir::call_meta::CallMeta {
                        callee_name,
                        arg_count,
                        is_intrinsic: false,
                    };
                    lowering.ir.call_sites_mut().insert(ir_id, call_meta);
                }
            }

            // PA19-r19-006: Pre-populate lambda_abi in emit_walker state before walk.
            // This ensures that when Lambda nodes are visited during walk (which may occur
            // before their Let binding is processed, if the Lambda has a lower node ID),
            // the ABI is already available for lookup.
            for i in 0..arena.len() {
                if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
                    if let Some(node) = arena.get(ast_id) {
                        if node.kind == paideia_as_ast::NodeKind::Let {
                            if let Some(paideia_as_ast::ItemData::Let {
                                abi: Some(cc),
                                value: value_id,
                                ..
                            }) = arena.item_data(ast_id)
                            {
                                // Convert AST CallingConvention to IR CallingConvention
                                let ir_cc = match cc {
                                    paideia_as_ast::CallingConvention::Ms => paideia_as_ir::CallingConvention::Ms,
                                    paideia_as_ast::CallingConvention::Sysv => paideia_as_ir::CallingConvention::Sysv,
                                };
                                // The value (RHS of Let, usually a Lambda) has the same numeric ID in IR
                                let ir_lambda_id = value_id.get();
                                emit_walker.state_mut().insert_lambda_abi(ir_lambda_id, ir_cc);
                            }
                        }
                    }
                }
            }

            emit_walker.walk(&mut lowering.ir);

            // Phase 15 m2-002: Verify mode_stack is properly cleaned up after walk.
            debug_assert!(
                emit_walker.state().mode_stack_is_empty()
                    || emit_walker.state().mode_stack_len() == 1,
                "EmitWalker mode_stack should be empty or have 1 entry at end of walk; got {}",
                emit_walker.state().mode_stack_len()
            );

            // Refactor 2026-07-07 Step 3 (FLOOR): drain the canonical typed
            // diagnostic pipe from EmitWalker into walker_sink so no new
            // T-code can be silently discarded. The legacy `Vec<String>`
            // buffer accessed via `emit_walker.diagnostics()` still exists
            // but is not drained here — retirement is a follow-up.
            for diag in emit_walker.take_typed_diagnostics() {
                let _ = walker_sink.emit(diag);
            }

            // Phase-5-m3-005: Run UnsafeWalker to elaborate pending unsafe blocks.
            // Take pending unsafe blocks from EmitWalker state and process them.
            let pending = emit_walker.state_mut().take_pending_unsafe();
            // Issue #1088: Clone pending for emit_pending_unsafe_bodies (after UnsafeWalker).
            let pending_for_ir_emit = pending.clone();
            let record_layouts = emit_walker.state().record_layouts();
            let local_bindings = emit_walker.state().local_bindings();
            let enabled_features = emit_walker.state().enabled_features();
            let (unsafe_labels, label_to_instr, first_instrs, unsafe_diags) = UnsafeWalker::run(
                &mut lowering.ir,
                &arena,
                pending,
                &source_map,
                &mut walker_sink,
                record_layouts,
                local_bindings,
                root_mode,
                enabled_features,
            );

            // Register collected unsafe block labels with emit_walker state
            for (label_name, _label_offset) in unsafe_labels {
                emit_walker.state_mut().register_label(label_name);
            }

            // Store label_to_instr mapping for use in label offset computation after encoding
            // (We'll use this to resolve label offsets based on instruction offsets from offset_map)
            emit_walker.state_mut().set_label_to_instr(label_to_instr);

            // PA8-m1-002b: Wire first_instrs back to lambda_first_instr for unsafe lambdas.
            // first_instrs[i] is the first instruction of the i-th pending unsafe block.
            // We look up which lambda corresponds to that pending index via unsafe_lambda_to_pending_idx.
            {
                let pending_idx_map: Vec<_> = emit_walker
                    .state()
                    .unsafe_lambda_to_pending_idx()
                    .iter()
                    .map(|(&lambda_id, &idx)| (lambda_id, idx))
                    .collect();
                for (lambda_id, idx) in pending_idx_map {
                    if let Some(Some(first_instr)) = first_instrs.get(idx) {
                        emit_walker
                            .state_mut()
                            .insert_lambda_first_instr(lambda_id, *first_instr);
                    }
                }
            }

            // Phase 7 m4-003: Emit pending unsafe-block statement bodies.
            // Issue #1088: After UnsafeWalker processes raw instructions and labels,
            // emit any pending action statements (call expressions, etc.) through the
            // standard IR emit pipeline.
            emit_walker.emit_pending_unsafe_bodies(
                pending_for_ir_emit,
                &lowering.ir,
                None,
            );
            for diag in emit_walker.take_typed_diagnostics() {
                let _ = walker_sink.emit(diag);
            }

            // Phase-7-m2-003: Resolve Operand::Var references to Operand::Reg.
            // Call resolve_var_operands on the arena's owned instruction table,
            // then re-clone for the encoder pipeline.
            // PA10-005 §3.5: Thread SymbolTable through for T0531 diagnostic.
            //
            // Refactor 2026-07-07 Step 2: resolve_var_operands now returns
            // typed `Diagnostic` values (was `Vec<String>` requiring a
            // string re-parse to recover the T-code). This retires the
            // fragile `msg_str.find(':')` decoder and the fabricated `700`
            // catch-all that hid diagnostic-catalog mismatches.
            {
                let symbol_table_clone = lowering.ir.symbols().clone();
                let bindings = emit_walker.state().local_bindings();
                let mut resolve_diags: Vec<paideia_as_diagnostics::Diagnostic> = Vec::new();
                resolve_var_operands::resolve_var_operands(
                    lowering.ir.instructions_mut(),
                    bindings,
                    Some(symbol_table_clone),
                    &mut resolve_diags,
                );
                for diag in resolve_diags {
                    let _ = walker_sink.emit(diag);
                }
            }

            for d in unsafe_diags {
                let _ = walker_sink.emit(d);
            }
        }

        // Drain walker diagnostics into the main sink for rendering.
        for d in walker_sink.into_diagnostics() {
            let _ = sink.emit(d);
        }
    }

    // Optimization pass: if --optimize >= 1, run peephole and other passes.
    // This runs after all semantic walkers but before encoding.
    if optimize >= 1 && !lowering.ir.is_empty() {
        let mut requested_passes = BTreeSet::new();
        requested_passes.insert("peephole".to_string());
        let mut opt_sink = OptDiagSink::new();

        // Run optimization passes on the root module (IrNodeId 1)
        if let Some(ir_root_id) = IrNodeId::new(1) {
            let _changes = dispatch::dispatch(&mut lowering.ir, ir_root_id, &requested_passes, &mut opt_sink);
            // Log optimization results
            if cfg!(debug_assertions) {
                eprintln!("[opt] {} changes applied from optimization passes", _changes);
            }
        }
    }

    // Re-sync instruction_table post-optimization to capture peephole changes
    instruction_table = lowering.ir.instructions().clone();

    // Phase-5-m6-005: Symbol name resolution pass.
    // Walk the AST to find Let bindings with actual names, then update the symbol table
    // to use the real binding names instead of "_let_<id>".
    // B3-004: Also extract the `pub` flag from each Let binding to set STB_GLOBAL.
    {
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
                                    let ir_abi = abi.map(|cc| match cc {
                                        paideia_as_ast::CallingConvention::Ms => paideia_as_ir::CallingConvention::Ms,
                                        paideia_as_ast::CallingConvention::Sysv => paideia_as_ir::CallingConvention::Sysv,
                                    });
                                    if let Some(ir_id) = paideia_as_ir::IrNodeId::new(ast_id.get()) {
                                        lowering.ir.let_meta_mut().insert(
                                            ir_id,
                                            paideia_as_ir::LetInfo::with_abi(*mutable, None, *align, *ring, link_section.clone(), ir_abi),
                                        );
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

    // Phase 19 PA19-r19-010: P0284 validation pass.
    // Check all Let bindings with @link_section directive. If the value is lambda-shaped,
    // emit P0284 error and reject (deferred to pa-r19-010b).
    {
        for i in 0..arena.len() {
            if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
                if let Some(node) = arena.get(ast_id) {
                    if node.kind == paideia_as_ast::NodeKind::Let {
                        if let Some(paideia_as_ast::ItemData::Let {
                            link_section: Some(_),
                            value: value_id,
                            ..
                        }) = arena.item_data(ast_id)
                        {
                            // Check if the value is a lambda (ExprLambda)
                            if let Some(value_node) = arena.get(*value_id) {
                                if value_node.kind == paideia_as_ast::NodeKind::ExprLambda {
                                    // Emit P0284: lambda bindings cannot use @link_section
                                    let code = paideia_as_diagnostics::DiagnosticCode::new(
                                        paideia_as_diagnostics::Category::P,
                                        paideia_as_diagnostics::Severity::Error,
                                        284,
                                    ).expect("valid P0284 code");
                                    let diag = paideia_as_diagnostics::Diagnostic::error(code)
                                        .message("lambda-shaped bindings cannot use @link_section (deferred to pa-r19-010b)")
                                        .with_span(value_node.span)
                                        .finish();
                                    let _ = sink.emit(diag);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Phase 19 PA19-r19-001: P0286 validation pass.
    // Check all Let bindings with @abi directive. If the value is NOT lambda-shaped,
    // emit P0286 error and reject.
    {
        for i in 0..arena.len() {
            if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
                if let Some(node) = arena.get(ast_id) {
                    if node.kind == paideia_as_ast::NodeKind::Let {
                        if let Some(paideia_as_ast::ItemData::Let {
                            abi: Some(_),
                            value: value_id,
                            ..
                        }) = arena.item_data(ast_id)
                        {
                            // Check if the value is a lambda (ExprLambda)
                            if let Some(value_node) = arena.get(*value_id) {
                                if value_node.kind != paideia_as_ast::NodeKind::ExprLambda {
                                    // Emit P0286: @abi is only valid on lambda-shaped bindings
                                    let code = paideia_as_diagnostics::DiagnosticCode::new(
                                        paideia_as_diagnostics::Category::P,
                                        paideia_as_diagnostics::Severity::Error,
                                        286,
                                    ).expect("valid P0286 code");
                                    let diag = paideia_as_diagnostics::Diagnostic::error(code)
                                        .message("@abi is only valid on function-shaped bindings")
                                        .with_span(value_node.span)
                                        .finish();
                                    let _ = sink.emit(diag);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Phase 19 PA19-r19-001: U1620 narrowed gate pass (PA19-r19-006).
    // Check all Let bindings with @abi("ms") directive that ARE lambda-shaped.
    // Emit U1620 only for unsupported MS x64 shapes:
    // - 5+ formal parameters (MS x64 only supports 4 register args)
    // - Body shape not in {Path, Literal, Infix(+, ...)}
    {
        for i in 0..arena.len() {
            if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
                if let Some(node) = arena.get(ast_id) {
                    if node.kind == paideia_as_ast::NodeKind::Let {
                        if let Some(paideia_as_ast::ItemData::Let {
                            abi: Some(paideia_as_ast::CallingConvention::Ms),
                            value: value_id,
                            ..
                        }) = arena.item_data(ast_id)
                        {
                            // Check if the value is a lambda (ExprLambda)
                            if let Some(value_node) = arena.get(*value_id) {
                                if value_node.kind == paideia_as_ast::NodeKind::ExprLambda {
                                    let mut should_emit_u1620 = false;
                                    let mut error_message = String::new();

                                    // Access Lambda expression data
                                    if let Some(expr_data) = arena.expr_data(*value_id) {
                                        if let paideia_as_ast::ExprData::Lambda { params, body, .. } = expr_data {
                                            // Check formal parameter count (MS x64 only supports 4 register args)
                                            if params.len() > 4 {
                                                should_emit_u1620 = true;
                                                error_message = format!(
                                                    "MS x64 calling convention does not support {} parameters (max 4 in registers); \
                                                     overflow to stack not yet emittable",
                                                    params.len()
                                                );
                                            } else {
                                                // Check the lambda body shape
                                                // Supported shapes for MVP:
                                                // - Path (identity: fn (x) -> x)
                                                // - Literal (literal return: fn () -> 42)
                                                // - Infix (binary op: fn (x) -> x + 1, x * 2, etc.)
                                                // All other shapes fire U1620.
                                                if let Some(body_node) = arena.get(*body) {
                                                    match body_node.kind {
                                                        paideia_as_ast::NodeKind::ExprPath => {
                                                            // Identity: fn (x) -> x ✓
                                                        }
                                                        paideia_as_ast::NodeKind::ExprLiteral => {
                                                            // Literal: fn () -> 42 ✓
                                                        }
                                                        paideia_as_ast::NodeKind::ExprInfix => {
                                                            // MS x64 narrowing: only addition with (ident, literal) operands supported.
                                                            // Pattern: fn (x) -> x + 1
                                                            // Reject all other patterns: x * x, x + y, 1 + x, etc.
                                                            if let Some(infix_data) = arena.expr_data(*body) {
                                                                if let paideia_as_ast::ExprData::Infix { op, lhs, rhs } = infix_data {
                                                                    // Check operator is addition
                                                                    let op_name = if let Some(op_node) = arena.get(*op) {
                                                                        let op_span = op_node.span;
                                                                        let start = op_span.byte_start() as usize;
                                                                        let len = op_span.byte_len() as usize;
                                                                        if start + len <= source_map.content(file).len() {
                                                                            source_map.content(file)[start..start + len].to_string()
                                                                        } else {
                                                                            String::new()
                                                                        }
                                                                    } else {
                                                                        String::new()
                                                                    };

                                                                    // Check LHS is a Path (ident reference)
                                                                    let lhs_ok = arena.get(*lhs).map(|n| n.kind == paideia_as_ast::NodeKind::ExprPath).unwrap_or(false);
                                                                    // Check RHS is a Literal (integer literal)
                                                                    let rhs_ok = arena.get(*rhs).map(|n| n.kind == paideia_as_ast::NodeKind::ExprLiteral).unwrap_or(false);

                                                                    // Only allow: + with (Path, Literal)
                                                                    if op_name != "+" || !lhs_ok || !rhs_ok {
                                                                        should_emit_u1620 = true;
                                                                        error_message = format!(
                                                                            "MS x64 body shape not yet emittable (only `x + literal` supported, got `{} {} {}`)",
                                                                            if lhs_ok { "ident" } else { "non-ident" },
                                                                            if op_name == "+" { "+" } else { &op_name },
                                                                            if rhs_ok { "literal" } else { "non-literal" }
                                                                        );
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        _ => {
                                                            should_emit_u1620 = true;
                                                            error_message = "MS x64 body shape not yet emittable (only identity/binary/literal returns supported)".to_string();
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    if should_emit_u1620 {
                                        let code = paideia_as_diagnostics::DiagnosticCode::new(
                                            paideia_as_diagnostics::Category::U,
                                            paideia_as_diagnostics::Severity::Error,
                                            1620,
                                        ).expect("valid U1620 code");
                                        let diag = paideia_as_diagnostics::Diagnostic::error(code)
                                            .message(error_message)
                                            .with_span(value_node.span)
                                            .finish();
                                        let _ = sink.emit(diag);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // PA-R17-003 / #988: Address-of pre-emit pass. Resolve `&fn_name` operands and populate AddrOfSideTable.
    // This must run after SymbolTable is populated (above) and before the data-table loop (below).
    // PA-r17-003b (#1038): T0535 signature check is wired in this pass.
    // #988 v2: Key by rhs_id (Borrow node) instead of let_id to disambiguate multiple Borrow per Let
    // (needed for record literals with multiple fnptr fields).
    if !lowering.ir.is_empty() {
        let arena_len = lowering.ir.len();
        // Collect all address-of entries first to avoid borrow issues.
        // Also collect let_id for T0535 checking.
        // #1074: Extended to track record-field context: Option<(rc_ir_id, field_idx0)>
        let mut addr_of_entries: Vec<(IrNodeId, IrNodeId, String, Option<(IrNodeId, usize)>)> = Vec::new();

        for i in 1..=arena_len as u32 {
            if let Some(let_id) = IrNodeId::new(i) {
                if let Some(node) = lowering.ir.get(let_id) {
                    if node.kind == paideia_as_ir::IrKind::Let {
                        let children: Vec<_> = lowering.ir.children(let_id).iter().copied().collect();
                        // Look for an RHS with IrKind::Borrow
                        for rhs_id in &children {
                            if let Some(rhs_node) = lowering.ir.get(*rhs_id) {
                                if rhs_node.kind == paideia_as_ir::IrKind::Borrow {
                                    // Locate the Borrow's single child (the operand).
                                    let borrow_children: Vec<_> = lowering.ir.children(*rhs_id).iter().copied().collect();
                                    if let Some(operand_id) = borrow_children.first() {
                                        // Use helper to extract and validate var_name
                                        if let Some(var_name) = extract_var_name_from_operand(
                                            *operand_id,
                                            &lowering,
                                            &source_map,
                                            file,
                                            &mut sink,
                                        ) {
                                            // #988 v2: Push (let_id, rhs_id, var_name, None) for T0535 checking
                                            // #1074: Tag as None (not from record field)
                                            addr_of_entries.push((let_id, *rhs_id, var_name, None));
                                        }
                                    }
                                }
                            }
                        }

                        // #988 v2: Also handle RecordCons fields with Borrow children
                        for rhs_id in &children {
                            if let Some(rhs_node) = lowering.ir.get(*rhs_id) {
                                if rhs_node.kind == paideia_as_ir::IrKind::RecordCons {
                                    // Skip type_name child (index 0), process field children
                                    let field_children: Vec<_> = lowering.ir.children(*rhs_id).iter().copied().collect();
                                    for (field_idx, &field_id) in field_children.iter().enumerate() {
                                        if field_idx == 0 {
                                            // Skip type_name at index 0
                                            continue;
                                        }
                                        if let Some(field_node) = lowering.ir.get(field_id) {
                                            if field_node.kind == paideia_as_ir::IrKind::Borrow {
                                                // Get the Borrow's operand
                                                let borrow_children: Vec<_> = lowering.ir.children(field_id).iter().copied().collect();
                                                if let Some(operand_id) = borrow_children.first() {
                                                    if let Some(var_name) = extract_var_name_from_operand(
                                                        *operand_id,
                                                        &lowering,
                                                        &source_map,
                                                        file,
                                                        &mut sink,
                                                    ) {
                                                        // Push (let_id, borrow_id, var_name, Some((rc_ir_id, field_idx0))) for record field
                                                        // field_idx is 1-based (0 is type_name), so field_idx0 = field_idx - 1
                                                        addr_of_entries.push((let_id, field_id, var_name, Some((*rhs_id, field_idx - 1))));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Construct interners for T0535 type checking
        let mut types = TypeInterner::new();
        let mut effects = EffectInterner::new();
        let mut caps = CapSetInterner::new();

        // Now populate the AddrOfSideTable and perform T0535 checks
        // #988 v2: Keyed by rhs_id (Borrow node) not let_id
        // #1074: Handle both top-level and record-field cases
        for (let_id, rhs_id, var_name, record_context) in addr_of_entries {
            lowering.ir.addr_of_mut().insert(
                rhs_id,
                paideia_as_ir::AddrOfMeta::new(var_name.clone()),
            );

            match record_context {
                None => {
                    // PA-r17-003b (#1038): T0535 signature check for top-level let bindings
                    // Get the type annotation NodeId from the AST Let node
                    let ast_let_id = AstNodeId::new(let_id.get()).unwrap();

                    // Let nodes can be either ItemData::Let (module-level) or StmtData::Let (statement-level)
                    let type_annotation_node_id = if let Some(item_data) = arena.item_data(ast_let_id) {
                        match item_data {
                            ItemData::Let { ty: Some(ty_node), .. } => Some(*ty_node),
                            _ => None,
                        }
                    } else if let Some(stmt_data) = arena.stmt_data(ast_let_id) {
                        match stmt_data {
                            StmtData::Let { ty: Some(ty_node), .. } => Some(*ty_node),
                            _ => None,
                        }
                    } else {
                        None
                    };

                    if let Some(lhs_type_node) = type_annotation_node_id {
                        // Lower the LHS type annotation to a TypeId
                        if let Ok(lhs_tid) = paideia_as_elaborator::lower_type::lower_type_ast(
                            &arena,
                            &source_map,
                            lhs_type_node,
                            &mut types,
                            &mut effects,
                            &mut caps,
                            &registry,
                        ) {
                            // Look up the lambda via symbol table
                            if let Some(symbol) = lowering.ir.symbols().lookup_by_name(&var_name) {
                                // Convert IR node ID to AST node ID (they're the same)
                                let lambda_ast_id = AstNodeId::new(symbol.ir_node.get()).unwrap();

                                // Derive the RHS signature from the lambda
                                if let Some(rhs_tid) = paideia_as_elaborator::derive_fn_sig::derive_fn_sig_from_lambda(
                                    &arena,
                                    &source_map,
                                    lambda_ast_id,
                                    &mut types,
                                    &mut effects,
                                    &mut caps,
                                    &registry,
                                ) {
                                    // T0535 check only applies to fn-ptr LHS types.
                                    if matches!(types.get(lhs_tid), paideia_as_types::Type::Fn { .. }) {
                                        // Check fn-ptr assignment compatibility
                                        let mut subst = Subst::new();
                                        let span = lowering.ir.get(rhs_id).map(|n| n.span).unwrap_or_else(|| {
                                            paideia_as_diagnostics::Span::new(
                                                paideia_as_diagnostics::FileId::new(1).unwrap(),
                                                0,
                                                0,
                                            )
                                        });
                                        let diags = paideia_as_elaborator::check_fn_ptr_assignment(
                                            &mut types,
                                            &mut subst,
                                            &effects,
                                            &caps,
                                            lhs_tid,
                                            rhs_tid,
                                            span,
                                        );
                                        // Push diagnostics to sink
                                        for diag in diags {
                                            let _ = sink.emit(diag);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Some((rc_ir_id, field_idx0)) => {
                    // #1074: T0535 signature check for record-field fn-ptr assignments
                    // Get the RecordTypeId from the IR's record_layout_table
                    if let Some(record_type_id) = lowering.ir.record_layout_table().get(rc_ir_id) {
                        // Get the field type node for this field index
                        if let Some(field_type_nodes) = registry.field_type_nodes.get(&record_type_id) {
                            if field_idx0 < field_type_nodes.len() {
                                let field_ty_node = field_type_nodes[field_idx0];

                                // Lower the field type
                                if let Ok(field_ty_tid) = paideia_as_elaborator::lower_type::lower_type_ast(
                                    &arena,
                                    &source_map,
                                    field_ty_node,
                                    &mut types,
                                    &mut effects,
                                    &mut caps,
                                    &registry,
                                ) {
                                    // Check if the field type is a function-pointer
                                    if matches!(types.get(field_ty_tid), paideia_as_types::Type::Fn { .. }) {
                                        // Look up the lambda via symbol table
                                        if let Some(symbol) = lowering.ir.symbols().lookup_by_name(&var_name) {
                                            // Convert IR node ID to AST node ID
                                            let lambda_ast_id = AstNodeId::new(symbol.ir_node.get()).unwrap();

                                            // Derive the RHS signature from the lambda
                                            if let Some(rhs_tid) = paideia_as_elaborator::derive_fn_sig::derive_fn_sig_from_lambda(
                                                &arena,
                                                &source_map,
                                                lambda_ast_id,
                                                &mut types,
                                                &mut effects,
                                                &mut caps,
                                                &registry,
                                            ) {
                                                // Check fn-ptr assignment compatibility
                                                let mut subst = Subst::new();
                                                let span = lowering.ir.get(rhs_id).map(|n| n.span).unwrap_or_else(|| {
                                                    paideia_as_diagnostics::Span::new(
                                                        paideia_as_diagnostics::FileId::new(1).unwrap(),
                                                        0,
                                                        0,
                                                    )
                                                });
                                                let diags = paideia_as_elaborator::check_fn_ptr_assignment(
                                                    &mut types,
                                                    &mut subst,
                                                    &effects,
                                                    &caps,
                                                    field_ty_tid,
                                                    rhs_tid,
                                                    span,
                                                );
                                                // Push diagnostics to sink
                                                for diag in diags {
                                                    let _ = sink.emit(diag);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Phase-5-m4-003: Populate data side-table for module-level data bindings.
    // This must run after walker passes and before emit format selection.
    // PA10-007 m1-001: Use actual binding names for data symbols instead of "data_<id>".
    if !lowering.ir.is_empty() {
        // Due to Rust borrowing rules, we need to collect the arena state before
        // calling data_mut(). We'll use a temporary struct to hold the necessary data.
        let arena_len = lowering.ir.len();
        let mut data_entries = Vec::new();

        // First pass: collect data entries (using only immutable borrows).
        for i in 1..=arena_len as u32 {
            if let Some(node_id) = IrNodeId::new(i) {
                if let Some(node) = lowering.ir.get(node_id) {
                    if node.kind == paideia_as_ir::IrKind::Let {
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

                                    // PA10-006s: Determine element byte width from AST type
                                    let element_width = array_element_byte_width(
                                        node_id,
                                        &arena,
                                        &source_map,
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
                                            }
                                        }
                                    }

                                    if element_count > 0 {
                                        let let_info = lowering.ir.let_meta().get(node_id);
                                        let explicit_align = let_info.and_then(|i| i.align);
                                        let is_mutable = let_info
                                            .map(|info| info.mutable).unwrap_or(false);
                                        let link_section = let_info.and_then(|i| i.link_section.clone());
                                        let entry = if is_mutable {
                                            let mut e = paideia_as_ir::DataEntry::new_data(
                                                packed_bytes,
                                                symbol_name,
                                                explicit_align.unwrap_or(8),
                                            );
                                            if let Some(name) = link_section {
                                                e = e.with_section_override(name);
                                            }
                                            e
                                        } else {
                                            let mut e = paideia_as_ir::DataEntry::new_rodata(
                                                packed_bytes,
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
                                } else if rhs_node.kind == paideia_as_ir::IrKind::Placeholder {
                                    // Phase 6 m5-005: Let with Placeholder (uninit) → Bss
                                    // Route all uninit to .bss regardless of mutability.
                                    // Phase 6 m5-005: Compute size from array type annotation if present.
                                    let size = compute_bss_size_from_type(
                                        node_id,
                                        &arena,
                                        &source_map,
                                        file,
                                    );
                                    let let_info = lowering.ir.let_meta().get(node_id);
                                    let explicit_align = let_info.and_then(|i| i.align);
                                    let link_section = let_info.and_then(|i| i.link_section.clone());
                                    let mut entry =
                                        paideia_as_ir::DataEntry::new_bss(symbol_name, explicit_align.unwrap_or(8), size);
                                    if let Some(name) = link_section {
                                        entry = entry.with_section_override(name);
                                    }
                                    data_entries.push((node_id, entry));
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
                                            node_id, &arena, &source_map, file,
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
                                    // #988 v2: RecordCons emit with fnptr relocation support.
                                    // Look up layout from record_layout_table, walk fields, and emit relocations.
                                    let record_type_id = lowering.ir.record_layout_table().get(rhs_id);
                                    if let Some(_type_id) = record_type_id {
                                        // For now, we don't have access to finalised_layout_table here (it's emitter-only),
                                        // so we'll use the IR structure directly: walk field children, check their kind,
                                        // and emit relocations for Borrow fields.
                                        let field_children: Vec<_> = lowering.ir.children(rhs_id).iter().copied().collect();
                                        let mut bytes = Vec::new();
                                        let mut relocs = Vec::new();

                                        // Skip type_name at index 0, process field children
                                        for (field_idx, &field_id) in field_children.iter().enumerate() {
                                            if field_idx == 0 {
                                                // Skip type_name
                                                continue;
                                            }

                                            if let Some(field_node) = lowering.ir.get(field_id) {
                                                if field_node.kind == paideia_as_ir::IrKind::Literal {
                                                    // Literal field: pack as 8 bytes (MVP assumes all fields are u64)
                                                    if let Some(value) = lowering.ir.literal_values().get(field_id) {
                                                        let field_bytes = paideia_as_elaborator::data_encoder::pack_u64_le(value);
                                                        bytes.extend(field_bytes);
                                                    }
                                                } else if field_node.kind == paideia_as_ir::IrKind::Borrow {
                                                    // Borrow field: emit 8 zero bytes + relocation
                                                    let offset = bytes.len() as u64;
                                                    bytes.extend_from_slice(&[0u8; 8]);

                                                    // Look up the Borrow in AddrOfSideTable (keyed by Borrow node id)
                                                    if let Some(meta) = lowering.ir.addr_of().get(field_id) {
                                                        let reloc = paideia_as_ir::RelocSpec::with_width(
                                                            offset,
                                                            meta.symbol.clone(),
                                                            paideia_as_ir::RelocWidth::W64,
                                                            meta.addend,
                                                        );
                                                        relocs.push(reloc);
                                                    }
                                                } else {
                                                    // Non-Literal, non-Borrow field: emit T0536-style diagnostic
                                                    let diag = Diagnostic::error(
                                                        DiagnosticCode::new(
                                                            Category::T,
                                                            Severity::Error,
                                                            536,
                                                        ).expect("T0536 is valid")
                                                    )
                                                    .message("record field must be a literal or function pointer")
                                                    .with_span(field_node.span)
                                                    .finish();
                                                    let _ = sink.emit(diag);
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
                                            node_id, &arena, &source_map, file,
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

    // PA-r15-009b (#1032): Populate jump tables after data table population.
    // This synthesizes rodata entries for @jump_table dense match dispatches.
    // Call this after the is_empty block to get mutable access to the arena.
    if !lowering.ir.is_empty() {
        EmitWalker::populate_jump_tables_from_arena(&mut lowering.ir);
    }

    let preview = sink
        .diagnostics()
        .iter()
        .any(|d| d.severity() == Severity::Error);

    match format {
        EmitFormat::Placeholder => {
            let to_write = if preview {
                None
            } else {
                Some(placeholder_for(&lowering.ir))
            };
            finish_placeholder(&source_map, catalog, sink, to_write, input, output, sarif)
        }
        EmitFormat::Elf64 => {
            let result = if preview {
                Ok(None)
            } else {
                build_elf_object(
                    &lowering.ir,
                    &mut instruction_table,
                    &emit_walker,
                    &source_map,
                    file,
                    encoder_warn,
                    &mut sink,
                )
                .map(Some)
            };
            match result {
                Ok(bytes) => finish_elf(&source_map, catalog, sink, bytes, input, output, sarif),
                Err(build_err) => finish_build_error(&source_map, catalog, sink, build_err, input, sarif),
            }
        }
        EmitFormat::Pax => {
            let bytes = if preview {
                None
            } else {
                Some(build_pax_object())
            };
            finish_pax(&source_map, catalog, sink, bytes, input, output, sarif)
        }
        EmitFormat::PeCoff => {
            let result = if preview {
                Ok(None)
            } else {
                build_pe_object(&mut lowering.ir, &source_map, file, encoder_warn).map(Some)
            };
            match result {
                Ok(bytes) => finish_pe(&source_map, catalog, sink, bytes, input, output, sarif),
                Err(build_err) => finish_build_error(&source_map, catalog, sink, build_err, input, sarif),
            }
        }
    }
}

/// Bridge: convert IR module metadata to PAX functors section.
///
/// Iterates over modules in the table; for each with a functor binding,
/// emit a FunctorEntry with hashes from the signature.
///
/// # Arguments
///
/// * `_table` - The IR module side-table.
/// * `_symbol_resolver` - Closure mapping IrNodeId → symbol_id (u64).
///
/// # Returns
///
/// A FunctorsSection ready for serialization. Closure data and flags
/// are placeholders (0) in phase-1; m5-012+ will populate them.
#[allow(dead_code)]
pub fn functors_from_modules(
    table: &ModuleSideTable,
    symbol_resolver: impl Fn(IrNodeId) -> u64,
) -> FunctorsSection {
    use paideia_as_emitter_pax::FunctorEntry;

    let mut section = FunctorsSection::new();
    for (id, info) in table.iter() {
        if let Some(fi) = &info.functor {
            section.push(FunctorEntry {
                functor_symbol_id: symbol_resolver(*id),
                param_signature_hash: fi.param_signature_hash,
                result_signature_hash: fi.result_signature_hash,
                closure_data_offset: 0,
                closure_data_size: 0,
                flags: 0,
            });
        }
    }
    section
}
