//! Side-table mapping identifier-use sites to their definition sites
//! and all references (the inverse mapping).
//!
//! Populated as a side-effect of elaborator name resolution. Replaces
//! the textual-occurrence matching in m8-007.

use crate::position_index::{ByteOffset, FileId};
use std::collections::HashMap;

/// A span in a source file, identified by file and byte offsets.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Span {
    /// Source file identifier.
    pub file: FileId,
    /// Start byte offset (inclusive).
    pub start: ByteOffset,
    /// End byte offset (exclusive).
    pub end: ByteOffset,
}

/// Side-table mapping identifier-use sites to their definition sites
/// and all references (the inverse mapping).
///
/// In Phase 3-m4, this is populated as the elaborator performs name
/// resolution. LSP handlers query this table instead of doing textual
/// occurrence matching.
#[derive(Default, Debug)]
pub struct NameResolutionTable {
    /// use_span → definition_span.
    uses: HashMap<Span, Span>,
    /// definition_span → all use_spans.
    references: HashMap<Span, Vec<Span>>,
}

impl NameResolutionTable {
    /// Create a new empty NameResolutionTable.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a name resolution: a use site that resolves to a definition site.
    ///
    /// Maintains both forward (use → def) and inverse (def → uses) mappings.
    pub fn record(&mut self, use_site: Span, def_site: Span) {
        self.uses.insert(use_site, def_site);
        self.references.entry(def_site).or_default().push(use_site);
    }

    /// Look up the definition for a use site.
    ///
    /// Returns the definition span if this use site was recorded, or `None`.
    pub fn definition_of(&self, use_site: Span) -> Option<Span> {
        self.uses.get(&use_site).copied()
    }

    /// Look up all references (use sites) of a definition.
    ///
    /// Returns a slice of all use sites that resolve to this definition span,
    /// or an empty slice if the definition was not recorded.
    pub fn references_of(&self, def_site: Span) -> &[Span] {
        self.references
            .get(&def_site)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Return the total number of use sites recorded.
    pub fn use_count(&self) -> usize {
        self.uses.len()
    }

    /// Return the total number of definition sites recorded.
    pub fn definition_count(&self) -> usize {
        self.references.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_resolution_table_starts_empty() {
        let table = NameResolutionTable::new();
        assert_eq!(table.use_count(), 0);
        assert_eq!(table.definition_count(), 0);
    }

    #[test]
    fn name_resolution_record_creates_inverse_mapping() {
        let mut table = NameResolutionTable::new();
        let use_site = Span {
            file: FileId(1),
            start: ByteOffset(10),
            end: ByteOffset(13),
        };
        let def_site = Span {
            file: FileId(1),
            start: ByteOffset(0),
            end: ByteOffset(3),
        };

        table.record(use_site, def_site);

        assert_eq!(table.use_count(), 1);
        assert_eq!(table.definition_count(), 1);

        // Forward mapping
        assert_eq!(table.definition_of(use_site), Some(def_site));

        // Inverse mapping
        let refs = table.references_of(def_site);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], use_site);
    }

    #[test]
    fn name_resolution_definition_of_returns_none_for_unknown_use() {
        let table = NameResolutionTable::new();
        let unknown_use = Span {
            file: FileId(1),
            start: ByteOffset(99),
            end: ByteOffset(102),
        };

        assert_eq!(table.definition_of(unknown_use), None);
    }

    #[test]
    fn name_resolution_references_of_returns_all_uses() {
        let mut table = NameResolutionTable::new();
        let def_site = Span {
            file: FileId(1),
            start: ByteOffset(0),
            end: ByteOffset(3),
        };

        let use_site_1 = Span {
            file: FileId(1),
            start: ByteOffset(10),
            end: ByteOffset(13),
        };
        let use_site_2 = Span {
            file: FileId(1),
            start: ByteOffset(20),
            end: ByteOffset(23),
        };

        table.record(use_site_1, def_site);
        table.record(use_site_2, def_site);

        let refs = table.references_of(def_site);
        assert_eq!(refs.len(), 2);
        assert!(refs.contains(&use_site_1));
        assert!(refs.contains(&use_site_2));
    }
}
