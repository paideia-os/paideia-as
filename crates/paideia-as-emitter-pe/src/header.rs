//! PE/COFF headers for Microsoft x64 / UEFI binaries.
//!
//! This module defines the binary layout and serialization for DOS, COFF, and
//! Optional (PE32+) headers according to the Microsoft PE/COFF specification.
//! All multi-byte fields are serialized in little-endian byte order.
//!
//! Layout summary:
//! - DOS header (64 bytes): Legacy DOS stub, ends with e_lfanew pointing to PE signature.
//! - NT Signature (4 bytes): `b"PE\0\0"` marker.
//! - COFF File Header (20 bytes): Machine type, section count, timestamps, symbol table info.
//! - Optional Header (240 bytes for PE32+): Entry point, image base, section alignment, data directories.

use static_assertions::const_assert_eq;

// ============================================================================
// Constants
// ============================================================================

/// DOS file magic: `b"MZ"`.
pub const DOS_MAGIC: [u8; 2] = *b"MZ";

/// NT Signature (PE identifier): `b"PE\0\0"`.
pub const NT_SIGNATURE: [u8; 4] = *b"PE\0\0";

/// COFF Machine type for x86-64 (AMD64).
pub const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;

/// COFF Characteristic: Executable image.
pub const IMAGE_FILE_EXECUTABLE_IMAGE: u16 = 0x0002;

/// Optional Header subsystem for UEFI applications.
pub const IMAGE_SUBSYSTEM_EFI_APPLICATION: u16 = 10;

/// Optional Header magic for PE32+ (64-bit).
pub const IMAGE_NT_OPTIONAL_HDR64_MAGIC: u16 = 0x20b;

/// DOS header size in bytes.
pub const DOS_HEADER_SIZE: usize = 64;

/// COFF file header size in bytes.
pub const COFF_FILE_HEADER_SIZE: usize = 20;

/// Optional Header (PE32+) size in bytes.
pub const OPTIONAL_HEADER_PE32PLUS_SIZE: usize = 240;

/// Data Directory entry size in bytes.
pub const DATA_DIRECTORY_SIZE: usize = 8;

/// Number of data directories in an Optional Header.
pub const NUMBER_OF_DATA_DIRECTORIES: usize = 16;

// Verify the Optional Header size at compile time.
// Standard PE32+ layout: 24 (magic + etc.) + 88 (more fields) + 16 * 8 (data dirs) = 240
const_assert_eq!(
    OPTIONAL_HEADER_PE32PLUS_SIZE,
    24 + 88 + DATA_DIRECTORY_SIZE * NUMBER_OF_DATA_DIRECTORIES
);

// ============================================================================
// DosHeader
// ============================================================================

/// DOS header (64 bytes).
///
/// Provides backward compatibility with DOS and marks the location of the
/// NT (PE) header via `e_lfanew`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DosHeader {
    /// Magic number: `b"MZ"`.
    pub e_magic: [u8; 2],
    /// Offset to the NT Signature (PE header location).
    pub e_lfanew: u32,
}

impl DosHeader {
    /// Create a new DOS header with defaults.
    ///
    /// Initializes `e_magic` to `b"MZ"` and `e_lfanew` to 64 (NT signature
    /// sits immediately after the 64-byte DOS stub).
    pub fn new() -> Self {
        Self {
            e_magic: DOS_MAGIC,
            e_lfanew: DOS_HEADER_SIZE as u32,
        }
    }

    /// Serialize to 64-byte little-endian representation.
    ///
    /// Layout:
    /// - Offset 0-1: magic
    /// - Offset 2-59: zeros (reserved)
    /// - Offset 60-63: e_lfanew (little-endian u32)
    pub fn to_bytes(&self) -> [u8; DOS_HEADER_SIZE] {
        let mut bytes = [0u8; DOS_HEADER_SIZE];

        // Offset 0: magic (2 bytes)
        bytes[0..2].copy_from_slice(&self.e_magic);

        // Offset 60: e_lfanew (4 bytes, little-endian)
        bytes[60..64].copy_from_slice(&self.e_lfanew.to_le_bytes());

        bytes
    }

