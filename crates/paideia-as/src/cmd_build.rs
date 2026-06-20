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

use crate::det;
use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{
    Catalog, DiagnosticSink, HumanRenderer, HumanSink, Severity, SourceMap, VecSink,
};
use paideia_as_elaborator::{
    CapWalker, EffectRowWalker, LinearityWalker, lower_ast_to_ir, placeholder_for,
    validate_file_module_mapping,
};
use paideia_as_emitter_elf::{Arch, ElfWriter, Kind, SymbolEntry, lower_add_one};
use paideia_as_emitter_pax::{
    Architecture, FunctorsSection, PAX_HEADER_SIZE, PaxHeader, SectionTable, compute_content_hash,
};
use paideia_as_emitter_pe::{
    COFF_FILE_HEADER_SIZE, CoffFileHeader, DOS_HEADER_SIZE, DosHeader, NT_SIGNATURE,
    OPTIONAL_HEADER_PE32PLUS_SIZE, OptionalHeaderPe32Plus, SectionTable as PeSectionTable,
    emit_text_from_instructions,
};
use paideia_as_ir::{IrNodeId, ModuleSideTable, walk};
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

/// Run `paideia-as build <input> [--emit <format>] [-o <output>]`.
pub fn run(input: &Path, output: Option<&Path>, emit: &str) -> ExitCode {
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
    let lowering = lower_ast_to_ir(&arena);

    // Run walkers over the IR to surface S/F/C diagnostics.
    // Phase-2-m1: walkers run with empty injection tables (from CLI), so only
    // diagnostics that depend on kind-only IR will fire (S0900/S0901/S0903).
    // Real effect (F1100, F1101, F1105, F1106) and capability (C1300) diagnostics
    // require per-node payloads that arrive in m3/m5.
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
        }

        // Drain walker diagnostics into the main sink for rendering.
        for d in walker_sink.into_diagnostics() {
            let _ = sink.emit(d);
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
            let bytes = if preview {
                None
            } else {
                Some(build_elf_object())
            };
            finish_elf(&source_map, catalog, sink, bytes, input, output)
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
            let bytes = if preview {
                None
            } else {
                Some(build_pe_object(&lowering.ir))
            };
            finish_pe(&source_map, catalog, sink, bytes, input, output)
        }
    }
}

/// Build the phase-1 ELF object body. The emitter currently produces a
/// single canonical "add one" function so the smoke test has a real
/// symbol to link against. Once the IR walker can dispatch on node
/// payloads this expands to the full lowering pipeline.
fn build_elf_object() -> Vec<u8> {
    let mut writer = ElfWriter::new(Arch::X86_64, Kind::Relocatable);
    let mut buf = paideia_as_emitter_elf::CodeBuffer::new();
    lower_add_one(&mut buf);
    let body_size = buf.bytes.len() as u64;
    writer.add_text_bytes(&buf.bytes);
    let _ = writer.add_symbol(SymbolEntry::func("add_one", 0, body_size));
    writer.finalize().unwrap_or_default()
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
fn build_pe_object(arena: &paideia_as_ir::IrArena) -> Vec<u8> {
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
    let _result = emit_text_from_instructions(arena.instructions(), &mut text_bytes);

    // If no instructions were encoded, use a minimal placeholder (ret instruction: 0xC3)
    if text_bytes.is_empty() {
        text_bytes.push(0xC3); // ret
    }

    sections.add_text(text_bytes);

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
    bytes
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
}
