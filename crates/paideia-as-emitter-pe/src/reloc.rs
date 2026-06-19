//! PE/COFF relocation table (.reloc) emission for Microsoft x64 / UEFI binaries.
//!
//! This module defines the binary layout, serialization, and parsing of the base relocation table
//! (.reloc section). All multi-byte fields are serialized in little-endian byte order.
//!
//! Layout summary:
//! - Base Relocation Block Header (12 bytes): Virtual address of the 4 KB page and size of block.
//! - Relocation Entries (2 bytes each): High 4 bits = relocation type, low 12 bits = offset within page.
//! - Padding: Odd entry count followed by 2-byte IMAGE_REL_BASED_ABSOLUTE pad to align to 4 bytes.

use static_assertions::const_assert_eq;

// ============================================================================
// Constants
// ============================================================================

/// Relocation type: no relocation needed (used for padding).
pub const IMAGE_REL_BASED_ABSOLUTE: u16 = 0;

/// Relocation type: 64-bit relocation (DIR64, RVA-relative addressing).
pub const IMAGE_REL_BASED_DIR64: u16 = 10;

/// Page size for base relocation blocks (4 KB).
pub const PAGE_SIZE: u32 = 0x1000;

/// Size of a base relocation block header in bytes (VA + size_of_block).
pub(crate) const IMAGE_BASE_RELOCATION_SIZE: usize = 12;

const_assert_eq!(IMAGE_BASE_RELOCATION_SIZE, 12);

// ============================================================================
// Relocation
// ============================================================================

/// A single relocation entry within a base relocation block.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Relocation {
    /// RVA of the relocation target.
    pub rva: u32,
    /// Relocation type (e.g., IMAGE_REL_BASED_DIR64).
    pub typ: u16,
}

// ============================================================================
// RelocSection
// ============================================================================

/// A collection of relocations grouped into base relocation blocks.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RelocSection {
    /// List of relocations (unordered until serialized).
    pub relocations: Vec<Relocation>,
}

impl RelocSection {
    /// Create a new empty relocation section.
    pub fn new() -> Self {
        Self {
            relocations: Vec::new(),
        }
    }

    /// Add a DIR64 (IMAGE_REL_BASED_DIR64) relocation at the given RVA.
    pub fn add_dir64(&mut self, rva: u32) {
        self.relocations.push(Relocation {
            rva,
            typ: IMAGE_REL_BASED_DIR64,
        });
    }

    /// Check if this relocation section is empty.
    pub fn is_empty(&self) -> bool {
        self.relocations.is_empty()
    }

    /// Return the number of relocations.
    pub fn len(&self) -> usize {
        self.relocations.len()
    }

    /// Serialize to bytes.
    ///
    /// Algorithm:
    /// 1. Sort relocations by RVA.
    /// 2. Group by 4 KB page (virtual_address & !0xFFF).
    /// 3. For each page:
    ///    - Write 12-byte header: virtual_address (u32 LE) + size_of_block (u32 LE) + 4 unused.
    ///    - For each entry: write ((typ << 12) | ((rva - page_rva) & 0xFFF)).to_le_bytes().
    ///    - If odd entry count, append IMAGE_REL_BASED_ABSOLUTE (0x0000) pad.
    ///    - Backpatch size_of_block into bytes [start+4..start+8] LE.
    /// 4. Return empty Vec if input is empty.
    pub fn to_bytes(&self) -> Vec<u8> {
        if self.relocations.is_empty() {
            return Vec::new();
        }

        let mut result = Vec::new();
        let mut sorted_relocs = self.relocations.clone();
        sorted_relocs.sort_by_key(|r| r.rva);

        let mut current_page: Option<u32> = None;
        let mut block_start: usize = 0;
        let mut block_entry_count: usize = 0;

        for reloc in sorted_relocs {
            let page_rva = reloc.rva & !0xFFF;

            // If we crossed a page boundary, flush the prior block and start a new one
            if current_page != Some(page_rva) {
                if let Some(_prev_page) = current_page {
                    // Flush prior block: add padding if odd count, then backpatch size
                    if block_entry_count % 2 == 1 {
                        result.extend_from_slice(&IMAGE_REL_BASED_ABSOLUTE.to_le_bytes());
                    }
                    let mut size_of_block = 12 + 2 * block_entry_count as u32;
                    if block_entry_count % 2 == 1 {
                        size_of_block += 2; // Padding for odd count
                    }
                    result[block_start + 4..block_start + 8]
                        .copy_from_slice(&size_of_block.to_le_bytes());
                }

                // Start new block with zeroed header
                block_start = result.len();
                result.extend_from_slice(&[0u8; IMAGE_BASE_RELOCATION_SIZE]);
                // Write virtual_address (page RVA)
                result[block_start..block_start + 4].copy_from_slice(&page_rva.to_le_bytes());
                current_page = Some(page_rva);
                block_entry_count = 0;
            }

            // Write relocation entry
            let offset_in_page = (reloc.rva - page_rva) as u16;
            let entry = ((reloc.typ << 12) | (offset_in_page & 0x0FFF)).to_le_bytes();
            result.extend_from_slice(&entry);
            block_entry_count += 1;
        }

        // Flush the final block
        if let Some(_page) = current_page {
            // If odd entry count, append IMAGE_REL_BASED_ABSOLUTE pad
            if block_entry_count % 2 == 1 {
                result.extend_from_slice(&IMAGE_REL_BASED_ABSOLUTE.to_le_bytes());
            }
            // Backpatch size_of_block
            let mut size_of_block = 12 + 2 * block_entry_count as u32;
            if block_entry_count % 2 == 1 {
                size_of_block += 2; // Padding
            }
            result[block_start + 4..block_start + 8].copy_from_slice(&size_of_block.to_le_bytes());
        }

        result
    }

