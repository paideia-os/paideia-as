//! Integration regression for issue #1328: encoder accepts `add r64, [mem]`
//! and `sub r64, [mem]` end-to-end via the `paideia-as build` pipeline.
//!
//! Pre-fix, `encode_add` / `encode_sub` in
//! `crates/paideia-as-encoder/src/encode_instruction.rs` matched only
//! `[Reg, Reg]` and `[Reg, Imm64]`. A memory-source operand fell through to
//! the catch-all arm and surfaced as `B1705 Unsupported("add form not
//! supported: expected reg64,reg64 or reg64,imm64")` (mirror for sub). This
//! test compiles the fixture and pins the expected `.text` bytes so a
//! regression would surface as a byte diff rather than a silent shape drift.
//!
//! Fixture: `tests/build-emit/add_sub_mem_1328.pdx`
//!
//! Golden bytes per body (each function ends in `ret` = 0xC3):
//!   add_r8_from_stack:  4C 03 45 C0 C3        (`add r8, [rbp - 64]; ret`)
//!   sub_rax_from_stack: 48 2B 44 24 18 C3     (`sub rax, [rsp + 24]; ret`)
//!   add_sib_scale_4:    48 03 44 8B 10 C3     (`add rax, [rbx + rcx*4 + 16]; ret`)
//!   sub_sib_extended:   4F 2B 24 C8 C3        (`sub r12, [r8 + r9*8]; ret`)

use object::{Object, ObjectSection};

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

fn build_text_bytes() -> Vec<u8> {
    let input = build_emit("add_sub_mem_1328.pdx");
    let out = run_build(input);
    out.assert_ok();
    let bytes = out.artifact_bytes();
    let file = object::File::parse(&*bytes).expect("output should parse as an ELF object");
    let text = file.section_by_name(".text").expect(".text section should exist");
    text.data().expect(".text section should have data").to_vec()
}

/// Locate the first occurrence of `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[test]
fn add_r64_mem_base_disp_encodes_and_reaches_text() {
    let text = build_text_bytes();
    // `add r8, [rbp - 64]; ret` — REX.R=1, opcode 03, mod=01 rm=101 (rbp),
    // disp8=0xC0 (= -0x40), then ret (0xC3).
    assert!(
        find(&text, &[0x4C, 0x03, 0x45, 0xC0, 0xC3]).is_some(),
        "expected `add r8, [rbp - 64]; ret` bytes (4C 03 45 C0 C3) in .text: {text:02X?}",
    );
}

#[test]
fn sub_r64_mem_base_disp_encodes_and_reaches_text() {
    let text = build_text_bytes();
    // `sub rax, [rsp + 24]; ret` — REX.W=1, opcode 2B, mod=01 rm=100 (SIB
    // escape for rsp), SIB=24 (scale=00 index=100 base=100), disp8=18, ret.
    assert!(
        find(&text, &[0x48, 0x2B, 0x44, 0x24, 0x18, 0xC3]).is_some(),
        "expected `sub rax, [rsp + 24]; ret` bytes (48 2B 44 24 18 C3) in .text: {text:02X?}",
    );
}

#[test]
fn add_r64_mem_sib_scale4_encodes_and_reaches_text() {
    let text = build_text_bytes();
    // `add rax, [rbx + rcx*4 + 16]; ret` — REX.W, opcode 03,
    // ModR/M=44 (mod=01 reg=000 rm=100), SIB=8B (scale=10 index=001 base=011),
    // disp8=10, ret.
    assert!(
        find(&text, &[0x48, 0x03, 0x44, 0x8B, 0x10, 0xC3]).is_some(),
        "expected `add rax, [rbx + rcx*4 + 16]; ret` bytes (48 03 44 8B 10 C3) in .text: {text:02X?}",
    );
}

#[test]
fn sub_r64_mem_sib_extended_regs_encodes_and_reaches_text() {
    let text = build_text_bytes();
    // `sub r12, [r8 + r9*8]; ret` — REX 4F (W=1 R=1 X=1 B=1),
    // opcode 2B, ModR/M=24 (mod=00 reg=100 rm=100), SIB=C8 (scale=11
    // index=001 base=000), no disp, ret.
    assert!(
        find(&text, &[0x4F, 0x2B, 0x24, 0xC8, 0xC3]).is_some(),
        "expected `sub r12, [r8 + r9*8]; ret` bytes (4F 2B 24 C8 C3) in .text: {text:02X?}",
    );
}
