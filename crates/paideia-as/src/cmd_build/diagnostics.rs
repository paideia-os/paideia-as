//! Common diagnostic finisher for encoder + emitter build errors.
//! Split out of `cmd_build.rs` (2026-07-08).

use std::path::Path;
use std::process::ExitCode;

use paideia_as_diagnostics::{Catalog, DiagnosticSink, HumanRenderer, HumanSink, SourceMap, VecSink};

use crate::cmd_common;
use super::BuildError;

pub(super) fn finish_build_error(
    source_map: &SourceMap,
    catalog: &Catalog,
    sink: VecSink,
    error: BuildError,
    _input: &Path,
    sarif: Option<&Path>,
) -> ExitCode {
    let diagnostics = sink.into_diagnostics();

    // Render human diagnostics to stderr (drive-by for symmetry).
    let stderr = std::io::stderr();
    let renderer = HumanRenderer::with_catalog(source_map, true, catalog);
    let mut human = HumanSink::new(stderr.lock(), renderer);
    for d in &diagnostics {
        let _ = human.emit(d.clone());
    }

    // Write SARIF if requested, even on encoder/emitter failure.
    if let Some(path) = sarif {
        let _ = cmd_common::write_sarif(source_map, catalog, &diagnostics, path);
    }

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
        BuildError::Emitter { message } => {
            // Phase 7 m1-002: Report symbol layout validation failures as B1703.
            eprintln!("error: symbol layout invalid: {}", message);
        }
    }

    ExitCode::from(2)
}
