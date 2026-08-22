//! Issue #1317 — R51-paideia-as-003: MMIO barriers (sfence/mfence/lfence).
//!
//! End-to-end witness that the fence primitives survive the whole
//! pdx→AST→IR→encode→ELF pipeline with:
//!   1. their canonical byte encoding (SDM Vol. 2A):
//!        sfence = 0F AE F8
//!        mfence = 0F AE F0
//!        lfence = 0F AE E8
//!   2. their emission order relative to a preceding store — the property
//!      NVMe SQyTDBL / AHCI CI doorbell writes depend on
//!      (paideia-os design/hardware/nvme-and-disk-substrate.md §8.3).
//!
//! Byte-exact fence encodings alone live in
//! `crates/paideia-as-encoder/tests/memory_ops/mem_fences.rs`; scheduler-level
//! barrier semantics live in `crates/paideia-as-ir/src/opt/schedule.rs`. This
//! test locks the end-to-end property: build the .pdx, extract .text, and
//! decode with iced-x86 to confirm the store *precedes* the fence in the
//! emitted stream and the fence bytes match the SDM encoding.

use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};
use object::{Object, ObjectSection, ObjectSymbol};

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Canonical three-byte fence encodings from Intel SDM Vol. 2A.
const SFENCE_BYTES: [u8; 3] = [0x0F, 0xAE, 0xF8];
const MFENCE_BYTES: [u8; 3] = [0x0F, 0xAE, 0xF0];
const LFENCE_BYTES: [u8; 3] = [0x0F, 0xAE, 0xE8];

/// Extract the bytes of the named `.text` function symbol from an ELF blob.
///
/// Panics with a helpful diagnostic if the symbol or the enclosing `.text`
/// section is missing, or if the symbol has zero size (a paideia-as emitter
/// regression — every `pub let fn` should land with a positive symbol size).
fn extract_fn_bytes(elf: &[u8], fn_name: &str) -> Vec<u8> {
    let file = object::File::parse(elf).expect("object should parse the ELF");

    let text = file
        .sections()
        .find(|s| s.name().unwrap_or("") == ".text")
        .expect(".text section should exist");
    let text_idx = text.index();
    let text_bytes = text.data().expect(".text data readable");

    let sym = file
        .symbols()
        .find(|s| s.name().unwrap_or("") == fn_name)
        .unwrap_or_else(|| panic!("symbol `{fn_name}` not found in ELF"));
    assert_eq!(
        sym.section_index(),
        Some(text_idx),
        "`{fn_name}` should live in .text"
    );

    let start = sym.address() as usize;
    let size = sym.size() as usize;
    assert!(size > 0, "`{fn_name}` should have a non-zero size in .text");
    assert!(
        start + size <= text_bytes.len(),
        "`{fn_name}` [start={start}, size={size}] must fit inside .text ({} bytes)",
        text_bytes.len(),
    );
    text_bytes[start..start + size].to_vec()
}

/// Decode `fn_bytes` at RIP 0 and assert the stream begins with a store
/// (whose destination is a memory operand) immediately followed by the given
/// fence mnemonic; also assert the fence's three encoded bytes match the SDM
/// canonical opcode.
fn assert_store_then_fence(
    fn_bytes: &[u8],
    store_mnem: IcedMnem,
    fence_mnem: IcedMnem,
    fence_bytes: [u8; 3],
) {
    let mut decoder = Decoder::new(64, fn_bytes, DecoderOptions::NONE);
    let insts: Vec<_> = decoder.iter().collect();

    assert!(
        insts.len() >= 2,
        "expected at least [store, fence] in decoded stream, got {} insts",
        insts.len()
    );

    // 1. Store first, and it must be a store-shaped mov / movnti.
    assert_eq!(
        insts[0].mnemonic(),
        store_mnem,
        "instruction 0 should be {store_mnem:?}, got {:?}",
        insts[0].mnemonic()
    );
    // op0 of a store is the memory destination.
    assert_eq!(
        insts[0].op0_kind(),
        iced_x86::OpKind::Memory,
        "store's op0 should be a memory destination, got {:?}",
        insts[0].op0_kind()
    );

    // 2. Fence follows the store — same byte-level ordering as the source.
    assert_eq!(
        insts[1].mnemonic(),
        fence_mnem,
        "instruction 1 should be {fence_mnem:?}, got {:?}",
        insts[1].mnemonic()
    );

    // 3. Byte-level witness: the three fence bytes appear immediately after
    //    the store bytes. This is the property "fence was NOT reordered past
    //    the prior store during emission" translated to bytes.
    let store_len = insts[0].len();
    assert!(
        fn_bytes.len() >= store_len + 3,
        "function body ({} bytes) too short to contain store+fence",
        fn_bytes.len()
    );
    assert_eq!(
        &fn_bytes[store_len..store_len + 3],
        &fence_bytes,
        "{fence_mnem:?} bytes at offset {store_len} should be {fence_bytes:02X?}, got {:02X?}",
        &fn_bytes[store_len..store_len + 3]
    );
}

