//! Regression: paideia-as #1249 — imm64 bitop expansion emission_order.
//!
//! Before the fix (commit 7701124), `expand_bitop_imm64` in
//! `crates/paideia-as-elaborator/src/imm64_expand.rs` hardcoded
//! `emission_order: 0` on both synthesized instructions (movabs r11, imm64
//! and the bitop reg,r11). Because `paideia-as-emitter-pe` sorts
//! instructions by (emission_order, node_id) before encoding, the pair
//! landed AT THE VERY FRONT of the object's `.text` — before the function's
//! own entry symbol. The function symbol still pointed at the first
//! properly-ordered instruction (its prologue), so at runtime a call
//! into the function bypassed the imm64 mask entirely.
//!
//! Existing unit tests in `crates/paideia-as-elaborator/tests/unsafe_walker/
//! imm64_bitops.rs` sort by node-id, so they were blind to the ordering bug.
//! This test parses the emitted ELF and asserts the movabs bytes appear at
//! the symbol offset of the function that requested the expansion, not
//! before it.

use object::{Object, ObjectSection, ObjectSymbol};
use std::path::PathBuf;
use std::process::Command;

fn build_emit_data(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../tests/build-emit");
    p.push(name);
    p
}

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

fn build_elf(fixture_name: &str) -> Vec<u8> {
    let input = build_emit_data(&format!("{}.pdx", fixture_name));
    let tmp = std::env::temp_dir().join(format!("paideia_as_{}.o", fixture_name));
    let _ = std::fs::remove_file(&tmp);

    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        tmp.to_str().unwrap(),
    ]);

    assert!(
        out.status.success(),
        "build failed for {}.pdx: {}",
        fixture_name,
        String::from_utf8_lossy(&out.stderr)
    );

    std::fs::read(&tmp).expect("output ELF should exist")
}

#[test]
fn imm64_bitop_expansion_lands_inside_function() {
    let bytes = build_elf("imm64_bitop_ordering");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Locate .text section and the `probe` symbol.
    let text = file
        .sections()
        .find(|s| s.name().unwrap_or("") == ".text")
        .expect(".text section should exist");
    let text_data = text.data().expect(".text data should exist");

    let probe_sym = file
        .symbols()
        .find(|s| s.name().unwrap_or("") == "probe")
        .expect("probe symbol should exist in imm64_bitop_ordering.o");

    // The symbol's address is the offset within .text where `probe` starts.
    let probe_offset = probe_sym.address() as usize;

    // #1249 regression: assert the movabs $0x8000000000000000, %r11 bytes
    // (49 BB 00 00 00 00 00 00 00 80) appear STARTING inside the function,
    // not before it. The offset within `probe` where movabs lands is
    // implementation-defined (paideia-as may or may not reorder around it),
    // but it must NOT precede `probe`'s symbol offset.
    //
    // Pre-fix behavior would have `probe_offset` = 0xD and movabs at offset
    // 0x0 (before the symbol). Post-fix: `probe_offset` = 0 and movabs at
    // some offset >= 0 within the function.
    let movabs_r11_prefix: [u8; 2] = [0x49, 0xBB];
    let imm_le: [u8; 8] = 0x8000000000000000u64.to_le_bytes();

    let mut needle = Vec::with_capacity(10);
    needle.extend_from_slice(&movabs_r11_prefix);
    needle.extend_from_slice(&imm_le);

    // Find the movabs pattern in .text.
    let movabs_pos = text_data
        .windows(10)
        .position(|w| w == needle.as_slice())
        .expect("movabs $0x8000000000000000, %r11 should appear in .text");

    assert!(
        movabs_pos >= probe_offset,
        "#1249 regression: movabs found at .text offset {:#x}, before probe symbol at {:#x} — imm64 bitop expansion emitted with emission_order:0, landing outside the function",
        movabs_pos,
        probe_offset
    );
}
