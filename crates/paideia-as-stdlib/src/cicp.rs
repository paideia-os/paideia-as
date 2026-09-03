//! CICP-tagged image-encoding helpers — Wave 0 Batch 3, v0.31 M1-003 (#1385).
//!
//! A [`CicpTag`] is the four-byte descriptor from ITU-T H.273 "Coding-
//! independent code points for video signal type identification" that names
//! the colour space of a pixel buffer:
//!
//! ```text
//!   colour_primaries : which chromaticities define R/G/B    (H.273 Table 2)
//!   transfer         : opto-electronic transfer function    (H.273 Table 3)
//!   matrix           : YCbCr <-> RGB matrix coefficients    (H.273 Table 4)
//!   full_range       : false = studio/limited, true = full range
//! ```
//!
//! These four bytes are precisely what PNG's `cICP` chunk, AVIF's `colr nclx`
//! box, and HEIF's colour-information property carry — the same tag flows
//! from the R89 canvas surface (paideia-os `KIND_TUI_CANVAS`), through the
//! compositor, and out to the KMS scanout plane, with no gamut / transfer
//! sniffing anywhere in between.
//!
//! # Named tuples the R89 kernel canvas needs
//!
//! | Constant                    | Space                   | CP | TC | MC | Full range |
//! |-----------------------------|-------------------------|----|----|----|------------|
//! | [`BT709`]                   | BT.709 SDR YCbCr (HD)   |  1 |  1 |  1 | false      |
//! | [`SRGB`]                    | sRGB RGB canvas         |  1 | 13 |  0 | true       |
//! | [`DISPLAY_P3`]              | Display-P3 RGB canvas   | 12 | 13 |  0 | true       |
//! | [`BT2020_PQ`]               | HDR10 RGB canvas        |  9 | 16 |  0 | true       |
//! | [`BT2020_HLG`]              | BT.2100 HLG RGB canvas  |  9 | 18 |  0 | true       |
//!
//! Matrix coefficients `0` (identity) mean "pixels are already RGB — no
//! chroma conversion required"; this is the correct value for every RGB
//! canvas surface the compositor hands to scanout. The `BT709` tuple keeps
//! matrix `1` so it can also tag legacy HD YCbCr video buffers that flow
//! through the same encode helpers.
//!
//! # Companion surface
//!
//! The `.pdx` sibling at `pdx/cicp.pdx` declares the same [`CicpTag`] struct
//! + a `CicpTuples` trait with five accessors, matching the option / result
//! pattern (type + trait surface documented in `.pdx`, executable body
//! carried by the Rust hook until the elaborator can lower struct-by-value
//! returns end-to-end).

/// ITU-T H.273 Coding-Independent Code Points tag.
///
/// Four bytes total: three one-byte code points from H.273 Tables 2–4 plus a
/// full-range flag. Copy semantics — the whole descriptor is 4 bytes, cheap
/// to pass by value everywhere.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct CicpTag {
    /// Colour primaries — H.273 Table 2. Common values:
    /// `1` = BT.709 / sRGB primaries, `9` = BT.2020 / BT.2100,
    /// `12` = SMPTE RP 431-2 (P3 DCI, used industry-wide for Display-P3).
    pub colour_primaries: u8,
    /// Transfer characteristics — H.273 Table 3. Common values:
    /// `1` = BT.709, `13` = IEC 61966-2-1 sRGB piecewise,
    /// `16` = SMPTE ST 2084 PQ, `18` = BT.2100 ARIB STD-B67 HLG.
    pub transfer: u8,
    /// Matrix coefficients — H.273 Table 4. `0` = identity (RGB, no chroma
    /// conversion), `1` = BT.709, `9` = BT.2020 non-constant luminance.
    pub matrix: u8,
    /// `true` = full range \[0, 255\] (8-bit) or equivalent at higher depth;
    /// `false` = studio / limited range (Y' in \[16, 235\], C' in \[16, 240\]).
    pub full_range: bool,
}

impl CicpTag {
    /// Const constructor — usable in `const` contexts so the named-tuple
    /// constants below stay `const`.
    pub const fn new(colour_primaries: u8, transfer: u8, matrix: u8, full_range: bool) -> Self {
        Self { colour_primaries, transfer, matrix, full_range }
    }

    /// Serialise as the four bytes emitted into PNG `cICP` / AVIF `nclx`:
    /// `[colour_primaries, transfer, matrix, full_range as 0|1]`.
    pub const fn to_bytes(self) -> [u8; 4] {
        [
            self.colour_primaries,
            self.transfer,
            self.matrix,
            if self.full_range { 1 } else { 0 },
        ]
    }
}

/// BT.709 SDR YCbCr — HD video / limited-range camera path.
/// CP=1, TC=1, MC=1, full_range=false.
pub const BT709: CicpTag = CicpTag::new(1, 1, 1, false);

