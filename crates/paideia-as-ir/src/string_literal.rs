//! Side-table for StringLiteral IR nodes recording .rodata placement.
//!
//! Each `IrKind::StringLiteral` node carries structural children in the arena.
//! This module provides a side-table (`StringLiteralTable`) mapping StringLiteral
//! node ids to their immutable UTF-8 byte slice metadata: the offset into .rodata
//! and the length in bytes.
//!
//! Phase-4-m8-002 records the (offset, len) tuple; actual .rodata emission gates
//! on phase-4-m4 (emitter integration).

use std::collections::HashMap;

use crate::node::IrNodeId;

/// Metadata for a StringLiteral IR node.
///
/// Records the placement of an immutable UTF-8 string in the .rodata section:
/// - `rodata_offset`: byte offset from the start of .rodata
/// - `len`: length of the string in bytes (not including null terminator)
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StringLiteralInfo {
    /// Byte offset into .rodata where this string begins.
    pub rodata_offset: u64,
    /// Length of the string in bytes.
    pub len: u64,
}

/// Side-table mapping StringLiteral IrNodeIds to their metadata.
///
/// Parallels the arena's design pattern: uses a HashMap indexed by `IrNodeId`
/// so that lookups are O(1) and portable across systems.
///
/// Phase-4-m8-002: populated as StringLiteral nodes are constructed.
/// The emitter (phase-4-m4+) reads entries to emit .rodata and generate
/// fat-pointer load instructions.
#[derive(Default, Debug, Clone)]
pub struct StringLiteralTable {
    /// Sparse mapping: StringLiteral node id -> StringLiteralInfo.
    /// Only StringLiteral nodes have entries; other nodes don't.
    entries: HashMap<IrNodeId, StringLiteralInfo>,
}

impl StringLiteralTable {
    /// Construct an empty string literal side-table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) the metadata for a StringLiteral node.
    ///
    /// Returns the previous entry if one existed.
    pub fn insert(&mut self, id: IrNodeId, info: StringLiteralInfo) -> Option<StringLiteralInfo> {
        self.entries.insert(id, info)
    }

    /// Look up the metadata for a StringLiteral node.
    ///
    /// Returns `None` if the node was never registered or is not a StringLiteral node.
    #[must_use]
    pub fn get(&self, id: IrNodeId) -> Option<&StringLiteralInfo> {
        self.entries.get(&id)
    }

    /// Look up (mutable) the metadata for a StringLiteral node.
    ///
    /// Allows elaborators to mutate the metadata (if needed in future phases)
    /// without cloning.
    pub fn get_mut(&mut self, id: IrNodeId) -> Option<&mut StringLiteralInfo> {
        self.entries.get_mut(&id)
    }

    /// Number of string literals registered in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no string literals are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_literal_table_insert_and_get() {
        let mut table = StringLiteralTable::new();
        let str_id = IrNodeId::new(1).unwrap();

        let info = StringLiteralInfo {
            rodata_offset: 0,
            len: 11,
        };

        // Insert and verify
        table.insert(str_id, info);
        let retrieved = table.get(str_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().rodata_offset, 0);
        assert_eq!(retrieved.unwrap().len, 11);
    }

    #[test]
    fn string_literal_table_handles_multiple_strings() {
        let mut table = StringLiteralTable::new();

        let str1 = IrNodeId::new(1).unwrap();
        let str2 = IrNodeId::new(2).unwrap();
        let str3 = IrNodeId::new(3).unwrap();

        table.insert(
            str1,
            StringLiteralInfo {
                rodata_offset: 0,
                len: 5,
            },
        );
        table.insert(
            str2,
            StringLiteralInfo {
                rodata_offset: 5,
                len: 6,
            },
        );
        table.insert(
            str3,
            StringLiteralInfo {
                rodata_offset: 11,
                len: 13,
            },
        );

        assert_eq!(table.len(), 3);
        assert_eq!(table.get(str1).unwrap().rodata_offset, 0);
        assert_eq!(table.get(str2).unwrap().rodata_offset, 5);
        assert_eq!(table.get(str3).unwrap().rodata_offset, 11);
    }

    #[test]
    fn string_literal_table_offsets_unique_per_entry() {
        let mut table = StringLiteralTable::new();

        // Simulate concatenated strings in .rodata
        let str1 = IrNodeId::new(1).unwrap();
        let str2 = IrNodeId::new(2).unwrap();
        let str3 = IrNodeId::new(3).unwrap();

        let len1 = 13; // "Hello, world!"
        let len2 = 8; // "foo bar "
        let len3 = 5; // "baz!!"

        table.insert(
            str1,
            StringLiteralInfo {
                rodata_offset: 0,
                len: len1,
            },
        );
        table.insert(
            str2,
            StringLiteralInfo {
                rodata_offset: len1,
                len: len2,
            },
        );
        table.insert(
            str3,
            StringLiteralInfo {
                rodata_offset: len1 + len2,
                len: len3,
            },
        );

        assert_eq!(table.get(str1).unwrap().rodata_offset, 0);
        assert_eq!(table.get(str2).unwrap().rodata_offset, len1);
        assert_eq!(table.get(str3).unwrap().rodata_offset, len1 + len2);
    }

    #[test]
    fn string_literal_table_get_returns_none_for_missing() {
        let table = StringLiteralTable::new();
        let unset_id = IrNodeId::new(999).unwrap();
        assert_eq!(table.get(unset_id), None);
    }

    #[test]
    fn string_literal_table_len_tracks_inserts() {
        let mut table = StringLiteralTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());

        for i in 1u32..=5 {
            let id = IrNodeId::new(i).unwrap();
            let info = StringLiteralInfo {
                rodata_offset: (i - 1) as u64 * 10,
                len: 10,
            };
            table.insert(id, info);
            assert_eq!(table.len(), i as usize);
        }

        assert!(!table.is_empty());
    }

    #[test]
    fn string_literal_table_get_mut_allows_mutation() {
        let mut table = StringLiteralTable::new();
        let str_id = IrNodeId::new(1).unwrap();

        let info = StringLiteralInfo {
            rodata_offset: 0,
            len: 5,
        };
        table.insert(str_id, info);

        // Mutate via get_mut
        if let Some(info_mut) = table.get_mut(str_id) {
            info_mut.rodata_offset = 10;
            info_mut.len = 15;
        }

        // Verify mutation took effect
        let retrieved = table.get(str_id).unwrap();
        assert_eq!(retrieved.rodata_offset, 10);
        assert_eq!(retrieved.len, 15);
    }

    #[test]
    fn string_literal_table_empty_by_default() {
        let table = StringLiteralTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());
    }
}
