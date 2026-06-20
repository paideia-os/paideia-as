//! Paideia DWARF vendor extensions per design/toolchain/debug-info.md.

#![allow(missing_docs)] // const-only file

// Vendor identifier.
pub const VENDOR_ID: &str = "paideia";

// Vendor version (1.0.0.0).
pub const VENDOR_VERSION_BYTES: [u8; 4] = [1, 0, 0, 0];

// Tags (DW_TAG_paideia_*).
pub const DW_TAG_PAIDEIA_CAPABILITY_BINDING: u16 = 0x4100;
pub const DW_TAG_PAIDEIA_EFFECT_ROW: u16 = 0x4101;
pub const DW_TAG_PAIDEIA_SIGNATURE: u16 = 0x4102;

// Attributes (DW_AT_paideia_*).
pub const DW_AT_PAIDEIA_LIN_CLASS: u16 = 0x2100;
pub const DW_AT_PAIDEIA_CAP_KIND: u16 = 0x2101;
pub const DW_AT_PAIDEIA_EFFECT_ID_LIST: u16 = 0x2102;
pub const DW_AT_PAIDEIA_ROW_VAR_ID: u16 = 0x2103;
pub const DW_AT_PAIDEIA_SIG_BLAKE3: u16 = 0x2104;

// Forms (DW_FORM_paideia_*).
pub const DW_FORM_PAIDEIA_EFFECT_LIST: u16 = 0x1f10;

// Section names.
pub const SECTION_CAPS: &str = ".debug.paideia.caps";
pub const SECTION_EFFECTS: &str = ".debug.paideia.effects";
pub const SECTION_SIG: &str = ".debug.paideia.sig";
pub const SECTION_VERSION: &str = ".debug.paideia.version";

// Legacy names (for backward compatibility with existing code).
/// Names of the empty vendor sections to emit.
pub const VENDOR_SECTIONS: &[&str] = &[".paideia.caps", ".paideia.effects", ".paideia.sig"];

/// Returns a zero-byte payload for each vendor section.
pub fn empty_vendor_payloads() -> Vec<(&'static str, Vec<u8>)> {
    VENDOR_SECTIONS.iter().map(|n| (*n, Vec::new())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_id_is_paideia() {
        assert_eq!(VENDOR_ID, "paideia");
    }

    #[test]
    fn tag_numbers_are_in_paideia_range() {
        for &n in &[
            DW_TAG_PAIDEIA_CAPABILITY_BINDING,
            DW_TAG_PAIDEIA_EFFECT_ROW,
            DW_TAG_PAIDEIA_SIGNATURE,
        ] {
            assert!(
                (0x4100..=0x41ff).contains(&n),
                "tag {n:#x} out of paideia range"
            );
        }
    }

    #[test]
    fn attr_numbers_are_in_paideia_range() {
        for &n in &[
            DW_AT_PAIDEIA_LIN_CLASS,
            DW_AT_PAIDEIA_CAP_KIND,
            DW_AT_PAIDEIA_EFFECT_ID_LIST,
            DW_AT_PAIDEIA_ROW_VAR_ID,
            DW_AT_PAIDEIA_SIG_BLAKE3,
        ] {
            assert!(
                (0x2100..=0x21ff).contains(&n),
                "attr {n:#x} out of paideia range"
            );
        }
    }

    #[test]
    fn vendor_sections_contain_three_paideia_names() {
        assert_eq!(VENDOR_SECTIONS.len(), 3);
    }

    #[test]
    fn vendor_sections_have_expected_names() {
        assert_eq!(VENDOR_SECTIONS[0], ".paideia.caps");
        assert_eq!(VENDOR_SECTIONS[1], ".paideia.effects");
        assert_eq!(VENDOR_SECTIONS[2], ".paideia.sig");
    }

    #[test]
    fn empty_vendor_payloads_are_zero_length() {
        let payloads = empty_vendor_payloads();
        assert_eq!(payloads.len(), 3);
        for (_name, data) in payloads {
            assert_eq!(data.len(), 0);
        }
    }

    #[test]
    fn vendor_section_names_match_emitter_elf() {
        // Expected names based on design/toolchain/debug-info.md
        let expected = [".paideia.caps", ".paideia.effects", ".paideia.sig"];
        for (i, name) in expected.iter().enumerate() {
            assert_eq!(VENDOR_SECTIONS[i], *name);
        }
    }
}
