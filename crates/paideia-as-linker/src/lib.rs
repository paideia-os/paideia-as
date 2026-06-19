//! paideia-link — the PAX-format linker.
//!
//! 4-phase pipeline: parse → resolve → relocate → emit.
//! m4-009 ships the parse phase; m4-010..012 ship the rest.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

use paideia_as_diagnostics::VecSink;
use std::path::{Path, PathBuf};

pub mod emit;
pub mod parse;
pub mod relocate;
pub mod resolve;

pub use emit::emit_final_pax;
pub use parse::{ParseError, ParsedPax, parse_inputs, parse_pax};
pub use relocate::{RelocatedLink, RelocatedPax, RelocationError, relocate_inputs};
pub use resolve::{
    GlobalCapabilityTable, GlobalSymbolTable, ResolvedLink, ResolvedPax, resolve_inputs,
};

/// Error type for the link driver.
#[derive(Debug, Clone)]
pub enum LinkError {
    /// Parse phase failed.
    Parse(ParseError),
    /// Resolve phase failed (undefined symbols or unbound capabilities).
    Resolve(String),
    /// I/O error writing output.
    Io(String),
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkError::Parse(e) => write!(f, "Parse error: {}", e),
            LinkError::Resolve(msg) => write!(f, "Resolve error: {}", msg),
            LinkError::Io(msg) => write!(f, "I/O error: {}", msg),
        }
    }
}

impl std::error::Error for LinkError {}

/// End-to-end link driver.
///
/// Orchestrates the 4-phase pipeline:
/// 1. Parse all inputs
/// 2. Resolve symbols and capabilities
/// 3. Relocate section contents
/// 4. Emit final PAX to output file
///
/// Returns Ok(()) on success; Err(LinkError) if any phase fails.
pub fn link(input_paths: &[PathBuf], output_path: &Path) -> Result<(), LinkError> {
    // Phase 1: Parse
    let inputs = parse_inputs(input_paths).map_err(LinkError::Parse)?;

    // Phase 2: Resolve
    let mut sink = VecSink::new();
    let resolved = resolve_inputs(&inputs, &mut sink).ok_or_else(|| {
        let diagnostics = sink.into_diagnostics();
        LinkError::Resolve(format!(
            "resolve phase failed with {} diagnostics",
            diagnostics.len()
        ))
    })?;

    // Phase 3: Relocate
    let relocated = relocate_inputs(&inputs, &resolved);

    // Phase 4: Emit
    let output_bytes = emit_final_pax(&inputs, &resolved, &relocated);

    // Write to file
    std::fs::write(output_path, output_bytes).map_err(|e| LinkError::Io(e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_emitter_pax::PaxHeader;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn write_tempfile(content: &[u8], suffix: &str) -> std::io::Result<PathBuf> {
        let counter = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let temp_dir = std::env::temp_dir();
        let filename = format!("paideia_as_link_test_{}_{}", counter, suffix);
        let path = temp_dir.join(&filename);

        let mut file = std::fs::File::create(&path)?;
        file.write_all(content)?;

        Ok(path)
    }

    fn build_minimal_pax_bytes() -> Vec<u8> {
        use paideia_as_emitter_pax::{Architecture, PaxHeader};

        let mut header = PaxHeader::new(Architecture::X86_64);
        header.section_table_offset = 96;
        header.section_count = 0;
        header.blake3_content_hash = *b"0123456789ABCDEF0123456789ABCDEF";

        let mut bytes = vec![];
        bytes.extend_from_slice(&header.to_bytes());

        bytes
    }

    #[test]
    fn link_end_to_end_2_inputs_to_one_output() {
        let bytes1 = build_minimal_pax_bytes();
        let bytes2 = build_minimal_pax_bytes();

        let path1 = write_tempfile(&bytes1, "in1.pax").expect("failed to write input 1");
        let path2 = write_tempfile(&bytes2, "in2.pax").expect("failed to write input 2");

        let output_path = std::env::temp_dir().join("paideia_as_link_test_output.pax");

        let result = link(&[path1, path2], &output_path);
        assert!(result.is_ok(), "link should succeed");
        assert!(output_path.exists(), "output file should exist");

        // Verify the output parses
        let output_bytes = std::fs::read(&output_path).expect("failed to read output file");
        let header = PaxHeader::from_bytes(&output_bytes).expect("failed to parse output header");
        assert_eq!(header.magic, *b"PAX\0");

        // Clean up
        let _ = std::fs::remove_file(&output_path);
    }
}