    /// Parse from a byte slice.
    ///
    /// Returns `Some(header)` if input is at least 64 bytes and magic matches `b"MZ"`.
    /// Returns `None` on short input or magic mismatch.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < DOS_HEADER_SIZE {
            return None;
        }

        // Offset 0: magic (2 bytes)
        let e_magic = [bytes[0], bytes[1]];
        if e_magic != DOS_MAGIC {
            return None;
        }

        // Offset 60: e_lfanew (4 bytes, little-endian)
        let e_lfanew = u32::from_le_bytes([bytes[60], bytes[61], bytes[62], bytes[63]]);

        Some(Self { e_magic, e_lfanew })
    }
}

impl Default for DosHeader {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// DataDirectory
// ============================================================================

/// Data Directory entry (8 bytes).
///
/// Refers to a table of a specific type within the PE file (e.g., export table,
/// import table, resources, etc.).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DataDirectory {
    /// Virtual address (RVA) of the table.
    pub virtual_address: u32,
    /// Size of the table in bytes.
    pub size: u32,
}

impl DataDirectory {
    /// Serialize to 8-byte little-endian representation.
    pub fn to_bytes(&self) -> [u8; DATA_DIRECTORY_SIZE] {
        let mut bytes = [0u8; DATA_DIRECTORY_SIZE];

        // Offset 0: virtual_address (4 bytes, little-endian)
        bytes[0..4].copy_from_slice(&self.virtual_address.to_le_bytes());

        // Offset 4: size (4 bytes, little-endian)
        bytes[4..8].copy_from_slice(&self.size.to_le_bytes());

        bytes
    }

    /// Parse from a byte slice.
    ///
    /// Returns `Some(dir)` if input is at least 8 bytes. Returns `None` on short input.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < DATA_DIRECTORY_SIZE {
            return None;
        }

        // Offset 0: virtual_address (4 bytes, little-endian)
        let virtual_address = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);

        // Offset 4: size (4 bytes, little-endian)
        let size = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);

        Some(Self {
            virtual_address,
            size,
        })
    }
}

// ============================================================================
// CoffFileHeader
// ============================================================================

/// COFF File Header (20 bytes).
///
/// Contains machine type, section information, symbol table details, and file
/// characteristics for a PE file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoffFileHeader {
    /// Machine type (e.g., AMD64 = 0x8664).
    pub machine: u16,
    /// Number of sections.
    pub number_of_sections: u16,
    /// Timestamp of file creation (seconds since Unix epoch).
    pub time_date_stamp: u32,
    /// File offset to COFF symbol table (deprecated; usually 0).
    pub pointer_to_symbol_table: u32,
    /// Number of symbols in the symbol table (deprecated; usually 0).
    pub number_of_symbols: u32,
    /// Size of Optional Header that follows.
    pub size_of_optional_header: u16,
    /// Characteristics (e.g., EXECUTABLE_IMAGE).
    pub characteristics: u16,
}

impl CoffFileHeader {
    /// Create a new COFF File Header preset for EFI AMD64.
    ///
    /// Initializes:
    /// - `machine` = AMD64 (0x8664)
    /// - `size_of_optional_header` = 240 (PE32+)
    /// - `characteristics` = EXECUTABLE_IMAGE (0x0002)
    /// - Symbol fields to 0
    pub fn new_efi_amd64() -> Self {
        Self {
            machine: IMAGE_FILE_MACHINE_AMD64,
            number_of_sections: 0,
            time_date_stamp: 0,
            pointer_to_symbol_table: 0,
            number_of_symbols: 0,
            size_of_optional_header: OPTIONAL_HEADER_PE32PLUS_SIZE as u16,
            characteristics: IMAGE_FILE_EXECUTABLE_IMAGE,
        }
    }

    /// Serialize to 20-byte little-endian representation.
    pub fn to_bytes(&self) -> [u8; COFF_FILE_HEADER_SIZE] {
        let mut bytes = [0u8; COFF_FILE_HEADER_SIZE];

        // Offset 0: machine (2 bytes, little-endian)
        bytes[0..2].copy_from_slice(&self.machine.to_le_bytes());

        // Offset 2: number_of_sections (2 bytes, little-endian)
        bytes[2..4].copy_from_slice(&self.number_of_sections.to_le_bytes());

        // Offset 4: time_date_stamp (4 bytes, little-endian)
        bytes[4..8].copy_from_slice(&self.time_date_stamp.to_le_bytes());

        // Offset 8: pointer_to_symbol_table (4 bytes, little-endian)
        bytes[8..12].copy_from_slice(&self.pointer_to_symbol_table.to_le_bytes());

        // Offset 12: number_of_symbols (4 bytes, little-endian)
        bytes[12..16].copy_from_slice(&self.number_of_symbols.to_le_bytes());

        // Offset 16: size_of_optional_header (2 bytes, little-endian)
        bytes[16..18].copy_from_slice(&self.size_of_optional_header.to_le_bytes());

        // Offset 18: characteristics (2 bytes, little-endian)
        bytes[18..20].copy_from_slice(&self.characteristics.to_le_bytes());

        bytes
    }

