//! PAX file header per the PaideiaOS Architectural Executable
//! specification.
//!
//! PAX (PaideiaOS Architectural Executable) is the canonical object format
//! for PaideiaOS binaries, carrying capability signatures, effect rows,
//! post-quantum signatures, and BLAKE3 content hashes. This module defines
//! the binary header layout and serialization.
//!
//! Layout (little-endian, total 96 bytes):
//!
//! | Offset | Size | Field                       |
//! |--------|------|-----------------------------|
//! | 0      | 4    | magic = b"PAX\0"            |
//! | 4      | 2    | format_version              |
//! | 6      | 2    | architecture                |
//! | 8      | 8    | flags                       |
//! | 16     | 8    | section_table_offset        |
//! | 24     | 4    | section_count               |
//! | 28     | 4    | reserved                    |
//! | 32     | 32   | blake3_content_hash         |
//! | 64     | 32   | pq_signature_placeholder    |
//!
//! Phase-2-m4 minimum: PQ signature field is zero-filled until m7
//! (post-quantum signing milestone). BLAKE3 hash is populated at finalize time
//! over the canonical content per PaideiaOS specification.

use static_assertions::const_assert_eq;

/// Magic number for PAX files: `b"PAX\0"`.
pub const PAX_MAGIC: [u8; 4] = *b"PAX\0";

/// Current PAX format version.
pub const PAX_FORMAT_VERSION: u16 = 1;

/// Total size of PAX header in bytes.
pub const PAX_HEADER_SIZE: usize = 96;

// Verify the header size is correct at compile time.
const_assert_eq!(PAX_HEADER_SIZE, 96);

/// Architecture identifier for PAX headers.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u16)]
pub enum Architecture {
    /// x86-64 (amd64) architecture.
    X86_64 = 1,
    // Aarch64 = 2 (future)
}

/// Flags field for PAX headers, composable via bitwise OR.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u64)]
pub enum HeaderFlag {
    /// No flags set.
    None = 0,
    /// File is position-independent (relocatable object).
    Relocatable = 1 << 0,
    /// File is executable (main binary).
    Executable = 1 << 1,
    /// File contains debug information.
    HasDebugInfo = 1 << 2,
}

/// PAX (PaideiaOS Architectural Executable) file header.
///
/// Contains the canonical header structure for PAX object files, including
/// the magic number, format version, architecture, flags, section table
/// location, and cryptographic hashes.
#[derive(Clone, Debug)]
pub struct PaxHeader {
    /// Magic number identifying a PAX file (`b"PAX\0"`).
    pub magic: [u8; 4],
    /// Format version of this header (currently 1).
    pub format_version: u16,
    /// Target architecture.
    pub architecture: Architecture,
    /// Flags composing file characteristics (relocatable, executable, etc.).
    pub flags: u64,
    /// Absolute offset to the section table in the file.
    pub section_table_offset: u64,
    /// Number of sections in the section table.
    pub section_count: u32,
    /// Reserved field for future use (must be zero).
    pub reserved: u32,
    /// BLAKE3 hash of the canonical file content (populated at finalize time).
    pub blake3_content_hash: [u8; 32],
    /// Post-quantum signature placeholder (zero-filled until m7).
    pub pq_signature_placeholder: [u8; 32],
}

impl PaxHeader {
    /// Construct a fresh PAX header with sensible defaults.
    ///
    /// Initializes:
    /// - Magic and version to their canonical values
    /// - Architecture to the given value
    /// - Flags to `None`
    /// - Hashes and signatures to zero-filled (to be populated at finalize time)
    /// - Section table offset and count to zero (to be populated later)
    ///
    /// # Example
    ///
    /// ```
    /// use paideia_as_emitter_pax::{PaxHeader, Architecture};
    ///
    /// let header = PaxHeader::new(Architecture::X86_64);
    /// assert_eq!(header.magic, *b"PAX\0");
    /// assert_eq!(header.format_version, 1);
    /// assert_eq!(header.architecture, Architecture::X86_64);
    /// ```
    pub fn new(architecture: Architecture) -> Self {
        Self {
            magic: PAX_MAGIC,
            format_version: PAX_FORMAT_VERSION,
            architecture,
            flags: HeaderFlag::None as u64,
            section_table_offset: 0,
            section_count: 0,
            reserved: 0,
            blake3_content_hash: [0u8; 32],
            pq_signature_placeholder: [0u8; 32],
        }
    }

