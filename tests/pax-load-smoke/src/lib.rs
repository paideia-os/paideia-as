//! Mock PaideiaOS supervisor — simulates loading a PAX without
//! executing any code.
//!
//! This harness implements phase-2-m12 simulation for PAX capability/effect/export
//! parsing and entry-point dispatch. The "dispatch" operation is purely symbolic:
//! we confirm the supervisor can read the entry-point symbol and would jump to
//! its offset, without executing any code.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

use std::path::Path;

use paideia_as_emitter_pax::{
    CapDescriptor, CapsSection, EffectsSection, ExportsSection, ImportsSection, PAX_HEADER_SIZE,
    PaxHeader, SectionTable, SectionType, SymEntry, SymTab,
};

/// What a load produces. The supervisor reads the PAX's metadata
/// sections and parses them into structured form.
#[derive(Debug, Clone)]
pub struct LoadedPax {
    /// The PAX header.
    pub header: PaxHeader,
    /// The section table.
    pub sections: SectionTable,
    /// The capability annotations section.
    pub caps: CapsSection,
    /// The effect annotations section.
    pub effects: EffectsSection,
    /// The imports section.
    pub imports: ImportsSection,
    /// The exports section.
    pub exports: ExportsSection,
    /// The symbol table.
    pub symtab: SymTab,
    /// Raw file bytes.
    pub bytes: Vec<u8>,
}

/// Errors that can occur during PAX loading.
#[derive(Debug, Clone)]
pub enum LoadError {
    /// IO error reading file.
    Io(String),
    /// File is not a valid PAX (magic mismatch).
    NotPax,
    /// Header is truncated or incomplete.
    TruncatedHeader,
    /// PAX format version is not supported.
    UnsupportedVersion(u16),
}

/// Mock PaideiaOS supervisor: loads PAX files and validates metadata.
#[derive(Debug)]
pub struct MockSupervisor {
    /// List of loaded PAX files.
    pub loaded: Vec<LoadedPax>,
}

impl MockSupervisor {
    /// Create a new supervisor.
    pub fn new() -> Self {
        Self { loaded: Vec::new() }
    }

    /// Read and parse a PAX from disk.
    ///
    /// Reads the file at `path`, validates the header, and parses all
    /// metadata sections (capabilities, effects, symbols, exports, imports).
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the PAX file
    ///
    /// # Returns
    ///
    /// `Ok(&LoadedPax)` on success; `Err(LoadError)` on failure.
    pub fn load(&mut self, path: &Path) -> Result<&LoadedPax, LoadError> {
        let bytes = std::fs::read(path).map_err(|e| LoadError::Io(e.to_string()))?;

        if bytes.len() < PAX_HEADER_SIZE {
            return Err(LoadError::TruncatedHeader);
        }

        let header = PaxHeader::from_bytes(&bytes).ok_or(LoadError::NotPax)?;

        if header.format_version != paideia_as_emitter_pax::PAX_FORMAT_VERSION {
            return Err(LoadError::UnsupportedVersion(header.format_version));
        }

        let section_table_offset = header.section_table_offset as usize;
        let section_count = header.section_count;

        let sections = SectionTable::from_bytes(&bytes[section_table_offset..], section_count)
            .ok_or(LoadError::TruncatedHeader)?;

        // Parse capability section
        let caps = parse_caps_section(&bytes, &sections);

        // Parse effects section
        let effects = parse_effects_section(&bytes, &sections);

        // Parse imports section
        let imports = parse_imports_section(&bytes, &sections);

        // Parse exports section
        let exports = parse_exports_section(&bytes, &sections);

        // Parse symbol table
        let symtab = parse_symtab_section(&bytes, &sections);

        let loaded_pax = LoadedPax {
            header,
            sections,
            caps,
            effects,
            imports,
            exports,
            symtab,
            bytes,
        };

        self.loaded.push(loaded_pax);
        Ok(&self.loaded[self.loaded.len() - 1])
    }