    /// Parse from a byte slice.
    ///
    /// Returns `Some(header)` if input is at least 20 bytes. Returns `None` on short input.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < COFF_FILE_HEADER_SIZE {
            return None;
        }

        // Offset 0: machine (2 bytes, little-endian)
        let machine = u16::from_le_bytes([bytes[0], bytes[1]]);

        // Offset 2: number_of_sections (2 bytes, little-endian)
        let number_of_sections = u16::from_le_bytes([bytes[2], bytes[3]]);

        // Offset 4: time_date_stamp (4 bytes, little-endian)
        let time_date_stamp = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);

        // Offset 8: pointer_to_symbol_table (4 bytes, little-endian)
        let pointer_to_symbol_table =
            u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);

        // Offset 12: number_of_symbols (4 bytes, little-endian)
        let number_of_symbols = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);

        // Offset 16: size_of_optional_header (2 bytes, little-endian)
        let size_of_optional_header = u16::from_le_bytes([bytes[16], bytes[17]]);

        // Offset 18: characteristics (2 bytes, little-endian)
        let characteristics = u16::from_le_bytes([bytes[18], bytes[19]]);

        Some(Self {
            machine,
            number_of_sections,
            time_date_stamp,
            pointer_to_symbol_table,
            number_of_symbols,
            size_of_optional_header,
            characteristics,
        })
    }
}

// ============================================================================
// OptionalHeaderPe32Plus
// ============================================================================

/// Optional Header for PE32+ (64-bit, 240 bytes).
///
/// Contains the entry point, image base, section alignment, and 16 data directory
/// entries for subsystems, imports, resources, etc.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionalHeaderPe32Plus {
    /// Magic number: 0x020b for PE32+.
    pub magic: u16,
    /// Major version of linker.
    pub major_linker_version: u8,
    /// Minor version of linker.
    pub minor_linker_version: u8,
    /// Size of code sections combined.
    pub size_of_code: u32,
    /// Size of initialized data sections combined.
    pub size_of_initialized_data: u32,
    /// Size of uninitialized data sections combined.
    pub size_of_uninitialized_data: u32,
    /// RVA of entry point (main or DLL entry).
    pub address_of_entry_point: u32,
    /// RVA of first code section.
    pub base_of_code: u32,
    /// Base address where image is loaded.
    pub image_base: u64,
    /// Alignment of sections in memory (minimum 0x1000).
    pub section_alignment: u32,
    /// Alignment of sections in file (usually 0x200).
    pub file_alignment: u32,
    /// Major version of OS required.
    pub major_operating_system_version: u16,
    /// Minor version of OS required.
    pub minor_operating_system_version: u16,
    /// Major version of image (user-defined).
    pub major_image_version: u16,
    /// Minor version of image (user-defined).
    pub minor_image_version: u16,
    /// Major version of subsystem required.
    pub major_subsystem_version: u16,
    /// Minor version of subsystem required.
    pub minor_subsystem_version: u16,
    /// Reserved; must be zero.
    pub win32_version_value: u32,
    /// Size of image in memory (rounded to section_alignment).
    pub size_of_image: u32,
    /// Size of headers (DOS + NT signature + COFF + Optional, rounded to file_alignment).
    pub size_of_headers: u32,
    /// Checksum (optional; often 0).
    pub checksum: u32,
    /// Subsystem (e.g., EFI_APPLICATION = 10).
    pub subsystem: u16,
    /// DLL characteristics (e.g., NO_BIND, NX_COMPAT).
    pub dll_characteristics: u16,
    /// Size of stack to reserve.
    pub size_of_stack_reserve: u64,
    /// Size of stack to commit.
    pub size_of_stack_commit: u64,
    /// Size of heap to reserve.
    pub size_of_heap_reserve: u64,
    /// Size of heap to commit.
    pub size_of_heap_commit: u64,
    /// Loader flags (reserved; usually 0).
    pub loader_flags: u32,
    /// Number of RVA and Size pairs in the data directories.
    pub number_of_rva_and_sizes: u32,
    /// Data directory table (16 entries, 8 bytes each = 128 bytes).
    pub data_directories: [DataDirectory; NUMBER_OF_DATA_DIRECTORIES],
}