    /// Serialize the header to its 96-byte little-endian representation.
    ///
    /// Returns a fixed-size array matching the canonical PAX header layout.
    /// All multi-byte integers are encoded in little-endian byte order.
    ///
    /// # Example
    ///
    /// ```
    /// use paideia_as_emitter_pax::{PaxHeader, Architecture, PAX_HEADER_SIZE};
    ///
    /// let header = PaxHeader::new(Architecture::X86_64);
    /// let bytes = header.to_bytes();
    /// assert_eq!(bytes.len(), PAX_HEADER_SIZE);
    /// assert_eq!(&bytes[0..4], b"PAX\0");
    /// ```
    pub fn to_bytes(&self) -> [u8; PAX_HEADER_SIZE] {
        let mut bytes = [0u8; PAX_HEADER_SIZE];

        // Offset 0: magic (4 bytes)
        bytes[0..4].copy_from_slice(&self.magic);

        // Offset 4: format_version (2 bytes, little-endian)
        bytes[4..6].copy_from_slice(&self.format_version.to_le_bytes());

        // Offset 6: architecture (2 bytes, little-endian)
        bytes[6..8].copy_from_slice(&(self.architecture as u16).to_le_bytes());

        // Offset 8: flags (8 bytes, little-endian)
        bytes[8..16].copy_from_slice(&self.flags.to_le_bytes());

        // Offset 16: section_table_offset (8 bytes, little-endian)
        bytes[16..24].copy_from_slice(&self.section_table_offset.to_le_bytes());

        // Offset 24: section_count (4 bytes, little-endian)
        bytes[24..28].copy_from_slice(&self.section_count.to_le_bytes());

        // Offset 28: reserved (4 bytes, little-endian, zero-filled)
        bytes[28..32].copy_from_slice(&self.reserved.to_le_bytes());

        // Offset 32: blake3_content_hash (32 bytes)
        bytes[32..64].copy_from_slice(&self.blake3_content_hash);

        // Offset 64: pq_signature_placeholder (32 bytes)
        bytes[64..96].copy_from_slice(&self.pq_signature_placeholder);

        bytes
    }

    /// Parse a PAX header from a byte slice.
    ///
    /// Returns `Some(header)` if the input contains at least 96 bytes and the
    /// magic number matches `b"PAX\0"`. Returns `None` on magic mismatch or
    /// short input.
    ///
    /// All multi-byte integers are decoded as little-endian.
    ///
    /// # Example
    ///
    /// ```
    /// use paideia_as_emitter_pax::{PaxHeader, Architecture};
    ///
    /// let header = PaxHeader::new(Architecture::X86_64);
    /// let bytes = header.to_bytes();
    /// let parsed = PaxHeader::from_bytes(&bytes);
    /// assert_eq!(parsed, Some(header));
    /// ```
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < PAX_HEADER_SIZE {
            return None;
        }

        // Offset 0: magic (4 bytes)
        let magic = [bytes[0], bytes[1], bytes[2], bytes[3]];
        if magic != PAX_MAGIC {
            return None;
        }

        // Offset 4: format_version (2 bytes, little-endian)
        let format_version = u16::from_le_bytes([bytes[4], bytes[5]]);

        // Offset 6: architecture (2 bytes, little-endian)
        let arch_u16 = u16::from_le_bytes([bytes[6], bytes[7]]);
        let architecture = match arch_u16 {
            1 => Architecture::X86_64,
            _ => return None,
        };

        // Offset 8: flags (8 bytes, little-endian)
        let flags = u64::from_le_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ]);

        // Offset 16: section_table_offset (8 bytes, little-endian)
        let section_table_offset = u64::from_le_bytes([
            bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23],
        ]);

        // Offset 24: section_count (4 bytes, little-endian)
        let section_count = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);

        // Offset 28: reserved (4 bytes, little-endian)
        let reserved = u32::from_le_bytes([bytes[28], bytes[29], bytes[30], bytes[31]]);

        // Offset 32: blake3_content_hash (32 bytes)
        let mut blake3_content_hash = [0u8; 32];
        blake3_content_hash.copy_from_slice(&bytes[32..64]);

        // Offset 64: pq_signature_placeholder (32 bytes)
        let mut pq_signature_placeholder = [0u8; 32];
        pq_signature_placeholder.copy_from_slice(&bytes[64..96]);

        Some(Self {
            magic,
            format_version,
            architecture,
            flags,
            section_table_offset,
            section_count,
            reserved,
            blake3_content_hash,
            pq_signature_placeholder,
        })
    }
}

