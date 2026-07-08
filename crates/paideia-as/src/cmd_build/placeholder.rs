//! Placeholder output: writes `<input>.placeholder` = BLAKE3 hash of pretty-printed IR.
//! Split out of `cmd_build.rs` (2026-07-08).

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use paideia_as_diagnostics::{Catalog, DiagnosticSink, HumanRenderer, HumanSink, Severity, SourceMap, VecSink};

pub(super) fn finish_placeholder(
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
pub(super) fn placeholder_path_for(input: &Path) -> PathBuf {
    let mut p = input.to_path_buf();
    let stem = p
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "input".to_string());
    p.set_file_name(format!("{stem}.placeholder"));
    p
}
