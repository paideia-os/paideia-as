//! Empty PaideiaOS vendor sections per debug-info.md §3 and §4.
//!
//! These sections (`.paideia.caps`, `.paideia.effects`, `.paideia.sig`) exist
//! at size 0 so the linker does not reject the object. They are
//! populated by later passes.

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
        let expected = vec![".paideia.caps", ".paideia.effects", ".paideia.sig"];
        for (i, name) in expected.iter().enumerate() {
            assert_eq!(VENDOR_SECTIONS[i], *name);
        }
    }
}
