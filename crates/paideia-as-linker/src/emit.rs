//! Emit phase of the paideia-link linker.
//!
//! Phase 4: Produce the final linked PAX byte stream.
//!
//! Takes the resolved and relocated input files and produces a single output PAX
//! with:
//! - Header with Executable flag (the output is not relocatable)
//! - Section table from the first input (or merged in later phases)
//! - Section contents post-relocation, concatenated by section index
//! - Updated BLAKE3 content hash
//!
//! Phase-2-m11 minimum: Uses the first input's section table; subsequent inputs'
//! contents are appended to matching sections by index. Full section-aware merging
//! is deferred to m4-012+.

use paideia_as_emitter_pax::{
    Architecture, HeaderFlag, PaxHeader, SectionTable, compute_content_hash,
};

use crate::parse::ParsedPax;
use crate::relocate::RelocatedLink;
use crate::resolve::ResolvedLink;

/// Emit the final linked PAX byte stream.
///
/// Combines all input files' sections (using the first input's section table)
/// and produces a single output PAX with:
/// - Executable flag set
/// - All section contents in order
/// - Computed BLAKE3 content hash
pub fn emit_final_pax(
    inputs: &[ParsedPax],
    _resolved: &ResolvedLink,
    relocated: &RelocatedLink,
) -> Vec<u8> {
    if inputs.is_empty() {
        // Empty input; return a minimal valid PAX
        return emit_minimal_pax();
    }

    // Use first input's section table and architecture
    let first_input = &inputs[0];
    let first_relocated = &relocated.pax_files[0];

    let mut header = first_input.header.clone();
    let section_table = first_input.section_table.clone();

    // Set the Executable flag
    header.flags |= HeaderFlag::Executable as u64;

    // Collect all section contents, merging by section index
    let mut merged_sections: Vec<Vec<u8>> = vec![Vec::new(); section_table.sections.len()];

    // Add first input's sections
    for (idx, content) in first_relocated.section_contents.iter().enumerate() {
        if idx < merged_sections.len() {
            merged_sections[idx].extend_from_slice(content);
        }
    }

    // Add subsequent inputs' sections to matching indices
    for relocated_pax in relocated.pax_files.iter().skip(1) {
        for (idx, content) in relocated_pax.section_contents.iter().enumerate() {
            if idx < merged_sections.len() {
                merged_sections[idx].extend_from_slice(content);
            }
        }
    }

    // Compute the content hash
    let content_refs: Vec<&[u8]> = merged_sections.iter().map(|v| v.as_slice()).collect();
    let hash = compute_content_hash(&header, &section_table, &content_refs);
    header.blake3_content_hash = hash;

    // Build the final byte stream
    let mut result = vec![];

    // Write header
    result.extend_from_slice(&header.to_bytes());

    // Update section table offset (immediately after header)
    let section_table_offset = result.len() as u64;
    header.section_table_offset = section_table_offset;

    // Write section table
    result.extend_from_slice(&section_table.to_bytes());

    // Write section contents in order
    for content in &merged_sections {
        result.extend_from_slice(content);
    }

    // Re-serialize header with correct offset and hash
    let mut header_bytes = [0u8; 96];
    let mut hdr = header.clone();
    hdr.section_table_offset = section_table_offset;
    hdr.blake3_content_hash = hash;
    header_bytes.copy_from_slice(&hdr.to_bytes());

    // Replace header in result
    result[0..96].copy_from_slice(&header_bytes);

    result
}