impl OptionalHeaderPe32Plus {
    /// Create a new Optional Header preset for EFI AMD64.
    ///
    /// Initializes:
    /// - `magic` = 0x20b (PE32+)
    /// - `subsystem` = EFI_APPLICATION (10)
    /// - `image_base` = 0
    /// - `section_alignment` = 0x1000
    /// - `file_alignment` = 0x200
    /// - `number_of_rva_and_sizes` = 16
    /// - All version fields to 0
    pub fn new_efi_amd64() -> Self {
        Self {
            magic: IMAGE_NT_OPTIONAL_HDR64_MAGIC,
            major_linker_version: 0,
            minor_linker_version: 0,
            size_of_code: 0,
            size_of_initialized_data: 0,
            size_of_uninitialized_data: 0,
            address_of_entry_point: 0,
            base_of_code: 0,
            image_base: 0,
            section_alignment: 0x1000,
            file_alignment: 0x200,
            major_operating_system_version: 0,
            minor_operating_system_version: 0,
            major_image_version: 0,
            minor_image_version: 0,
            major_subsystem_version: 0,
            minor_subsystem_version: 0,
            win32_version_value: 0,
            size_of_image: 0,
            size_of_headers: 0,
            checksum: 0,
            subsystem: IMAGE_SUBSYSTEM_EFI_APPLICATION,
            dll_characteristics: 0,
            size_of_stack_reserve: 0,
            size_of_stack_commit: 0,
            size_of_heap_reserve: 0,
            size_of_heap_commit: 0,
            loader_flags: 0,
            number_of_rva_and_sizes: NUMBER_OF_DATA_DIRECTORIES as u32,
            data_directories: [DataDirectory::default(); NUMBER_OF_DATA_DIRECTORIES],
        }
    }

