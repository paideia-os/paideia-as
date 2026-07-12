//! ELF64 object emit path: instruction encoding, symbol/reloc emission, sink drain, file write.
//! Split out of `cmd_build.rs` (2026-07-08).

use std::collections::HashMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use paideia_as_diagnostics::{Catalog, DiagnosticSink, HumanRenderer, HumanSink, Severity, SourceMap, Span, VecSink};
use paideia_as_emitter_elf::{Arch, ElfWriter, EmitterError, Kind, PVH_DEFAULT_ENTRY_ADDR, SymKind, SymbolEntry};
use paideia_as_emitter_pe::emit_text_from_instructions;
use paideia_as_encoder::EncodeStats;
use paideia_as_ir::{InstructionSideTable, IrNodeId, Visibility};

use crate::cmd_common;
use super::BuildError;
use super::fixup::patch_label_fixups;

/// Build the phase-1 ELF object body.
///
/// Phase-5-m5-003: Real symbol-table emission from SymbolTable.
/// Iterates over arena.symbols().iter() and emits one symbol per entry.
/// For each function symbol, the value is the byte offset where its first
/// instruction was emitted (from EmitResult.offset_map).
/// For each data symbol, the value is the byte offset in .rodata/.data.
/// Phase-5-m4-004: Collects relocation sites from instruction encoding and emits
/// them to the .rela.text section.
/// Phase-6-m1-004: Propagates encoder failures as BuildError::Encoder instead of silently falling back.
/// Phase-6-m4-004: Applies label fixups after .text encoding completes.
/// Phase-7-m1-001: Emits B0007 diagnostic when no symbols are exported.
pub(super) fn build_elf_object(
    arena: &paideia_as_ir::IrArena,
    instruction_table: &mut InstructionSideTable,
    emit_walker: &paideia_as_elaborator::EmitWalker,
    _source_map: &SourceMap,
    file: paideia_as_diagnostics::FileId,
    encoder_warn: bool,
    sink: &mut dyn DiagnosticSink,
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
                // Phase 8 m1-004: Emit typed diagnostic B1705/B1706 and return BuildError::Failed.
                use super::diagnostics::{encoder_error, encoder_warn as encoder_warn_diag, span_of, find_failing_instruction};

                if let Some(failed_node_id) = find_failing_instruction(instruction_table) {
                    let msg = format!("encoder failed on IR node {}: {}", failed_node_id.get(), e);
                    let span = span_of(arena, failed_node_id).or_else(|| Some(Span::new(file, 0, 1)));

                    if encoder_warn {
                        // Phase-5 behaviour: warn and drop instruction
                        let diag = encoder_warn_diag(failed_node_id, &msg, span);
                        let _ = sink.emit(diag);
                        paideia_as_emitter_pe::EmitResult {
                            encode_stats: EncodeStats::new(),
                            offset_map: std::collections::HashMap::new(),
                            reloc_sites: Vec::new(),
                            label_fixups: Vec::new(),
                        }
                    } else {
                        // Phase-6 default: propagate error
                        let diag = encoder_error(failed_node_id, &msg, span);
                        let _ = sink.emit(diag);
                        return Err(BuildError::Failed);
                    }
                } else {
                    let msg = format!("encoder failed: {}", e);
                    let span = Some(Span::new(file, 0, 1));
                    if encoder_warn {
                        let diag = encoder_warn_diag(IrNodeId::new(1).unwrap(), &msg, span);
                        let _ = sink.emit(diag);
                    } else {
                        let diag = encoder_error(IrNodeId::new(1).unwrap(), &msg, span);
                        let _ = sink.emit(diag);
                    }
                    return Err(BuildError::Failed);
                }
            }
        }
    };

    // Phase-6-m4-004: Patch label fixups after .text encoding is complete.
    // Compute actual label offsets using label_to_instr mapping and offset_map.
    let label_to_instr = emit_walker.state().label_to_instr();
    let direct_labels = emit_walker.state().labels();
    let offset_map = &emit_result.offset_map;
    let mut resolved_labels: HashMap<String, u32> = HashMap::new();

    // First, add direct labels (populated by register_label during emit)
    for (label_name, offset) in direct_labels {
        resolved_labels.insert(label_name.clone(), *offset);
    }

    // Then, add labels from label_to_instr (computed via offset_map)
    for (label_name, instr_id) in label_to_instr {
        if let Some(&byte_offset_u64) = offset_map.get(instr_id) {
            resolved_labels.insert(label_name.clone(), byte_offset_u64 as u32);
        }
    }

    let strict_mode = true;
    patch_label_fixups(
        &mut text_bytes,
        &emit_result.label_fixups,
        &resolved_labels,
        strict_mode,
        sink,
        arena,
        instruction_table,
        file,
    )?;

    writer.add_text_bytes(&text_bytes);

    // Phase-7-m1-003: emit_walker's `estimated_offset` is now ADVISORY (the
    // authoritative byte position is the per-Instruction `byte_offset_in_text`
    // recorded by the encoder pass). The walker's estimate diverges from the
    // encoder's reality in many legitimate shapes (e.g., let-fn bodies with
    // calls expand under encoding). Don't assert equality — just emit a
    // debug log on divergence so regressions are visible without aborting.
    let estimated = emit_walker.state().estimated_offset() as usize;
    if cfg!(debug_assertions) && estimated != text_bytes.len() {
        eprintln!(
            "[m1-003] estimated_offset {estimated} != encoded text_bytes {} \
             (expected for advisory tracker — encoder owns the truth via \
             InstructionSideTable::byte_offset_in_text)",
            text_bytes.len()
        );
    }

    // Phase-5-m4-003: Emit data entries from the data side-table.
    // Also create symbols for each data entry so relocations can reference them.
    let data_table = arena.data();
    let mut data_offsets: std::collections::HashMap<IrNodeId, u64> = std::collections::HashMap::new();
    for (node_id, entry) in data_table.iter() {
        // Phase 19 PA19-r19-010: Check for custom section override first
        let (data_offset, section_kind, section_name_opt) = if let Some(ref section_name) = entry.section_name_override {
            // Emit to custom-named section
            let writable = matches!(entry.section, paideia_as_ir::SectionKind::Data | paideia_as_ir::SectionKind::Bss);
            let (_, offset) = writer.add_bytes_to_named_section(
                section_name,
                &entry.bytes,
                entry.align,
                writable,
            );
            (offset, None, Some(section_name.clone()))
        } else {
            // Standard section routing
            let data_offset = match entry.section {
                paideia_as_ir::SectionKind::Rodata => writer.add_rodata_bytes(&entry.bytes, entry.align),
                paideia_as_ir::SectionKind::Data => writer.add_data_bytes(&entry.bytes, entry.align),
                paideia_as_ir::SectionKind::Bss => {
                    // Phase 6 m5-002: allocate uninitialized space in .bss section
                    writer.add_bss_space(entry.size_hint, entry.align)
                }
                paideia_as_ir::SectionKind::Text => {
                    // Phase 15 m2-002: code section entries (deferred implementation)
                    unimplemented!("SectionKind::Text code emission not yet implemented")
                }
            };
            (data_offset, Some(entry.section), None)
        };
        data_offsets.insert(*node_id, data_offset);
        // Phase-5-m4-003: Create a symbol for the data entry so relocations can reference it
        // Phase 6 m5-003: include section information for .bss symbols
        // PA10-007 FIX: Use actual binding name from data entry, not hardcoded data_<id>
        let sym_name = entry.symbol_name.clone();
        let size = match entry.section {
            paideia_as_ir::SectionKind::Bss => entry.size_hint,
            _ => entry.bytes.len() as u64,
        };
        let _ = writer.add_symbol(SymbolEntry {
            name: sym_name,
            offset: Some(data_offset),
            size,
            kind: SymKind::Data,
            is_global: true, // PA10-007: Data symbols must be global for cross-module linkage
            section: section_kind,
            section_name: section_name_opt,
        });
    }

    // Phase-5-m5-003: Emit real symbols from SymbolTable (REORDERED TO FIX BUG 2).
    // CRITICAL FIX (PA-R17-003): Register all function symbols BEFORE emitting data
    // relocations. This prevents the data-relocation loop from calling add_undefined_symbol
    // for function targets, which would later conflict when add_symbol tries to register
    // the actual function symbol. Sequence: emit data entries, emit function symbols,
    // THEN emit data relocations.
    //
    // Original order (1668-1775) emitted data relocations (1632-1666) before function
    // symbols, causing duplicate symbol names when a Borrow relocation targeted a
    // function not yet registered.
    //
    // v3 retirement: derive symbol offsets solely from lambda_first_instr × offset_map.
    // Lambdas whose first_instr is not in offset_map fall through to the None branch (B1704).
    let mut function_offsets: HashMap<u32, u64> = HashMap::new();
    {
        let lambda_first_instr = emit_walker.state().lambda_first_instr();
        let offset_map = &emit_result.offset_map;
        for (lambda_id, &first_instr) in lambda_first_instr {
            if let Some(&byte_off) = offset_map.get(&first_instr) {
                function_offsets.insert(*lambda_id, byte_off);
            }
        }
    }
    let _emitted_lambdas = emit_walker.emitted_lambdas();
    let mut emitted_any_symbol = false;

    // PA8-m1-002: Pre-compute sorted, deduplicated offsets once to avoid O(N²) lookup.
    let mut sorted_offsets: Vec<u64> = function_offsets.values().copied().collect();
    sorted_offsets.sort_unstable();
    sorted_offsets.dedup();

    for symbol in arena.symbols().iter() {
        match symbol.kind {
            paideia_as_ir::SymbolKind::Function => {
                // Phase 7 m1-001: Emit symbols for all function bindings, regardless of whether
                // they emitted bytecode. If a lambda didn't emit (e.g., unsupported shape),
                // use offset 0 and size 0 as a placeholder.
                // PA8-m1-002: Fix defect A by looking up recorded offset and computing size
                // via binary search rather than picking an unrelated lambda's offset.
                let recorded = function_offsets.get(&symbol.ir_node.get()).copied();
                let (offset, size) = match recorded {
                    Some(off) => {
                        // Find the next offset in sorted order and use it as the exclusive upper bound.
                        let idx = sorted_offsets.partition_point(|&o| o <= off);
                        let end = sorted_offsets
                            .get(idx)
                            .copied()
                            .unwrap_or(text_bytes.len() as u64);
                        (off, end - off)
                    }
                    None => {
                        // PA8-m1-002: Deferred lambdas (m1-004+) that don't call record_lambda_entry
                        // will have no offset. For these, emit st_value=0, st_size=0 and continue.
                        // This allows the build to succeed even for lambdas deferred to later passes
                        // that haven't been implemented yet.
                        use super::diagnostics::function_symbol_no_offset;
                        let diag = function_symbol_no_offset(&symbol.name, symbol.ir_node.get());
                        let _ = sink.emit(diag);
                        (0u64, 0u64)
                    }
                };

                let sym_entry = SymbolEntry {
                    name: symbol.name.clone(),
                    kind: SymKind::Func,
                    is_global: matches!(symbol.visibility, Visibility::Global),
                    offset: Some(offset),
                    size,
                    section: None,
                    section_name: None,
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

    // Phase 7 m1-001: B1702 is defined in the catalog but not currently
    // emitted. A legitimately empty source (no fn, no unsafe block, no data,
    // no .bss) is rare and produces an obvious empty .o; the linker's own
    // diagnostics are more useful than ours. The catalog entry exists for
    // future use by a stricter `paideia-as check --pedantic` mode.
    let _ = emitted_any_symbol; // reserved for future B1702 logic

    // PA10-006u (LOAD-BEARING): Emit relocations for data sections.
    // Iterate entry.relocations and call writer.add_relocation for each.
    // This was missing in PA10-002, leaving string-literal pointers unpatched.
    // (MOVED AFTER function symbol emission to fix PA-R17-003 bug 2)
    // Phase 19 PA19-r19-010: Route relocations to custom sections when applicable.
    for (node_id, entry) in data_table.iter() {
        if !entry.relocations.is_empty() {
            let data_offset = data_offsets
                .get(node_id)
                .copied()
                .expect("data_offset must exist for every entry");
            // Determine target section: check for custom section override first
            let target_section = if let Some(ref section_name) = entry.section_name_override {
                // Get custom section ID via the method we'll add
                writer.get_custom_section_id(section_name)
                    .expect("custom section must have been created")
            } else {
                match entry.section {
                    paideia_as_ir::SectionKind::Rodata => writer.rodata_section_id(),
                    paideia_as_ir::SectionKind::Data => writer.data_section_id(),
                    paideia_as_ir::SectionKind::Bss => writer.bss_section_id(),
                    paideia_as_ir::SectionKind::Text => writer.text_section_id(),
                }
            };
            for spec in &entry.relocations {
                let reloc_offset = data_offset + spec.offset;
                let reloc_kind = match spec.width {
                    paideia_as_ir::RelocWidth::W32 => paideia_as_emitter_elf::relocs::RelocKind::Abs32,
                    paideia_as_ir::RelocWidth::W64 => paideia_as_emitter_elf::relocs::RelocKind::Abs64,
                };
                let reloc_entry = paideia_as_emitter_elf::relocs::RelocEntry {
                    offset: reloc_offset,
                    target: spec.symbol.clone(),
                    kind: reloc_kind,
                    addend: spec.addend,
                };
                let _ = writer.add_relocation(target_section, reloc_entry);
            }
        }
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

    // PA10-001: Emit PVH note section for QEMU `-kernel` acceptance.
    // The linker script controls whether the note is retained in the executable.
    let _ = writer.add_pvh_note_section(PVH_DEFAULT_ENTRY_ADDR);

    // Phase 7 m1-002: Finalize and validate symbol layout invariants.
    // Phase 8 m1-004: Emit typed diagnostic B1703 and return BuildError::Failed.
    writer.finalize().map_err(|err| match err {
        EmitterError::SymbolLayoutInvalid { message } => {
            use super::diagnostics::symbol_layout_invalid;
            let diag = symbol_layout_invalid(&message);
            let _ = sink.emit(diag);
            BuildError::Failed
        }
    })
}


pub(super) fn finish_elf(
    source_map: &SourceMap,
    catalog: &Catalog,
    sink: VecSink,
    bytes: Option<Vec<u8>>,
    input: &Path,
    output: Option<&Path>,
    sarif: Option<&Path>,
) -> ExitCode {
    let diagnostics = sink.into_diagnostics();
    let stderr = std::io::stderr();
    let renderer = HumanRenderer::with_catalog(source_map, true, catalog);
    let mut human = HumanSink::new(stderr.lock(), renderer);
    for d in &diagnostics {
        let _ = human.emit(d.clone());
    }

    // Write SARIF if requested.
    if let Some(path) = sarif {
        let _ = cmd_common::write_sarif(source_map, catalog, &diagnostics, path);
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

fn elf_path_for(input: &Path) -> PathBuf {
    let mut p = input.to_path_buf();
    let stem = p
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "input".to_string());
    p.set_file_name(format!("{stem}.o"));
    p
}