/// Emit a minimal valid PAX for empty input.
fn emit_minimal_pax() -> Vec<u8> {
    let mut header = PaxHeader::new(Architecture::X86_64);
    header.flags |= HeaderFlag::Executable as u64;
    header.section_table_offset = 96;
    header.section_count = 0;

    let section_table = SectionTable::new();
    let content_refs: Vec<&[u8]> = vec![];

    let hash = compute_content_hash(&header, &section_table, &content_refs);
    header.blake3_content_hash = hash;

    let mut result = vec![];
    result.extend_from_slice(&header.to_bytes());
    result.extend_from_slice(&section_table.to_bytes());

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::VecSink;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn write_tempfile(content: &[u8]) -> std::io::Result<std::path::PathBuf> {
        let counter = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let temp_dir = std::env::temp_dir();
        let filename = format!("paideia_as_emit_test_{}.pax", counter);
        let path = temp_dir.join(&filename);

        let mut file = std::fs::File::create(&path)?;
        file.write_all(content)?;

        Ok(path)
    }

    fn build_minimal_pax_bytes() -> Vec<u8> {
        let mut header = PaxHeader::new(Architecture::X86_64);
        header.section_table_offset = 96;
        header.section_count = 0;
        header.blake3_content_hash = *b"0123456789ABCDEF0123456789ABCDEF";

        let mut bytes = vec![];
        bytes.extend_from_slice(&header.to_bytes());

        bytes
    }

    #[test]
    fn emit_single_input_writes_valid_pax() {
        let bytes = build_minimal_pax_bytes();
        let path = write_tempfile(&bytes).expect("failed to write tempfile");

        let parsed = crate::parse::parse_pax(path).expect("parse failed");
        let mut sink = VecSink::new();
        let resolved = crate::resolve::resolve_inputs(std::slice::from_ref(&parsed), &mut sink)
            .expect("resolve failed");
        let relocated = crate::relocate::relocate_inputs(std::slice::from_ref(&parsed), &resolved);

        let output = emit_final_pax(std::slice::from_ref(&parsed), &resolved, &relocated);

        // Check magic number
        assert_eq!(&output[0..4], b"PAX\0");
    }

    #[test]
    fn emit_output_has_executable_flag_set() {
        let bytes = build_minimal_pax_bytes();
        let path = write_tempfile(&bytes).expect("failed to write tempfile");

        let parsed = crate::parse::parse_pax(path).expect("parse failed");
        let mut sink = VecSink::new();
        let resolved = crate::resolve::resolve_inputs(std::slice::from_ref(&parsed), &mut sink)
            .expect("resolve failed");
        let relocated = crate::relocate::relocate_inputs(std::slice::from_ref(&parsed), &resolved);

        let output = emit_final_pax(std::slice::from_ref(&parsed), &resolved, &relocated);

        // Parse header and check flags
        let header = PaxHeader::from_bytes(&output).expect("failed to parse output header");
        assert_ne!(
            header.flags & (HeaderFlag::Executable as u64),
            0,
            "Executable flag should be set"
        );
    }

    #[test]
    fn emit_output_hash_is_nonzero() {
        let bytes = build_minimal_pax_bytes();
        let path = write_tempfile(&bytes).expect("failed to write tempfile");

        let parsed = crate::parse::parse_pax(path).expect("parse failed");
        let mut sink = VecSink::new();
        let resolved = crate::resolve::resolve_inputs(std::slice::from_ref(&parsed), &mut sink)
            .expect("resolve failed");
        let relocated = crate::relocate::relocate_inputs(std::slice::from_ref(&parsed), &resolved);

        let output = emit_final_pax(std::slice::from_ref(&parsed), &resolved, &relocated);

        // Parse header and check hash
        let header = PaxHeader::from_bytes(&output).expect("failed to parse output header");
        assert!(!header.blake3_content_hash.iter().all(|&b| b == 0));
    }

    #[test]
    fn emit_2_input_link_snapshot() {
        let bytes1 = build_minimal_pax_bytes();
        let bytes2 = build_minimal_pax_bytes();

        let path1 = write_tempfile(&bytes1).expect("failed to write tempfile");
        let path2 = write_tempfile(&bytes2).expect("failed to write tempfile");

        let parsed1 = crate::parse::parse_pax(path1).expect("parse failed");
        let parsed2 = crate::parse::parse_pax(path2).expect("parse failed");

        let mut sink = VecSink::new();
        let inputs = vec![parsed1, parsed2];
        let resolved = crate::resolve::resolve_inputs(&inputs, &mut sink).expect("resolve failed");
        let relocated = crate::relocate::relocate_inputs(&inputs, &resolved);

        let output = emit_final_pax(&inputs, &resolved, &relocated);

        // Verify it parses
        let header = PaxHeader::from_bytes(&output).expect("failed to parse output header");
        let section_table = SectionTable::from_bytes(
            &output[header.section_table_offset as usize..],
            header.section_count,
        )
        .expect("failed to parse section table");

        assert_eq!(header.magic, *b"PAX\0");
        assert_eq!(section_table.len(), 0);
    }

    #[test]
    fn emit_output_is_loadable_by_paxintrospect_test_pattern() {
        let bytes = build_minimal_pax_bytes();
        let path = write_tempfile(&bytes).expect("failed to write tempfile");

        let parsed = crate::parse::parse_pax(path).expect("parse failed");
        let mut sink = VecSink::new();
        let resolved = crate::resolve::resolve_inputs(std::slice::from_ref(&parsed), &mut sink)
            .expect("resolve failed");
        let relocated = crate::relocate::relocate_inputs(std::slice::from_ref(&parsed), &resolved);

        let output = emit_final_pax(std::slice::from_ref(&parsed), &resolved, &relocated);

        // The output file should parse cleanly
        let header = PaxHeader::from_bytes(&output).expect("failed to parse output header");
        let _section_table = SectionTable::from_bytes(
            &output[header.section_table_offset as usize..],
            header.section_count,
        )
        .expect("failed to parse section table");

        assert_eq!(header.magic, *b"PAX\0");
    }
}