    /// Serialize to 240-byte little-endian representation.
    pub fn to_bytes(&self) -> [u8; OPTIONAL_HEADER_PE32PLUS_SIZE] {
        let mut bytes = [0u8; OPTIONAL_HEADER_PE32PLUS_SIZE];

        // Offset 0: magic (2 bytes, little-endian)
        bytes[0..2].copy_from_slice(&self.magic.to_le_bytes());

        // Offset 2: major_linker_version (1 byte)
        bytes[2] = self.major_linker_version;

        // Offset 3: minor_linker_version (1 byte)
        bytes[3] = self.minor_linker_version;

        // Offset 4: size_of_code (4 bytes, little-endian)
        bytes[4..8].copy_from_slice(&self.size_of_code.to_le_bytes());

        // Offset 8: size_of_initialized_data (4 bytes, little-endian)
        bytes[8..12].copy_from_slice(&self.size_of_initialized_data.to_le_bytes());

        // Offset 12: size_of_uninitialized_data (4 bytes, little-endian)
        bytes[12..16].copy_from_slice(&self.size_of_uninitialized_data.to_le_bytes());

        // Offset 16: address_of_entry_point (4 bytes, little-endian)
        bytes[16..20].copy_from_slice(&self.address_of_entry_point.to_le_bytes());

        // Offset 20: base_of_code (4 bytes, little-endian)
        bytes[20..24].copy_from_slice(&self.base_of_code.to_le_bytes());

        // Offset 24: image_base (8 bytes, little-endian)
        bytes[24..32].copy_from_slice(&self.image_base.to_le_bytes());

        // Offset 32: section_alignment (4 bytes, little-endian)
        bytes[32..36].copy_from_slice(&self.section_alignment.to_le_bytes());

        // Offset 36: file_alignment (4 bytes, little-endian)
        bytes[36..40].copy_from_slice(&self.file_alignment.to_le_bytes());

        // Offset 40: major_operating_system_version (2 bytes, little-endian)
        bytes[40..42].copy_from_slice(&self.major_operating_system_version.to_le_bytes());

        // Offset 42: minor_operating_system_version (2 bytes, little-endian)
        bytes[42..44].copy_from_slice(&self.minor_operating_system_version.to_le_bytes());

        // Offset 44: major_image_version (2 bytes, little-endian)
        bytes[44..46].copy_from_slice(&self.major_image_version.to_le_bytes());

        // Offset 46: minor_image_version (2 bytes, little-endian)
        bytes[46..48].copy_from_slice(&self.minor_image_version.to_le_bytes());

        // Offset 48: major_subsystem_version (2 bytes, little-endian)
        bytes[48..50].copy_from_slice(&self.major_subsystem_version.to_le_bytes());

        // Offset 50: minor_subsystem_version (2 bytes, little-endian)
        bytes[50..52].copy_from_slice(&self.minor_subsystem_version.to_le_bytes());

        // Offset 52: win32_version_value (4 bytes, little-endian)
        bytes[52..56].copy_from_slice(&self.win32_version_value.to_le_bytes());

        // Offset 56: size_of_image (4 bytes, little-endian)
        bytes[56..60].copy_from_slice(&self.size_of_image.to_le_bytes());

        // Offset 60: size_of_headers (4 bytes, little-endian)
        bytes[60..64].copy_from_slice(&self.size_of_headers.to_le_bytes());

        // Offset 64: checksum (4 bytes, little-endian)
        bytes[64..68].copy_from_slice(&self.checksum.to_le_bytes());

        // Offset 68: subsystem (2 bytes, little-endian)
        bytes[68..70].copy_from_slice(&self.subsystem.to_le_bytes());

        // Offset 70: dll_characteristics (2 bytes, little-endian)
        bytes[70..72].copy_from_slice(&self.dll_characteristics.to_le_bytes());

        // Offset 72: size_of_stack_reserve (8 bytes, little-endian)
        bytes[72..80].copy_from_slice(&self.size_of_stack_reserve.to_le_bytes());

        // Offset 80: size_of_stack_commit (8 bytes, little-endian)
        bytes[80..88].copy_from_slice(&self.size_of_stack_commit.to_le_bytes());

        // Offset 88: size_of_heap_reserve (8 bytes, little-endian)
        bytes[88..96].copy_from_slice(&self.size_of_heap_reserve.to_le_bytes());

        // Offset 96: size_of_heap_commit (8 bytes, little-endian)
        bytes[96..104].copy_from_slice(&self.size_of_heap_commit.to_le_bytes());

        // Offset 104: loader_flags (4 bytes, little-endian)
        bytes[104..108].copy_from_slice(&self.loader_flags.to_le_bytes());

        // Offset 108: number_of_rva_and_sizes (4 bytes, little-endian)
        bytes[108..112].copy_from_slice(&self.number_of_rva_and_sizes.to_le_bytes());

        // Offset 112: data_directories (16 * 8 = 128 bytes)
        for (i, dir) in self.data_directories.iter().enumerate() {
            let offset = 112 + i * DATA_DIRECTORY_SIZE;
            bytes[offset..offset + DATA_DIRECTORY_SIZE].copy_from_slice(&dir.to_bytes());
        }

        bytes
    }

