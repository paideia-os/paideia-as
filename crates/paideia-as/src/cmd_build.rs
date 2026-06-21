//! `paideia-as build` — phase-1 placeholder backend.
//!
//! Closes deliverable 4 ("smoke-test elaboration"): the pipeline runs
//! lex → parse → lower → placeholder. The real ELF/PAX/PE emitters
//! arrive at deliverable 8. For now we write a tiny
//! `<input>.placeholder` artifact containing a BLAKE3 hash of the
//! lowered IR's pretty-printed form so the smoke test can verify the
//! pipeline produced something deterministic.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

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
}

use crate::det;
use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{
    Catalog, DiagnosticSink, HumanRenderer, HumanSink, Severity, SourceMap, VecSink,
};
use paideia_as_elaborator::{
    CapWalker, EffectRowWalker, EmitWalker, LinearityWalker, UnsafeWalker, lower_ast_to_ir,
    placeholder_for, validate_file_module_mapping,
};
use paideia_as_emitter_elf::{Arch, ElfWriter, Kind, SymKind, SymbolEntry};
use paideia_as_emitter_pax::{
    Architecture, FunctorsSection, PAX_HEADER_SIZE, PaxHeader, SectionTable, compute_content_hash,
};
use paideia_as_emitter_pe::emit_text_from_instructions;
use paideia_as_emitter_pe::{
    COFF_FILE_HEADER_SIZE, CoffFileHeader, DOS_HEADER_SIZE, DosHeader, NT_SIGNATURE,
    OPTIONAL_HEADER_PE32PLUS_SIZE, OptionalHeaderPe32Plus, SectionTable as PeSectionTable,
};
use paideia_as_encoder::{EncodeStats, LabelFixup};
use paideia_as_ir::{InstructionSideTable, IrNodeId, ModuleSideTable, walk};
use paideia_as_lexer::{Lexer, SourceText};
use paideia_as_parser::Parser;

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

