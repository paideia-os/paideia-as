//! PE/COFF object emit path.
//! Split out of `cmd_build.rs` (2026-07-08).

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use paideia_as_diagnostics::{Catalog, DiagnosticSink, HumanRenderer, HumanSink, Severity, SourceMap, Span, VecSink};
use paideia_as_emitter_pe::emit_text_from_instructions;
use paideia_as_emitter_pe::{
    COFF_FILE_HEADER_SIZE, CoffFileHeader, DOS_HEADER_SIZE, DosHeader, NT_SIGNATURE,
    OPTIONAL_HEADER_PE32PLUS_SIZE, OptionalHeaderPe32Plus, NamedSectionError, Section, SectionTable as PeSectionTable,
};
use paideia_as_encoder::EncodeStats;
use paideia_as_ir::{IrNodeId, SectionKind};

use crate::det;
use crate::cmd_common;

use super::BuildError;

/// Build the phase-4-m2-001 PE/COFF object body. Constructs a PE/COFF with
/// .text section populated from InstructionSideTable.
/// Phase-6-m1-004: Propagates encoder failures as BuildError::Encoder.
/// Phase 8 m1-004: Routes errors through DiagnosticSink as typed diagnostics.
pub(super) fn build_pe_object(
    arena: &mut paideia_as_ir::IrArena,
    _source_map: &SourceMap,
    file: paideia_as_diagnostics::FileId,
    encoder_warn: bool,
    sink: &mut dyn DiagnosticSink,
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
    // Phase 8 m1-004: Route through DiagnosticSink as B1705/B1706.
    use super::diagnostics::{encoder_error, encoder_warn as encoder_warn_diag, find_failing_instruction};

    let emit_result = match emit_text_from_instructions(arena.instructions_mut(), &mut text_bytes) {
        Ok(result) => result,
        Err(e) => {
            use paideia_as_emitter_pe::TextEmitterError;

            // Prefer the node from EncodeErrorAt if available; fall back to find_failing_instruction
            let failed_node_id = match &e {
                TextEmitterError::EncodeErrorAt { node, .. } => Some(*node),
                _ => find_failing_instruction(arena.instructions()),
            };

            if let Some(failed_node_id) = failed_node_id {
                let msg = format!("encoder failed on IR node {}: {}", failed_node_id.get(), e);
                let span = Some(Span::new(file, 0, 1));
                if encoder_warn {
                    let diag = encoder_warn_diag(failed_node_id, &msg, span);
                    let _ = sink.emit(diag);
                    paideia_as_emitter_pe::EmitResult {
                        encode_stats: EncodeStats::new(),
                        offset_map: std::collections::HashMap::new(),
                        reloc_sites: Vec::new(),
                        label_fixups: Vec::new(),
                    }
                } else {
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
    };

    // If no instructions were encoded, use a minimal placeholder (ret instruction: 0xC3)
    if text_bytes.is_empty() {
        text_bytes.push(0xC3); // ret
    }

    sections.add_text(text_bytes);

    // Track symbol offsets and relocations for data entries.
    // We need to collect relocations and process them after finalization.
    let mut data_symbol_offsets: std::collections::HashMap<String, (usize, u64)> = std::collections::HashMap::new();
    // Each tuple: (target_symbol, source_section_idx, relocation_offset, width, addend)
    let mut reloc_specs: Vec<(String, usize, u64, paideia_as_ir::RelocWidth, i64)> = Vec::new();

    // Iterate through data entries and add them to appropriate sections.
    // Phase 19 R19-M1 (pa-r19-013-followup): iterate arena.data() and emit data sections.
    for (_node_id, entry) in arena.data().iter() {
        let (section_idx, offset_in_section) = if let Some(name) = &entry.section_name_override {
            let writable = matches!(entry.section, SectionKind::Data | SectionKind::Bss);
            match sections.add_bytes_to_named_section(name, &entry.bytes, entry.align, writable) {
                Ok(offset) => {
                    // Find the section index for this custom section
                    let idx = sections.find_section_by_name(name)
                        .expect("custom section must exist after add_bytes_to_named_section");
                    (idx, offset)
                },
                Err(NamedSectionError::NameTooLong { name: ref _section_name, len: _len }) => {
                    // Emit P0289 through the sink (if sink is available)
                    // For now we skip this entry; a diagnostic would be emitted at elaboration time
                    #[cfg(debug_assertions)]
                    eprintln!("[pe-emit] section name '{}' is {} bytes; PE section names must be at most 8", _section_name, _len);
                    continue;
                }
            }
        } else {
            match entry.section {
                SectionKind::Rodata => {
                    let offset = sections.add_rodata_bytes(&entry.bytes, entry.align);
                    let idx = sections.find_section_by_name(".rdata")
                        .expect(".rdata section must exist after add_rodata_bytes");
                    (idx, offset)
                },
                SectionKind::Data => {
                    let offset = sections.add_data_bytes(&entry.bytes, entry.align);
                    let idx = sections.find_section_by_name(".data")
                        .expect(".data section must exist after add_data_bytes");
                    (idx, offset)
                },
                SectionKind::Bss => {
                    let offset = sections.add_bss_space(entry.size_hint, entry.align);
                    let idx = sections.find_section_by_name(".bss")
                        .expect(".bss section must exist after add_bss_space");
                    (idx, offset)
                },
                SectionKind::Text => {
                    // Skip text entries for now
                    continue;
                }
            }
        };

        // Track symbol offset for relocation resolution
        data_symbol_offsets.insert(entry.symbol_name.clone(), (section_idx, offset_in_section));

        // Collect relocations for processing after finalization
        for reloc in &entry.relocations {
            reloc_specs.push((
                reloc.symbol.clone(),  // Target symbol (not source)
                section_idx,
                offset_in_section + reloc.offset,
                reloc.width,
                reloc.addend,
            ));
        }
    }

    // Store offset_map for DWARF emit-stage (Phase-4-m2-002).
    // This enables DWARF .debug_line reconstruction with post-rewrite offsets.
    let _offset_map = emit_result.offset_map;

    // Reserve space for .reloc section if we have relocations
    let has_relocations = !sections.relocations.is_empty();
    if has_relocations {
        // Add placeholder .reloc section (will be populated after finalization)
        sections.sections.push(Section::new_named(
            b".reloc\0\0",
            paideia_as_emitter_pe::CHARACTERISTICS_RDATA,
            Vec::new(),
            0,
        ));
    }

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

    // Resolve relocations: patch data with target RVAs and record base relocations.
    // Convert symbol offsets to RVAs and process each relocation.
    for (symbol_name, section_idx, data_offset, width, addend) in reloc_specs {
        // Get the target symbol's RVA
        if let Some((target_section_idx, target_offset)) = data_symbol_offsets.get(&symbol_name) {
            if *target_section_idx < sections.sections.len() {
                let target_section = &sections.sections[*target_section_idx];
                let target_rva = target_section.header.virtual_address + *target_offset as u32;

                // Patch the data slot with the resolved RVA
                if section_idx < sections.sections.len() {
                    let section = &mut sections.sections[section_idx];
                    let data_slot = data_offset as usize;

                    match width {
                        paideia_as_ir::RelocWidth::W64 => {
                            if data_slot + 8 <= section.content.len() {
                                let patched_value = (target_rva as i64 + addend).to_le_bytes();
                                section.content[data_slot..data_slot + 8].copy_from_slice(&patched_value);
                            }
                        },
                        paideia_as_ir::RelocWidth::W32 => {
                            if data_slot + 4 <= section.content.len() {
                                let patched_value = ((target_rva as i64 + addend) as u32).to_le_bytes();
                                section.content[data_slot..data_slot + 4].copy_from_slice(&patched_value);
                            }
                        },
                    }
                }

                // Record the relocation for the .reloc section
                sections.add_relocation(section_idx, data_offset, paideia_as_emitter_pe::IMAGE_REL_BASED_DIR64);
            }
        }
    }

    // Populate the .reloc section if we have relocations
    if has_relocations {
        let reloc_section = sections.build_reloc_section();
        let reloc_bytes = reloc_section.to_bytes();
        if let Some(reloc_sec_idx) = sections.find_section_by_name(".reloc") {
            if reloc_sec_idx < sections.sections.len() {
                let reloc_sec = &mut sections.sections[reloc_sec_idx];
                reloc_sec.content = reloc_bytes;
                let content_len = reloc_sec.content.len() as u32;
                reloc_sec.header.virtual_size = content_len;
                // Update size_of_raw_data to account for the populated content
                // (aligned to file_alignment, same as was done in finalize())
                let aligned_size = paideia_as_emitter_pe::align_up(content_len, opt.file_alignment);
                reloc_sec.header.size_of_raw_data = aligned_size;

                // Update opt.size_of_image if .reloc extended the image
                let reloc_end = reloc_sec.header.virtual_address + content_len;
                if reloc_end > opt.size_of_image {
                    opt.size_of_image = reloc_end;
                }
            }
        }
    }

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

/// `<dir>/<basename>.efi` next to the input file.
pub(super) fn pe_path_for(input: &Path) -> PathBuf {
    let mut p = input.to_path_buf();
    let stem = p
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "input".to_string());
    p.set_file_name(format!("{stem}.efi"));
    p
}

pub(super) fn finish_pe(
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
    let renderer = HumanRenderer::with_catalog(source_map, crate::color::should_use_color(), catalog);
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
