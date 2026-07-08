//! Common diagnostic finisher for encoder + emitter build errors.
//! Split out of `cmd_build.rs` (2026-07-08).

use std::path::Path;
use std::process::ExitCode;

use paideia_as_diagnostics::{Catalog, SourceMap, VecSink};

use super::BuildError;

pub(super) fn finish_build_error(
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
        BuildError::Emitter { message } => {
            // Phase 7 m1-002: Report symbol layout validation failures as B1703.
            eprintln!("error: symbol layout invalid: {}", message);
        }
    }

    ExitCode::from(2)
}