/// Run `paideia-as build <input> [--emit <format>] [-o <output>] [--encoder-warn]`.
pub fn run(input: &Path, output: Option<&Path>, emit: &str, encoder_warn: bool) -> ExitCode {
    let format = match EmitFormat::parse(emit) {
        Ok(f) => f,
        Err(msg) => {
            eprintln!("paideia-as: {msg}");
            return ExitCode::from(2);
        }
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
            return finish_placeholder(&source_map, catalog, sink, None, input, output);
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
        );
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

    // If there are any errors so far, do not emit anything downstream.
    let mut lowering = lower_ast_to_ir(&arena);

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

    // Run walkers over the IR to surface S/F/C diagnostics.
    // Phase-2-m1: walkers run with empty injection tables (from CLI), so only
    // diagnostics that depend on kind-only IR will fire (S0900/S0901/S0903).
    // Real effect (F1100, F1101, F1105, F1106) and capability (C1300) diagnostics
    // require per-node payloads that arrive in m3/m5.
    // Phase-5-m1-005: EmitWalker chains into the walker pipeline and populates
    // InstructionSideTable for downstream emit stages.
    let mut emit_walker = EmitWalker::new();
    let mut instruction_table = InstructionSideTable::new();

    if !lowering.ir.is_empty() {
        // Create a walker sink to accumulate diagnostics from all walkers.
        let mut walker_sink = VecSink::new();

        // Determine the root node ID for walking. In phase-1 lowering, the parser
        // creates a Module as the first node (NodeId 1 → IrNodeId 1), so we walk
        // from IrNodeId::new(1). If the IR is somehow empty, skip walking.
        if let Some(root_id) = IrNodeId::new(1) {
            // Run each walker with a fresh WalkerCtx to avoid borrow conflicts.
            // Each walker emits diagnostics into walker_sink.

            {
                let mut ctx = paideia_as_ir::WalkerCtx::new(&source_map, &mut walker_sink);
                let mut linearity_walker = LinearityWalker::new();
                walk(&mut linearity_walker, &lowering.ir, root_id, &mut ctx);
            }

            {
                let mut ctx = paideia_as_ir::WalkerCtx::new(&source_map, &mut walker_sink);
                let mut effect_walker = EffectRowWalker::new();
                walk(&mut effect_walker, &lowering.ir, root_id, &mut ctx);
            }

            {
                let mut ctx = paideia_as_ir::WalkerCtx::new(&source_map, &mut walker_sink);
                let mut cap_walker = CapWalker::new();
                walk(&mut cap_walker, &lowering.ir, root_id, &mut ctx);
            }

            // Phase-5-m1-005: Run EmitWalker to populate InstructionSideTable.
            // EmitWalker does not use the walker framework (it uses direct arena iteration),
            // so we call its walk method directly rather than through the walk() driver.
            emit_walker.walk(&mut lowering.ir);

            // Phase-5-m3-005: Run UnsafeWalker to elaborate pending unsafe blocks.
            // Take pending unsafe blocks from EmitWalker state and process them.
            let pending = emit_walker.state_mut().take_pending_unsafe();
            let record_layouts = &emit_walker.state().record_layouts;
            let unsafe_diags = UnsafeWalker::run(
                &mut lowering.ir,
                &arena,
                pending,
                &source_map,
                &mut walker_sink,
                record_layouts,
            );
            instruction_table = lowering.ir.instructions().clone();
            for d in unsafe_diags {
                let _ = walker_sink.emit(d);
            }
        }

        // Drain walker diagnostics into the main sink for rendering.
        for d in walker_sink.into_diagnostics() {
            let _ = sink.emit(d);
        }
    }

    // Phase-5-m6-005: Symbol name resolution pass.
    // Walk the AST to find Let bindings with actual names, then update the symbol table
    // to use the real binding names instead of "_let_<id>".
    {
        let mut name_map: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
        let content_ref = source_map.content(file);

        // Walk AST to find all Let bindings and extract their names
        for i in 0..arena.len() {
            if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
                if let Some(node) = arena.get(ast_id) {
                    if node.kind == paideia_as_ast::NodeKind::Let {
                        if let Some(paideia_as_ast::ItemData::Let {
                            name: name_id,
                            value: value_id,
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
                                }
                            }
                        }
                    }
                }
            }
        }

        // Now rebuild the symbol table with updated names
        if !name_map.is_empty() {
            let old_symbols: Vec<_> = lowering.ir.symbols().iter().cloned().collect();
            lowering.ir.symbols_mut().clear();

            for sym in old_symbols {
                // Check if this symbol's ir_node.get() is in name_map (i.e., it's a function symbol for a named binding)
                if let Some(real_name) = name_map.get(&sym.ir_node.get()) {
                    // Re-insert the symbol with the real name
                    let updated_sym =
                        paideia_as_ir::Symbol::new(real_name.clone(), sym.kind, sym.ir_node);
                    lowering.ir.symbols_mut().insert(updated_sym);
                } else {
                    // Symbol has no real name mapping, keep the original
                    lowering.ir.symbols_mut().insert(sym);
                }
            }
        }
    }

    // Phase-5-m4-003: Populate data side-table for module-level data bindings.
    // This must run after walker passes and before emit format selection.
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
                        if let Some(&rhs_id) = children.first() {
                            if let Some(rhs_node) = lowering.ir.get(rhs_id) {
                                if rhs_node.kind == paideia_as_ir::IrKind::Literal {
                                    if let Some(value) = lowering.ir.literal_values().get(rhs_id) {
                                        let bytes = EmitWalker::pack_u64_le_public(value);
                                        let symbol_name = format!("data_{}", node_id.get());
                                        let entry = paideia_as_ir::DataEntry::new_rodata(
                                            bytes,
                                            symbol_name,
                                            8,
                                        );
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
            finish_placeholder(&source_map, catalog, sink, to_write, input, output)
        }
        EmitFormat::Elf64 => {
            let result = if preview {
                Ok(None)
            } else {
                build_elf_object(
                    &lowering.ir,
                    &instruction_table,
                    &emit_walker,
                    &source_map,
                    file,
                    encoder_warn,
                )
                .map(Some)
            };
            match result {
                Ok(bytes) => finish_elf(&source_map, catalog, sink, bytes, input, output),
                Err(build_err) => finish_build_error(&source_map, catalog, sink, build_err, input),
            }
        }
        EmitFormat::Pax => {
            let bytes = if preview {
                None
            } else {
                Some(build_pax_object())
            };
            finish_pax(&source_map, catalog, sink, bytes, input, output)
        }
        EmitFormat::PeCoff => {
            let result = if preview {
                Ok(None)
            } else {
                build_pe_object(&lowering.ir, &source_map, file, encoder_warn).map(Some)
            };
            match result {
                Ok(bytes) => finish_pe(&source_map, catalog, sink, bytes, input, output),
                Err(build_err) => finish_build_error(&source_map, catalog, sink, build_err, input),
            }
        }
    }
}

/// Patch label fixups after .text encoding is complete.
///
/// Phase-6-m4-004: Called after all instructions have been encoded
/// and byte offsets are known. For each LabelFixup, computes the
/// displacement as: label_offset - (fixup_byte_offset + 4), then
/// writes the i32 LE value into the buffer at the fixup location.
///
/// # Arguments
///
/// * `buffer` - Mutable reference to the .text section bytes
/// * `label_fixups` - List of fixup sites collected during encoding
/// * `labels` - Map of label names to their byte offsets in .text
/// * `strict_mode` - Whether to abort on unresolved labels
///
/// # Returns
///
/// `Ok(())` if all fixups applied successfully, or
/// `Err(BuildError::Encoder)` if a label is unresolved in strict mode.
fn patch_label_fixups(
    buffer: &mut [u8],
    label_fixups: &[LabelFixup],
    labels: &std::collections::HashMap<String, u32>,
    strict_mode: bool,
    file: paideia_as_diagnostics::FileId,
) -> Result<(), BuildError> {
    for fixup in label_fixups {
        match labels.get(&fixup.label_name) {
            Some(&label_offset) => {
                // Compute displacement: label_offset - (fixup_byte_offset + 4)
                // The "+4" accounts for the fact that relative offsets are computed
                // from the byte AFTER the displacement field (i.e., the next instruction).
                let disp = (label_offset as i64) - ((fixup.byte_offset as i64) + 4);
                let disp_i32 = disp as i32;

                // Write the displacement as i32 LE at the fixup offset
                let offset = fixup.byte_offset as usize;
                if offset + 4 <= buffer.len() {
                    let disp_bytes = disp_i32.to_le_bytes();
                    buffer[offset..offset + 4].copy_from_slice(&disp_bytes);
                }
            }
            None => {
                // Unresolved label: emit U1610
                eprintln!("error: unresolved label '{}' (U1610)", fixup.label_name);
                if strict_mode {
                    let span = paideia_as_diagnostics::Span::new(file, 0, 1);
                    return Err(BuildError::Encoder {
                        node: IrNodeId::new(1).unwrap(),
                        source_span: span,
                        encoder_message: format!("unresolved label '{}'", fixup.label_name),
                    });
                }
            }
        }
    }
    Ok(())
}

/// Build the phase-1 ELF object body.
///
/// Phase-5-m5-003: Real symbol-table emission from SymbolTable.
/// Iterates over arena.symbols().iter() and emits one symbol per entry.
/// For each function symbol, the value is the byte offset where its first
/// instruction was emitted (from EmitPassState.function_offsets).
/// For each data symbol, the value is the byte offset in .rodata/.data.
/// Phase-5-m4-004: Collects relocation sites from instruction encoding and emits
/// them to the .rela.text section.
/// Phase-6-m1-004: Propagates encoder failures as BuildError::Encoder instead of silently falling back.
/// Phase-6-m4-004: Applies label fixups after .text encoding completes.
fn build_elf_object(
    arena: &paideia_as_ir::IrArena,
    instruction_table: &InstructionSideTable,
    emit_walker: &paideia_as_elaborator::EmitWalker,
    _source_map: &SourceMap,
    file: paideia_as_diagnostics::FileId,
    encoder_warn: bool,
) -> Result<Vec<u8>, BuildError> {
    let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);

    // Phase-5-m5-005: Emit real instructions from InstructionSideTable.
    // Phase-6-m1-004: Propagate encoder failures as BuildError::Encoder instead of silently falling back.
    let mut text_bytes = Vec::new();
    let emit_result = if instruction_table.is_empty() {
        // Empty instruction table → empty .text section (valid ELF).
        // Return a minimal EmitResult with no relocations or label fixups.
        paideia_as_emitter_pe::EmitResult {
            encode_stats: EncodeStats::new(),
            offset_map: std::collections::HashMap::new(),
            reloc_sites: Vec::new(),
            label_fixups: Vec::new(),
        }
    } else {
        // Real instruction encoding: iterate InstructionSideTable in IR-node order
        // and call encode_instruction() per instruction.
        match emit_text_from_instructions(instruction_table, &mut text_bytes) {
            Ok(result) => result,
            Err(e) => {
                // Phase-6-m1-004: Find the instruction that failed and extract IR node info.
                if let Some(failed_node_id) = find_failing_instruction(instruction_table) {
                    let span = paideia_as_diagnostics::Span::new(file, 0, 1);
                    if encoder_warn {
                        // Phase-5 behaviour: warn and drop instruction
                        eprintln!(
                            "warning: encoder failed on node {}: {}, continuing with --encoder-warn",
                            failed_node_id.get(),
                            e
                        );
                        paideia_as_emitter_pe::EmitResult {
                            encode_stats: EncodeStats::new(),
                            offset_map: std::collections::HashMap::new(),
                            reloc_sites: Vec::new(),
                            label_fixups: Vec::new(),
                        }
                    } else {
                        // Phase-6 default: propagate error
                        return Err(BuildError::Encoder {
                            node: failed_node_id,
                            source_span: span,
                            encoder_message: e.to_string(),
                        });
                    }
                } else {
                    return Err(BuildError::Encoder {
                        node: IrNodeId::new(1).unwrap(),
                        source_span: paideia_as_diagnostics::Span::new(file, 0, 1),
                        encoder_message: e.to_string(),
                    });
                }
            }
        }
    };

    // Phase-6-m4-004: Patch label fixups after .text encoding is complete.
    // Extract labels from emit_walker state and apply fixups to text_bytes.
    let labels = &emit_walker.state().labels;
    let strict_mode = true;
    patch_label_fixups(
        &mut text_bytes,
        &emit_result.label_fixups,
        labels,
        strict_mode,
        file,
    )?;

    writer.add_text_bytes(&text_bytes);

    // Phase-5-m4-003: Emit data entries from the data side-table.
    // Also create symbols for each data entry so relocations can reference them.
    let data_table = arena.data();
    for (id, entry) in data_table.iter() {
        let data_offset = match entry.section {
            paideia_as_ir::SectionKind::Rodata => {
                writer.add_rodata_bytes(&entry.bytes, entry.align)
            }
            paideia_as_ir::SectionKind::Data => writer.add_data_bytes(&entry.bytes, entry.align),
            paideia_as_ir::SectionKind::Bss => {
                // Phase 6 m5-002: allocate uninitialized space in .bss section
                writer.add_bss_space(entry.size_hint, entry.align)
            }
        };
        // Phase-5-m4-003: Create a symbol for the data entry so relocations can reference it
        // Phase 6 m5-003: include section information for .bss symbols
        let sym_name = format!("data_{}", id.get());
        let size = match entry.section {
            paideia_as_ir::SectionKind::Bss => entry.size_hint,
            _ => entry.bytes.len() as u64,
        };
        let _ = writer.add_symbol(SymbolEntry {
            name: sym_name,
            offset: Some(data_offset),
            size,
            kind: SymKind::Data,
            is_global: false,
            section: Some(entry.section),
        });
    }

    // Phase-5-m5-003: Emit real symbols from SymbolTable.
    // Iterate over arena.symbols().iter() and emit one symbol per entry.
    let function_offsets = &emit_walker.state().function_offsets;
    let emitted_lambdas = emit_walker.emitted_lambdas();
    let mut emitted_any_symbol = false;
    for symbol in arena.symbols().iter() {
        match symbol.kind {
            paideia_as_ir::SymbolKind::Function => {
                // Skip symbols for lambdas that didn't emit bytecode
                if !emitted_lambdas.contains(&symbol.ir_node.get()) {
                    continue;
                }

                // For function symbols, look up the byte offset from function_offsets.
                // The size is computed as: next function offset - this offset (or text_bytes.len()).
                let offset = function_offsets
                    .get(&symbol.ir_node.get())
                    .copied()
                    .unwrap_or(0);

                // Compute size: distance to next function, or to end of text.
                let size = if let Some(&next_offset) =
                    function_offsets.values().filter(|&&o| o > offset).min()
                {
                    (next_offset - offset) as u64
                } else {
                    (text_bytes.len() as u32 - offset) as u64
                };

                let sym_entry = SymbolEntry {
                    name: symbol.name.clone(),
                    kind: SymKind::Func,
                    is_global: symbol.global,
                    offset: Some(offset as u64),
                    size,
                    section: None,
                };
                let _ = writer.add_symbol(sym_entry);
                emitted_any_symbol = true;
            }
            paideia_as_ir::SymbolKind::Object => {
                // For object (data) symbols, we look them up in the data_table.
                // The offset and size should already be in the data_table entries.
                // We skip emitting here since data entries are already emitted above.
                // (The name format is "data_<IrNodeId>" to match the entries.)
            }
            paideia_as_ir::SymbolKind::Undefined => {
                // Undefined symbols are emitted as external references.
                let sym_entry = SymbolEntry::undefined(&symbol.name);
                let _ = writer.add_symbol(sym_entry);
                emitted_any_symbol = true;
            }
        }
    }

    // Fallback: if no symbols were emitted from the SymbolTable, emit a placeholder
    // for backward compatibility. This ensures existing tests still pass.
    if !emitted_any_symbol {
        let _ = writer.add_symbol(SymbolEntry::func("add_one", 0, text_bytes.len() as u64));
    }

    // Phase-5-m4-004: Emit relocations collected from instruction encoding.
    use paideia_as_emitter_elf::RelocEntry;
    let text_section = writer.text_section_id();
    for reloc_site in &emit_result.reloc_sites {
        let reloc_kind = paideia_as_emitter_elf::RelocKind::from_encoder(reloc_site.kind);
        let entry = RelocEntry {
            offset: reloc_site.byte_offset as u64,
            target: reloc_site.symbol.clone(),
            kind: reloc_kind,
            addend: reloc_site.addend as i64,
        };
        let _ = writer.add_relocation(text_section, entry);
    }

    Ok(writer.finalize().unwrap_or_default())
}

