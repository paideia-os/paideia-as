//! PE/COFF import table (.idata) emission for Microsoft x64 / UEFI binaries.
//!
//! This module defines the binary layout, serialization, and parsing of the import table
//! (.idata section). All multi-byte fields are serialized in little-endian byte order.
//!
//! Layout summary:
//! - Import Descriptor Array (20 bytes each): One per DLL, plus a null-terminator.
//! - Import Lookup Table (ILT): Array of u64 RVA entries (one per symbol), null-terminated.
//! - Import Address Table (IAT): Identical copy of ILT (filled in by loader at runtime).
//! - Hint+Name Structs: Pairs of [u16 hint LE][name bytes][NUL][2-byte alignment].
//! - DLL Name Strings: NUL-terminated ASCII strings.

use crate::section::align_up;
use static_assertions::const_assert_eq;

// ============================================================================
// Constants
// ============================================================================

/// Size of an import descriptor in bytes (PE/COFF standard).
pub const IMAGE_IMPORT_DESCRIPTOR_SIZE: usize = 20;

/// Ordinal flag for 64-bit imports (high bit set to indicate ordinal import).
pub const IMPORT_ORDINAL_FLAG_64: u64 = 1 << 63;

/// Alignment for hint+name structs (must be 2-byte aligned).
pub const HINT_NAME_ALIGN: u32 = 2;

const_assert_eq!(IMAGE_IMPORT_DESCRIPTOR_SIZE, 20);

// ============================================================================
// ImportDescriptor
// ============================================================================

/// A single import descriptor (20 bytes).
///
/// Describes the location of symbol names (ILT) and addresses (IAT) for a single DLL.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ImportDescriptor {
    /// RVA of the Import Lookup Table (symbol name pointers).
    pub import_lookup_table_rva: u32,
    /// Time/date stamp (always 0 per spec).
    pub time_date_stamp: u32,
    /// Forwarder chain (always 0 per spec).
    pub forwarder_chain: u32,
    /// RVA of NUL-terminated ASCII DLL name.
    pub name_rva: u32,
    /// RVA of the Import Address Table (loader fills this at runtime).
    pub import_address_table_rva: u32,
}

impl ImportDescriptor {
    /// Serialize to a 20-byte little-endian representation.
    pub fn to_bytes(&self) -> [u8; IMAGE_IMPORT_DESCRIPTOR_SIZE] {
        let mut bytes = [0u8; IMAGE_IMPORT_DESCRIPTOR_SIZE];

        bytes[0..4].copy_from_slice(&self.import_lookup_table_rva.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.time_date_stamp.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.forwarder_chain.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.name_rva.to_le_bytes());
        bytes[16..20].copy_from_slice(&self.import_address_table_rva.to_le_bytes());

        bytes
    }

    /// Parse from a byte slice.
    ///
    /// Returns `Some(descriptor)` if input is at least 20 bytes.
    /// Returns `None` on short input.
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < IMAGE_IMPORT_DESCRIPTOR_SIZE {
            return None;
        }

        Some(Self {
            import_lookup_table_rva: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            time_date_stamp: u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            forwarder_chain: u32::from_le_bytes([b[8], b[9], b[10], b[11]]),
            name_rva: u32::from_le_bytes([b[12], b[13], b[14], b[15]]),
            import_address_table_rva: u32::from_le_bytes([b[16], b[17], b[18], b[19]]),
        })
    }
}

// ============================================================================
// Import & ImportSection
// ============================================================================

/// A single DLL import with its list of symbols.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    /// DLL name (e.g., "KERNEL32.dll").
    pub dll_name: String,
    /// Symbol names (function names to import).
    pub symbols: Vec<String>,
}

/// A collection of imports from multiple DLLs.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ImportSection {
    /// List of imports, one per DLL.
    pub imports: Vec<Import>,
}

impl ImportSection {
    /// Create a new empty import section.
    pub fn new() -> Self {
        Self {
            imports: Vec::new(),
        }
    }

    /// Add a DLL import with its symbol list.
    pub fn add(&mut self, dll_name: &str, symbols: Vec<String>) {
        self.imports.push(Import {
            dll_name: dll_name.to_string(),
            symbols,
        });
    }

