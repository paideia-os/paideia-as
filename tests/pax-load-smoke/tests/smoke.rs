//! Mock supervisor PAX loader smoke tests.
//!
//! This harness validates that the mock supervisor can:
//! - Load PAX files from disk
//! - Parse capability/effect/export/import/symbol sections
//! - Dispatch to named exports (symbolic, no execution)
//! - Identify entry-point symbols (Global + Default)

use paideia_as_emitter_pax::{
    Architecture, CapDescriptor, CapEntry, CapKind, CapsSection, EffectRowEntry, EffectsSection,
    ExportsSection, HeaderFlag, LinClass, PAX_HEADER_SIZE, PaxHeader, SECTION_DESCRIPTOR_SIZE,
    Section, SectionTable, SectionType, SiteKind, SymBinding, SymEntry, SymTab, SymVisibility,
};
use pax_load_smoke::MockSupervisor;

/// Build a minimal "hello-world" PAX file programmatically.
///
/// Structure:
/// - Header (96 bytes): Executable flag set
/// - Section Table (5 sections × 64 bytes = 320 bytes)
///   - .code (16 bytes of zeros)
///   - .symtab (1 symbol: "hello_main" Global/Default)
///   - .paideia.caps (1 entry: MmioMemCap)
///   - .exports (1 entry: "hello_main")
///   - .paideia.effects (1 entry: empty effects)
/// - Content sections appended in order
///
/// Returns the complete file bytes.
fn build_hello_world_pax() -> Vec<u8> {
    let mut header = PaxHeader::new(Architecture::X86_64);
    header.flags = HeaderFlag::Executable as u64;

    // Section 0: .code (16 bytes of placeholder)
    let code_content = vec![0u8; 16];

    // Section 1: .symtab (1 symbol entry)
    // Symbol: "hello_main" @ offset 0, size 16, section_index 0 (code section)
    // Binding: Global, Visibility: Default
    let hello_main_hash = blake3::hash(b"hello_main");
    let hello_main_hash_u64 =
        u64::from_le_bytes(hello_main_hash.as_bytes()[..8].try_into().unwrap());

    let mut symtab = SymTab::new();
    symtab.push(SymEntry::new(
        0,                      // value (offset in .code)
        16,                     // size
        0,                      // section_index (points to .code)
        SymBinding::Global,     // binding
        SymVisibility::Default, // visibility
        0,                      // name_offset
        hello_main_hash_u64,    // blake3_name_hash
    ));
    let symtab_content = symtab.to_bytes();

    // Section 2: .paideia.caps (1 capability entry)
    // MmioMemCap at FunctionParam binding site, Linear class
    let mut caps = CapsSection::new();
    caps.push(CapEntry::new(
        SiteKind::FunctionParam,
        LinClass::Linear,
        0, // location_id (symbol 0)
        1, // lam_tag
        CapKind::MmioMemCap,
        "mmio_mem",
    ));
    let caps_content = caps.to_bytes();

    // Section 3: .exports (1 export entry)
    let mut exports = ExportsSection::new();
    exports.push(CapDescriptor::new(
        0,                   // name_offset
        hello_main_hash_u64, // blake3_name_hash (matches symbol)
        CapKind::MmioMemCap,
        LinClass::Linear,
        0, // flags (not optional, not deprecated)
    ));
    let exports_content = exports.to_bytes();

    // Section 4: .paideia.effects (1 effect entry)
    let mut effects = EffectsSection::new();
    effects.push(EffectRowEntry::new(
        0,      // function_symbol_id (symbol 0)
        vec![], // fixed_effects (empty for now)
        None,   // row_var_id (closed row)
    ));
    let effects_content = effects.to_bytes();

    // Build section table
    let mut sections = SectionTable::new();
    let mut current_offset = PAX_HEADER_SIZE as u64 + (5 * SECTION_DESCRIPTOR_SIZE) as u64;

    sections.push(Section::code(current_offset, code_content.len() as u64));
    current_offset += code_content.len() as u64;

    sections.push(Section {
        ty: SectionType::Symtab,
        flags: 0,
        content_offset: current_offset,
        content_size: symtab_content.len() as u64,
        virtual_address: 0,
        alignment: 8,
        name: ".symtab".to_owned(),
    });
    current_offset += symtab_content.len() as u64;

    sections.push(Section {
        ty: SectionType::Caps,
        flags: 0,
        content_offset: current_offset,
        content_size: caps_content.len() as u64,
        virtual_address: 0,
        alignment: 8,
        name: ".paideia.caps".to_owned(),
    });
    current_offset += caps_content.len() as u64;

    sections.push(Section {
        ty: SectionType::Exports,
        flags: 0,
        content_offset: current_offset,
        content_size: exports_content.len() as u64,
        virtual_address: 0,
        alignment: 8,
        name: ".exports".to_owned(),
    });
    current_offset += exports_content.len() as u64;

    sections.push(Section {
        ty: SectionType::Effects,
        flags: 0,
        content_offset: current_offset,
        content_size: effects_content.len() as u64,
        virtual_address: 0,
        alignment: 8,
        name: ".paideia.effects".to_owned(),
    });

    // Finalize header with section table info
    header.section_table_offset = PAX_HEADER_SIZE as u64;
    header.section_count = sections.len() as u32;

    // Compute BLAKE3 hash
    let content_hash = paideia_as_emitter_pax::compute_content_hash(
        &header,
        &sections,
        &[
            &code_content,
            &symtab_content,
            &caps_content,
            &exports_content,
            &effects_content,
        ],
    );
    header.blake3_content_hash = content_hash;

    // Assemble final PAX file
    let mut pax = Vec::new();
    pax.extend_from_slice(&header.to_bytes());
    pax.extend_from_slice(&sections.to_bytes());
    pax.extend_from_slice(&code_content);
    pax.extend_from_slice(&symtab_content);
    pax.extend_from_slice(&caps_content);
    pax.extend_from_slice(&exports_content);
    pax.extend_from_slice(&effects_content);

    pax
}