    /// Parse from a byte slice.
    ///
    /// Returns `Some(header)` if input is at least 240 bytes and magic matches 0x20b.
    /// Returns `None` on short input or magic mismatch.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < OPTIONAL_HEADER_PE32PLUS_SIZE {
            return None;
        }

        // Offset 0: magic (2 bytes, little-endian)
        let magic = u16::from_le_bytes([bytes[0], bytes[1]]);
        if magic != IMAGE_NT_OPTIONAL_HDR64_MAGIC {
            return None;
        }

        // Offset 2: major_linker_version (1 byte)
        let major_linker_version = bytes[2];

        // Offset 3: minor_linker_version (1 byte)
        let minor_linker_version = bytes[3];

        // Offset 4: size_of_code (4 bytes, little-endian)
        let size_of_code = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);

        // Offset 8: size_of_initialized_data (4 bytes, little-endian)
        let size_of_initialized_data =
            u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);

        // Offset 12: size_of_uninitialized_data (4 bytes, little-endian)
        let size_of_uninitialized_data =
            u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);

        // Offset 16: address_of_entry_point (4 bytes, little-endian)
        let address_of_entry_point =
            u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);

        // Offset 20: base_of_code (4 bytes, little-endian)
        let base_of_code = u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);

        // Offset 24: image_base (8 bytes, little-endian)
        let image_base = u64::from_le_bytes([
            bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31],
        ]);

        // Offset 32: section_alignment (4 bytes, little-endian)
        let section_alignment = u32::from_le_bytes([bytes[32], bytes[33], bytes[34], bytes[35]]);

        // Offset 36: file_alignment (4 bytes, little-endian)
        let file_alignment = u32::from_le_bytes([bytes[36], bytes[37], bytes[38], bytes[39]]);

        // Offset 40: major_operating_system_version (2 bytes, little-endian)
        let major_operating_system_version = u16::from_le_bytes([bytes[40], bytes[41]]);

        // Offset 42: minor_operating_system_version (2 bytes, little-endian)
        let minor_operating_system_version = u16::from_le_bytes([bytes[42], bytes[43]]);

        // Offset 44: major_image_version (2 bytes, little-endian)
        let major_image_version = u16::from_le_bytes([bytes[44], bytes[45]]);

        // Offset 46: minor_image_version (2 bytes, little-endian)
        let minor_image_version = u16::from_le_bytes([bytes[46], bytes[47]]);

        // Offset 48: major_subsystem_version (2 bytes, little-endian)
        let major_subsystem_version = u16::from_le_bytes([bytes[48], bytes[49]]);

        // Offset 50: minor_subsystem_version (2 bytes, little-endian)
        let minor_subsystem_version = u16::from_le_bytes([bytes[50], bytes[51]]);

        // Offset 52: win32_version_value (4 bytes, little-endian)
        let win32_version_value = u32::from_le_bytes([bytes[52], bytes[53], bytes[54], bytes[55]]);

        // Offset 56: size_of_image (4 bytes, little-endian)
        let size_of_image = u32::from_le_bytes([bytes[56], bytes[57], bytes[58], bytes[59]]);

        // Offset 60: size_of_headers (4 bytes, little-endian)
        let size_of_headers = u32::from_le_bytes([bytes[60], bytes[61], bytes[62], bytes[63]]);

        // Offset 64: checksum (4 bytes, little-endian)
        let checksum = u32::from_le_bytes([bytes[64], bytes[65], bytes[66], bytes[67]]);

        // Offset 68: subsystem (2 bytes, little-endian)
        let subsystem = u16::from_le_bytes([bytes[68], bytes[69]]);

        // Offset 70: dll_characteristics (2 bytes, little-endian)
        let dll_characteristics = u16::from_le_bytes([bytes[70], bytes[71]]);

        // Offset 72: size_of_stack_reserve (8 bytes, little-endian)
        let size_of_stack_reserve = u64::from_le_bytes([
            bytes[72], bytes[73], bytes[74], bytes[75], bytes[76], bytes[77], bytes[78], bytes[79],
        ]);

        // Offset 80: size_of_stack_commit (8 bytes, little-endian)
        let size_of_stack_commit = u64::from_le_bytes([
            bytes[80], bytes[81], bytes[82], bytes[83], bytes[84], bytes[85], bytes[86], bytes[87],
        ]);

        // Offset 88: size_of_heap_reserve (8 bytes, little-endian)
        let size_of_heap_reserve = u64::from_le_bytes([
            bytes[88], bytes[89], bytes[90], bytes[91], bytes[92], bytes[93], bytes[94], bytes[95],
        ]);

        // Offset 96: size_of_heap_commit (8 bytes, little-endian)
        let size_of_heap_commit = u64::from_le_bytes([
            bytes[96], bytes[97], bytes[98], bytes[99], bytes[100], bytes[101], bytes[102],
            bytes[103],
        ]);

        // Offset 104: loader_flags (4 bytes, little-endian)
        let loader_flags = u32::from_le_bytes([bytes[104], bytes[105], bytes[106], bytes[107]]);

        // Offset 108: number_of_rva_and_sizes (4 bytes, little-endian)
        let number_of_rva_and_sizes =
            u32::from_le_bytes([bytes[108], bytes[109], bytes[110], bytes[111]]);

        // Offset 112: data_directories (16 * 8 = 128 bytes)
        let mut data_directories = [DataDirectory::default(); NUMBER_OF_DATA_DIRECTORIES];

        for (i, dir) in data_directories.iter_mut().enumerate() {
            let offset = 112 + i * DATA_DIRECTORY_SIZE;
            if let Some(parsed_dir) = DataDirectory::from_bytes(&bytes[offset..]) {
                *dir = parsed_dir;
            } else {
                return None;
            }
        }

        Some(Self {
            magic,
            major_linker_version,
            minor_linker_version,
            size_of_code,
            size_of_initialized_data,
            size_of_uninitialized_data,
            address_of_entry_point,
            base_of_code,
            image_base,
            section_alignment,
            file_alignment,
            major_operating_system_version,
            minor_operating_system_version,
            major_image_version,
            minor_image_version,
            major_subsystem_version,
            minor_subsystem_version,
            win32_version_value,
            size_of_image,
            size_of_headers,
            checksum,
            subsystem,
            dll_characteristics,
            size_of_stack_reserve,
            size_of_stack_commit,
            size_of_heap_reserve,
            size_of_heap_commit,
            loader_flags,
            number_of_rva_and_sizes,
            data_directories,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn const_asserts_at_runtime() {
        assert_eq!(DOS_HEADER_SIZE, 64);
        assert_eq!(COFF_FILE_HEADER_SIZE, 20);
        assert_eq!(OPTIONAL_HEADER_PE32PLUS_SIZE, 240);
        assert_eq!(DATA_DIRECTORY_SIZE, 8);
    }

    #[test]
    fn dos_header_round_trips() {
        let original = DosHeader::new();
        let bytes = original.to_bytes();
        let parsed = DosHeader::from_bytes(&bytes);
        assert_eq!(parsed, Some(original));
    }

    #[test]
    fn coff_file_header_round_trips() {
        let original = CoffFileHeader::new_efi_amd64();
        let bytes = original.to_bytes();
        let parsed = CoffFileHeader::from_bytes(&bytes);
        assert_eq!(parsed, Some(original));
    }

    #[test]
    fn optional_header_pe32_plus_round_trips() {
        let original = OptionalHeaderPe32Plus::new_efi_amd64();
        let bytes = original.to_bytes();
        let parsed = OptionalHeaderPe32Plus::from_bytes(&bytes);
        assert_eq!(parsed, Some(original));
    }

    #[test]
    fn minimal_valid_pe_concat_lengths() {
        let dos = DosHeader::new();
        let coff = CoffFileHeader::new_efi_amd64();
        let opt = OptionalHeaderPe32Plus::new_efi_amd64();

        let dos_bytes = dos.to_bytes();
        let nt_sig = NT_SIGNATURE;
        let coff_bytes = coff.to_bytes();
        let opt_bytes = opt.to_bytes();

        let total_len = dos_bytes.len() + nt_sig.len() + coff_bytes.len() + opt_bytes.len();
        assert_eq!(total_len, 328);

        // e_lfanew should point to the NT signature at offset 64
        assert_eq!(dos.e_lfanew as usize, DOS_HEADER_SIZE);
        assert_eq!(&nt_sig, b"PE\0\0");
    }

    #[test]
    fn snapshot_optional_header_byte_layout() {
        let opt = OptionalHeaderPe32Plus::new_efi_amd64();
        let bytes = opt.to_bytes();

        // Check magic at offset 0-1 (0x20b in little-endian = [0x0b, 0x02])
        assert_eq!(&bytes[0..2], &[0x0b, 0x02]);

        // Check subsystem at offset 68-69 (EFI_APPLICATION = 10)
        let subsystem = u16::from_le_bytes([bytes[68], bytes[69]]);
        assert_eq!(subsystem, IMAGE_SUBSYSTEM_EFI_APPLICATION);

        // Check number_of_rva_and_sizes at offset 108-111 (should be 16)
        let num_dirs = u32::from_le_bytes([bytes[108], bytes[109], bytes[110], bytes[111]]);
        assert_eq!(num_dirs, 16);
    }

    #[test]
    fn magic_mismatch_rejects_on_parse() {
        let mut bytes = [0u8; OPTIONAL_HEADER_PE32PLUS_SIZE];
        // Set wrong magic
        bytes[0] = 0xFF;
        bytes[1] = 0xFF;
        // Set subsystem to EFI_APPLICATION
        bytes[68] = 10;
        bytes[69] = 0;

        let result = OptionalHeaderPe32Plus::from_bytes(&bytes);
        assert_eq!(result, None);
    }

    #[test]
    fn e_lfanew_field_at_dos_offset_60() {
        let dos = DosHeader::new();
        let bytes = dos.to_bytes();

        // e_lfanew should be at offset 60-63, value should be 64
        let e_lfanew = u32::from_le_bytes([bytes[60], bytes[61], bytes[62], bytes[63]]);
        assert_eq!(e_lfanew, 64u32);
    }
}
