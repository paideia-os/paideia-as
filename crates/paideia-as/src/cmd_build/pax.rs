//! PAX (PaideiaOS Architectural Executable) object emit path.
//! Split out of `cmd_build.rs` (2026-07-08).

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use paideia_as_diagnostics::{Catalog, DiagnosticSink, HumanRenderer, HumanSink, Severity, SourceMap, VecSink};
use paideia_as_emitter_pax::{
    Architecture, PAX_HEADER_SIZE, PaxHeader, SectionTable, compute_content_hash,
};

/// Build the phase-2-m4 PAX object body. Constructs a minimal PAX with
/// empty section table and a canonical BLAKE3 content hash.
pub(super) fn build_pax_object() -> Vec<u8> {
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

pub(super) fn finish_pax(
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

/// `<dir>/<basename>.pax` next to the input file.
pub(super) fn pax_path_for(input: &Path) -> PathBuf {
    let mut p = input.to_path_buf();
    let stem = p
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "input".to_string());
    p.set_file_name(format!("{stem}.pax"));
    p
}