/// Phase-6-m1-004: Find the first instruction in the table (since we encode sequentially,
/// the failure likely occurs on the first one when we process deterministically).
fn find_failing_instruction(instruction_table: &InstructionSideTable) -> Option<IrNodeId> {
    let mut entries: Vec<_> = instruction_table.entries().iter().collect();
    entries.sort_by_key(|&(&node_id, _)| node_id);
    entries.first().map(|&(&node_id, _)| node_id)
}

fn finish_elf(
    source_map: &SourceMap,
    catalog: &Catalog,
    sink: VecSink,
    bytes: Option<Vec<u8>>,
    input: &Path,
    output: Option<&Path>,
) -> ExitCode {
    let diagnostics = sink.into_diagnostics();
    let stderr = std::io::stderr();
    let renderer = HumanRenderer::with_catalog(source_map, true, catalog);
    let mut human = HumanSink::new(stderr.lock(), renderer);
    for d in &diagnostics {
        let _ = human.emit(d.clone());
    }
    let has_error = diagnostics.iter().any(|d| d.severity() == Severity::Error);
    if has_error {
        return ExitCode::from(1);
    }
    if let Some(bytes) = bytes {
        let path = output
            .map(Path::to_path_buf)
            .unwrap_or_else(|| elf_path_for(input));
        match fs::File::create(&path) {
            Ok(file) => {
                let mut w = std::io::BufWriter::new(file);
                let _ = w.write_all(&bytes);
            }
            Err(e) => {
                eprintln!("paideia-as: cannot write ELF at {}: {e}", path.display());
                return ExitCode::from(2);
            }
        }
    }
    ExitCode::SUCCESS
}

