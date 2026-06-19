//! Resolve phase of the paideia-link linker.
//!
//! Phase 2: Build a global symbol table across all input PAXes, match each undefined symbol
//! against an export, and match each unbound capability against an import.
//!
//! Produces:
//! - GlobalSymbolTable: maps blake3_name_hash → (pax_index, local_symbol_index) for defined symbols
//! - GlobalCapabilityTable: maps blake3_name_hash → pax_index for exported capabilities
//! - Per-PAX symbol_mapping: local_symbol_index → global_symbol_id
//!
//! Diagnostic codes:
//! - B1700: undefined symbol (no exporting PAX found)
//! - B1701: unbound capability (no export matches an import)

use std::collections::HashMap;
use std::path::PathBuf;

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticSink, Severity};
use paideia_as_emitter_pax::{ExportsSection, ImportsSection, SectionType, SymBinding, SymTab};

use crate::parse::ParsedPax;

/// Phase-2 resolution result.
///
/// `pax_files` carries the per-PAX resolution info needed by m4-011 (relocate).
/// Diagnostics are collected in the sink by resolve_inputs().
#[derive(Debug)]
pub struct ResolvedLink {
    /// Per-PAX resolution mappings.
    pub pax_files: Vec<ResolvedPax>,
    /// Global symbol table indexed by BLAKE3 name hash.
    pub global_symbol_table: GlobalSymbolTable,
    /// Global capability table indexed by BLAKE3 name hash.
    pub global_capability_table: GlobalCapabilityTable,
}

/// Per-PAX resolution info.
#[derive(Debug)]
pub struct ResolvedPax {
    /// Index into ResolvedLink.pax_files.
    pub index: usize,
    /// Path to the PAX file.
    pub path: PathBuf,
    /// Map: local symbol_index → global symbol id (blake3_name_hash).
    /// Only populated for defined symbols and undefined symbols that were resolved.
    pub symbol_mapping: HashMap<u64, u64>,
}

/// Global symbol table indexed by BLAKE3 name hash.
#[derive(Default, Debug)]
pub struct GlobalSymbolTable {
    /// Map: blake3_name_hash → (defining_pax_index, local_symbol_index).
    /// Strong symbols win over Weak when both exist for the same hash.
    pub by_hash: HashMap<u64, (usize, u64)>,
    /// Map: blake3_name_hash → SymBinding (to track strong vs weak).
    pub bindings: HashMap<u64, SymBinding>,
}

/// Global capability table indexed by BLAKE3 name hash.
#[derive(Default, Debug)]
pub struct GlobalCapabilityTable {
    /// Map: blake3_name_hash → defining_pax_index.
    pub provided: HashMap<u64, usize>,
}

/// Find a section by type in a parsed PAX.
fn find_section_content_by_type(pax: &ParsedPax, ty: SectionType) -> Option<&[u8]> {
    for section in &pax.section_table.sections {
        if section.ty == ty {
            return Some(pax.section_content(section));
        }
    }
    None
}

