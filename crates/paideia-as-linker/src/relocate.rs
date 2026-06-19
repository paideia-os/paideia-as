//! Relocate phase of the paideia-link linker.
//!
//! Phase 3: Apply relocation entries to section content bytes. Each relocation
//! specifies a target offset within a section, a symbol reference, a relocation kind,
//! and an addend. The relocator walks each input's .relocs section, resolves the
//! symbol to a global id or address, and writes the result + addend to the target.
//!
//! Phase-2-m11 minimum: Only Abs64 relocations are applied; other kinds log a TODO.

use paideia_as_emitter_pax::{RelocEntry, RelocKind, SectionType};

use crate::parse::ParsedPax;
use crate::resolve::{ResolvedLink, ResolvedPax};

/// Error type for relocation phase (currently unused; reserved for future).
#[derive(Debug, Clone)]
pub struct RelocationError;

impl std::fmt::Display for RelocationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "relocation error")
    }
}

impl std::error::Error for RelocationError {}

/// Per-PAX relocated section bytes.
///
/// After relocation, each section's content is updated with resolved addresses
/// and addends.
#[derive(Debug, Clone)]
pub struct RelocatedPax {
    /// Index into the RelocatedLink.pax_files array.
    pub index: usize,
    /// Each section's bytes after relocations are applied.
    /// Keyed by section index in the original section table.
    pub section_contents: Vec<Vec<u8>>,
}

/// Result of the relocate phase.
///
/// Contains the relocated section contents for all input PAX files.
#[derive(Default, Debug)]
pub struct RelocatedLink {
    /// Per-input PAX files with relocated section contents.
    pub pax_files: Vec<RelocatedPax>,
}

/// Find a section by type in a parsed PAX.
fn find_section_by_type(pax: &ParsedPax, ty: SectionType) -> Option<&[u8]> {
    for section in &pax.section_table.sections {
        if section.ty == ty {
            return Some(pax.section_content(section));
        }
    }
    None
}

