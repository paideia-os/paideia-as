//! `paideia-as build` — phase-1 placeholder backend.
//!
//! Issue #1110 (F15): Output selection is now required — callers must specify
//! either `--target <triplet>` or `--emit <format>` (including `--emit placeholder`
//! for the old default behavior). The clap ArgGroup enforces this at parse time,
//! so the `(None, None)` case is unreachable.
//!
//! The pipeline runs lex → parse → lower → <emit>, where <emit> is determined
//! by the selected format (placeholder, elf64, pax, or pe-coff). The real ELF/PAX/PE
//! emitters arrive at deliverable 8; Phase-1 placeholder remains fully available
//! under `--emit placeholder` for backward compatibility.
//!
//! # Internal structure
//!
//! Refactored 2026-07-08 from a single 3261-line file. The `run()`
//! orchestrator remains here (with `BuildError`, `EmitFormat`, and
//! `functors_from_modules`); every extracted helper is `pub(super)`-scoped
//! and lives under `cmd_build/`. Nothing new is public.
//!
//! Further refactored 2026-09-07 (issue #1401): the phase bodies of `run()`
//! moved to sibling modules — see `populate`, `walker_pipeline`,
//! `resolve_names`, `validate`, `addr_of_pass`, and `data_pass`.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use crate::cli::Target;
use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{
    Catalog, DiagnosticSink, Severity, SourceMap, VecSink,
};
use paideia_as_elaborator::{
    EmitWalker, build_struct_registry, build_enum_registry, finalise_enum_layouts,
    finalise_enum_variant_payloads, lower_ast_to_ir, placeholder_for,
    validate_file_module_mapping,
};
use paideia_as_emitter_pax::FunctorsSection;
use paideia_as_types::{TypeInterner, CapSetInterner};
use paideia_as_effects::EffectInterner;
use paideia_as_ir::{IrNodeId, ModuleSideTable};
use paideia_as_ir::opt::OptDiagSink;
use paideia_as_ir::opt::dispatch;
use paideia_as_lexer::{Lexer, SourceText};
use paideia_as_parser::Parser;

// --- Internal submodules ---
mod addr_of;
mod addr_of_pass;
mod data_pass;
mod diagnostics;
mod elf;
mod fixup;
mod identifier;
mod layout;
mod pax;
mod pe;
mod placeholder;
mod populate;
mod resolve_names;
mod root_attrs;
mod validate;
mod walker_pipeline;

#[cfg(test)]
mod tests;

use diagnostics::finish_build_error;
use elf::{build_elf_object, finish_elf};
use pax::{build_pax_object, finish_pax};
use pe::{build_pe_object, finish_pe};
use placeholder::finish_placeholder;

/// Error type for build operations.
/// Phase 8 m1-004: Collapsed to a marker enum — all diagnostics are emitted
/// through DiagnosticSink at the error site (encoder, emitter, fixup).
/// finish_build_error only needs to know that a build failed to return exit code 2.
#[derive(Debug, Clone)]
pub enum BuildError {
    /// Build failed (diagnostics already emitted through sink).
    Failed,
}

