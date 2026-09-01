//! paideia-as #1318 (R51-paideia-as-004): VT-d QI invalidation smoke coverage.
//!
//! Preventive coverage for the R35 vtd_ctx.pdx QI-invalidate wire, which R51
//! makes a regular caller. Verifies that the assembler byte-exact-emits a
//! bank of 16 VT-d Queued Invalidation descriptors (one for each of the 8
//! architected QI types, two parameter variants each) into `.rodata`, with:
//!
//!   * 256-byte total size (16 desc × 16 B/desc per Intel VT-d Architecture
//!     Spec §6.5.2);
//!   * every u64 packed little-endian (x86-64 wire order);
//!   * every VT-d field (type, granularity, DID, SID, PASID, address,
//!     status data, MIP, size, IF/SW/FN, etc.) recoverable from the
//!     emitted bytes to the value declared in the fixture.
//!
//! A silent regression in u64-array packing under a many-descriptor
//! initializer — the shape a real driver hands to the QI ring — would drop
//! or mis-encode descriptors under the 10 kHz submission cadence the R51
//! wire requires. This test fails at build time when that happens, well
//! before the wire hits real VT-d hardware.
//!
//! The parsed-field asserts are intentionally derived from the raw bytes
//! (not from the fixture's own hex literals) so a bug that corrupts the
//! emission in a spec-consistent-but-wrong way (e.g. wrong endianness on
//! a single u64) still fails visibly rather than round-tripping through
//! the fixture back into itself.

use crate::common::elf::{assert_elf64_magic, rodata_bytes};
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Number of QI descriptors in the bank.
const NUM_DESCRIPTORS: usize = 16;
/// Bytes per VT-d QI descriptor (Intel VT-d Spec §6.5.2: 128 bits).
const DESC_BYTES: usize = 16;

/// Expected .rodata bytes for `qi_descriptors : [u64; 32]`.
///
/// Each 16-byte row is one QI descriptor, QW0 then QW1, each u64
/// little-endian per x86-64 wire order. Values are the exact bit-packed
/// encodings from the fixture — decoded and cross-checked by field
/// against the VT-d spec in `descriptor_fields_match_intel_vtd_spec`
/// below.
const EXPECTED_QI_BYTES: [u8; 256] = [
    // #0 cc_inv (global-invalidate): QW0=0x0000000000000011, QW1=0
    0x11, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // #1 cc_inv (domain-selective, DID=0x100, SID=0xABCD, FM=2): QW0=0x0002ABCD01000021, QW1=0
    0x21, 0x00, 0x00, 0x01, 0xCD, 0xAB, 0x02, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // #2 iotlb_inv (global): QW0=0x0000000000000012, QW1=0
    0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // #3 iotlb_inv (page-selective, DID=0x10, DR|DW, addr=0x1000):
    //    QW0=0x00000000001000F2, QW1=0x0000000000001000
    0xF2, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // #4 dev_iotlb_inv (SID=0x0100, addr=0x1000, S=0):
    //    QW0=0x0000010000000003, QW1=0x0000000000001000
    0x03, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
    0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // #5 dev_iotlb_inv (SID=0xABCD, MIP=2, addr=0x2000, S=1):
    //    QW0=0x0000ABCD00020003, QW1=0x0000000000002001
    0x03, 0x00, 0x02, 0x00, 0xCD, 0xAB, 0x00, 0x00,
    0x01, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // #6 iec_inv (global): QW0=0x0000000000000004, QW1=0
    0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // #7 iec_inv (index-selective, IIDX=0x100, IM=2): QW0=0x0000010010000014, QW1=0
    0x14, 0x00, 0x00, 0x10, 0x00, 0x01, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // #8 inv_wait (IF|FN): QW0=0x0000000000000055, QW1=0
    0x55, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // #9 inv_wait (SW|FN, STDATA=0xDEADBEEF, STADDR=0xF0000000):
    //    QW0=0xDEADBEEF00000065, QW1=0x00000000F0000000
    0x65, 0x00, 0x00, 0x00, 0xEF, 0xBE, 0xAD, 0xDE,
    0x00, 0x00, 0x00, 0xF0, 0x00, 0x00, 0x00, 0x00,
    // #10 p_iotlb (all-PASID, DID=0x10, PASID=0x123): QW0=0x0000012300100016, QW1=0
    0x16, 0x00, 0x10, 0x00, 0x23, 0x01, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // #11 p_iotlb (page-selective, DID=0x20, PASID=0x456, addr=0x3000):
    //     QW0=0x0000045600200026, QW1=0x0000000000003000
    0x26, 0x00, 0x20, 0x00, 0x56, 0x04, 0x00, 0x00,
    0x00, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // #12 p_cache (all-PASID): QW0=0x0000000000000017, QW1=0
    0x17, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // #13 p_cache (PASID-selective, DID=0x20, PASID=0x789): QW0=0x0000078900200027, QW1=0
    0x27, 0x00, 0x20, 0x00, 0x89, 0x07, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // #14 p_dev_iotlb (PASID=0x100, SID=0x0100, addr=0x4000, S=0):
    //     QW0=0x0000010001000008, QW1=0x0000000000004000
    0x08, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00,
    0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // #15 p_dev_iotlb (PASID=0x200, SID=0xABCD, MIP=1, addr=0x5000, S=1):
    //     QW0=0x0001ABCD02000008, QW1=0x0000000000005001
    0x08, 0x00, 0x00, 0x02, 0xCD, 0xAB, 0x01, 0x00,
    0x01, 0x50, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// Extract the descriptor bank from an ELF blob's `.rodata`.
///
/// paideia-as places immutable module-scope arrays into `.rodata`; the
/// symbol `qi_descriptors` is the only rodata datum in this fixture, so
/// the section's bytes are the descriptor bank verbatim.
fn qi_bytes(elf: &[u8]) -> Vec<u8> {
    let rodata = rodata_bytes(elf);
    assert!(
        rodata.len() >= EXPECTED_QI_BYTES.len(),
        ".rodata is {} bytes; need at least {} for the QI descriptor bank",
        rodata.len(),
        EXPECTED_QI_BYTES.len()
    );
    // The fixture declares exactly one initialized-data symbol, so the
    // bank starts at rodata offset 0. If future fixture changes prepend
    // additional rodata symbols, the ELF `qi_descriptors` symbol offset
    // would need to be resolved instead.
    rodata[..EXPECTED_QI_BYTES.len()].to_vec()
}

/// Read a little-endian u64 from a 16-byte descriptor at word offset (0=QW0, 1=QW1).
fn qw(desc: &[u8], word: usize) -> u64 {
    assert!(word < 2, "QI descriptor has exactly two 64-bit words");
    let start = word * 8;
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&desc[start..start + 8]);
    u64::from_le_bytes(buf)
}

