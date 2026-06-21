//! ELF note encoding helpers for paideia-as.
//!
//! Encodes ELF notes in the format expected by `readelf -n` and the `object` crate.
//! Phase 6 m3-006: Provides support for emitting `.note.paideia` sections containing
//! JSON-serialised record layout metadata.

/// ELF note type constant for PaideiaOS record layouts.
///
/// Used in `.note.paideia` sections to identify the note type.
/// Defined as 0x50441600 (PDX_LAYOUTS in big-endian integer literal form).
pub const PDX_LAYOUTS: u32 = 0x50441600;

/// ELF note name constant for PaideiaOS.
///
/// The note name must be exactly 8 bytes with a null terminator: b"paideia\0".
/// When encoded in the ELF note, n_namesz = 8 (including the null terminator).
pub const NOTE_NAME: &[u8] = b"paideia\0";

/// Encodes an ELF note containing JSON-serialised record layouts.
///
/// Per ELF specification (gABI), a note has the structure:
/// ```text
/// struct {
///   Elf64_Word n_namesz;      // Size of name + null terminator (8 bytes for "paideia\0")
///   Elf64_Word n_descsz;      // Size of descriptor data
///   Elf64_Word n_type;        // Note type (PDX_LAYOUTS = 0x50441600)
///   char       n_name[...];   // Name string (padded to 4-byte alignment)
///   char       n_desc[...];   // Descriptor data (padded to 4-byte alignment)
/// }
/// ```
///
/// # Arguments
///
/// * `record_layouts_json` - JSON bytes from `serde_json::to_vec(&record_layouts)`
///
/// # Returns
///
/// A byte vector suitable for writing to a `.note.paideia` section.
///
/// # Example
///
/// ```ignore
/// use paideia_as_emitter_elf::notes::{encode_paideia_note, PDX_LAYOUTS};
/// use paideia_as_ir::record_layout::{FinalisedLayoutTable, RecordTypeId, RecordLayout, FieldLayout};
/// use serde_json;
///
/// let mut table = FinalisedLayoutTable::new();
/// let layout = RecordLayout::new(32, 8, vec![
///     FieldLayout { offset: 0, size: 8 },
///     FieldLayout { offset: 8, size: 8 },
/// ]);
/// table.insert(RecordTypeId(1), layout);
///
/// let json = serde_json::to_vec(&table).unwrap();
/// let note_bytes = encode_paideia_note(&json);
///
/// // note_bytes can now be written to a .note.paideia section in an ELF object.
/// ```
pub fn encode_paideia_note(record_layouts_json: &[u8]) -> Vec<u8> {
    let n_namesz = NOTE_NAME.len() as u32;
    let n_descsz = record_layouts_json.len() as u32;
    let n_type = PDX_LAYOUTS;

    // Start with the fixed header (3 x 4 bytes).
    let mut note = Vec::with_capacity(12 + NOTE_NAME.len() + record_layouts_json.len() + 16);

    // Write the note header in little-endian (ELF64 default for x86-64).
    note.extend_from_slice(&n_namesz.to_le_bytes());
    note.extend_from_slice(&n_descsz.to_le_bytes());
    note.extend_from_slice(&n_type.to_le_bytes());

    // Write the name (already null-terminated, and 8 bytes exactly).
    note.extend_from_slice(NOTE_NAME);

    // Write the descriptor data.
    note.extend_from_slice(record_layouts_json);

    // Pad the entire note to 4-byte alignment as required by ELF.
    // The offset at which padding is needed is: 12 (header) + namesz + descsz.
    let unaligned_size = 12 + NOTE_NAME.len() + record_layouts_json.len();
    let padding_needed = (4 - (unaligned_size % 4)) % 4;
    note.extend_from_slice(&vec![0u8; padding_needed]);

    note
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_paideia_note_header_format() {
        let descriptor = vec![0x7B, 0x7D]; // "{}"
        let note_bytes = encode_paideia_note(&descriptor);

        // Verify the header structure:
        // bytes 0-3: n_namesz = 8 (little-endian)
        // bytes 4-7: n_descsz = 2 (little-endian)
        // bytes 8-11: n_type = 0x50441600 (little-endian)
        assert_eq!(
            u32::from_le_bytes([note_bytes[0], note_bytes[1], note_bytes[2], note_bytes[3]]),
            8
        );
        assert_eq!(
            u32::from_le_bytes([note_bytes[4], note_bytes[5], note_bytes[6], note_bytes[7]]),
            2
        );
        assert_eq!(
            u32::from_le_bytes([note_bytes[8], note_bytes[9], note_bytes[10], note_bytes[11]]),
            0x50441600
        );
    }

    #[test]
    fn encode_paideia_note_name_presence() {
        let descriptor = vec![0x7B, 0x7D];
        let note_bytes = encode_paideia_note(&descriptor);

        // After header (12 bytes), the name should be present.
        assert_eq!(&note_bytes[12..20], b"paideia\0");
    }

    #[test]
    fn encode_paideia_note_descriptor_presence() {
        let descriptor = vec![0x7B, 0x7D];
        let note_bytes = encode_paideia_note(&descriptor);

        // After header (12 bytes) + name (8 bytes), the descriptor should be present.
        assert_eq!(&note_bytes[20..22], &descriptor[..]);
    }

    #[test]
    fn encode_paideia_note_alignment() {
        let descriptor = vec![0x7B, 0x7D]; // 2 bytes
        let note_bytes = encode_paideia_note(&descriptor);

        // Total unaligned size: 12 (header) + 8 (name) + 2 (descriptor) = 22 bytes.
        // Padding needed: (4 - (22 % 4)) % 4 = (4 - 2) % 4 = 2 bytes.
        // Final size should be 24 bytes (divisible by 4).
        assert_eq!(note_bytes.len() % 4, 0);
        assert_eq!(note_bytes.len(), 24);
    }

    #[test]
    fn encode_paideia_note_empty_descriptor() {
        let descriptor = vec![];
        let note_bytes = encode_paideia_note(&descriptor);

        // Header (12) + name (8) + descriptor (0) = 20 bytes.
        // Padding needed: (4 - (20 % 4)) % 4 = 0 bytes (already aligned).
        assert_eq!(note_bytes.len() % 4, 0);
        assert_eq!(note_bytes.len(), 20);
    }

    #[test]
    fn encode_paideia_note_large_descriptor() {
        let descriptor = vec![0u8; 100];
        let note_bytes = encode_paideia_note(&descriptor);

        // Header (12) + name (8) + descriptor (100) = 120 bytes.
        // Already aligned (120 % 4 = 0), no padding needed.
        assert_eq!(note_bytes.len() % 4, 0);
        assert_eq!(note_bytes.len(), 120);
    }

    #[test]
    fn encode_paideia_note_type_constant() {
        // Verify the type constant is correctly defined.
        assert_eq!(PDX_LAYOUTS, 0x50441600);
    }

    #[test]
    fn encode_paideia_note_name_constant() {
        assert_eq!(NOTE_NAME, b"paideia\0");
        assert_eq!(NOTE_NAME.len(), 8);
    }
}