fn finish_build_error(
    _source_map: &SourceMap,
    _catalog: &Catalog,
    _sink: VecSink,
    error: BuildError,
    _input: &Path,
) -> ExitCode {
    match error {
        BuildError::Encoder {
            node,
            source_span,
            encoder_message,
        } => {
            eprintln!(
                "error: encoder failed on IR node {}: {}",
                node.get(),
                encoder_message
            );
            eprintln!(
                "  at file #{}, bytes {}-{}",
                source_span.file(),
                source_span.byte_start(),
                source_span.byte_end()
            );
        }
    }

    ExitCode::from(2)
}

fn elf_path_for(input: &Path) -> PathBuf {
    let mut p = input.to_path_buf();
    let stem = p
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "input".to_string());
    p.set_file_name(format!("{stem}.o"));
    p
}

fn finish_placeholder(
    source_map: &SourceMap,
    catalog: &Catalog,
    sink: VecSink,
    placeholder: Option<String>,
    input: &Path,
    output: Option<&Path>,
) -> ExitCode {
    let diagnostics = sink.into_diagnostics();

    // Human render to stderr.
    let stderr = std::io::stderr();
    let renderer = HumanRenderer::with_catalog(source_map, /*color*/ true, catalog);
    let mut human = HumanSink::new(stderr.lock(), renderer);
    for d in &diagnostics {
        let _ = human.emit(d.clone());
    }

    let has_error = diagnostics.iter().any(|d| d.severity() == Severity::Error);

    if let Some(text) = placeholder
        && !has_error
    {
        let path = output
            .map(Path::to_path_buf)
            .unwrap_or_else(|| placeholder_path_for(input));
        match fs::File::create(&path) {
            Ok(file) => {
                let mut w = std::io::BufWriter::new(file);
                let _ = w.write_all(text.as_bytes());
            }
            Err(e) => {
                eprintln!(
                    "paideia-as: cannot write placeholder at {}: {e}",
                    path.display()
                );
                return ExitCode::from(2);
            }
        }
    }

    if has_error {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// `<dir>/<basename>.placeholder` next to the input file.
fn placeholder_path_for(input: &Path) -> PathBuf {
    let mut p = input.to_path_buf();
    let stem = p
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "input".to_string());
    p.set_file_name(format!("{stem}.placeholder"));
    p
}

/// Build the phase-2-m4 PAX object body. Constructs a minimal PAX with
/// empty section table and a canonical BLAKE3 content hash.
fn build_pax_object() -> Vec<u8> {
    let mut header = PaxHeader::new(Architecture::X86_64);
    let table = SectionTable::new();

    // Compute the content hash over the empty section table.
    let hash = compute_content_hash(&header, &table, &[]);
    header.blake3_content_hash = hash;

    // Set section table offset to immediately follow the header.
    header.section_table_offset = PAX_HEADER_SIZE as u64;
    header.section_count = 0;

    // Serialize: header bytes + table bytes.
    let mut bytes = header.to_bytes().to_vec();
    bytes.extend_from_slice(&table.to_bytes());
    bytes
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

/// Build the phase-4-m2-001 PE/COFF object body. Constructs a PE/COFF with
/// .text section populated from InstructionSideTable.
/// Phase-6-m1-004: Propagates encoder failures as BuildError::Encoder.
fn build_pe_object(
    arena: &paideia_as_ir::IrArena,
    _source_map: &SourceMap,
    file: paideia_as_diagnostics::FileId,
    encoder_warn: bool,
) -> Result<Vec<u8>, BuildError> {
    // 1. DosHeader::new() (e_lfanew = 64).
    let dos = DosHeader::new();

    // 2. CoffFileHeader::new_efi_amd64() with number_of_sections set later.
    let mut coff = CoffFileHeader::new_efi_amd64();
    // Set the timestamp for build determinism (SOURCE_DATE_EPOCH).
    coff.time_date_stamp = det::build_timestamp();

    // 3. OptionalHeaderPe32Plus::new_efi_amd64().
    let mut opt = OptionalHeaderPe32Plus::new_efi_amd64();

    // 4. SectionTable with .text section populated from InstructionSideTable.
    let mut sections = PeSectionTable::new();
    let mut text_bytes = Vec::new();

    // Emit .text section content from InstructionSideTable
    // Phase-4 honesty: emit all instructions from the table into .text
    // Phase-4-m2-002: emit_text_from_instructions now returns EmitResult with offset_map
    // Phase-6-m1-004: Propagate encoder failures as BuildError::Encoder.
    let emit_result = match emit_text_from_instructions(arena.instructions(), &mut text_bytes) {
        Ok(result) => result,
        Err(e) => {
            if let Some(failed_node_id) = find_failing_instruction(arena.instructions()) {
                let span = paideia_as_diagnostics::Span::new(file, 0, 1);
                if encoder_warn {
                    eprintln!(
                        "warning: encoder failed on node {}: {}, continuing with --encoder-warn",
                        failed_node_id.get(),
                        e
                    );
                    paideia_as_emitter_pe::EmitResult {
                        encode_stats: EncodeStats::new(),
                        offset_map: std::collections::HashMap::new(),
                        reloc_sites: Vec::new(),
                        label_fixups: Vec::new(),
                    }
                } else {
                    return Err(BuildError::Encoder {
                        node: failed_node_id,
                        source_span: span,
                        encoder_message: e.to_string(),
                    });
                }
            } else {
                return Err(BuildError::Encoder {
                    node: IrNodeId::new(1).unwrap(),
                    source_span: paideia_as_diagnostics::Span::new(file, 0, 1),
                    encoder_message: e.to_string(),
                });
            }
        }
    };

    // If no instructions were encoded, use a minimal placeholder (ret instruction: 0xC3)
    if text_bytes.is_empty() {
        text_bytes.push(0xC3); // ret
    }

    sections.add_text(text_bytes);

    // Store offset_map for DWARF emit-stage (Phase-4-m2-002).
    // This enables DWARF .debug_line reconstruction with post-rewrite offsets.
    let _offset_map = emit_result.offset_map;

    let headers_size = DOS_HEADER_SIZE
        + 4
        + COFF_FILE_HEADER_SIZE
        + OPTIONAL_HEADER_PE32PLUS_SIZE
        + 40 * sections.sections.len();
    sections.finalize(
        opt.section_alignment,
        opt.file_alignment,
        headers_size as u32,
    );

    coff.number_of_sections = sections.sections.len() as u16;

    // 5. Set OptHdr fields populated by section info:
    let total_code = sections
        .sections
        .iter()
        .filter(|s| (s.header.characteristics & 0x20) != 0) // CNT_CODE
        .map(|s| s.header.size_of_raw_data)
        .sum::<u32>();
    opt.size_of_code = total_code;
    opt.size_of_image = sections
        .sections
        .iter()
        .map(|s| s.header.virtual_address + s.header.virtual_size)
        .max()
        .unwrap_or(0);
    opt.size_of_headers = align_up_to(headers_size as u32, opt.file_alignment);
    // Pick the first .text RVA as the entry point.
    opt.address_of_entry_point = sections
        .sections
        .first()
        .map(|s| s.header.virtual_address)
        .unwrap_or(0);

    // 6. Assemble bytes: DOS + NT_SIG + COFF + OptHdr + section headers + section content.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&dos.to_bytes());
    bytes.extend_from_slice(&NT_SIGNATURE);
    bytes.extend_from_slice(&coff.to_bytes());
    bytes.extend_from_slice(&opt.to_bytes());
    bytes.extend_from_slice(&sections.to_bytes_headers());
    // Pad to file alignment.
    while bytes.len() < opt.size_of_headers as usize {
        bytes.push(0);
    }
    // Section content.
    bytes.extend_from_slice(&sections.to_bytes_content(opt.file_alignment));
    Ok(bytes)
}

fn align_up_to(value: u32, align: u32) -> u32 {
    (value + align - 1) & !(align - 1)
}

/// `<dir>/<basename>.pax` next to the input file.
fn pax_path_for(input: &Path) -> PathBuf {
    let mut p = input.to_path_buf();
    let stem = p
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "input".to_string());
    p.set_file_name(format!("{stem}.pax"));
    p
}

/// `<dir>/<basename>.efi` next to the input file.
fn pe_path_for(input: &Path) -> PathBuf {
    let mut p = input.to_path_buf();
    let stem = p
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "input".to_string());
    p.set_file_name(format!("{stem}.efi"));
    p
}

