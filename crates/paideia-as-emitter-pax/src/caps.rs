//! `.paideia.caps` section content: per-site capability descriptors.
//!
//! Each entry is 32 bytes:
//!
//! | Offset | Size | Field           |
//! |--------|------|-----------------|
//! | 0      | 4    | site_kind       |
//! | 4      | 4    | class           |
//! | 8      | 8    | location_id     |
//! | 16     | 4    | lam_tag         |
//! | 20     | 4    | cap_kind        |
//! | 24     | 8    | name_hash       |

use static_assertions::const_assert_eq;

/// Size of a single capability entry in bytes.
pub const CAP_ENTRY_SIZE: usize = 32;

// Verify the capability entry size is correct at compile time.
const_assert_eq!(CAP_ENTRY_SIZE, 32);

/// Capability binding site kind.
#[repr(u32)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SiteKind {
    /// Function parameter.
    FunctionParam = 0x01,
    /// Struct field.
    StructField = 0x02,
    /// Local let binding.
    LocalLet = 0x03,
}

/// Linearity class of a capability.
#[repr(u32)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum LinClass {
    /// Linear: must be used exactly once.
    Linear = 0x01,
    /// Affine: may be used at most once.
    Affine = 0x02,
    /// Ordered: must respect happens-before constraint.
    Ordered = 0x03,
    /// Unrestricted: no linearity constraint.
    Unrestricted = 0x04,
}

/// Kind of capability.
#[repr(u32)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum CapKind {
    /// Memory-mapped IO capability.
    MmioMemCap = 0x01,
    /// IPC channel capability.
    IpcChannel = 0x02,
    /// Filesystem capability.
    FsCap = 0x03,
    /// Network capability.
    NetCap = 0x04,
    /// Generic / extensible.
    Other = 0xFF,
}

/// A single capability binding site entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapEntry {
    /// Kind of binding site.
    pub site_kind: SiteKind,
    /// Linearity class.
    pub class: LinClass,
    /// Symbol-table index or AST node id depending on context.
    pub location_id: u64,
    /// LAM tag value for the capability's pointer at runtime.
    pub lam_tag: u32,
    /// Kind of capability.
    pub cap_kind: CapKind,
    /// BLAKE3-derived 8-byte hash of the binding name (for cross-
    /// PAX reconciliation without storing full strings here).
    pub name_hash: u64,
}

impl CapEntry {
    /// Create a new capability entry.
    ///
    /// # Arguments
    ///
    /// * `site_kind` - Kind of binding site
    /// * `class` - Linearity class
    /// * `location_id` - Symbol-table index or AST node id
    /// * `lam_tag` - LAM tag value
    /// * `cap_kind` - Kind of capability
    /// * `name` - The binding name (will be hashed)
    ///
    /// # Returns
    ///
    /// A new CapEntry with the name hashed for storage.
    pub fn new(
        site_kind: SiteKind,
        class: LinClass,
        location_id: u64,
        lam_tag: u32,
        cap_kind: CapKind,
        name: &str,
    ) -> Self {
        let name_hash = hash_name(name);
        Self {
            site_kind,
            class,
            location_id,
            lam_tag,
            cap_kind,
            name_hash,
        }
    }

    /// Serialize this entry to its 32-byte little-endian representation.
    ///
    /// Returns a fixed-size array matching the canonical PAX capability
    /// entry layout.
    pub fn to_bytes(&self) -> [u8; CAP_ENTRY_SIZE] {
        let mut bytes = [0u8; CAP_ENTRY_SIZE];

        // Offset 0: site_kind (4 bytes, little-endian)
        bytes[0..4].copy_from_slice(&(self.site_kind as u32).to_le_bytes());

        // Offset 4: class (4 bytes, little-endian)
        bytes[4..8].copy_from_slice(&(self.class as u32).to_le_bytes());

        // Offset 8: location_id (8 bytes, little-endian)
        bytes[8..16].copy_from_slice(&self.location_id.to_le_bytes());

        // Offset 16: lam_tag (4 bytes, little-endian)
        bytes[16..20].copy_from_slice(&self.lam_tag.to_le_bytes());

        // Offset 20: cap_kind (4 bytes, little-endian)
        bytes[20..24].copy_from_slice(&(self.cap_kind as u32).to_le_bytes());

        // Offset 24: name_hash (8 bytes, little-endian)
        bytes[24..32].copy_from_slice(&self.name_hash.to_le_bytes());

        bytes
    }

