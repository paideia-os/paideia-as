//! Section content builders for the paideia vendor extensions.

use crate::vendor;

/// Build the .debug.paideia.caps section content.
///
/// Per-DIE: 4-byte (DW_TAG ID + 1-byte lin_class + 1-byte cap_kind +
/// 8-byte name_hash).
pub fn build_caps_section(
    entries: &[(
        u8,  /*lin_class*/
        u8,  /*cap_kind*/
        u64, /*name_hash*/
    )],
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(entries.len() * 16);
    for &(lin_class, cap_kind, name_hash) in entries {
        bytes.extend_from_slice(&(vendor::DW_TAG_PAIDEIA_CAPABILITY_BINDING as u32).to_le_bytes());
        bytes.push(lin_class);
        bytes.push(cap_kind);
        bytes.extend_from_slice(&[0u8; 2]); // padding to 8-byte align
        bytes.extend_from_slice(&name_hash.to_le_bytes());
    }
    bytes
}

/// Build the .debug.paideia.effects section content.
pub fn build_effects_section(
    entries: &[(
        u64,         /*function_sym*/
        Vec<u32>,    /*fixed_effects*/
        Option<u32>, /*row_var_id*/
    )],
) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (function_sym, fixed_effects, row_var_id) in entries {
        bytes.extend_from_slice(&function_sym.to_le_bytes());
        bytes.extend_from_slice(&(fixed_effects.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&row_var_id.unwrap_or(0).to_le_bytes());
        for &eid in fixed_effects {
            bytes.extend_from_slice(&eid.to_le_bytes());
        }
    }
    bytes
}

/// Build the .debug.paideia.sig section content.
pub fn build_sig_section(
    entries: &[(u64 /*function_sym*/, u64 /*blake3 first 8 bytes*/)],
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(entries.len() * 16);
    for &(function_sym, sig_hash) in entries {
        bytes.extend_from_slice(&function_sym.to_le_bytes());
        bytes.extend_from_slice(&sig_hash.to_le_bytes());
    }
    bytes
}

/// Build the .debug.paideia.version section (always 4 bytes).
pub fn build_version_section() -> [u8; 4] {
    vendor::VENDOR_VERSION_BYTES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_caps_section_one_entry_produces_16_bytes() {
        let entries = [(1u8, 2u8, 0x0123456789abcdefu64)];
        let bytes = build_caps_section(&entries);
        assert_eq!(bytes.len(), 16);
        // First 4 bytes: DW_TAG_PAIDEIA_CAPABILITY_BINDING as LE u32
        assert_eq!(
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            vendor::DW_TAG_PAIDEIA_CAPABILITY_BINDING as u32
        );
        // Bytes 4-5: lin_class and cap_kind
        assert_eq!(bytes[4], 1);
        assert_eq!(bytes[5], 2);
        // Bytes 6-7: padding
        assert_eq!(bytes[6], 0);
        assert_eq!(bytes[7], 0);
        // Bytes 8-15: name_hash as LE u64
        assert_eq!(
            u64::from_le_bytes([
                bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14],
                bytes[15]
            ]),
            0x0123456789abcdefu64
        );
    }

    #[test]
    fn build_caps_section_multi_entry_concatenates() {
        let entries = [
            (1u8, 2u8, 0x111111111111111fu64),
            (3u8, 4u8, 0x222222222222222fu64),
        ];
        let bytes = build_caps_section(&entries);
        assert_eq!(bytes.len(), 32);
    }

    #[test]
    fn build_effects_section_includes_function_sym_and_row_var() {
        let entries = [(0x1234567890abcdefu64, vec![1u32, 2u32, 3u32], Some(42u32))];
        let bytes = build_effects_section(&entries);
        // 8 bytes function_sym + 4 bytes count + 4 bytes row_var_id + 3*4 bytes effects
        assert_eq!(bytes.len(), 8 + 4 + 4 + 12);
        // Verify function_sym
        assert_eq!(
            u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]
            ]),
            0x1234567890abcdefu64
        );
        // Verify count
        assert_eq!(
            u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            3u32
        );
        // Verify row_var_id
        assert_eq!(
            u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
            42u32
        );
    }

    #[test]
    fn build_sig_section_produces_16_bytes_per_entry() {
        let entries = [
            (0xaabbccddee112233u64, 0x0123456789abcdefu64),
            (0x1122334455667788u64, 0xfedcba9876543210u64),
        ];
        let bytes = build_sig_section(&entries);
        assert_eq!(bytes.len(), 32);
        // First entry: first 8 bytes are function_sym, next 8 are sig_hash
        assert_eq!(
            u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]
            ]),
            0xaabbccddee112233u64
        );
        assert_eq!(
            u64::from_le_bytes([
                bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14],
                bytes[15]
            ]),
            0x0123456789abcdefu64
        );
    }

    #[test]
    fn build_version_section_returns_1_0_0_0() {
        let version = build_version_section();
        assert_eq!(version, [1, 0, 0, 0]);
    }
}
