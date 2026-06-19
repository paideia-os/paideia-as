//! PQ signature corpus: test helpers for m7-001..006 signing/verification round-trips.
//!
//! This crate provides fixtures and helper functions for exercising the
//! post-quantum signing harness end-to-end, covering:
//! - m7-001: Ed25519 and ML-DSA-65 individual key generation and signing
//! - m7-002: Hybrid composition (AND semantics)
//! - m7-003: PAX content-hash signing and verification via emitter
//! - m7-004: Scope-checking (KeyScope ⊇ effects)
//! - m7-006: Soft-HSM file round-trips

#![warn(missing_docs)]
#![forbid(unsafe_code)]

use paideia_as_emitter_pax::{
    Architecture, CapEntry, CapKind, CapsSection, EffectRowEntry, EffectsSection, ExportsSection,
    HeaderFlag, PAX_HEADER_SIZE, PaxHeader, SECTION_DESCRIPTOR_SIZE, Section, SectionTable,
    SectionType, SiteKind, SymBinding, SymEntry, SymTab, SymVisibility,
};

/// Build a minimal PAX file with a single .code section, .symtab, .paideia.caps,
/// .exports, and .paideia.effects section.
///
/// # Arguments
///
/// * `code_size` - Size of the .code section in bytes
/// * `symbol_name` - Name of the symbol to add to .symtab
/// * `capability_count` - Number of capability entries to add
///
/// # Returns
///
/// Complete PAX file bytes ready to write to disk.
pub fn build_minimal_pax(code_size: usize, symbol_name: &str, capability_count: usize) -> Vec<u8> {
    let mut header = PaxHeader::new(Architecture::X86_64);
    header.flags = HeaderFlag::Executable as u64;

    // Section 0: .code (placeholder bytes)
    let code_content = vec![0u8; code_size];

    // Section 1: .symtab (1 symbol entry)
    let symbol_hash = blake3::hash(symbol_name.as_bytes());
    let symbol_hash_u64 = u64::from_le_bytes(symbol_hash.as_bytes()[..8].try_into().unwrap());

    let mut symtab = SymTab::new();
    symtab.push(SymEntry::new(
        0,                      // value (offset in .code)
        code_size as u64,       // size
        0,                      // section_index (points to .code)
        SymBinding::Global,     // binding
        SymVisibility::Default, // visibility
        0,                      // name_offset
        symbol_hash_u64,        // blake3_name_hash
    ));
    let symtab_content = symtab.to_bytes();

    // Section 2: .paideia.caps (multiple capability entries)
    let mut caps = CapsSection::new();
    for i in 0..capability_count {
        caps.push(CapEntry::new(
            SiteKind::FunctionParam,
            paideia_as_emitter_pax::LinClass::Linear,
            i as u64,       // location_id
            (i as u32) + 1, // lam_tag
            CapKind::MmioMemCap,
            &format!("cap_{}", i),
        ));
    }
    let caps_content = caps.to_bytes();

    // Section 3: .exports (1 export entry matching the symbol)
    let mut exports = ExportsSection::new();
    exports.push(paideia_as_emitter_pax::CapDescriptor::new(
        0,               // name_offset
        symbol_hash_u64, // blake3_name_hash (matches symbol)
        CapKind::MmioMemCap,
        paideia_as_emitter_pax::LinClass::Linear,
        0, // flags
    ));
    let exports_content = exports.to_bytes();

    // Section 4: .paideia.effects (1 effect entry with capability effects)
    let mut effects = EffectsSection::new();
    let mut fixed_effects = vec![];
    for i in 0..capability_count {
        fixed_effects.push(i as u32 + 100); // arbitrary effect IDs for testing
    }
    effects.push(EffectRowEntry::new(
        0,             // function_symbol_id (symbol 0)
        fixed_effects, // fixed_effects
        None,          // row_var_id (closed row)
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

    // Finalize header
    header.section_table_offset = PAX_HEADER_SIZE as u64;
    header.section_count = sections.len() as u32;

    // Compute BLAKE3 content hash
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

/// Build a PAX with 10 effects for scope-checking tests.
pub fn build_pax_with_effects(effect_ids: &[u32]) -> Vec<u8> {
    let mut header = PaxHeader::new(Architecture::X86_64);
    header.flags = HeaderFlag::Executable as u64;

    let code_content = vec![0u8; 32];

    // Symbol
    let symbol_hash = blake3::hash(b"test_func");
    let symbol_hash_u64 = u64::from_le_bytes(symbol_hash.as_bytes()[..8].try_into().unwrap());

    let mut symtab = SymTab::new();
    symtab.push(SymEntry::new(
        0,
        32,
        0,
        SymBinding::Global,
        SymVisibility::Default,
        0,
        symbol_hash_u64,
    ));
    let symtab_content = symtab.to_bytes();

    // Empty caps
    let caps_content = CapsSection::new().to_bytes();

    // Empty exports
    let exports_content = ExportsSection::new().to_bytes();

    // Effects with specified IDs
    let mut effects = EffectsSection::new();
    effects.push(EffectRowEntry::new(0, effect_ids.to_vec(), None));
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

    header.section_table_offset = PAX_HEADER_SIZE as u64;
    header.section_count = sections.len() as u32;

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