// ============================================================================
// Smoke Tests
// ============================================================================

#[test]
fn mock_supervisor_loads_hello_world() {
    // AC 1: Build hello-world PAX bytes; write to tempfile.
    let pax_bytes = build_hello_world_pax();
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let pax_path = temp_dir.path().join("hello.pax");
    std::fs::write(&pax_path, &pax_bytes).expect("Failed to write PAX file");

    // Spawn MockSupervisor; call `.load(path)`.
    let mut supervisor = MockSupervisor::new();
    let result = supervisor.load(&pax_path);

    // Assert no error; assert loaded.header has Executable flag.
    assert!(result.is_ok(), "Load should succeed");
    let loaded = result.unwrap();
    assert_eq!(
        loaded.header.flags & (paideia_as_emitter_pax::HeaderFlag::Executable as u64),
        paideia_as_emitter_pax::HeaderFlag::Executable as u64
    );
}

#[test]
fn mock_supervisor_dispatch_to_hello_main() {
    // AC 1: Load hello-world; call `.dispatch(0, hash("hello_main"))`.
    let pax_bytes = build_hello_world_pax();
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let pax_path = temp_dir.path().join("hello.pax");
    std::fs::write(&pax_path, &pax_bytes).expect("Failed to write PAX file");

    let mut supervisor = MockSupervisor::new();
    supervisor.load(&pax_path).expect("Load should succeed");

    let hello_main_hash = blake3::hash(b"hello_main");
    let hello_main_hash_u64 =
        u64::from_le_bytes(hello_main_hash.as_bytes()[..8].try_into().unwrap());

    // Expect Some(CapDescriptor) for the matching export.
    let result = supervisor.dispatch(0, hello_main_hash_u64);
    assert!(result.is_some(), "Dispatch should find the export");

    let cap_desc = result.unwrap();
    assert_eq!(cap_desc.blake3_name_hash, hello_main_hash_u64);
    assert_eq!(cap_desc.cap_kind, CapKind::MmioMemCap);
}

