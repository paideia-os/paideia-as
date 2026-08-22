//! Regression pin #1316 (R51-paideia-as-002): byte-accurate packing of the
//! two DMA descriptor shapes paideia-os R51 will use unmodified over the
//! PCIe wire.
//!
//! Both shapes are struct-of-integers layouts with strict spec sizes:
//!
//! * **AHCI PRDT entry** — `{ dba: u64, reserved: u32, dbc_i: u32 }` MUST be
//!   exactly 16 bytes with no interior padding drift.  The whole PRDT array
//!   is walked at 16-byte stride by AHCI hardware; any tail-pad or
//!   alignment-hole in the entry would offset every subsequent descriptor
//!   and the device would DMA into unrelated memory.
//!
//! * **NVMe PRP list** — a page of u64 physical addresses at 8-byte stride,
//!   512 entries × 8 B = 4096 B per page.  Any per-element padding (say,
//!   emitting each u64 as 16 B) would spill the page and break the "next
//!   PRP-list pointer" convention.
//!
//! Coverage split with existing regression pins:
//!   * `pa10_006s_u64_array_16_bytes` (array_element_widths.rs) already
//!     pins `[u64; 2]` at 16 B.
//!   * `repeat_literal_u64_512_is_not_small_n_special_cased`
//!     (array_storage_arity.rs) already pins `[u64; 512] = [0; 512]` at
//!     4096 B.
//!   * `record_global_emits_to_rodata` (pa_r17_010c) already pins a
//!     homogeneous `{u64, u64}` record.
//!
//! The gap this pin closes: byte-exact **mixed-width** struct packing
//! (u64 + u32 + u32) and a **value-carrying** u64 array whose per-slot
//! offset is verified end-to-end from source through ELF.

use object::{Object, ObjectSection, ObjectSymbol};

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Return `(size, bytes)` of a symbol resolved through whichever section it
/// actually lives in (`.rodata`, `.data`, `@link_section` override).
///
/// Mirrors the private helper in `build_emit::array_storage_arity`; kept
/// local so this pin does not couple to that module's file layout.
fn symbol_storage(elf_bytes: &[u8], symbol: &str) -> (u64, Vec<u8>) {
    let file = object::File::parse(elf_bytes).expect("object should parse the ELF");

    let sym = file
        .symbols()
        .find(|s| s.name().unwrap_or("") == symbol)
        .unwrap_or_else(|| panic!("symbol `{symbol}` not found in ELF"));

    let section_index = match sym.section() {
        object::SymbolSection::Section(idx) => idx,
        other => panic!("symbol `{symbol}` is not in a regular section: {other:?}"),
    };
    let section = file
        .section_by_index(section_index)
        .expect("symbol's section index resolves");
    let section_data = section.data().unwrap_or(b"");

    let start = sym.address() as usize;
    let size = sym.size() as usize;
    assert!(
        start + size <= section_data.len(),
        "symbol `{symbol}` at {start}+{size} overruns section `{}` of {} bytes",
        section.name().unwrap_or("?"),
        section_data.len()
    );

    (sym.size(), section_data[start..start + size].to_vec())
}

/// PRDT entry must occupy exactly 16 bytes with each field at its spec offset.
///
/// AHCI 1.3.1 §4.2.3.3 — Physical Region Descriptor Table Entry:
///   DW0/DW1  (offset 0)  = Data Base Address (DBA), 64-bit, 2-byte aligned
///   DW2      (offset 8)  = Reserved
///   DW3      (offset 12) = Byte Count (bits 21:0) | I (bit 31)
///
/// This test would fail loudly if the compiler ever:
///   * inserted padding between `dba` and `reserved`
///   * inserted padding between `reserved` and `dbc_i`
///   * tail-padded the struct past 16 bytes (e.g. to 24 chasing an 8-byte
///     alignment past the last field)
///   * silently reordered fields
#[test]
fn prdt_entry_packs_to_exactly_sixteen_bytes() {
    let out = run_build(build_emit("pa1316_prdt_entry.pdx"));
    out.assert_ok();

    let (size, bytes) = symbol_storage(&out.artifact_bytes(), "prdt_entry");

    assert_eq!(
        size, 16,
        "PRDT struct {{ dba: u64, reserved: u32, dbc_i: u32 }} must link at exactly 16 bytes; \
         got {size} — the encoder inserted padding or the layout pass drifted from spec"
    );

    // Expected little-endian byte pattern, per fixture:
    //   dba      = 0x0102030405060708 → 08 07 06 05 04 03 02 01
    //   reserved = 0x11223344         → 44 33 22 11
    //   dbc_i    = 0x55667788         → 88 77 66 55
    let expected: [u8; 16] = [
        0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, // dba (offset 0..8)
        0x44, 0x33, 0x22, 0x11, // reserved (offset 8..12)
        0x88, 0x77, 0x66, 0x55, // dbc_i (offset 12..16)
    ];
    assert_eq!(
        bytes.as_slice(),
        &expected,
        "PRDT byte layout must match spec exactly. \
         A mismatch means the encoder shuffled fields, drifted an offset, or truncated a field."
    );
}

/// PRP-list entries (u64 physical addresses) must sit at 8-byte stride with
/// no per-element padding.  Each address must land at exactly `offset = i * 8`
/// with its full 8 little-endian bytes intact.
///
/// The 512-entry, all-zero variant is already pinned by
/// `repeat_literal_u64_512_is_not_small_n_special_cased`; this test adds the
/// value-carrying explicit-list variant, which is what surfaces per-element
/// width bugs (a `pack_u64_le`-vs-`pack_int_le(width=4)` mismatch would
/// silently double the slot stride and shift every entry off).
#[test]
fn prp_list_entries_pack_at_eight_byte_stride() {
    let out = run_build(build_emit("pa1316_prp_list_entries.pdx"));
    out.assert_ok();

    let (size, bytes) = symbol_storage(&out.artifact_bytes(), "prp_list_entries");

    assert_eq!(
        size, 64,
        "[u64; 8] must link at exactly 8 * 8 = 64 bytes; got {size} — per-element width drift"
    );

    // Each entry i has value 0x1000 * (i + 1), stored little-endian at offset i * 8.
    let mut expected = Vec::with_capacity(64);
    for i in 0..8u64 {
        let value: u64 = 0x1000 * (i + 1);
        expected.extend_from_slice(&value.to_le_bytes());
    }
    assert_eq!(
        bytes, expected,
        "PRP-list bytes must match distinct per-slot addresses at 8-byte stride. \
         A mismatch means an entry landed at the wrong offset or width."
    );

    // Explicit per-slot offset check — reads the layout with the exact
    // arithmetic hardware performs (`entry_ptr = base + i * 8`) so any
    // regression that survives a matching size + concatenated bytes check
    // (e.g. a subtle endianness flip inside one slot) still fires here.
    for i in 0..8usize {
        let off = i * 8;
        let slot = &bytes[off..off + 8];
        let read = u64::from_le_bytes(slot.try_into().expect("8 bytes"));
        let want = 0x1000u64 * (i as u64 + 1);
        assert_eq!(
            read, want,
            "slot {i} at byte offset {off} must decode to 0x{want:x}, got 0x{read:x}"
        );
    }
}
