//! PVH (Paravirtual Hypervisor) ELF note encoding for QEMU `-kernel` acceptance.
//!
//! PA10-001: Emit `.note.Xen` section with SHF_ALLOC flag to make the kernel
//! loadable by QEMU's `-kernel` option, which expects a PVH entry point note.
//! Per ELF specification, PVH notes use the Xen note type with a 24-byte payload.

/// Xen ELF note type for PVH entry point (ELFNOTE_PHYS32_ENTRY).
pub const XEN_ELFNOTE_PHYS32_ENTRY: u32 = 18;

/// Xen note name: exactly 4 bytes including NUL terminator.
pub const PVH_NOTE_NAME: &[u8] = b"Xen\0";

/// Default PVH entry address (kernel load at 1 MB).
pub const PVH_DEFAULT_ENTRY_ADDR: u32 = 0x100000;

/// Encodes a PVH ELF note with the correct 24-byte structure.
///
/// The note follows the ELF specification with:
/// - `n_namesz = 4` (b"Xen\0")
/// - `n_descsz = 8` (ELFCLASS64 entry: 4 bytes value + 4 bytes padding)
/// - `n_type = 18` (XEN_ELFNOTE_PHYS32_ENTRY)
/// - Name: 4 bytes (already 4-aligned)
/// - Desc: 8 bytes (4-byte LE entry_addr + 4 zero bytes for upper 32 bits)
///
/// Total: 12 + 4 + 8 = 24 bytes, 4-aligned.
///
/// # Arguments
///
/// * `entry_addr` - Physical entry point address (typically 0x100000 for 1 MB)
///
/// # Returns
///
/// A 24-byte vector representing the complete PVH note payload.
pub fn encode_pvh_note(entry_addr: u32) -> Vec<u8> {
    let n_namesz = 4u32; // b"Xen\0" is 4 bytes
    let n_descsz = 8u32; // ELFCLASS64: 4 bytes value + 4 bytes upper 32 bits
    let n_type = XEN_ELFNOTE_PHYS32_ENTRY;

    // Capacity: 12 (header) + 4 (name) + 8 (desc) = 24 bytes
    let mut note = Vec::with_capacity(24);

    // Write note header in little-endian (ELF64 default for x86-64).
    note.extend_from_slice(&n_namesz.to_le_bytes());
    note.extend_from_slice(&n_descsz.to_le_bytes());
    note.extend_from_slice(&n_type.to_le_bytes());

    // Write the name: "Xen\0" (already 4-aligned).
    note.extend_from_slice(PVH_NOTE_NAME);

    // Write the descriptor: 4-byte LE entry_addr + 4 zero bytes (upper 32 bits per ELFCLASS64).
    note.extend_from_slice(&entry_addr.to_le_bytes());
    note.extend_from_slice(&[0u8; 4]); // Upper 32 bits = 0 for addresses below 4GB

    note
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pvh_note_constants_are_correct() {
        assert_eq!(XEN_ELFNOTE_PHYS32_ENTRY, 18);
        assert_eq!(PVH_NOTE_NAME, b"Xen\0");
        assert_eq!(PVH_NOTE_NAME.len(), 4);
        assert_eq!(PVH_DEFAULT_ENTRY_ADDR, 0x100000);
    }

    #[test]
    fn encode_pvh_note_has_correct_size() {
        let note = encode_pvh_note(0x100000);
        // 12 (header) + 4 (name) + 8 (descriptor) = 24 bytes
        assert_eq!(note.len(), 24, "PVH note must be exactly 24 bytes");
    }

    #[test]
    fn encode_pvh_note_is_aligned() {
        let note = encode_pvh_note(0x100000);
        assert_eq!(note.len() % 4, 0, "PVH note must be 4-aligned");
    }

    #[test]
    fn encode_pvh_note_header_format() {
        let note = encode_pvh_note(0x100000);

        // Verify n_namesz = 4 (LE at bytes 0-3)
        let n_namesz = u32::from_le_bytes([note[0], note[1], note[2], note[3]]);
        assert_eq!(n_namesz, 4, "n_namesz should be 4 for b\"Xen\\0\"");

        // Verify n_descsz = 8 (LE at bytes 4-7)
        let n_descsz = u32::from_le_bytes([note[4], note[5], note[6], note[7]]);
        assert_eq!(n_descsz, 8, "n_descsz should be 8 (ELFCLASS64)");

        // Verify n_type = 18 (LE at bytes 8-11)
        let n_type = u32::from_le_bytes([note[8], note[9], note[10], note[11]]);
        assert_eq!(n_type, 18, "n_type should be XEN_ELFNOTE_PHYS32_ENTRY (18)");
    }

    #[test]
    fn encode_pvh_note_name_presence() {
        let note = encode_pvh_note(0x100000);
        // Name is at bytes 12-15
        assert_eq!(&note[12..16], b"Xen\0", "name should be \"Xen\\0\"");
    }

    #[test]
    fn encode_pvh_note_descriptor_presence() {
        let note = encode_pvh_note(0x100000);
        // Descriptor is at bytes 16-23
        // First 4 bytes: 0x100000 in LE = 0x00 0x00 0x10 0x00
        let entry_addr = u32::from_le_bytes([note[16], note[17], note[18], note[19]]);
        assert_eq!(entry_addr, 0x100000, "entry address should be 0x100000");

        // Next 4 bytes: 0x00000000 (upper 32 bits)
        let upper = u32::from_le_bytes([note[20], note[21], note[22], note[23]]);
        assert_eq!(upper, 0, "upper 32 bits should be 0");
    }

    #[test]
    fn encode_pvh_note_entry_addr_variations() {
        // Test with different entry addresses
        let note_0x0 = encode_pvh_note(0x0);
        let addr = u32::from_le_bytes([note_0x0[16], note_0x0[17], note_0x0[18], note_0x0[19]]);
        assert_eq!(addr, 0x0);

        let note_0x200000 = encode_pvh_note(0x200000);
        let addr = u32::from_le_bytes([
            note_0x200000[16],
            note_0x200000[17],
            note_0x200000[18],
            note_0x200000[19],
        ]);
        assert_eq!(addr, 0x200000);

        let note_0xffffffff = encode_pvh_note(0xffffffff);
        let addr = u32::from_le_bytes([
            note_0xffffffff[16],
            note_0xffffffff[17],
            note_0xffffffff[18],
            note_0xffffffff[19],
        ]);
        assert_eq!(addr, 0xffffffff);
    }

    #[test]
    fn encode_pvh_note_snapshot_0x100000() {
        // Snapshot test: verify byte-for-byte layout at entry_addr=0x100000
        let note = encode_pvh_note(0x100000);
        let expected = vec![
            // n_namesz = 4 (LE)
            0x04, 0x00, 0x00, 0x00, // n_descsz = 8 (LE)
            0x08, 0x00, 0x00, 0x00, // n_type = 18 (LE)
            0x12, 0x00, 0x00, 0x00, // name: "Xen\0"
            0x58, 0x65, 0x6e, 0x00, // desc: 0x100000 (LE) + 0x00000000
            0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        assert_eq!(note, expected, "PVH note layout mismatch");
    }
}