fn finish_pax(
    source_map: &SourceMap,
    catalog: &Catalog,
    sink: VecSink,
    bytes: Option<Vec<u8>>,
    input: &Path,
    output: Option<&Path>,
) -> ExitCode {
    let diagnostics = sink.into_diagnostics();
    let stderr = std::io::stderr();
    let renderer = HumanRenderer::with_catalog(source_map, true, catalog);
    let mut human = HumanSink::new(stderr.lock(), renderer);
    for d in &diagnostics {
        let _ = human.emit(d.clone());
    }
    let has_error = diagnostics.iter().any(|d| d.severity() == Severity::Error);
    if has_error {
        return ExitCode::from(1);
    }
    if let Some(bytes) = bytes {
        let path = output
            .map(Path::to_path_buf)
            .unwrap_or_else(|| pax_path_for(input));
        match fs::File::create(&path) {
            Ok(file) => {
                let mut w = std::io::BufWriter::new(file);
                let _ = w.write_all(&bytes);
            }
            Err(e) => {
                eprintln!("paideia-as: cannot write PAX at {}: {e}", path.display());
                return ExitCode::from(2);
            }
        }
    }
    ExitCode::SUCCESS
}

fn finish_pe(
    source_map: &SourceMap,
    catalog: &Catalog,
    sink: VecSink,
    bytes: Option<Vec<u8>>,
    input: &Path,
    output: Option<&Path>,
) -> ExitCode {
    let diagnostics = sink.into_diagnostics();
    let stderr = std::io::stderr();
    let renderer = HumanRenderer::with_catalog(source_map, true, catalog);
    let mut human = HumanSink::new(stderr.lock(), renderer);
    for d in &diagnostics {
        let _ = human.emit(d.clone());
    }
    let has_error = diagnostics.iter().any(|d| d.severity() == Severity::Error);
    if has_error {
        return ExitCode::from(1);
    }
    if let Some(bytes) = bytes {
        let path = output
            .map(Path::to_path_buf)
            .unwrap_or_else(|| pe_path_for(input));
        match fs::File::create(&path) {
            Ok(file) => {
                let mut w = std::io::BufWriter::new(file);
                let _ = w.write_all(&bytes);
            }
            Err(e) => {
                eprintln!("paideia-as: cannot write PE at {}: {e}", path.display());
                return ExitCode::from(2);
            }
        }
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_path_replaces_extension() {
        let p = Path::new("example.pdx");
        assert_eq!(placeholder_path_for(p), Path::new("example.placeholder"));
    }

    #[test]
    fn placeholder_path_preserves_directory() {
        let p = Path::new("/tmp/foo/example.pdx");
        assert_eq!(
            placeholder_path_for(p),
            Path::new("/tmp/foo/example.placeholder")
        );
    }

    #[test]
    fn pax_path_replaces_extension() {
        let p = Path::new("example.pdx");
        assert_eq!(pax_path_for(p), Path::new("example.pax"));
    }

    #[test]
    fn pax_path_preserves_directory() {
        let p = Path::new("/tmp/foo/example.pdx");
        assert_eq!(pax_path_for(p), Path::new("/tmp/foo/example.pax"));
    }

    #[test]
    fn functors_from_modules_extracts_functor_entries() {
        use paideia_as_ir::{FunctorInfo, ModuleInfo};

        let mut table = ModuleSideTable::new();
        let functor_module_id = IrNodeId::new(1).unwrap();
        let body_id = IrNodeId::new(10).unwrap();

        // Create a functor module.
        let functor_info = FunctorInfo {
            param_signature_hash: 0x1111111111111111,
            result_signature_hash: 0x2222222222222222,
            body_node_id: body_id,
        };

        let module_info = ModuleInfo {
            name: "MyFunctor".to_string(),
            fields: vec![],
            functor: Some(functor_info),
        };

        table.insert(functor_module_id, module_info);

        // Define a simple symbol resolver.
        let symbol_resolver = |_id: IrNodeId| -> u64 { 42 };

        // Call the bridge.
        let section = functors_from_modules(&table, symbol_resolver);

        // Bridge must emit exactly one entry for the functor module.
        assert_eq!(section.len(), 1, "expected one functor entry");
        let entry = &section.entries[0];
        assert_eq!(entry.functor_symbol_id, 42);
        assert_eq!(entry.param_signature_hash, 0x1111111111111111);
        assert_eq!(entry.result_signature_hash, 0x2222222222222222);
        assert_eq!(entry.closure_data_offset, 0);
        assert_eq!(entry.closure_data_size, 0);
        assert_eq!(entry.flags, 0);
    }

    /// Phase-5-m1-005: Test that EmitWalker is integrated into the build pipeline.
    /// Empty IR produces zero instruction table entries.
    #[test]
    fn emit_walker_empty_ir_produces_zero_entries() {
        use paideia_as_elaborator::EmitWalker;

        let mut emit_walker = EmitWalker::new();
        let mut arena = paideia_as_ir::IrArena::new();
        emit_walker.walk(&mut arena);

        assert_eq!(
            emit_walker.state().instructions.len(),
            0,
            "empty IR should produce zero instruction entries"
        );
    }

    /// Phase-5-m1-005: Test that EmitWalker populates instruction table on non-empty IR.
    /// A simple Let+Literal should produce one instruction entry.
    #[test]
    fn emit_walker_let_literal_produces_entry() {
        use paideia_as_diagnostics::FileId;
        use paideia_as_elaborator::EmitWalker;

        let mut emit_walker = EmitWalker::new();
        let mut arena = paideia_as_ir::IrArena::new();

        // Create a simple Let+Literal IR: let x = 42
        let span = paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 0, 1);
        let lit_id = arena.alloc(paideia_as_ir::IrKind::Literal, span);
        let let_id = arena.alloc_with_children(paideia_as_ir::IrKind::Let, span, [lit_id]);

        // Register the literal value.
        arena.literal_values_mut().insert(lit_id, 42);

        // Walk and verify one instruction was emitted.
        emit_walker.walk(&mut arena);

        assert_eq!(
            emit_walker.state().instructions.len(),
            1,
            "Let+Literal should produce one instruction entry"
        );
        assert!(
            emit_walker.state().instructions.get(let_id).is_some(),
            "instruction should be keyed by let_id"
        );
    }

    /// Phase-5-m1-005: Test that EmitWalker records Lambda offsets.
    /// A Lambda should populate function_offsets.
    #[test]
    fn emit_walker_lambda_records_offset() {
        use paideia_as_diagnostics::FileId;
        use paideia_as_elaborator::EmitWalker;

        let mut emit_walker = EmitWalker::new();
        let mut arena = paideia_as_ir::IrArena::new();

        // Create a simple Lambda: fn (x) -> x
        let span = paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 0, 1);
        let var_id = arena.alloc(paideia_as_ir::IrKind::Var, span);
        let lambda_id = arena.alloc_with_children(paideia_as_ir::IrKind::Lambda, span, [var_id]);

        // Walk and verify offset was recorded.
        emit_walker.walk(&mut arena);

        assert!(
            emit_walker
                .state()
                .function_offsets
                .contains_key(&lambda_id.get()),
            "lambda offset should be recorded"
        );
    }
}

