//! `.imports` and `.exports` section content: capability/service descriptors.
//!
//! Both sections use the same 32-byte entry format:
//!
//! | Offset | Size | Field              |
//! |--------|------|--------------------|
//! | 0      | 8    | name_offset        |
//! | 8      | 8    | blake3_name_hash   |
//! | 16     | 4    | cap_kind           |
//! | 20     | 4    | required_lin_class |
//! | 24     | 8    | flags              |
//!
//! ImportsSection: required capabilities (what this PAX needs from the loader).
//! ExportsSection: provided services (what this PAX offers).

use crate::caps::{CapKind, LinClass};
use static_assertions::const_assert_eq;

/// Size of a single capability descriptor entry in bytes.
pub const CAP_DESC_SIZE: usize = 32;

// Verify the capability descriptor size is correct at compile time.
const_assert_eq!(CAP_DESC_SIZE, 32);

/// Flags for capability descriptors (bits in the 8-byte flags field).
pub mod cap_flags {
    /// Bit 0: capability is optional (can fail at runtime).
    pub const OPTIONAL: u64 = 0x01;
    /// Bit 1: capability is marked as deprecated.
    pub const DEPRECATED: u64 = 0x02;
}

/// A single capability/service descriptor entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapDescriptor {
    /// Offset into string table for the capability/service name.
    pub name_offset: u64,
    /// BLAKE3 hash (first 8 bytes) of the capability/service name.
    pub blake3_name_hash: u64,
    /// Capability kind.
    pub cap_kind: CapKind,
    /// Required linearity class.
    pub required_lin_class: LinClass,
    /// Flags: bit 0 = optional, bit 1 = deprecated.
    pub flags: u64,
}

impl CapDescriptor {
    /// Create a new capability descriptor.
    ///
    /// # Arguments
    ///
    /// * `name_offset` - Offset into string table
    /// * `blake3_name_hash` - BLAKE3 hash of the name
    /// * `cap_kind` - Capability kind
    /// * `required_lin_class` - Required linearity class
    /// * `flags` - Flags (bit 0 = optional, bit 1 = deprecated)
    ///
    /// # Returns
    ///
    /// A new CapDescriptor.
    pub fn new(
        name_offset: u64,
        blake3_name_hash: u64,
        cap_kind: CapKind,
        required_lin_class: LinClass,
        flags: u64,
    ) -> Self {
        Self {
            name_offset,
            blake3_name_hash,
            cap_kind,
            required_lin_class,
            flags,
        }
    }

    /// Check if this descriptor is marked optional.
    pub fn is_optional(&self) -> bool {
        (self.flags & cap_flags::OPTIONAL) != 0
    }

    /// Check if this descriptor is marked deprecated.
    pub fn is_deprecated(&self) -> bool {
        (self.flags & cap_flags::DEPRECATED) != 0
    }

    /// Serialize this entry to its 32-byte little-endian representation.
    pub fn to_bytes(&self) -> [u8; CAP_DESC_SIZE] {
        let mut bytes = [0u8; CAP_DESC_SIZE];

        // Offset 0: name_offset (8 bytes, little-endian)
        bytes[0..8].copy_from_slice(&self.name_offset.to_le_bytes());

        // Offset 8: blake3_name_hash (8 bytes, little-endian)
        bytes[8..16].copy_from_slice(&self.blake3_name_hash.to_le_bytes());

        // Offset 16: cap_kind (4 bytes, little-endian)
        bytes[16..20].copy_from_slice(&(self.cap_kind as u32).to_le_bytes());

        // Offset 20: required_lin_class (4 bytes, little-endian)
        bytes[20..24].copy_from_slice(&(self.required_lin_class as u32).to_le_bytes());

        // Offset 24: flags (8 bytes, little-endian)
        bytes[24..32].copy_from_slice(&self.flags.to_le_bytes());

        bytes
    }