impl PartialEq for PaxHeader {
    fn eq(&self, other: &Self) -> bool {
        self.magic == other.magic
            && self.format_version == other.format_version
            && self.architecture == other.architecture
            && self.flags == other.flags
            && self.section_table_offset == other.section_table_offset
            && self.section_count == other.section_count
            && self.reserved == other.reserved
            && self.blake3_content_hash == other.blake3_content_hash
            && self.pq_signature_placeholder == other.pq_signature_placeholder
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pax_header_size_is_96_bytes() {
        // Const assertion: this will not compile if PAX_HEADER_SIZE != 96.
        const _: () = {
            const _: [(); 1] = [(); (PAX_HEADER_SIZE == 96) as usize];
        };
        assert_eq!(PAX_HEADER_SIZE, 96);
    }

    #[test]
    fn new_header_has_magic_and_version() {
        let header = PaxHeader::new(Architecture::X86_64);
        assert_eq!(header.magic, *b"PAX\0");
        assert_eq!(header.format_version, PAX_FORMAT_VERSION);
        assert_eq!(header.architecture, Architecture::X86_64);
    }

    #[test]
    fn to_bytes_round_trips_through_from_bytes() {
        let original = PaxHeader::new(Architecture::X86_64);
        let bytes = original.to_bytes();
        let parsed = PaxHeader::from_bytes(&bytes);
        assert_eq!(parsed, Some(original));
    }

    #[test]
    fn from_bytes_rejects_bad_magic() {
        let mut bytes = [0u8; 96];
        bytes[0] = b'B';
        bytes[1] = b'A';
        bytes[2] = b'D';
        bytes[3] = 0;
        let result = PaxHeader::from_bytes(&bytes);
        assert_eq!(result, None);
    }

    #[test]
    fn from_bytes_rejects_short_input() {
        let bytes = [0u8; 50];
        let result = PaxHeader::from_bytes(&bytes);
        assert_eq!(result, None);
    }

    #[test]
    fn flags_compose_via_bitor() {
        let flags = (HeaderFlag::Relocatable as u64) | (HeaderFlag::HasDebugInfo as u64);
        assert_eq!(flags, 0b101);
    }

    #[test]
    fn architecture_x86_64_encoded_as_1() {
        let header = PaxHeader::new(Architecture::X86_64);
        let bytes = header.to_bytes();
        let arch_bytes = &bytes[6..8];
        let arch_u16 = u16::from_le_bytes([arch_bytes[0], arch_bytes[1]]);
        assert_eq!(arch_u16, 1);
    }

    #[test]
    fn snapshot_header_byte_layout() {
        let mut header = PaxHeader::new(Architecture::X86_64);
        header.section_table_offset = 96; // Section table immediately follows header
        header.section_count = 5;

        let bytes = header.to_bytes();

        // Check magic at offset 0
        assert_eq!(&bytes[0..4], b"PAX\0");

        // Check format_version (1) at offset 4 (little-endian)
        assert_eq!(&bytes[4..6], &[1u8, 0u8]);

        // Check architecture (1) at offset 6 (little-endian)
        assert_eq!(&bytes[6..8], &[1u8, 0u8]);

        // Check flags (0) at offset 8
        assert_eq!(&bytes[8..16], &[0u8; 8]);

        // Check section_table_offset (96) at offset 16 (little-endian)
        assert_eq!(&bytes[16..24], &[96u8, 0, 0, 0, 0, 0, 0, 0]);

        // Check section_count (5) at offset 24 (little-endian)
        assert_eq!(&bytes[24..28], &[5u8, 0, 0, 0]);

        // Check reserved (0) at offset 28
        assert_eq!(&bytes[28..32], &[0u8; 4]);

        // Check blake3 hash (all zeros) at offset 32
        assert_eq!(&bytes[32..64], &[0u8; 32]);

        // Check pq signature (all zeros) at offset 64
        assert_eq!(&bytes[64..96], &[0u8; 32]);
    }
}