/// Output format selector for `paideia-as build --emit`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum EmitFormat {
    /// Front-end smoke artifact: write a `<stem>.placeholder` hash next to input.
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
        (None, None) => unreachable!("clap ArgGroup(required=true) should enforce output_mode"),
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
    let lex_errored = lex_sink.error_count() > 0;
    for d in lex_sink.into_diagnostics() {
        let _ = sink.emit(d);
    }

    // Parse.
    let mut arena = AstArena::new();
    let root_id;
    let parse_errored;
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
        parse_errored = parser_sink.error_count() > 0;
        for d in parser_sink.into_diagnostics() {
            let _ = sink.emit(d);
        }
    }

    // Validate file-to-module mapping (after parse, before lower).
    // #1264: skip when the lexer or parser already errored — M0305 runs on
    // a broken tree and produces cascade noise that hides the real error.
    if let Some(root) = root_id
        && !parse_errored
        && !lex_errored
    {
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

    // paideia-as#1307 (unblocks paideia-os#1024 R29.M2-002): AST-level
    // effect-row → cap-set coupling check. For every `let` binding
    // whose annotated type carries an effect row, verify that any
    // effect listed in the row whose static binding (see
    // paideia_as_effects::effect_cap_binding) requires a capability
    // has that capability declared in the same signature's `@{...}`.
    // Emits C1301 per (function, missing cap) pair. Runs on the raw
    // AST since it is purely syntactic; no IR lowering required.
    // Skipped when the parser errored, mirroring the file_module
    // validation gate — a broken tree yields cascade noise.
    if let Some(root) = root_id
        && !parse_errored
        && !lex_errored
    {
        for d in paideia_as_elaborator::check_effect_cap_coupling(&arena, &source_map, root) {
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

    // Issue #1219: Construct interners for populating LetInfo::ty
    let mut types = TypeInterner::new();
    let mut effects = EffectInterner::new();
    let mut caps = CapSetInterner::new();

    // Issue #1219: Populate LetInfo::ty from explicit type annotations on Let bindings
    // Issue #1222: Also populate LetInfo::enum_type_id for enum-typed bindings
    paideia_as_elaborator::populate_let_meta_ty(
        &arena,
        &mut lowering.ir,
        &lowering.ast_to_ir,
        &source_map,
        &mut types,
        &mut effects,
        &mut caps,
        &registry,
        &enum_registry,
    );

    // Issue #1156: Populate lambda_param_enum_types for enum-typed lambda parameters.
    // This enables register_nested_lambda_params to detect enum-typed pos-0 parameters
    // and install (RAX, RDX) pair bindings instead of scalar RDI binding.
    paideia_as_elaborator::populate_lambda_param_enum_types(
        &arena,
        &mut lowering.ir,
        &lowering.ast_to_ir,
        &source_map,
        &enum_registry,
    );

    // PA-r17-007 (#1050): Populate enum layouts from the enum registry.
    // This enables emit_walker to look up enum layouts during EnumCons and EnumDiscriminant lowering.
    // Issue #1090: Also thread StructRegistry for struct-typed variant payloads fallback.
    // Issue #1160: Also extract primitive payload widths for tight-pack encoding.
    let (enum_layouts, enum_primitive_widths) = finalise_enum_layouts(&enum_registry, &registry, &arena, &source_map, &mut sink);
    for (type_id, layout) in enum_layouts {
        lowering.ir.enum_layout_table_mut().insert(type_id, layout);
    }

    // Issue #1160: Populate the enum_variant_primitive_widths table.
    for ((enum_id, variant_idx), width) in enum_primitive_widths {
        lowering.ir.enum_variant_primitive_widths_mut()
            .insert(enum_id, variant_idx, width);
    }

    // Populate the enum_variant_payload_table from the computed payload_map.
    // Single source of truth pattern: payload_map computed once above,
    // used both for populate_match_arm_meta during lowering and here for the IR side-table.
    for ((enum_id, variant_idx), payload) in payload_map {
        lowering.ir.enum_variant_payload_table_mut()
            .insert(enum_id, variant_idx, payload);
    }

    // AST → IR side-table population passes.
    populate::populate_literal_values(&arena, &source_map, file, &mut lowering);
    populate::populate_module_let_names(&arena, &source_map, file, &mut lowering);
    populate::populate_stmt_let_names(&arena, &source_map, file, &mut lowering);
    populate::populate_public_lets(&arena, &mut lowering);
    populate::populate_lambda_params(&arena, &source_map, file, &mut lowering);
    populate::populate_var_binding_names(&arena, &source_map, file, &mut lowering);
    populate::populate_literal_bytes(&arena, &source_map, file, &mut lowering);

    // Run walkers over the IR to surface S/F/C diagnostics.
    // Walkers now traverse the real IR subtree; diagnostic firing remains gated
    // by payload injection (m3/m5). See issue #1237 for root-walker fix and
    // design/paideia-as/non-milestone-issue-1237-cmd-build-root-walker.md.
    // Phase-5-m1-005: EmitWalker chains into the walker pipeline and populates
    // InstructionSideTable for downstream emit stages.
    let mut emit_walker = EmitWalker::new();
    let mut instruction_table: paideia_as_ir::InstructionSideTable;

    walker_pipeline::run_walker_pipeline(
        &arena,
        root_id,
        &source_map,
        file,
        &registry,
        &mut lowering,
        &mut emit_walker,
        &mut sink,
    );

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

    // Phase-5-m6-005 / B3-004 / PA19-r19-010: symbol name resolution + let_meta seeding.
    resolve_names::resolve_symbol_names_and_let_meta(&arena, &source_map, file, &mut lowering);

    // Late attribute-validation passes: P0284 (@link_section on lambda),
    // P0286 (@abi on non-lambda), U1620 anchor.
    validate::validate_link_section_on_non_lambda(&arena, &mut sink);
    validate::validate_abi_on_lambda(&arena, &mut sink);
    validate::validate_ms_x64_emit_anchor(&arena, &mut sink);

    // PA-R17-003 / #988 / #1310 / #1074: address-of pre-emit + T0535 checks.
    addr_of_pass::run_addr_of_pass(
        &arena,
        &source_map,
        file,
        &registry,
        &enum_registry,
        &mut types,
        &mut effects,
        &mut caps,
        &mut lowering,
        &mut sink,
    );

    // Phase-5-m4-003 / Phase 14 PA14-r14-008: data table + ring buffer synthesis.
    data_pass::populate_data_table(&arena, &source_map, file, &mut lowering, &mut sink);

    // PA-r15-009b (#1032): populate jump tables after data table population.
    data_pass::populate_jump_tables(&mut lowering);

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
                build_pe_object(&mut lowering.ir, &emit_walker, &source_map, file, encoder_warn, &mut sink).map(Some)
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