#[test]
fn sfence_follows_store_in_emitted_text() {
    let out = run_build(build_emit("pa1317_mmio_barriers.pdx"));
    out.assert_ok();
    let bytes = out.artifact_bytes();
    let fn_bytes = extract_fn_bytes(&bytes, "store_then_sfence");
    assert_store_then_fence(&fn_bytes, IcedMnem::Mov, IcedMnem::Sfence, SFENCE_BYTES);
}

#[test]
fn mfence_follows_store_in_emitted_text() {
    let out = run_build(build_emit("pa1317_mmio_barriers.pdx"));
    out.assert_ok();
    let bytes = out.artifact_bytes();
    let fn_bytes = extract_fn_bytes(&bytes, "store_then_mfence");
    assert_store_then_fence(&fn_bytes, IcedMnem::Mov, IcedMnem::Mfence, MFENCE_BYTES);
}

#[test]
fn lfence_follows_store_in_emitted_text() {
    let out = run_build(build_emit("pa1317_mmio_barriers.pdx"));
    out.assert_ok();
    let bytes = out.artifact_bytes();
    let fn_bytes = extract_fn_bytes(&bytes, "store_then_lfence");
    assert_store_then_fence(&fn_bytes, IcedMnem::Mov, IcedMnem::Lfence, LFENCE_BYTES);
}

#[test]
fn sfence_follows_non_temporal_store_in_emitted_text() {
    // The exact NVMe SQyTDBL / AHCI CI doorbell shape: a non-temporal store
    // (movnti) must land before the sfence in the emitted stream so the
    // doorbell write becomes visible only *after* the submission-queue
    // entries retire.
    let out = run_build(build_emit("pa1317_mmio_barriers.pdx"));
    out.assert_ok();
    let bytes = out.artifact_bytes();
    let fn_bytes = extract_fn_bytes(&bytes, "movnti_then_sfence");
    assert_store_then_fence(&fn_bytes, IcedMnem::Movnti, IcedMnem::Sfence, SFENCE_BYTES);
}

/// Belt-and-braces: scan the whole `.text` section for the raw fence byte
/// sequences so a regression that silently drops or rewrites the fence bytes
/// (e.g. a wrong opcode table entry) surfaces without depending on symbol
/// lookup to succeed.
#[test]
fn all_three_fence_byte_sequences_present_in_text() {
    let out = run_build(build_emit("pa1317_mmio_barriers.pdx"));
    out.assert_ok();
    let bytes = out.artifact_bytes();

    let file = object::File::parse(&*bytes).expect("object should parse the ELF");
    let text = file
        .sections()
        .find(|s| s.name().unwrap_or("") == ".text")
        .expect(".text section should exist");
    let text_bytes = text.data().expect(".text data readable");

    for (label, needle) in [
        ("sfence", SFENCE_BYTES),
        ("mfence", MFENCE_BYTES),
        ("lfence", LFENCE_BYTES),
    ] {
        assert!(
            text_bytes.windows(3).any(|w| w == needle),
            "expected {label} bytes {needle:02X?} to appear in .text (len={})",
            text_bytes.len()
        );
    }
}