    /// Parse a capability descriptor from a byte slice.
    ///
    /// Returns `Some(descriptor)` if the input contains at least CAP_DESC_SIZE bytes
    /// and all enum fields are valid. Returns `None` on invalid enum value or
    /// short input.
    ///
    /// All multi-byte integers are decoded as little-endian.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < CAP_DESC_SIZE {
            return None;
        }

        // Offset 0: name_offset (8 bytes, little-endian)
        let name_offset = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);

        // Offset 8: blake3_name_hash (8 bytes, little-endian)
        let blake3_name_hash = u64::from_le_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ]);

        // Offset 16: cap_kind (4 bytes, little-endian)
        let cap_kind_u32 = u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let cap_kind = match cap_kind_u32 {
            0x01 => CapKind::MmioMemCap,
            0x02 => CapKind::IpcChannel,
            0x03 => CapKind::FsCap,
            0x04 => CapKind::NetCap,
            0xFF => CapKind::Other,
            _ => return None,
        };

        // Offset 20: required_lin_class (4 bytes, little-endian)
        let lin_class_u32 = u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        let required_lin_class = match lin_class_u32 {
            0x01 => LinClass::Linear,
            0x02 => LinClass::Affine,
            0x03 => LinClass::Ordered,
            0x04 => LinClass::Unrestricted,
            _ => return None,
        };

        // Offset 24: flags (8 bytes, little-endian)
        let flags = u64::from_le_bytes([
            bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31],
        ]);

        Some(Self {
            name_offset,
            blake3_name_hash,
            cap_kind,
            required_lin_class,
            flags,
        })
    }
}

/// Imports section: list of required capabilities.
#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub struct ImportsSection {
    /// List of required capability descriptors.
    pub entries: Vec<CapDescriptor>,
}

impl ImportsSection {
    /// Create a new, empty imports section.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a capability descriptor to the imports section.
    pub fn push(&mut self, e: CapDescriptor) {
        self.entries.push(e);
    }

    /// Serialize the entire section to a byte vector.
    ///
    /// Each entry is serialized to CAP_DESC_SIZE bytes in order.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.entries.len() * CAP_DESC_SIZE);
        for entry in &self.entries {
            bytes.extend_from_slice(&entry.to_bytes());
        }
        bytes
    }

    /// Parse an imports section from a byte slice.
    ///
    /// Returns `Some(section)` if the input length is a multiple of CAP_DESC_SIZE
    /// and all entries parse successfully. Returns `None` on invalid size or
    /// unparseable entries.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if !bytes.len().is_multiple_of(CAP_DESC_SIZE) {
            return None;
        }

        let count = bytes.len() / CAP_DESC_SIZE;
        let mut entries = Vec::with_capacity(count);

        for i in 0..count {
            let offset = i * CAP_DESC_SIZE;
            let entry_bytes = &bytes[offset..offset + CAP_DESC_SIZE];
            let entry = CapDescriptor::from_bytes(entry_bytes)?;
            entries.push(entry);
        }

        Some(Self { entries })
    }

    /// Return the number of entries in the section.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the section is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Exports section: list of provided services.
#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub struct ExportsSection {
    /// List of provided capability/service descriptors.
    pub entries: Vec<CapDescriptor>,
}

impl ExportsSection {
    /// Create a new, empty exports section.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a capability descriptor to the exports section.
    pub fn push(&mut self, e: CapDescriptor) {
        self.entries.push(e);
    }