/// Apply a single relocation entry to the section contents.
///
/// For Abs64 relocations, writes the resolved symbol address (represented as a global id
/// for now) + addend at the specified offset in the target section (section 0 for m11).
/// Other kinds emit a TODO log comment and do not modify content.
fn apply_reloc(
    sections: &mut [Vec<u8>],
    r: &RelocEntry,
    resolved_pax: &ResolvedPax,
    _resolved: &ResolvedLink,
) {
    match r.kind {
        RelocKind::Abs64 => {
            // Phase-2-m11: target section is always 0 (`.code`)
            if sections.is_empty() {
                return;
            }

            let section = &mut sections[0];

            // Offset within the section
            let offset = r.offset as usize;
            if offset + 8 > section.len() {
                // Out of bounds; skip silently for now
                return;
            }

            // Look up the symbol's global id (blake3_name_hash)
            let symbol_id =
                if let Some(&global_hash) = resolved_pax.symbol_mapping.get(&r.symbol_index) {
                    global_hash
                } else {
                    // Symbol not in mapping; use symbol_index as fallback
                    r.symbol_index
                };

            // Compute the final value: symbol_id + addend
            let value = (symbol_id as i64).wrapping_add(r.addend) as u64;

            // Write as little-endian u64
            section[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        }
        RelocKind::Pc32 | RelocKind::GotPc32 | RelocKind::PltPc32 | RelocKind::CapBind => {
            // Phase-2-m11: log TODO and skip
            eprintln!("TODO: relocation kind {:?} not yet implemented", r.kind);
        }
    }
}

/// Apply each input's .relocs to its target section bytes.
///
/// Walks each input's relocation section, resolves each symbol via the global
/// symbol table, and writes the relocated value at the specified offset.
/// Returns a RelocatedLink containing the relocated section contents.
pub fn relocate_inputs(inputs: &[ParsedPax], resolved: &ResolvedLink) -> RelocatedLink {
    let mut pax_files = Vec::with_capacity(inputs.len());

    for (i, pax) in inputs.iter().enumerate() {
        let resolved_pax = &resolved.pax_files[i];

        // Copy section contents into mutable vectors
        let mut section_contents: Vec<Vec<u8>> = pax
            .section_table
            .sections
            .iter()
            .map(|s| pax.section_content(s).to_vec())
            .collect();

        // Find and apply relocations if present
        if let Some(reloc_bytes) = find_section_by_type(pax, SectionType::Relocs)
            && let Some(relocs) = paideia_as_emitter_pax::Relocs::from_bytes(reloc_bytes)
        {
            for r in &relocs.entries {
                apply_reloc(&mut section_contents, r, resolved_pax, resolved);
            }
        }

        pax_files.push(RelocatedPax {
            index: i,
            section_contents,
        });
    }

    RelocatedLink { pax_files }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::VecSink;
    use paideia_as_emitter_pax::{
        Architecture, PaxHeader, RelocEntry, Relocs, Section, SectionTable,
    };
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn write_tempfile(content: &[u8]) -> std::io::Result<std::path::PathBuf> {
        let counter = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let temp_dir = std::env::temp_dir();
        let filename = format!("paideia_as_relocate_test_{}.pax", counter);
        let path = temp_dir.join(&filename);

        let mut file = std::fs::File::create(&path)?;
        file.write_all(content)?;

        Ok(path)
    }

    fn build_pax_with_relocs(code_content: &[u8], relocs: Option<Relocs>) -> Vec<u8> {
        let mut header = PaxHeader::new(Architecture::X86_64);
        header.blake3_content_hash = *b"0123456789ABCDEF0123456789ABCDEF";

        // Compute the relocs bytes upfront
        let relocs_bytes_opt = relocs.map(|r| r.to_bytes());

        // Compute offsets
        let section_table_offset = 96u64;
        let section_table_size = if relocs_bytes_opt.is_some() { 128 } else { 64 };
        let code_offset = section_table_offset + section_table_size;
        let relocs_offset = code_offset + code_content.len() as u64;

        // Build section table
        let mut section_table = SectionTable::new();
        section_table.push(Section::code(code_offset, code_content.len() as u64));
        if let Some(ref relocs_bytes) = relocs_bytes_opt {
            section_table.push(Section {
                ty: SectionType::Relocs,
                flags: 0,
                content_offset: relocs_offset,
                content_size: relocs_bytes.len() as u64,
                virtual_address: 0,
                alignment: 8,
                name: ".relocs".to_string(),
            });
        }

        // Update header
        header.section_table_offset = section_table_offset;
        header.section_count = section_table.len() as u32;

        // Build final result
        let mut result = vec![];
        result.extend_from_slice(&header.to_bytes());
        result.extend_from_slice(&section_table.to_bytes());
        result.extend_from_slice(code_content);
        if let Some(relocs_bytes) = relocs_bytes_opt {
            result.extend_from_slice(&relocs_bytes);
        }

        result
    }

    #[test]
    fn relocate_no_relocs_passes_through() {
        let code_content = vec![0xABu8; 64];
        let bytes = build_pax_with_relocs(&code_content, None);
        let path = write_tempfile(&bytes).expect("failed to write tempfile");

        let parsed = crate::parse::parse_pax(path).expect("parse failed");
        let mut sink = VecSink::new();
        let resolved = crate::resolve::resolve_inputs(std::slice::from_ref(&parsed), &mut sink)
            .expect("resolve failed");

        let relocated = relocate_inputs(std::slice::from_ref(&parsed), &resolved);

        assert_eq!(relocated.pax_files.len(), 1);
        let relocated_pax = &relocated.pax_files[0];
        assert_eq!(relocated_pax.section_contents.len(), 1);
        assert_eq!(relocated_pax.section_contents[0], code_content);
    }

    #[test]
    fn relocate_one_abs64_writes_value_at_offset() {
        let code_content = vec![0u8; 64];
        let mut relocs = Relocs::new();

        // Create a relocation: offset 16, symbol_index 0, Abs64, addend 0x100
        let reloc = RelocEntry::new(16, 0, RelocKind::Abs64, 0x100);
        relocs.push(reloc);

        let bytes = build_pax_with_relocs(&code_content, Some(relocs));
        let path = write_tempfile(&bytes).expect("failed to write tempfile");

        let parsed = crate::parse::parse_pax(path).expect("parse failed");
        let mut sink = VecSink::new();
        let resolved = crate::resolve::resolve_inputs(std::slice::from_ref(&parsed), &mut sink)
            .expect("resolve failed");

        let relocated = relocate_inputs(std::slice::from_ref(&parsed), &resolved);

        assert_eq!(relocated.pax_files.len(), 1);
        let relocated_pax = &relocated.pax_files[0];
        let section = &relocated_pax.section_contents[0];

        // Read the 8 bytes at offset 16
        let value_bytes = &section[16..24];
        let value = u64::from_le_bytes([
            value_bytes[0],
            value_bytes[1],
            value_bytes[2],
            value_bytes[3],
            value_bytes[4],
            value_bytes[5],
            value_bytes[6],
            value_bytes[7],
        ]);

        // Should be 0 (symbol_id) + 0x100 (addend) = 0x100
        assert_eq!(value, 0x100);
    }

    #[test]
    fn relocate_unknown_kind_does_not_panic() {
        let code_content = vec![0u8; 64];
        let mut relocs = Relocs::new();

        // Create a PltPc32 relocation (not yet supported)
        let reloc = RelocEntry::new(0, 0, RelocKind::PltPc32, 0);
        relocs.push(reloc);

        let bytes = build_pax_with_relocs(&code_content, Some(relocs));
        let path = write_tempfile(&bytes).expect("failed to write tempfile");

        let parsed = crate::parse::parse_pax(path).expect("parse failed");
        let mut sink = VecSink::new();
        let resolved = crate::resolve::resolve_inputs(std::slice::from_ref(&parsed), &mut sink)
            .expect("resolve failed");

        // Should not panic
        let relocated = relocate_inputs(std::slice::from_ref(&parsed), &resolved);

        assert_eq!(relocated.pax_files.len(), 1);
        let relocated_pax = &relocated.pax_files[0];
        // Content should be unchanged for unsupported kinds
        assert_eq!(relocated_pax.section_contents[0], code_content);
    }

    #[test]
    fn relocated_link_has_same_pax_count_as_inputs() {
        let code_content = vec![0u8; 64];

        let bytes1 = build_pax_with_relocs(&code_content, None);
        let bytes2 = build_pax_with_relocs(&code_content, None);
        let bytes3 = build_pax_with_relocs(&code_content, None);

        let path1 = write_tempfile(&bytes1).expect("failed to write tempfile");
        let path2 = write_tempfile(&bytes2).expect("failed to write tempfile");
        let path3 = write_tempfile(&bytes3).expect("failed to write tempfile");

        let parsed1 = crate::parse::parse_pax(path1).expect("parse failed");
        let parsed2 = crate::parse::parse_pax(path2).expect("parse failed");
        let parsed3 = crate::parse::parse_pax(path3).expect("parse failed");

        let mut sink = VecSink::new();
        let inputs = vec![parsed1, parsed2, parsed3];
        let resolved = crate::resolve::resolve_inputs(&inputs, &mut sink).expect("resolve failed");

        let relocated = relocate_inputs(&inputs, &resolved);

        assert_eq!(relocated.pax_files.len(), 3);
    }
}