/// sRGB RGB canvas — the web / classic-desktop baseline.
/// CP=1 (BT.709 primaries), TC=13 (IEC 61966-2-1 sRGB piecewise), MC=0
/// (identity, already RGB), full_range=true.
pub const SRGB: CicpTag = CicpTag::new(1, 13, 0, true);

/// Display-P3 RGB canvas — Apple's wide-gamut default and the AVIF
/// industry convention for the D65 P3 image path.
/// CP=12 (SMPTE RP 431-2), TC=13 (sRGB piecewise), MC=0, full_range=true.
pub const DISPLAY_P3: CicpTag = CicpTag::new(12, 13, 0, true);

/// HDR10 RGB canvas — BT.2020 primaries with the PQ transfer, full-range
/// RGB scanout.
/// CP=9 (BT.2020), TC=16 (SMPTE ST 2084 PQ), MC=0, full_range=true.
pub const BT2020_PQ: CicpTag = CicpTag::new(9, 16, 0, true);

/// BT.2020 HLG RGB canvas — BT.2100 primaries with the ARIB STD-B67 HLG
/// transfer.
/// CP=9 (BT.2020), TC=18 (BT.2100 HLG), MC=0, full_range=true.
pub const BT2020_HLG: CicpTag = CicpTag::new(9, 18, 0, true);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cicp_bt709_resolves_to_h273_tuple() {
        assert_eq!(BT709.colour_primaries, 1);
        assert_eq!(BT709.transfer, 1);
        assert_eq!(BT709.matrix, 1);
        assert!(!BT709.full_range);
        assert_eq!(BT709.to_bytes(), [1, 1, 1, 0]);
    }

    #[test]
    fn cicp_srgb_resolves_to_h273_tuple() {
        assert_eq!(SRGB.colour_primaries, 1);
        assert_eq!(SRGB.transfer, 13);
        assert_eq!(SRGB.matrix, 0);
        assert!(SRGB.full_range);
        assert_eq!(SRGB.to_bytes(), [1, 13, 0, 1]);
    }

    #[test]
    fn cicp_display_p3_resolves_to_h273_tuple() {
        assert_eq!(DISPLAY_P3.colour_primaries, 12);
        assert_eq!(DISPLAY_P3.transfer, 13);
        assert_eq!(DISPLAY_P3.matrix, 0);
        assert!(DISPLAY_P3.full_range);
        assert_eq!(DISPLAY_P3.to_bytes(), [12, 13, 0, 1]);
    }

    #[test]
    fn cicp_bt2020_pq_resolves_to_h273_tuple() {
        assert_eq!(BT2020_PQ.colour_primaries, 9);
        assert_eq!(BT2020_PQ.transfer, 16);
        assert_eq!(BT2020_PQ.matrix, 0);
        assert!(BT2020_PQ.full_range);
        assert_eq!(BT2020_PQ.to_bytes(), [9, 16, 0, 1]);
    }

    #[test]
    fn cicp_bt2020_hlg_resolves_to_h273_tuple() {
        assert_eq!(BT2020_HLG.colour_primaries, 9);
        assert_eq!(BT2020_HLG.transfer, 18);
        assert_eq!(BT2020_HLG.matrix, 0);
        assert!(BT2020_HLG.full_range);
        assert_eq!(BT2020_HLG.to_bytes(), [9, 18, 0, 1]);
    }

    #[test]
    fn five_named_tuples_are_pairwise_distinct() {
        // Sanity: no accidental duplicate constants — every canvas surface a
        // R89 scanout target might carry has a distinct tag.
        let all = [
            BT709.to_bytes(),
            SRGB.to_bytes(),
            DISPLAY_P3.to_bytes(),
            BT2020_PQ.to_bytes(),
            BT2020_HLG.to_bytes(),
        ];
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(
                    all[i], all[j],
                    "CICP tuples at index {} and {} collide: {:?}",
                    i, j, all[i]
                );
            }
        }
    }

    #[test]
    fn ctor_round_trips_all_fields() {
        // Guard against a future refactor swapping struct-field order under
        // us — the constructor is positional (cp, tc, mc, fr).
        let t = CicpTag::new(9, 16, 0, true);
        assert_eq!(t.colour_primaries, 9);
        assert_eq!(t.transfer, 16);
        assert_eq!(t.matrix, 0);
        assert!(t.full_range);
    }

    #[test]
    fn full_range_flag_maps_to_last_byte() {
        assert_eq!(CicpTag::new(1, 1, 1, false).to_bytes()[3], 0);
        assert_eq!(CicpTag::new(1, 1, 1, true).to_bytes()[3], 1);
    }

    #[test]
    fn pdx_source_file_is_present() {
        // The .pdx surface companion must ship next to this Rust hook so the
        // .pdx parse-cleanliness test (tests/parse_pdx.rs) has a target.
        let pdx = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/cicp.pdx");
        assert!(
            pdx.is_file(),
            "expected pdx/cicp.pdx alongside src/cicp.rs (was: {})",
            pdx.display()
        );
    }
}