/// Run the resolve phase across all parsed inputs.
///
/// 1. Walk every input's SymTab; index defined symbols (section_index != 0xFFFFFFFF)
///    into GlobalSymbolTable.
/// 2. For each undefined symbol in each input, look it up; emit B1700 if missing.
/// 3. Walk every input's ExportsSection; index by blake3_name_hash.
/// 4. For each ImportsSection entry, look it up; emit B1701 if missing.
///
/// Returns Some(ResolvedLink) if no errors, None if any B1700/B1701 were emitted.
/// The caller still gets the partial result for diagnostics.
pub fn resolve_inputs(inputs: &[ParsedPax], sink: &mut dyn DiagnosticSink) -> Option<ResolvedLink> {
    let mut global_symbol_table = GlobalSymbolTable::default();
    let mut global_capability_table = GlobalCapabilityTable::default();
    let mut pax_files = vec![];
    let mut had_errors = false;

    // Phase 1: Index all defined symbols from all PAX files.
    for (pax_index, pax) in inputs.iter().enumerate() {
        if let Some(symtab_bytes) = find_section_content_by_type(pax, SectionType::Symtab)
            && let Some(symtab) = SymTab::from_bytes(symtab_bytes)
        {
            for (local_idx, entry) in symtab.entries.iter().enumerate() {
                // Only defined symbols (section_index != 0xFFFFFFFF)
                if entry.section_index != 0xFFFFFFFF {
                    let hash = entry.blake3_name_hash;
                    let local_idx_u64 = local_idx as u64;

                    // Strong symbols win over Weak
                    match global_symbol_table.bindings.get(&hash) {
                        None => {
                            // First occurrence
                            global_symbol_table
                                .by_hash
                                .insert(hash, (pax_index, local_idx_u64));
                            global_symbol_table.bindings.insert(hash, entry.binding);
                        }
                        Some(&SymBinding::Weak) => {
                            // Weak exists; strong always overrides
                            if entry.binding != SymBinding::Weak {
                                global_symbol_table
                                    .by_hash
                                    .insert(hash, (pax_index, local_idx_u64));
                                global_symbol_table.bindings.insert(hash, entry.binding);
                            }
                            // If both weak, keep first
                        }
                        Some(&SymBinding::Local | &SymBinding::Global) => {
                            // Strong or local exists; only strong overrides
                            if entry.binding == SymBinding::Global
                                && global_symbol_table.bindings[&hash] == SymBinding::Weak
                            {
                                global_symbol_table
                                    .by_hash
                                    .insert(hash, (pax_index, local_idx_u64));
                                global_symbol_table.bindings.insert(hash, entry.binding);
                            }
                            // Otherwise keep the existing one
                        }
                    }
                }
            }
        }
    }

    // Phase 2: Index all exported capabilities.
    for (pax_index, pax) in inputs.iter().enumerate() {
        if let Some(exports_bytes) = find_section_content_by_type(pax, SectionType::Exports)
            && let Some(exports) = ExportsSection::from_bytes(exports_bytes)
        {
            for cap in &exports.entries {
                global_capability_table
                    .provided
                    .insert(cap.blake3_name_hash, pax_index);
            }
        }
    }

    // Phase 3: Check undefined symbols and emit L0700 for missing ones.
    for pax in inputs {
        if let Some(symtab_bytes) = find_section_content_by_type(pax, SectionType::Symtab)
            && let Some(symtab) = SymTab::from_bytes(symtab_bytes)
        {
            for entry in &symtab.entries {
                // Only undefined symbols (section_index == 0xFFFFFFFF)
                if entry.section_index == 0xFFFFFFFF {
                    let hash = entry.blake3_name_hash;
                    if !global_symbol_table.by_hash.contains_key(&hash) {
                        // L0700: undefined symbol
                        had_errors = true;
                        let code = paideia_as_diagnostics::DiagnosticCode::new(
                            Category::B,
                            Severity::Error,
                            1700,
                        )
                        .expect("B1700 should be valid");
                        let diag = Diagnostic::error(code)
                            .message(format!(
                                "undefined symbol: {} (hash: {:#018x})",
                                pax.path.display(),
                                hash
                            ))
                            .finish();
                        let _ = sink.emit(diag);
                    }
                }
            }
        }
    }

    // Phase 4: Check imported capabilities and emit L0701 for missing ones.
    for pax in inputs {
        if let Some(imports_bytes) = find_section_content_by_type(pax, SectionType::Imports)
            && let Some(imports) = ImportsSection::from_bytes(imports_bytes)
        {
            for cap in &imports.entries {
                if !global_capability_table
                    .provided
                    .contains_key(&cap.blake3_name_hash)
                {
                    // L0701: unbound capability
                    had_errors = true;
                    let code = paideia_as_diagnostics::DiagnosticCode::new(
                        Category::B,
                        Severity::Error,
                        1701,
                    )
                    .expect("B1701 should be valid");
                    let diag = Diagnostic::error(code)
                        .message(format!(
                            "unbound capability: {} (hash: {:#018x})",
                            pax.path.display(),
                            cap.blake3_name_hash
                        ))
                        .finish();
                    let _ = sink.emit(diag);
                }
            }
        }
    }

    // Phase 5: Build per-PAX symbol mappings.
    for (pax_index, pax) in inputs.iter().enumerate() {
        let mut symbol_mapping = HashMap::new();

        if let Some(symtab_bytes) = find_section_content_by_type(pax, SectionType::Symtab)
            && let Some(symtab) = SymTab::from_bytes(symtab_bytes)
        {
            for (local_idx, entry) in symtab.entries.iter().enumerate() {
                let hash = entry.blake3_name_hash;
                // Map this local index to the global hash
                symbol_mapping.insert(local_idx as u64, hash);
            }
        }

        pax_files.push(ResolvedPax {
            index: pax_index,
            path: pax.path.clone(),
            symbol_mapping,
        });
    }

    if had_errors {
        None
    } else {
        Some(ResolvedLink {
            pax_files,
            global_symbol_table,
            global_capability_table,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_emitter_pax::{
        Architecture, CapDescriptor, CapKind, LinClass, PaxHeader, Section, SectionTable, SymEntry,
    };
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn write_tempfile(content: &[u8]) -> std::io::Result<PathBuf> {
        let counter = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let temp_dir = std::env::temp_dir();
        let filename = format!("paideia_as_resolve_test_{}.pax", counter);
        let path = temp_dir.join(&filename);

        let mut file = std::fs::File::create(&path)?;
        file.write_all(content)?;

        Ok(path)
    }

    fn build_pax_with_symtab(
        entries: Vec<SymEntry>,
        imports: Option<Vec<CapDescriptor>>,
        exports: Option<Vec<CapDescriptor>>,
    ) -> Vec<u8> {
        let mut header = PaxHeader::new(Architecture::X86_64);
        header.blake3_content_hash = *b"0123456789ABCDEF0123456789ABCDEF";

        let mut section_table = SectionTable::new();
        let mut bytes = vec![];

        // Write header
        bytes.extend_from_slice(&header.to_bytes());
        let section_table_offset = bytes.len() as u64;

        // Prepare sections
        let mut current_offset = section_table_offset + 64; // Placeholder for section table

        // Symtab section (if entries exist)
        let symtab_section = if !entries.is_empty() {
            let symtab = SymTab {
                entries: entries.clone(),
            };
            let symtab_bytes = symtab.to_bytes();
            let section = Section::code(current_offset, symtab_bytes.len() as u64);
            section_table.push(Section {
                ty: SectionType::Symtab,
                name: ".symtab".to_string(),
                ..section
            });
            current_offset += symtab_bytes.len() as u64;
            Some(symtab_bytes)
        } else {
            None
        };

        // Imports section (if present)
        let imports_bytes_opt = imports.map(|caps| {
            let imports = ImportsSection { entries: caps };
            let imports_bytes = imports.to_bytes();
            let section = Section::code(current_offset, imports_bytes.len() as u64);
            section_table.push(Section {
                ty: SectionType::Imports,
                name: ".imports".to_string(),
                ..section
            });
            current_offset += imports_bytes.len() as u64;
            imports_bytes
        });

        // Exports section (if present)
        let exports_bytes_opt = exports.map(|caps| {
            let exports = ExportsSection { entries: caps };
            let exports_bytes = exports.to_bytes();
            let section = Section::code(current_offset, exports_bytes.len() as u64);
            section_table.push(Section {
                ty: SectionType::Exports,
                name: ".exports".to_string(),
                ..section
            });
            current_offset += exports_bytes.len() as u64;
            exports_bytes
        });

        // Update header with section info
        header.section_table_offset = section_table_offset;
        header.section_count = section_table.len() as u32;

        // Rebuild with correct offsets
        let mut final_bytes = vec![];
        final_bytes.extend_from_slice(&header.to_bytes());
        final_bytes.extend_from_slice(&section_table.to_bytes());

        if let Some(symtab_bytes) = symtab_section {
            final_bytes.extend_from_slice(&symtab_bytes);
        }
        if let Some(imports_bytes) = imports_bytes_opt {
            final_bytes.extend_from_slice(&imports_bytes);
        }
        if let Some(exports_bytes) = exports_bytes_opt {
            final_bytes.extend_from_slice(&exports_bytes);
        }

        final_bytes
    }

    #[test]
    fn resolve_empty_input_succeeds() {
        let inputs = vec![];
        let mut sink = paideia_as_diagnostics::VecSink::new();

        let result = resolve_inputs(&inputs, &mut sink);
        assert!(result.is_some(), "empty input should resolve successfully");

        let resolved = result.unwrap();
        assert_eq!(resolved.pax_files.len(), 0);
        assert!(resolved.global_symbol_table.by_hash.is_empty());
        assert!(resolved.global_capability_table.provided.is_empty());
    }

    #[test]
    fn resolve_single_self_contained_pax_succeeds() {
        let sym1 = SymEntry::new(
            0x1000,
            0x100,
            1,
            SymBinding::Global,
            paideia_as_emitter_pax::SymVisibility::Default,
            0,
            0xABCD,
        );

        let bytes = build_pax_with_symtab(vec![sym1], None, None);
        let path = write_tempfile(&bytes).expect("failed to write tempfile");

        let parsed = crate::parse::parse_pax(path.clone()).expect("parse failed");
        let mut sink = paideia_as_diagnostics::VecSink::new();

        let result = resolve_inputs(&[parsed], &mut sink);
        assert!(
            result.is_some(),
            "self-contained PAX should resolve successfully"
        );

        let resolved = result.unwrap();
        assert_eq!(resolved.pax_files.len(), 1);
        assert_eq!(resolved.global_symbol_table.by_hash.len(), 1);
        assert_eq!(sink.count(), 0, "no diagnostics should be emitted");
    }

    #[test]
    #[allow(non_snake_case)]
    fn resolve_missing_symbol_emits_L0700() {
        let undef_sym = SymEntry::new(
            0,
            0,
            0xFFFFFFFF,
            SymBinding::Global,
            paideia_as_emitter_pax::SymVisibility::Default,
            0,
            0xDEADBEEF,
        );

        let bytes = build_pax_with_symtab(vec![undef_sym], None, None);
        let path = write_tempfile(&bytes).expect("failed to write tempfile");

        let parsed = crate::parse::parse_pax(path.clone()).expect("parse failed");
        let mut sink = paideia_as_diagnostics::VecSink::new();

        let result = resolve_inputs(&[parsed], &mut sink);
        assert!(
            result.is_none(),
            "should fail when undefined symbol is not resolved"
        );
        assert_eq!(
            sink.count(),
            1,
            "exactly one diagnostic (L0700) should be emitted"
        );
        let diag = &sink.into_diagnostics()[0];
        assert_eq!(diag.code().category(), Category::B);
        assert_eq!(diag.code().number(), 1700);
    }

    #[test]
    fn resolve_matched_symbol_across_two_pax_succeeds() {
        // PAX A: undefined symbol with hash 0x1111
        let undef_sym = SymEntry::new(
            0,
            0,
            0xFFFFFFFF,
            SymBinding::Global,
            paideia_as_emitter_pax::SymVisibility::Default,
            0,
            0x1111,
        );

        let bytes_a = build_pax_with_symtab(vec![undef_sym], None, None);
        let path_a = write_tempfile(&bytes_a).expect("failed to write tempfile");

        // PAX B: defined symbol with hash 0x1111
        let def_sym = SymEntry::new(
            0x2000,
            0x100,
            1,
            SymBinding::Global,
            paideia_as_emitter_pax::SymVisibility::Default,
            0,
            0x1111,
        );

        let bytes_b = build_pax_with_symtab(vec![def_sym], None, None);
        let path_b = write_tempfile(&bytes_b).expect("failed to write tempfile");

        let parsed_a = crate::parse::parse_pax(path_a).expect("parse failed");
        let parsed_b = crate::parse::parse_pax(path_b).expect("parse failed");
        let mut sink = paideia_as_diagnostics::VecSink::new();

        let result = resolve_inputs(&[parsed_a, parsed_b], &mut sink);
        assert!(
            result.is_some(),
            "symbol should be resolved across PAX files"
        );
        assert_eq!(sink.count(), 0, "no diagnostics should be emitted");

        let resolved = result.unwrap();
        assert_eq!(resolved.global_symbol_table.by_hash.len(), 1);
        let (def_pax_idx, _) = resolved.global_symbol_table.by_hash[&0x1111];
        assert_eq!(
            def_pax_idx, 1,
            "defined symbol should come from PAX B (index 1)"
        );
    }

    #[test]
    #[allow(non_snake_case)]
    fn resolve_unbound_capability_emits_L0701() {
        let unbound_cap =
            CapDescriptor::new(0, 0xCAFEBABE, CapKind::MmioMemCap, LinClass::Linear, 0);

        let bytes = build_pax_with_symtab(vec![], Some(vec![unbound_cap]), None);
        let path = write_tempfile(&bytes).expect("failed to write tempfile");

        let parsed = crate::parse::parse_pax(path.clone()).expect("parse failed");
        let mut sink = paideia_as_diagnostics::VecSink::new();

        let result = resolve_inputs(&[parsed], &mut sink);
        assert!(
            result.is_none(),
            "should fail when capability is not provided"
        );
        assert_eq!(
            sink.count(),
            1,
            "exactly one diagnostic (L0701) should be emitted"
        );
        let diag = &sink.into_diagnostics()[0];
        assert_eq!(diag.code().category(), Category::B);
        assert_eq!(diag.code().number(), 1701);
    }

    #[test]
    fn resolve_matched_capability_across_two_pax_succeeds() {
        // PAX A: imports capability with hash 0x2222
        let import_cap = CapDescriptor::new(0, 0x2222, CapKind::IpcChannel, LinClass::Affine, 0);
        let bytes_a = build_pax_with_symtab(vec![], Some(vec![import_cap]), None);
        let path_a = write_tempfile(&bytes_a).expect("failed to write tempfile");

        // PAX B: exports capability with hash 0x2222
        let export_cap = CapDescriptor::new(0, 0x2222, CapKind::IpcChannel, LinClass::Affine, 0);
        let bytes_b = build_pax_with_symtab(vec![], None, Some(vec![export_cap]));
        let path_b = write_tempfile(&bytes_b).expect("failed to write tempfile");

        let parsed_a = crate::parse::parse_pax(path_a).expect("parse failed");
        let parsed_b = crate::parse::parse_pax(path_b).expect("parse failed");
        let mut sink = paideia_as_diagnostics::VecSink::new();

        let result = resolve_inputs(&[parsed_a, parsed_b], &mut sink);
        assert!(
            result.is_some(),
            "capability should be resolved across PAX files"
        );
        assert_eq!(sink.count(), 0, "no diagnostics should be emitted");

        let resolved = result.unwrap();
        assert_eq!(resolved.global_capability_table.provided.len(), 1);
        assert_eq!(resolved.global_capability_table.provided[&0x2222], 1);
    }

    #[test]
    fn resolve_weak_symbol_loses_to_strong() {
        // Weak symbol from PAX A
        let weak_sym = SymEntry::new(
            0x3000,
            0x100,
            1,
            SymBinding::Weak,
            paideia_as_emitter_pax::SymVisibility::Default,
            0,
            0x3333,
        );

        let bytes_a = build_pax_with_symtab(vec![weak_sym], None, None);
        let path_a = write_tempfile(&bytes_a).expect("failed to write tempfile");

        // Strong symbol from PAX B
        let strong_sym = SymEntry::new(
            0x4000,
            0x100,
            2,
            SymBinding::Global,
            paideia_as_emitter_pax::SymVisibility::Default,
            0,
            0x3333,
        );

        let bytes_b = build_pax_with_symtab(vec![strong_sym], None, None);
        let path_b = write_tempfile(&bytes_b).expect("failed to write tempfile");

        let parsed_a = crate::parse::parse_pax(path_a).expect("parse failed");
        let parsed_b = crate::parse::parse_pax(path_b).expect("parse failed");
        let mut sink = paideia_as_diagnostics::VecSink::new();

        let result = resolve_inputs(&[parsed_a, parsed_b], &mut sink);
        assert!(result.is_some(), "should resolve successfully");
        assert_eq!(sink.count(), 0, "no diagnostics should be emitted");

        let resolved = result.unwrap();
        let (winning_pax_idx, _) = resolved.global_symbol_table.by_hash[&0x3333];
        assert_eq!(
            winning_pax_idx, 1,
            "Global symbol should win; strong symbol from PAX B should be selected"
        );
        assert_eq!(
            resolved.global_symbol_table.bindings[&0x3333],
            SymBinding::Global
        );
    }

    #[test]
    fn resolved_link_contains_expected_pax_count() {
        let bytes1 = build_pax_with_symtab(vec![], None, None);
        let bytes2 = build_pax_with_symtab(vec![], None, None);
        let bytes3 = build_pax_with_symtab(vec![], None, None);

        let path1 = write_tempfile(&bytes1).expect("failed to write tempfile");
        let path2 = write_tempfile(&bytes2).expect("failed to write tempfile");
        let path3 = write_tempfile(&bytes3).expect("failed to write tempfile");

        let parsed1 = crate::parse::parse_pax(path1).expect("parse failed");
        let parsed2 = crate::parse::parse_pax(path2).expect("parse failed");
        let parsed3 = crate::parse::parse_pax(path3).expect("parse failed");
        let mut sink = paideia_as_diagnostics::VecSink::new();

        let result = resolve_inputs(&[parsed1, parsed2, parsed3], &mut sink);
        assert!(result.is_some());

        let resolved = result.unwrap();
        assert_eq!(
            resolved.pax_files.len(),
            3,
            "should contain 3 pax_files entries (AC-3)"
        );
    }

    #[test]
    fn resolve_different_undefined_symbols_in_two_pax() {
        // PAX A: one undefined symbol
        let undef_sym1 = SymEntry::new(
            0,
            0,
            0xFFFFFFFF,
            SymBinding::Global,
            paideia_as_emitter_pax::SymVisibility::Default,
            0,
            0xABCDEF01,
        );

        let bytes_a = build_pax_with_symtab(vec![undef_sym1], None, None);
        let path_a = write_tempfile(&bytes_a).expect("failed to write tempfile");

        // PAX B: another undefined symbol
        let undef_sym2 = SymEntry::new(
            0,
            0,
            0xFFFFFFFF,
            SymBinding::Global,
            paideia_as_emitter_pax::SymVisibility::Default,
            0,
            0xDEADBEEF,
        );

        let bytes_b = build_pax_with_symtab(vec![undef_sym2], None, None);
        let path_b = write_tempfile(&bytes_b).expect("failed to write tempfile");

        let parsed_a = crate::parse::parse_pax(path_a).expect("parse failed");
        let parsed_b = crate::parse::parse_pax(path_b).expect("parse failed");
        let mut sink = paideia_as_diagnostics::VecSink::new();

        let result = resolve_inputs(&[parsed_a, parsed_b], &mut sink);
        assert!(result.is_none(), "should fail when undefined symbols exist");
        assert_eq!(
            sink.count(),
            2,
            "should collect 2 diagnostics (one B1700 for each undefined symbol)"
        );
    }

    #[test]
    fn resolve_preserves_local_symbol_indices_in_mapping() {
        let sym1 = SymEntry::new(
            0x1000,
            0x100,
            1,
            SymBinding::Global,
            paideia_as_emitter_pax::SymVisibility::Default,
            0,
            0x1111,
        );
        let sym2 = SymEntry::new(
            0x2000,
            0x200,
            1,
            SymBinding::Global,
            paideia_as_emitter_pax::SymVisibility::Default,
            0,
            0x2222,
        );

        let bytes = build_pax_with_symtab(vec![sym1, sym2], None, None);
        let path = write_tempfile(&bytes).expect("failed to write tempfile");

        let parsed = crate::parse::parse_pax(path).expect("parse failed");
        let mut sink = paideia_as_diagnostics::VecSink::new();

        let result = resolve_inputs(&[parsed], &mut sink);
        assert!(result.is_some());

        let resolved = result.unwrap();
        let resolved_pax = &resolved.pax_files[0];
        assert_eq!(resolved_pax.symbol_mapping.len(), 2);
        assert_eq!(resolved_pax.symbol_mapping[&0], 0x1111);
        assert_eq!(resolved_pax.symbol_mapping[&1], 0x2222);
    }
}