    /// Parse from a byte slice.
    ///
    /// Walk blocks: read 12-byte header (virtual_address u32 LE + size_of_block u32 LE + 4 unused).
    /// Validate:
    /// - size_of_block >= 12.
    /// - (size_of_block - 12) % 2 == 0.
    /// - offset + size_of_block <= len.
    ///
    /// For each u16 entry: extract typ = word >> 12, offset12 = word & 0xFFF,
    /// reconstruct rva = virtual_address + offset12. Skip type-0 (ABSOLUTE) pads.
    /// Return None on any structural violation.
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        let mut relocations = Vec::new();
        let mut offset = 0;

        while offset < b.len() {
            // Read 12-byte header
            if offset + IMAGE_BASE_RELOCATION_SIZE > b.len() {
                return None; // Incomplete header
            }

            let virtual_address =
                u32::from_le_bytes([b[offset], b[offset + 1], b[offset + 2], b[offset + 3]]);
            let size_of_block =
                u32::from_le_bytes([b[offset + 4], b[offset + 5], b[offset + 6], b[offset + 7]]);

            // Validate header
            if size_of_block < 12 {
                return None; // Invalid size
            }
            if !(size_of_block - 12).is_multiple_of(2) {
                return None; // Misaligned entry count
            }
            if offset + size_of_block as usize > b.len() {
                return None; // Block exceeds input length
            }

            // Read entries
            let entry_count = (size_of_block - 12) / 2;
            let entries_start = offset + IMAGE_BASE_RELOCATION_SIZE;

            for i in 0..entry_count {
                let entry_offset = entries_start + (i as usize) * 2;
                let entry = u16::from_le_bytes([b[entry_offset], b[entry_offset + 1]]);

                let typ = entry >> 12;
                let offset12 = entry & 0x0FFF;

                // Skip IMAGE_REL_BASED_ABSOLUTE pads
                if typ == IMAGE_REL_BASED_ABSOLUTE {
                    continue;
                }

                let rva = virtual_address + offset12 as u32;
                relocations.push(Relocation { rva, typ });
            }

            // Advance to next block
            offset += size_of_block as usize;
        }

        Some(Self { relocations })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_reloc_section_serialises_to_empty() {
        let section = RelocSection::new();
        assert_eq!(section.to_bytes(), Vec::<u8>::new());
    }

    #[test]
    fn single_dir64_entry_produces_one_block() {
        let mut section = RelocSection::new();
        section.add_dir64(0x1000);

        let bytes = section.to_bytes();
        // Header (12) + 1 entry (2) + padding for odd count (2) = 16 bytes
        assert_eq!(bytes.len(), 16);

        // Verify header: VA = 0x1000
        assert_eq!(
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            0x1000
        );
        // Verify header: size_of_block = 0x10
        assert_eq!(
            u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            0x10
        );
    }