    /// Serialize the entire section to a byte vector.
    ///
    /// Each entry is serialized to CAP_DESC_SIZE bytes in order.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.entries.len() * CAP_DESC_SIZE);
        for entry in &self.entries {
            bytes.extend_from_slice(&entry.to_bytes());
        }
        bytes
    }

    /// Parse an exports section from a byte slice.
    ///
    /// Returns `Some(section)` if the input length is a multiple of CAP_DESC_SIZE
    /// and all entries parse successfully. Returns `None` on invalid size or
    /// unparseable entries.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if !bytes.len().is_multiple_of(CAP_DESC_SIZE) {
            return None;
        }

        let count = bytes.len() / CAP_DESC_SIZE;
        let mut entries = Vec::with_capacity(count);

        for i in 0..count {
            let offset = i * CAP_DESC_SIZE;
            let entry_bytes = &bytes[offset..offset + CAP_DESC_SIZE];
            let entry = CapDescriptor::from_bytes(entry_bytes)?;
            entries.push(entry);
        }

        Some(Self { entries })
    }

    /// Return the number of entries in the section.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the section is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caps::{CapKind, LinClass};

    #[test]
    fn cap_desc_size_is_32_bytes() {
        assert_eq!(CAP_DESC_SIZE, 32);
    }

    #[test]
    fn single_cap_descriptor_roundtrip() {
        let desc = CapDescriptor::new(
            0,
            0x1234567890ABCDEF,
            CapKind::MmioMemCap,
            LinClass::Linear,
            0,
        );

        let bytes = desc.to_bytes();
        assert_eq!(bytes.len(), CAP_DESC_SIZE);

        let parsed = CapDescriptor::from_bytes(&bytes);
        assert_eq!(parsed, Some(desc));
    }

    #[test]
    fn cap_descriptor_with_flags_roundtrip() {
        let desc = CapDescriptor::new(
            100,
            0xFEDCBA9876543210,
            CapKind::IpcChannel,
            LinClass::Affine,
            cap_flags::OPTIONAL | cap_flags::DEPRECATED,
        );

        assert!(desc.is_optional());
        assert!(desc.is_deprecated());

        let bytes = desc.to_bytes();
        let parsed = CapDescriptor::from_bytes(&bytes);
        assert_eq!(parsed, Some(desc));
        if let Some(parsed_desc) = parsed {
            assert!(parsed_desc.is_optional());
            assert!(parsed_desc.is_deprecated());
        } else {
            panic!("Failed to parse descriptor");
        }
    }

    #[test]
    fn imports_section_1_entry_roundtrip() {
        let mut section = ImportsSection::new();
        let cap = CapDescriptor::new(
            0,
            0xABCD,
            CapKind::MmioMemCap,
            LinClass::Linear,
            cap_flags::OPTIONAL,
        );
        section.push(cap);

        let bytes = section.to_bytes();
        assert_eq!(bytes.len(), CAP_DESC_SIZE);

        let parsed = ImportsSection::from_bytes(&bytes);
        assert_eq!(parsed, Some(section));
    }

    #[test]
    fn exports_section_1_entry_roundtrip() {
        let mut section = ExportsSection::new();
        let cap = CapDescriptor::new(50, 0xDEADBEEF, CapKind::FsCap, LinClass::Unrestricted, 0);
        section.push(cap);

        let bytes = section.to_bytes();
        assert_eq!(bytes.len(), CAP_DESC_SIZE);

        let parsed = ExportsSection::from_bytes(&bytes);
        assert_eq!(parsed, Some(section));
    }

    #[test]
    fn imports_section_3_entries_roundtrip() {
        let mut section = ImportsSection::new();

        let c1 = CapDescriptor::new(0, 111, CapKind::MmioMemCap, LinClass::Linear, 0);
        let c2 = CapDescriptor::new(
            100,
            222,
            CapKind::IpcChannel,
            LinClass::Affine,
            cap_flags::OPTIONAL,
        );
        let c3 = CapDescriptor::new(
            200,
            333,
            CapKind::NetCap,
            LinClass::Ordered,
            cap_flags::DEPRECATED,
        );

        section.push(c1);
        section.push(c2);
        section.push(c3);

        let bytes = section.to_bytes();
        assert_eq!(bytes.len(), 3 * CAP_DESC_SIZE);

        let parsed = ImportsSection::from_bytes(&bytes);
        assert_eq!(parsed, Some(section));
    }

    #[test]
    fn exports_section_3_entries_roundtrip() {
        let mut section = ExportsSection::new();

        let c1 = CapDescriptor::new(0, 111, CapKind::MmioMemCap, LinClass::Linear, 0);
        let c2 = CapDescriptor::new(
            100,
            222,
            CapKind::IpcChannel,
            LinClass::Affine,
            cap_flags::OPTIONAL,
        );
        let c3 = CapDescriptor::new(
            200,
            333,
            CapKind::NetCap,
            LinClass::Ordered,
            cap_flags::DEPRECATED,
        );

        section.push(c1);
        section.push(c2);
        section.push(c3);

        let bytes = section.to_bytes();
        assert_eq!(bytes.len(), 3 * CAP_DESC_SIZE);

        let parsed = ExportsSection::from_bytes(&bytes);
        assert_eq!(parsed, Some(section));
    }

    #[test]
    fn cap_descriptor_schema_snapshot_byte_layout() {
        let desc = CapDescriptor {
            name_offset: 0x0102030405060708,
            blake3_name_hash: 0x0A0B0C0D0E0F1011,
            cap_kind: CapKind::IpcChannel,
            required_lin_class: LinClass::Affine,
            flags: 0x1213141516171819,
        };

        let bytes = desc.to_bytes();

        // Offset 0..8: name_offset
        assert_eq!(
            &bytes[0..8],
            &[0x08u8, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
        );

        // Offset 8..16: blake3_name_hash
        assert_eq!(
            &bytes[8..16],
            &[0x11u8, 0x10, 0x0F, 0x0E, 0x0D, 0x0C, 0x0B, 0x0A]
        );

        // Offset 16..20: cap_kind (CapKind::IpcChannel = 0x02)
        assert_eq!(&bytes[16..20], &[0x02u8, 0x00, 0x00, 0x00]);

        // Offset 20..24: required_lin_class (LinClass::Affine = 0x02)
        assert_eq!(&bytes[20..24], &[0x02u8, 0x00, 0x00, 0x00]);

        // Offset 24..32: flags
        assert_eq!(
            &bytes[24..32],
            &[0x19u8, 0x18, 0x17, 0x16, 0x15, 0x14, 0x13, 0x12]
        );
    }

    #[test]
    fn from_bytes_rejects_truncated_cap_descriptor() {
        let bytes = [0u8; CAP_DESC_SIZE - 1];
        let result = CapDescriptor::from_bytes(&bytes);
        assert_eq!(result, None);
    }

    #[test]
    fn from_bytes_rejects_truncated_imports_section() {
        let bytes = [0u8; CAP_DESC_SIZE - 1];
        let result = ImportsSection::from_bytes(&bytes);
        assert_eq!(result, None);
    }

    #[test]
    fn from_bytes_rejects_truncated_exports_section() {
        let bytes = [0u8; CAP_DESC_SIZE - 1];
        let result = ExportsSection::from_bytes(&bytes);
        assert_eq!(result, None);
    }

    #[test]
    fn empty_imports_section_roundtrips() {
        let section = ImportsSection::new();
        assert!(section.is_empty());
        assert_eq!(section.len(), 0);

        let bytes = section.to_bytes();
        assert_eq!(bytes.len(), 0);

        let parsed = ImportsSection::from_bytes(&bytes);
        assert_eq!(parsed, Some(section));
    }

    #[test]
    fn empty_exports_section_roundtrips() {
        let section = ExportsSection::new();
        assert!(section.is_empty());
        assert_eq!(section.len(), 0);

        let bytes = section.to_bytes();
        assert_eq!(bytes.len(), 0);

        let parsed = ExportsSection::from_bytes(&bytes);
        assert_eq!(parsed, Some(section));
    }

    #[test]
    fn imports_and_exports_coexist_independently() {
        let mut imports = ImportsSection::new();
        let mut exports = ExportsSection::new();

        let cap_import = CapDescriptor::new(0, 111, CapKind::MmioMemCap, LinClass::Linear, 0);
        let cap_export = CapDescriptor::new(100, 222, CapKind::FsCap, LinClass::Unrestricted, 0);

        imports.push(cap_import);
        exports.push(cap_export);

        assert_eq!(imports.len(), 1);
        assert_eq!(exports.len(), 1);

        let imports_bytes = imports.to_bytes();
        let exports_bytes = exports.to_bytes();

        let parsed_imports = ImportsSection::from_bytes(&imports_bytes);
        let parsed_exports = ExportsSection::from_bytes(&exports_bytes);

        assert_eq!(parsed_imports, Some(imports));
        assert_eq!(parsed_exports, Some(exports));
    }

    #[test]
    fn every_cap_kind_in_descriptor_round_trips() {
        let kinds = [
            CapKind::MmioMemCap,
            CapKind::IpcChannel,
            CapKind::FsCap,
            CapKind::NetCap,
            CapKind::Other,
        ];

        for expected_kind in kinds {
            let desc = CapDescriptor::new(0, 0, expected_kind, LinClass::Linear, 0);

            let bytes = desc.to_bytes();
            let parsed = CapDescriptor::from_bytes(&bytes).expect("Failed to parse descriptor");
            assert_eq!(parsed.cap_kind, expected_kind);
        }
    }

    #[test]
    fn every_lin_class_in_descriptor_round_trips() {
        let classes = [
            LinClass::Linear,
            LinClass::Affine,
            LinClass::Ordered,
            LinClass::Unrestricted,
        ];

        for expected_class in classes {
            let desc = CapDescriptor::new(0, 0, CapKind::MmioMemCap, expected_class, 0);

            let bytes = desc.to_bytes();
            let parsed = CapDescriptor::from_bytes(&bytes).expect("Failed to parse descriptor");
            assert_eq!(parsed.required_lin_class, expected_class);
        }
    }
}