    /// Parse a capability entry from a byte slice.
    ///
    /// Returns `Some(entry)` if the input contains at least CAP_ENTRY_SIZE bytes
    /// and all enum fields are valid. Returns `None` on invalid enum value or
    /// short input.
    ///
    /// All multi-byte integers are decoded as little-endian.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < CAP_ENTRY_SIZE {
            return None;
        }

        // Offset 0: site_kind (4 bytes, little-endian)
        let site_kind_u32 = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let site_kind = match site_kind_u32 {
            0x01 => SiteKind::FunctionParam,
            0x02 => SiteKind::StructField,
            0x03 => SiteKind::LocalLet,
            _ => return None,
        };

        // Offset 4: class (4 bytes, little-endian)
        let class_u32 = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let class = match class_u32 {
            0x01 => LinClass::Linear,
            0x02 => LinClass::Affine,
            0x03 => LinClass::Ordered,
            0x04 => LinClass::Unrestricted,
            _ => return None,
        };

        // Offset 8: location_id (8 bytes, little-endian)
        let location_id = u64::from_le_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ]);

        // Offset 16: lam_tag (4 bytes, little-endian)
        let lam_tag = u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);

        // Offset 20: cap_kind (4 bytes, little-endian)
        let cap_kind_u32 = u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        let cap_kind = match cap_kind_u32 {
            0x01 => CapKind::MmioMemCap,
            0x02 => CapKind::IpcChannel,
            0x03 => CapKind::FsCap,
            0x04 => CapKind::NetCap,
            0xFF => CapKind::Other,
            _ => return None,
        };

        // Offset 24: name_hash (8 bytes, little-endian)
        let name_hash = u64::from_le_bytes([
            bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31],
        ]);

        Some(Self {
            site_kind,
            class,
            location_id,
            lam_tag,
            cap_kind,
            name_hash,
        })
    }
}

/// Compute a BLAKE3 hash of a name, returning the first 8 bytes as u64.
fn hash_name(name: &str) -> u64 {
    // BLAKE3 produces 32 bytes; we use the first 8 as the u64 hash.
    let h = blake3::hash(name.as_bytes());
    u64::from_le_bytes(h.as_bytes()[..8].try_into().unwrap())
}

/// Whole-section content: a sequence of CapEntry.
#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub struct CapsSection {
    /// List of capability entries.
    pub entries: Vec<CapEntry>,
}

impl CapsSection {
    /// Create a new, empty capabilities section.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a capability entry to the section.
    pub fn push(&mut self, e: CapEntry) {
        self.entries.push(e);
    }