#[test]
fn mock_supervisor_consumes_paideia_caps() {
    // AC 2: Load hello-world; assert cap_binding_sites returns the 1 MmioMemCap entry.
    let pax_bytes = build_hello_world_pax();
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let pax_path = temp_dir.path().join("hello.pax");
    std::fs::write(&pax_path, &pax_bytes).expect("Failed to write PAX file");

    let mut supervisor = MockSupervisor::new();
    supervisor.load(&pax_path).expect("Load should succeed");

    let cap_sites = supervisor.cap_binding_sites(0);
    assert_eq!(cap_sites.len(), 1, "Should have 1 capability binding site");

    let cap = &cap_sites[0];
    assert_eq!(cap.cap_kind, CapKind::MmioMemCap);
    assert_eq!(cap.class, LinClass::Linear);
    assert_eq!(cap.site_kind, SiteKind::FunctionParam);
}

#[test]
fn mock_supervisor_parsed_bindings_snapshot() {
    // AC 3: Load hello-world; assert specific field values on the loaded structure
    // (section count, cap count, export name hash, etc.).
    let pax_bytes = build_hello_world_pax();
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let pax_path = temp_dir.path().join("hello.pax");
    std::fs::write(&pax_path, &pax_bytes).expect("Failed to write PAX file");

    let mut supervisor = MockSupervisor::new();
    let loaded = supervisor.load(&pax_path).expect("Load should succeed");

    // Verify section count
    assert_eq!(loaded.sections.len(), 5, "Should have 5 sections");

    // Verify cap count
    assert_eq!(
        loaded.caps.entries.len(),
        1,
        "Should have 1 capability entry"
    );

    // Verify export count and hash
    assert_eq!(loaded.exports.entries.len(), 1, "Should have 1 export");
    let hello_main_hash = blake3::hash(b"hello_main");
    let hello_main_hash_u64 =
        u64::from_le_bytes(hello_main_hash.as_bytes()[..8].try_into().unwrap());
    assert_eq!(
        loaded.exports.entries[0].blake3_name_hash,
        hello_main_hash_u64
    );

    // Verify symtab count
    assert_eq!(loaded.symtab.len(), 1, "Should have 1 symbol");

    // Verify effects section
    assert_eq!(loaded.effects.entries.len(), 1, "Should have 1 effect row");
}

#[test]
fn mock_supervisor_rejects_non_pax() {
    // Test: file with garbage magic → LoadError::NotPax.
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let pax_path = temp_dir.path().join("garbage.bin");
    // Write 96+ bytes with invalid magic to trigger NotPax (not TruncatedHeader)
    let mut garbage = vec![0u8; 96];
    garbage[0..4].copy_from_slice(b"JUNK");
    std::fs::write(&pax_path, garbage).expect("Failed to write file");

    let mut supervisor = MockSupervisor::new();
    let result = supervisor.load(&pax_path);

    assert!(result.is_err(), "Load should fail for non-PAX file");
    match result {
        Err(pax_load_smoke::LoadError::NotPax) => {
            // Expected
        }
        _ => panic!("Expected LoadError::NotPax"),
    }
}

#[test]
fn mock_supervisor_returns_entry_point_symbol() {
    // Test: hello-world has one Global / Default sym → entry_point returns Some.
    let pax_bytes = build_hello_world_pax();
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let pax_path = temp_dir.path().join("hello.pax");
    std::fs::write(&pax_path, &pax_bytes).expect("Failed to write PAX file");

    let mut supervisor = MockSupervisor::new();
    supervisor.load(&pax_path).expect("Load should succeed");

    let entry_point = supervisor.entry_point(0);
    assert!(entry_point.is_some(), "Should find entry point symbol");

    let sym = entry_point.unwrap();
    assert_eq!(sym.binding, SymBinding::Global);
    assert_eq!(sym.visibility, SymVisibility::Default);
    assert_eq!(sym.value, 0, "Entry point should be at offset 0");
}