    /// Serialize to bytes with the given base RVA.
    ///
    /// Two-pass layout algorithm:
    /// 1. Pass 1: Assign offsets to each section.
    /// 2. Pass 2: Write bytes.
    ///
    /// Layout order:
    /// - Descriptor array (N descriptors + 1 null-terminator = 20 * (N+1) bytes)
    /// - Per-DLL ILT (array of u64 entries, each DLL gets its own array + null terminator)
    /// - Per-DLL IAT (identical copy of ILT, same size)
    /// - Hint+Name structs (2-byte aligned, grouped by DLL)
    /// - DLL name strings (NUL-terminated, one per DLL)
    pub fn to_bytes(&self, base_rva: u32) -> Vec<u8> {
        let num_imports = self.imports.len();

        // Pass 1: Compute offsets
        let mut offset = 0u32;

        // Descriptor array (N + 1 null-terminator)
        let descriptor_array_offset = offset;
        offset += (IMAGE_IMPORT_DESCRIPTOR_SIZE as u32) * (num_imports as u32 + 1);

        // Per-DLL ILTs
        let mut ilt_offsets = Vec::new();
        for import in &self.imports {
            let ilt_offset = offset;
            ilt_offsets.push(ilt_offset);
            // Each symbol is a u64 entry, plus a null terminator
            offset += 8 * (import.symbols.len() as u32 + 1);
        }

        // Per-DLL IATs (identical layout)
        let mut iat_offsets = Vec::new();
        for import in &self.imports {
            let iat_offset = offset;
            iat_offsets.push(iat_offset);
            offset += 8 * (import.symbols.len() as u32 + 1);
        }

        // Hint+Name structs (2-byte aligned)
        let mut hint_name_offsets = Vec::new();
        for import in &self.imports {
            let mut dll_hint_offsets = Vec::new();
            for symbol in &import.symbols {
                offset = align_up(offset, HINT_NAME_ALIGN);
                let hint_offset = offset;
                dll_hint_offsets.push(hint_offset);
                // 2 bytes (hint) + name length + 1 (NUL)
                offset += 2 + symbol.len() as u32 + 1;
            }
            hint_name_offsets.push(dll_hint_offsets);
        }

        // DLL name strings
        let mut dll_name_offsets = Vec::new();
        for import in &self.imports {
            let dll_offset = offset;
            dll_name_offsets.push(dll_offset);
            offset += import.dll_name.len() as u32 + 1;
        }

        let total_len = offset as usize;

        // Pass 2: Write bytes
        let mut result = vec![0u8; total_len];

        // Write descriptor array
        for (i, _import) in self.imports.iter().enumerate() {
            let desc_offset = descriptor_array_offset as usize + i * IMAGE_IMPORT_DESCRIPTOR_SIZE;
            let descriptor = ImportDescriptor {
                import_lookup_table_rva: base_rva + ilt_offsets[i],
                time_date_stamp: 0,
                forwarder_chain: 0,
                name_rva: base_rva + dll_name_offsets[i],
                import_address_table_rva: base_rva + iat_offsets[i],
            };
            result[desc_offset..desc_offset + IMAGE_IMPORT_DESCRIPTOR_SIZE]
                .copy_from_slice(&descriptor.to_bytes());
        }

        // Write null-terminator descriptor
        let null_desc_offset =
            descriptor_array_offset as usize + num_imports * IMAGE_IMPORT_DESCRIPTOR_SIZE;
        result[null_desc_offset..null_desc_offset + IMAGE_IMPORT_DESCRIPTOR_SIZE]
            .copy_from_slice(&[0u8; IMAGE_IMPORT_DESCRIPTOR_SIZE]);

        // Write ILTs and IATs (identical)
        for (i, import) in self.imports.iter().enumerate() {
            for (j, _symbol) in import.symbols.iter().enumerate() {
                let hint_name_rva = base_rva + hint_name_offsets[i][j];
                let entry = (hint_name_rva as u64).to_le_bytes();

                // Write to ILT (u64, so 8 bytes)
                let ilt_entry_offset = ilt_offsets[i] as usize + j * 8;
                result[ilt_entry_offset..ilt_entry_offset + 8].copy_from_slice(&entry);

                // Write to IAT (identical)
                let iat_entry_offset = iat_offsets[i] as usize + j * 8;
                result[iat_entry_offset..iat_entry_offset + 8].copy_from_slice(&entry);
            }

            // Write null terminators for ILT and IAT (u64, so 8 bytes)
            let ilt_null_offset = ilt_offsets[i] as usize + import.symbols.len() * 8;
            result[ilt_null_offset..ilt_null_offset + 8].copy_from_slice(&0u64.to_le_bytes());

            let iat_null_offset = iat_offsets[i] as usize + import.symbols.len() * 8;
            result[iat_null_offset..iat_null_offset + 8].copy_from_slice(&0u64.to_le_bytes());
        }

        // Write hint+name structs
        for (i, import) in self.imports.iter().enumerate() {
            for (j, symbol) in import.symbols.iter().enumerate() {
                let hint_offset = hint_name_offsets[i][j] as usize;
                // Hint (set to 0 for now)
                result[hint_offset..hint_offset + 2].copy_from_slice(&0u16.to_le_bytes());
                // Name bytes
                let name_start = hint_offset + 2;
                result[name_start..name_start + symbol.len()].copy_from_slice(symbol.as_bytes());
                // NUL terminator
                result[name_start + symbol.len()] = 0;
            }
        }

        // Write DLL name strings
        for (i, import) in self.imports.iter().enumerate() {
            let dll_offset = dll_name_offsets[i] as usize;
            result[dll_offset..dll_offset + import.dll_name.len()]
                .copy_from_slice(import.dll_name.as_bytes());
            result[dll_offset + import.dll_name.len()] = 0;
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_import_section_serialises_to_empty_or_min() {
        let section = ImportSection::new();
        // Empty section should just have the null-terminator descriptor
        let bytes = section.to_bytes(0x1000);
        // No imports, so just 1 null-terminator descriptor (20 bytes)
        assert_eq!(bytes.len(), IMAGE_IMPORT_DESCRIPTOR_SIZE);
        // Should all be zeros
        assert_eq!(&bytes[..], &[0u8; IMAGE_IMPORT_DESCRIPTOR_SIZE]);
    }

    #[test]
    fn single_import_dll_round_trips() {
        let mut section = ImportSection::new();
        section.add("KERNEL32.dll", vec!["ExitProcess".to_string()]);

        let base_rva = 0x1000u32;
        let bytes = section.to_bytes(base_rva);

        // Minimum structure should be present
        assert!(!bytes.is_empty());

        // Parse descriptor at offset 0
        let desc = ImportDescriptor::from_bytes(&bytes).expect("Failed to parse descriptor");

        // Verify that RVAs point inside the blob
        assert!(desc.import_lookup_table_rva >= base_rva);
        assert!(desc.import_address_table_rva >= base_rva);
        assert!(desc.name_rva >= base_rva);

        // Verify offsets are reasonable
        let ilt_offset = (desc.import_lookup_table_rva - base_rva) as usize;
        let iat_offset = (desc.import_address_table_rva - base_rva) as usize;
        let name_offset = (desc.name_rva - base_rva) as usize;

        assert!(ilt_offset < bytes.len());
        assert!(iat_offset < bytes.len());
        assert!(name_offset < bytes.len());
    }

    #[test]
    fn multi_dll_imports_have_correct_descriptor_count() {
        let mut section = ImportSection::new();
        section.add("KERNEL32.dll", vec!["ExitProcess".to_string()]);
        section.add("NTDLL.dll", vec!["RtlCopyMemory".to_string()]);

        let bytes = section.to_bytes(0x1000);

        // Should have 3 descriptors (2 DLLs + 1 null-terminator)
        // First descriptor at offset 0
        let desc1 = ImportDescriptor::from_bytes(&bytes[0..20]).expect("desc1 parse failed");
        assert_ne!(desc1, ImportDescriptor::default());

        // Second descriptor at offset 20
        let desc2 = ImportDescriptor::from_bytes(&bytes[20..40]).expect("desc2 parse failed");
        assert_ne!(desc2, ImportDescriptor::default());

        // Null-terminator at offset 40
        let null_desc =
            ImportDescriptor::from_bytes(&bytes[40..60]).expect("null_desc parse failed");
        assert_eq!(null_desc, ImportDescriptor::default());
    }

    #[test]
    fn efi_boot_services_allocate_pages_descriptor() {
        let mut section = ImportSection::new();
        section.add("EFI_BOOT_SERVICES", vec!["AllocatePages".to_string()]);

        let base_rva = 0x2000u32;
        let bytes = section.to_bytes(base_rva);

        // Parse descriptor
        let desc = ImportDescriptor::from_bytes(&bytes).expect("descriptor parse failed");

        // Get offsets relative to base_rva
        let ilt_offset = (desc.import_lookup_table_rva - base_rva) as usize;
        let name_offset = (desc.name_rva - base_rva) as usize;

        // Verify ILT points to hint+name
        assert!(ilt_offset < bytes.len());
        let ilt_entry_rva = u32::from_le_bytes([
            bytes[ilt_offset],
            bytes[ilt_offset + 1],
            bytes[ilt_offset + 2],
            bytes[ilt_offset + 3],
        ]);

        // Hint+name should be somewhere in the blob
        let hint_name_offset = (ilt_entry_rva - base_rva) as usize;
        assert!(hint_name_offset < bytes.len());

        // Verify DLL name is present and correct
        assert!(name_offset + "EFI_BOOT_SERVICES".len() < bytes.len());
        assert_eq!(
            &bytes[name_offset..name_offset + "EFI_BOOT_SERVICES".len()],
            b"EFI_BOOT_SERVICES"
        );
    }
}