    /// Serialize the entire section to a byte vector.
    ///
    /// Each entry is serialized to CAP_ENTRY_SIZE bytes in order.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.entries.len() * CAP_ENTRY_SIZE);
        for entry in &self.entries {
            bytes.extend_from_slice(&entry.to_bytes());
        }
        bytes
    }

    /// Parse a capabilities section from a byte slice.
    ///
    /// Returns `Some(section)` if the input length is a multiple of CAP_ENTRY_SIZE
    /// and all entries parse successfully. Returns `None` on invalid size or
    /// unparseable entries.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if !bytes.len().is_multiple_of(CAP_ENTRY_SIZE) {
            return None;
        }

        let count = bytes.len() / CAP_ENTRY_SIZE;
        let mut entries = Vec::with_capacity(count);

        for i in 0..count {
            let offset = i * CAP_ENTRY_SIZE;
            let entry_bytes = &bytes[offset..offset + CAP_ENTRY_SIZE];
            let entry = CapEntry::from_bytes(entry_bytes)?;
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

    #[test]
    fn cap_entry_size_is_32_bytes() {
        // Const assertion: this will not compile if CAP_ENTRY_SIZE != 32.
        const _: () = {
            const _: [(); 1] = [(); (CAP_ENTRY_SIZE == 32) as usize];
        };
        assert_eq!(CAP_ENTRY_SIZE, 32);
    }

    #[test]
    fn hash_name_is_stable() {
        let name1 = "test_function";
        let hash1a = hash_name(name1);
        let hash1b = hash_name(name1);
        assert_eq!(hash1a, hash1b, "Same name should produce same hash");

        let name2 = "different_function";
        let hash2 = hash_name(name2);
        assert_ne!(
            hash1a, hash2,
            "Different names should (very likely) produce different hashes"
        );
    }

    #[test]
    fn mmio_memcap_linear_function_param_roundtrip() {
        // AC 1: build an entry for a linear MmioMemCap function param;
        // serialise; parse; equal.
        let entry = CapEntry::new(
            SiteKind::FunctionParam,
            LinClass::Linear,
            42,
            0xDEADBEEF,
            CapKind::MmioMemCap,
            "uart_mem_cap",
        );

        let bytes = entry.to_bytes();
        assert_eq!(bytes.len(), CAP_ENTRY_SIZE);

        let parsed = CapEntry::from_bytes(&bytes);
        assert_eq!(parsed, Some(entry));
    }

    #[test]
    fn caps_section_3_entries_roundtrip() {
        // AC 2: build 3 entries, serialise the section, deserialise, assert.
        let mut section = CapsSection::new();

        let e1 = CapEntry::new(
            SiteKind::FunctionParam,
            LinClass::Linear,
            1,
            0x00000001,
            CapKind::MmioMemCap,
            "mmio_1",
        );
        let e2 = CapEntry::new(
            SiteKind::StructField,
            LinClass::Affine,
            2,
            0x00000002,
            CapKind::IpcChannel,
            "ipc_ch_1",
        );
        let e3 = CapEntry::new(
            SiteKind::LocalLet,
            LinClass::Ordered,
            3,
            0x00000003,
            CapKind::FsCap,
            "fs_root",
        );

        section.push(e1);
        section.push(e2);
        section.push(e3);

        let bytes = section.to_bytes();
        assert_eq!(bytes.len(), 3 * CAP_ENTRY_SIZE);

        let parsed = CapsSection::from_bytes(&bytes);
        assert_eq!(parsed, Some(section));
    }

    #[test]
    fn schema_snapshot_byte_layout() {
        // AC 3: hand-built entry; assert specific byte offsets.
        let entry = CapEntry {
            site_kind: SiteKind::FunctionParam,
            class: LinClass::Linear,
            location_id: 0x0102030405060708,
            lam_tag: 0x0A0B0C0D,
            cap_kind: CapKind::MmioMemCap,
            name_hash: 0x1112131415161718,
        };

        let bytes = entry.to_bytes();

        // Offset 0: site_kind (0x01) as u32 little-endian
        assert_eq!(&bytes[0..4], &[0x01u8, 0x00, 0x00, 0x00]);

        // Offset 4: class (0x01) as u32 little-endian
        assert_eq!(&bytes[4..8], &[0x01u8, 0x00, 0x00, 0x00]);

        // Offset 8: location_id (0x0102030405060708) as u64 little-endian
        assert_eq!(
            &bytes[8..16],
            &[0x08u8, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
        );

        // Offset 16: lam_tag (0x0A0B0C0D) as u32 little-endian
        assert_eq!(&bytes[16..20], &[0x0Du8, 0x0C, 0x0B, 0x0A]);

        // Offset 20: cap_kind (0x01) as u32 little-endian
        assert_eq!(&bytes[20..24], &[0x01u8, 0x00, 0x00, 0x00]);

        // Offset 24: name_hash (0x1112131415161718) as u64 little-endian
        assert_eq!(
            &bytes[24..32],
            &[0x18u8, 0x17, 0x16, 0x15, 0x14, 0x13, 0x12, 0x11]
        );
    }

    #[test]
    fn empty_caps_section_roundtrips() {
        let section = CapsSection::new();
        assert!(section.is_empty());
        assert_eq!(section.len(), 0);

        let bytes = section.to_bytes();
        assert_eq!(bytes.len(), 0);

        let parsed = CapsSection::from_bytes(&bytes);
        assert_eq!(parsed, Some(section));
    }

    #[test]
    fn from_bytes_rejects_truncated_input() {
        let bytes = [0u8; CAP_ENTRY_SIZE - 1];
        let result = CapEntry::from_bytes(&bytes);
        assert_eq!(result, None);
    }

    #[test]
    fn every_site_kind_round_trips() {
        let kinds = [
            SiteKind::FunctionParam,
            SiteKind::StructField,
            SiteKind::LocalLet,
        ];

        for expected_kind in kinds {
            let entry = CapEntry::new(
                expected_kind,
                LinClass::Linear,
                0,
                0,
                CapKind::MmioMemCap,
                "test",
            );

            let bytes = entry.to_bytes();
            let parsed = CapEntry::from_bytes(&bytes).expect("Failed to parse entry");
            assert_eq!(parsed.site_kind, expected_kind);
        }
    }

    #[test]
    fn every_lin_class_round_trips() {
        let classes = [
            LinClass::Linear,
            LinClass::Affine,
            LinClass::Ordered,
            LinClass::Unrestricted,
        ];

        for expected_class in classes {
            let entry = CapEntry::new(
                SiteKind::FunctionParam,
                expected_class,
                0,
                0,
                CapKind::MmioMemCap,
                "test",
            );

            let bytes = entry.to_bytes();
            let parsed = CapEntry::from_bytes(&bytes).expect("Failed to parse entry");
            assert_eq!(parsed.class, expected_class);
        }
    }

    #[test]
    fn every_cap_kind_round_trips() {
        let kinds = [
            CapKind::MmioMemCap,
            CapKind::IpcChannel,
            CapKind::FsCap,
            CapKind::NetCap,
            CapKind::Other,
        ];

        for expected_kind in kinds {
            let entry = CapEntry::new(
                SiteKind::FunctionParam,
                LinClass::Linear,
                0,
                0,
                expected_kind,
                "test",
            );

            let bytes = entry.to_bytes();
            let parsed = CapEntry::from_bytes(&bytes).expect("Failed to parse entry");
            assert_eq!(parsed.cap_kind, expected_kind);
        }
    }
}
