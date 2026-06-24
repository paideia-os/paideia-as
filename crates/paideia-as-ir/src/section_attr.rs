//! Section attributes side-table (Phase 15 m2-002).
//!
//! Maps sections to their instruction mode (Mode64 or Mode32).

use std::collections::HashMap;

/// A section attribute record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SectionAttr {
    /// Instruction mode for this section (Mode64 or Mode32).
    pub instr_mode: crate::instruction::InstrMode,
}

/// Side-table mapping section names to attributes.
#[derive(Default, Debug, Clone)]
pub struct SectionAttrTable {
    /// Sparse mapping: section name -> SectionAttr.
    entries: HashMap<String, SectionAttr>,
}

impl SectionAttrTable {
    /// Construct an empty section attribute table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) section attributes.
    pub fn insert(&mut self, section_name: String, attr: SectionAttr) {
        self.entries.insert(section_name, attr);
    }

    /// Look up section attributes.
    #[must_use]
    pub fn get(&self, section_name: &str) -> Option<SectionAttr> {
        self.entries.get(section_name).copied()
    }
}