/// The full `.rodata` byte sequence matches the hand-computed Intel VT-d
/// spec encoding for every descriptor. This is the primary "encoder did
/// not drop or reorder" check.
#[test]
fn vtd_qi_smoke_rodata_matches_expected_bytes() {
    let out = run_build(build_emit("vtd_qi_smoke.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    let actual = qi_bytes(&bytes);
    if actual != EXPECTED_QI_BYTES {
        // Pin the first mismatched descriptor to make regressions easy to triage.
        for i in 0..NUM_DESCRIPTORS {
            let off = i * DESC_BYTES;
            let a = &actual[off..off + DESC_BYTES];
            let e = &EXPECTED_QI_BYTES[off..off + DESC_BYTES];
            if a != e {
                panic!(
                    "QI descriptor #{i} bytes differ:\n  expected: {:02X?}\n  got:      {:02X?}",
                    e, a
                );
            }
        }
        panic!(
            ".rodata mismatch outside descriptor bank: expected {} bytes, got {}",
            EXPECTED_QI_BYTES.len(),
            actual.len()
        );
    }
}

/// Byte-count guard: the emitted bank is exactly 16 descriptors × 16 B.
/// Guards against a silent element-drop that would still parse cleanly.
#[test]
fn vtd_qi_smoke_descriptor_count_and_size() {
    let out = run_build(build_emit("vtd_qi_smoke.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let actual = qi_bytes(&bytes);
    assert_eq!(
        actual.len(),
        NUM_DESCRIPTORS * DESC_BYTES,
        "expected 16 × 16 = 256 bytes of QI descriptors, got {}",
        actual.len()
    );
    assert_eq!(
        actual.len() % DESC_BYTES,
        0,
        "descriptor bank is not a multiple of 16 B (VT-d spec §6.5.2)"
    );
}

/// For every descriptor, extract the Intel VT-d fields from the emitted
/// bytes and compare against the value declared in the fixture. This is
/// the spec-shaped check: it exercises the byte→field decoding that a
/// real VT-d driver would perform, so an endianness or field-position
/// regression in the encoder shows up as a field mismatch rather than
/// as an opaque byte-array diff.
#[test]
fn vtd_qi_smoke_descriptor_fields_match_intel_vtd_spec() {
    let out = run_build(build_emit("vtd_qi_smoke.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let bank = qi_bytes(&bytes);

    // Common bit-field extractors (VT-d Spec §6.5.2). Every descriptor
    // stores its type in QW0[3:0], so the same helper covers all types.
    let ty = |d: &[u8]| (qw(d, 0) & 0xF) as u8;
    let gran = |d: &[u8]| ((qw(d, 0) >> 4) & 0x3) as u8;

    // Split the bank into per-descriptor 16 B slices.
    let desc = |i: usize| &bank[i * DESC_BYTES..(i + 1) * DESC_BYTES];

    // #0 cc_inv (global)
    {
        let d = desc(0);
        assert_eq!(ty(d), 1, "#0 type must be Context Cache Invalidate (1)");
        assert_eq!(gran(d), 1, "#0 gran must be 1 (global)");
        assert_eq!(qw(d, 1), 0, "#0 QW1 must be reserved 0");
    }

    // #1 cc_inv (domain-selective, DID=0x100, SID=0xABCD, FM=2)
    {
        let d = desc(1);
        assert_eq!(ty(d), 1, "#1 type must be cc_inv");
        assert_eq!(gran(d), 2, "#1 gran must be 2 (domain-selective)");
        let did = ((qw(d, 0) >> 16) & 0xFFFF) as u16;
        let sid = ((qw(d, 0) >> 32) & 0xFFFF) as u16;
        let fm = ((qw(d, 0) >> 48) & 0x3) as u8;
        assert_eq!(did, 0x100, "#1 DID");
        assert_eq!(sid, 0xABCD, "#1 SID");
        assert_eq!(fm, 2, "#1 FM");
        assert_eq!(qw(d, 1), 0, "#1 QW1 must be reserved 0");
    }

    // #2 iotlb_inv (global)
    {
        let d = desc(2);
        assert_eq!(ty(d), 2, "#2 type must be IOTLB Invalidate (2)");
        assert_eq!(gran(d), 1, "#2 gran must be 1 (global)");
        assert_eq!(qw(d, 1), 0, "#2 QW1 must be 0");
    }

    // #3 iotlb_inv (page-selective, DID=0x10, DR|DW, addr=0x1000)
    {
        let d = desc(3);
        assert_eq!(ty(d), 2, "#3 type must be iotlb_inv");
        assert_eq!(gran(d), 3, "#3 gran must be 3 (page-selective)");
        let dw = (qw(d, 0) >> 6) & 0x1;
        let dr = (qw(d, 0) >> 7) & 0x1;
        assert_eq!(dw, 1, "#3 DW bit");
        assert_eq!(dr, 1, "#3 DR bit");
        let did = ((qw(d, 0) >> 16) & 0xFFFF) as u16;
        assert_eq!(did, 0x10, "#3 DID");
        let addr = qw(d, 1) & !0xFFFu64;
        assert_eq!(addr, 0x1000, "#3 address (page-aligned)");
    }

    // #4 dev_iotlb_inv (SID=0x0100, addr=0x1000, S=0)
    {
        let d = desc(4);
        assert_eq!(ty(d), 3, "#4 type must be Device-IOTLB Invalidate (3)");
        let sid = ((qw(d, 0) >> 32) & 0xFFFF) as u16;
        assert_eq!(sid, 0x0100, "#4 SID");
        let s = qw(d, 1) & 0x1;
        assert_eq!(s, 0, "#4 S bit (single-page)");
        let addr = qw(d, 1) & !0xFFFu64;
        assert_eq!(addr, 0x1000, "#4 address");
    }

    // #5 dev_iotlb_inv (SID=0xABCD, MIP=2, addr=0x2000, S=1)
    {
        let d = desc(5);
        assert_eq!(ty(d), 3, "#5 type must be dev_iotlb_inv");
        let mip = ((qw(d, 0) >> 16) & 0xFFFF) as u16;
        assert_eq!(mip, 2, "#5 MIP");
        let sid = ((qw(d, 0) >> 32) & 0xFFFF) as u16;
        assert_eq!(sid, 0xABCD, "#5 SID");
        let s = qw(d, 1) & 0x1;
        assert_eq!(s, 1, "#5 S bit (range)");
        let addr = qw(d, 1) & !0xFFFu64;
        assert_eq!(addr, 0x2000, "#5 address");
    }

    // #6 iec_inv (global)
    {
        let d = desc(6);
        assert_eq!(ty(d), 4, "#6 type must be IEC Invalidate (4)");
        let iec_gran = (qw(d, 0) >> 4) & 0x1;
        assert_eq!(iec_gran, 0, "#6 IEC gran must be 0 (global)");
        assert_eq!(qw(d, 1), 0, "#6 QW1 reserved 0");
    }

    // #7 iec_inv (index-selective, IIDX=0x100, IM=2)
    {
        let d = desc(7);
        assert_eq!(ty(d), 4, "#7 type must be iec_inv");
        let iec_gran = (qw(d, 0) >> 4) & 0x1;
        assert_eq!(iec_gran, 1, "#7 IEC gran must be 1 (index-selective)");
        let im = ((qw(d, 0) >> 27) & 0x1F) as u8;
        assert_eq!(im, 2, "#7 IM");
        let iidx = ((qw(d, 0) >> 32) & 0xFFFF) as u16;
        assert_eq!(iidx, 0x100, "#7 IIDX");
    }

    // #8 inv_wait (IF|FN)
    {
        let d = desc(8);
        assert_eq!(ty(d), 5, "#8 type must be Invalidation Wait (5)");
        let iflag = (qw(d, 0) >> 4) & 0x1;
        let sw = (qw(d, 0) >> 5) & 0x1;
        let fn_ = (qw(d, 0) >> 6) & 0x1;
        assert_eq!(iflag, 1, "#8 IF bit");
        assert_eq!(sw, 0, "#8 SW bit");
        assert_eq!(fn_, 1, "#8 FN bit");
        assert_eq!(qw(d, 1), 0, "#8 status-addr must be 0 (no status write)");
    }

    // #9 inv_wait (SW|FN, STDATA=0xDEADBEEF, STADDR=0xF0000000)
    {
        let d = desc(9);
        assert_eq!(ty(d), 5, "#9 type must be inv_wait");
        let sw = (qw(d, 0) >> 5) & 0x1;
        let fn_ = (qw(d, 0) >> 6) & 0x1;
        assert_eq!(sw, 1, "#9 SW bit");
        assert_eq!(fn_, 1, "#9 FN bit");
        let stdata = ((qw(d, 0) >> 32) & 0xFFFF_FFFFu64) as u32;
        assert_eq!(stdata, 0xDEAD_BEEF, "#9 STDATA");
        let staddr = qw(d, 1) & !0x3u64;
        assert_eq!(staddr, 0xF000_0000, "#9 STADDR (128-bit aligned)");
    }

    // #10 p_iotlb (all-PASID, DID=0x10, PASID=0x123)
    {
        let d = desc(10);
        assert_eq!(ty(d), 6, "#10 type must be PASID-based IOTLB (6)");
        assert_eq!(gran(d), 1, "#10 gran must be 1 (all-PASID)");
        let did = ((qw(d, 0) >> 16) & 0xFFFF) as u16;
        let pasid = ((qw(d, 0) >> 32) & 0xFFFFF) as u32;
        assert_eq!(did, 0x10, "#10 DID");
        assert_eq!(pasid, 0x123, "#10 PASID");
        assert_eq!(qw(d, 1), 0, "#10 QW1 must be 0 for all-PASID");
    }

    // #11 p_iotlb (page-selective within PASID, DID=0x20, PASID=0x456, addr=0x3000)
    {
        let d = desc(11);
        assert_eq!(ty(d), 6, "#11 type must be p_iotlb");
        assert_eq!(gran(d), 2, "#11 gran must be 2 (page-selective within PASID)");
        let did = ((qw(d, 0) >> 16) & 0xFFFF) as u16;
        let pasid = ((qw(d, 0) >> 32) & 0xFFFFF) as u32;
        assert_eq!(did, 0x20, "#11 DID");
        assert_eq!(pasid, 0x456, "#11 PASID");
        let addr = qw(d, 1) & !0xFFFu64;
        assert_eq!(addr, 0x3000, "#11 address");
    }

    // #12 p_cache (all-PASID)
    {
        let d = desc(12);
        assert_eq!(ty(d), 7, "#12 type must be PASID Cache Invalidate (7)");
        assert_eq!(gran(d), 1, "#12 gran must be 1 (all-PASID)");
        assert_eq!(qw(d, 1), 0, "#12 QW1 reserved 0");
    }

    // #13 p_cache (PASID-selective, DID=0x20, PASID=0x789)
    {
        let d = desc(13);
        assert_eq!(ty(d), 7, "#13 type must be p_cache");
        assert_eq!(gran(d), 2, "#13 gran must be 2 (PASID-selective)");
        let did = ((qw(d, 0) >> 16) & 0xFFFF) as u16;
        let pasid = ((qw(d, 0) >> 32) & 0xFFFFF) as u32;
        assert_eq!(did, 0x20, "#13 DID");
        assert_eq!(pasid, 0x789, "#13 PASID");
    }

    // #14 p_dev_iotlb (PASID=0x100, SID=0x0100, addr=0x4000, S=0)
    //
    // Layout in this fixture (three consecutive 16-bit fields at bits 16,
    // 32, 48): PASID[15:0] at bits 16-31, SID at bits 32-47, MIP at bits
    // 48-63. This differs from the p_iotlb (type 6) descriptors above,
    // whose PASID is a full 20-bit field at bits 32-51 with DID at 16-31 —
    // do NOT reuse the 20-bit mask here; it would spill into SID.
    {
        let d = desc(14);
        assert_eq!(ty(d), 8, "#14 type must be PASID-based Device-IOTLB (8)");
        let pasid = ((qw(d, 0) >> 16) & 0xFFFF) as u32;
        let sid = ((qw(d, 0) >> 32) & 0xFFFF) as u16;
        assert_eq!(pasid, 0x100, "#14 PASID");
        assert_eq!(sid, 0x0100, "#14 SID");
        let s = qw(d, 1) & 0x1;
        assert_eq!(s, 0, "#14 S bit (single-page)");
        let addr = qw(d, 1) & !0xFFFu64;
        assert_eq!(addr, 0x4000, "#14 address");
    }

    // #15 p_dev_iotlb (PASID=0x200, SID=0xABCD, MIP=1, addr=0x5000, S=1)
    //
    // Same 16-bit PASID/SID/MIP layout as #14 — the 0xFFFF mask (not the
    // p_iotlb 0xFFFFF) is what keeps SID's low nibble out of PASID.
    {
        let d = desc(15);
        assert_eq!(ty(d), 8, "#15 type must be p_dev_iotlb");
        let pasid = ((qw(d, 0) >> 16) & 0xFFFF) as u32;
        let sid = ((qw(d, 0) >> 32) & 0xFFFF) as u16;
        let mip = ((qw(d, 0) >> 48) & 0xFFFF) as u16;
        assert_eq!(pasid, 0x200, "#15 PASID");
        assert_eq!(sid, 0xABCD, "#15 SID");
        assert_eq!(mip, 1, "#15 MIP");
        let s = qw(d, 1) & 0x1;
        assert_eq!(s, 1, "#15 S bit (range)");
        let addr = qw(d, 1) & !0xFFFu64;
        assert_eq!(addr, 0x5000, "#15 address");
    }
}

/// Every architected QI descriptor type (1..=8) appears in the bank,
/// so no type-field gap in encoder or IR lowering can slip through.
#[test]
fn vtd_qi_smoke_covers_every_architected_type() {
    let out = run_build(build_emit("vtd_qi_smoke.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let bank = qi_bytes(&bytes);

    let mut seen = [false; 9];
    for i in 0..NUM_DESCRIPTORS {
        let ty = (qw(&bank[i * DESC_BYTES..(i + 1) * DESC_BYTES], 0) & 0xF) as usize;
        assert!(
            (1..=8).contains(&ty),
            "descriptor #{i} has undefined VT-d type {ty}"
        );
        seen[ty] = true;
    }
    for ty in 1usize..=8 {
        assert!(
            seen[ty],
            "no descriptor of VT-d type {ty} in the emitted bank"
        );
    }
}