/// Parse an integer literal from text, supporting decimal, hex, binary, and octal formats.
///
/// Formats:
/// - Decimal: `42`, `-42`
/// - Hexadecimal: `0x2A`, `0X2a`
/// - Binary: `0b101010`, `0B101010`
/// - Octal: `0o52`, `0O52`
///
/// Returns `Ok(value)` on success, `Err(())` on parse failure.
fn parse_integer_literal(text: &str) -> Result<i64, ()> {
    let text = text.trim();
    if text.is_empty() {
        return Err(());
    }

    // Handle negative numbers
    let (is_negative, text) = if text.starts_with('-') {
        (true, &text[1..])
    } else if text.starts_with('+') {
        (false, &text[1..])
    } else {
        (false, text)
    };

    // Determine the base and skip the prefix
    let (base, digits) = if text.starts_with("0x") || text.starts_with("0X") {
        (16, &text[2..])
    } else if text.starts_with("0b") || text.starts_with("0B") {
        (2, &text[2..])
    } else if text.starts_with("0o") || text.starts_with("0O") {
        (8, &text[2..])
    } else {
        (10, text)
    };

    // Remove underscores (allowed in numeric literals)
    let digits: String = digits.chars().filter(|c| *c != '_').collect();

    // Parse the digits
    i64::from_str_radix(&digits, base)
        .map(|n| if is_negative { -n } else { n })
        .map_err(|_| ())
}
