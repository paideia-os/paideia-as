//! PE/COFF object emit path.
//! Split out of `cmd_build.rs` (2026-07-08).

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use paideia_as_diagnostics::{Catalog, DiagnosticSink, HumanRenderer, HumanSink, Severity, SourceMap, VecSink};
use paideia_as_emitter_pe::emit_text_from_instructions;
use paideia_as_emitter_pe::{
    COFF_FILE_HEADER_SIZE, CoffFileHeader, DOS_HEADER_SIZE, DosHeader, NT_SIGNATURE,
    OPTIONAL_HEADER_PE32PLUS_SIZE, OptionalHeaderPe32Plus, NamedSectionError, SectionTable as PeSectionTable,
};
use paideia_as_encoder::EncodeStats;
use paideia_as_ir::{IrNodeId, SectionKind};

use crate::det;
use crate::cmd_common;

use super::BuildError;
use super::elf::find_failing_instruction;

/// Build the phase-4-m2-001 PE/COFF object body. Constructs a PE/COFF with
/// .text section populated from InstructionSideTable.
/// Phase-6-m1-004: Propagates encoder failures as BuildError::Encoder.
pub(super) fn build_pe_object(
    arena: &mut paideia_as_ir::IrArena,
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
    let emit_result = match emit_text_from_instructions(arena.instructions_mut(), &mut text_bytes) {
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

    // Iterate through data entries and add them to appropriate sections.
    // Phase 19 R19-M1 (pa-r19-013-followup): iterate arena.data() and emit data sections.
    for (_node_id, entry) in arena.data().iter() {
        if !entry.relocations.is_empty() {
            #[cfg(debug_assertions)]
            eprintln!("[pe-emit] skipping DataEntry with cross-section relocations (deferred #1105)");
            continue;
        }
        if let Some(name) = &entry.section_name_override {
            let writable = matches!(entry.section, SectionKind::Data | SectionKind::Bss);
            match sections.add_bytes_to_named_section(name, &entry.bytes, entry.align, writable) {
                Ok(_) => {},
                Err(NamedSectionError::NameTooLong { name: ref section_name, len }) => {
                    // Emit P0289 through the sink (if sink is available)
                    // For now we skip this entry; a diagnostic would be emitted at elaboration time
                    #[cfg(debug_assertions)]
                    eprintln!("[pe-emit] section name '{}' is {} bytes; PE section names must be at most 8", section_name, len);
                    continue;
                }
            }
        } else {
            match entry.section {
                SectionKind::Rodata => { sections.add_rodata_bytes(&entry.bytes, entry.align); }
                SectionKind::Data   => { sections.add_data_bytes(&entry.bytes, entry.align); }
                SectionKind::Bss    => { sections.add_bss_space(entry.size_hint, entry.align); }
                SectionKind::Text   => { /* deferred */ }
            }
        }
    }

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