    #[test]
    fn two_entries_same_page_produce_one_block() {
        let mut section = RelocSection::new();
        section.add_dir64(0x1000);
        section.add_dir64(0x1234);

        let bytes = section.to_bytes();
        // Header (12) + 2 entries (4) = 16 bytes (even count, no padding)
        assert_eq!(bytes.len(), 16);

        // Verify header
        assert_eq!(
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            0x1000
        );
        assert_eq!(
            u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            0x10
        );
    }

    #[test]
    fn two_entries_different_pages_produce_two_blocks() {
        let mut section = RelocSection::new();
        section.add_dir64(0x1000);
        section.add_dir64(0x2234); // Different page: 0x2000

        let bytes = section.to_bytes();
        // First block: header (12) + 1 entry (2) + padding (2) = 16 bytes
        // Second block: header (12) + 1 entry (2) + padding (2) = 16 bytes
        // Total = 32 bytes
        assert_eq!(bytes.len(), 32);

        // Verify first block header
        assert_eq!(
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            0x1000
        );
        assert_eq!(
            u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            0x10
        );

        // Verify second block header
        assert_eq!(
            u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]),
            0x2000
        );
        assert_eq!(
            u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]),
            0x10
        );
    }

    #[test]
    fn entries_get_sorted_by_rva() {
        let mut section = RelocSection::new();
        // Add in reverse order
        section.add_dir64(0x1234);
        section.add_dir64(0x1100);

        let bytes = section.to_bytes();
        // Should be sorted: 0x1100 first, then 0x1234

        // Header: VA should be 0x1000 (page of both)
        assert_eq!(
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            0x1000
        );

        // First entry at offset 12: should have offset 0x100 (0x1100 - 0x1000)
        let entry1_word = u16::from_le_bytes([bytes[12], bytes[13]]);
        assert_eq!(entry1_word & 0x0FFF, 0x100);

        // Second entry at offset 14: should have offset 0x234 (0x1234 - 0x1000)
        let entry2_word = u16::from_le_bytes([bytes[14], bytes[15]]);
        assert_eq!(entry2_word & 0x0FFF, 0x234);
    }

    #[test]
    fn round_trip_through_from_bytes() {
        let mut original = RelocSection::new();
        original.add_dir64(0x1000);
        original.add_dir64(0x1234);
        original.add_dir64(0x2500);

        let bytes = original.to_bytes();
        let parsed = RelocSection::from_bytes(&bytes).expect("from_bytes failed");

        // Should have same number of relocations
        assert_eq!(parsed.len(), 3);

        // Verify all relocations present (order may differ, so check as sets)
        let parsed_rvas: std::collections::HashSet<_> =
            parsed.relocations.iter().map(|r| r.rva).collect();
        assert_eq!(parsed_rvas.len(), 3);
        assert!(parsed_rvas.contains(&0x1000));
        assert!(parsed_rvas.contains(&0x1234));
        assert!(parsed_rvas.contains(&0x2500));
    }

    #[test]
    fn snapshot_byte_layout() {
        let mut section = RelocSection::new();
        section.add_dir64(0x1234);

        let bytes = section.to_bytes();

        // Header VA = 0x1234 -> page = 0x1000
        // bytes [0..4]: VA (0x1000 LE)
        let va = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert_eq!(va, 0x1000);
        assert_eq!(&bytes[0..4], &[0x00, 0x10, 0x00, 0x00]);

        // bytes [4..8]: size = 0x10 LE
        let size = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(size, 0x10);
        assert_eq!(&bytes[4..8], &[0x10, 0x00, 0x00, 0x00]);

        // Entry = (10 << 12) | 0x234 = 0xA234 LE
        // bytes [12..14]: entry
        let entry = u16::from_le_bytes([bytes[12], bytes[13]]);
        assert_eq!(entry, 0xA234);
        assert_eq!(&bytes[12..14], &[0x34, 0xA2]);

        // ABSOLUTE pad
        // bytes [14..16]: pad (0x0000 LE)
        assert_eq!(&bytes[14..16], &[0x00, 0x00]);
    }
}