    /// Pretend to dispatch to a named export.
    ///
    /// Returns the export's CapDescriptor if it matches the name_hash,
    /// None otherwise.
    ///
    /// # Arguments
    ///
    /// * `pax_index` - Index of the loaded PAX
    /// * `name_hash` - BLAKE3-derived hash of the export name
    ///
    /// # Returns
    ///
    /// `Some(&CapDescriptor)` if found; `None` otherwise.
    pub fn dispatch(&self, pax_index: usize, name_hash: u64) -> Option<&CapDescriptor> {
        let loaded = self.loaded.get(pax_index)?;
        loaded
            .exports
            .entries
            .iter()
            .find(|e| e.blake3_name_hash == name_hash)
    }

    /// Return the symbol marked as the entry point.
    ///
    /// Phase-2-m12 definition: the first SymEntry whose binding is Global
    /// and visibility is Default.
    ///
    /// # Arguments
    ///
    /// * `pax_index` - Index of the loaded PAX
    ///
    /// # Returns
    ///
    /// `Some(&SymEntry)` if an entry point is found; `None` otherwise.
    pub fn entry_point(&self, pax_index: usize) -> Option<&SymEntry> {
        let loaded = self.loaded.get(pax_index)?;
        loaded.symtab.entries.iter().find(|e| {
            e.binding == paideia_as_emitter_pax::SymBinding::Global
                && e.visibility == paideia_as_emitter_pax::SymVisibility::Default
        })
    }

    /// Return the per-PAX capability binding sites.
    ///
    /// # Arguments
    ///
    /// * `pax_index` - Index of the loaded PAX
    ///
    /// # Returns
    ///
    /// Slice of CapEntry for this PAX.
    pub fn cap_binding_sites(&self, pax_index: usize) -> &[paideia_as_emitter_pax::CapEntry] {
        self.loaded
            .get(pax_index)
            .map(|p| p.caps.entries.as_slice())
            .unwrap_or(&[])
    }
}

impl Default for MockSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Section Parsing Helpers
// ============================================================================

/// Parse the capabilities section.
fn parse_caps_section(bytes: &[u8], sections: &SectionTable) -> CapsSection {
    for section in &sections.sections {
        if section.ty == SectionType::Caps {
            let start = section.content_offset as usize;
            let size = section.content_size as usize;
            if start + size <= bytes.len()
                && let Some(caps) = CapsSection::from_bytes(&bytes[start..start + size])
            {
                return caps;
            }
        }
    }
    CapsSection::default()
}

/// Parse the effects section.
fn parse_effects_section(bytes: &[u8], sections: &SectionTable) -> EffectsSection {
    for section in &sections.sections {
        if section.ty == SectionType::Effects {
            let start = section.content_offset as usize;
            let size = section.content_size as usize;
            if start + size <= bytes.len()
                && let Some(effects) = EffectsSection::from_bytes(&bytes[start..start + size])
            {
                return effects;
            }
        }
    }
    EffectsSection::default()
}

/// Parse the imports section.
fn parse_imports_section(bytes: &[u8], sections: &SectionTable) -> ImportsSection {
    for section in &sections.sections {
        if section.ty == SectionType::Imports {
            let start = section.content_offset as usize;
            let size = section.content_size as usize;
            if start + size <= bytes.len()
                && let Some(imports) = ImportsSection::from_bytes(&bytes[start..start + size])
            {
                return imports;
            }
        }
    }
    ImportsSection::default()
}

/// Parse the exports section.
fn parse_exports_section(bytes: &[u8], sections: &SectionTable) -> ExportsSection {
    for section in &sections.sections {
        if section.ty == SectionType::Exports {
            let start = section.content_offset as usize;
            let size = section.content_size as usize;
            if start + size <= bytes.len()
                && let Some(exports) = ExportsSection::from_bytes(&bytes[start..start + size])
            {
                return exports;
            }
        }
    }
    ExportsSection::default()
}

/// Parse the symbol table section.
fn parse_symtab_section(bytes: &[u8], sections: &SectionTable) -> SymTab {
    for section in &sections.sections {
        if section.ty == SectionType::Symtab {
            let start = section.content_offset as usize;
            let size = section.content_size as usize;
            if start + size <= bytes.len()
                && let Some(symtab) = SymTab::from_bytes(&bytes[start..start + size])
            {
                return symtab;
            }
        }
    }
    SymTab::new()
}
