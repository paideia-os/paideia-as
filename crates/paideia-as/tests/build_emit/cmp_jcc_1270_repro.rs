//! Regression tests for issue #1270: `cmp+jne after mov reg64,imm+mov reg64,imm
//! miscompiles in some contexts`.
//!
//! Root cause (not the encoder — its per-instruction bytes were already
//! byte-exact, see `cmp_jcc_1270.rs` in the encoder crate): inside an unsafe
//! block, a `StmtExpr` (call-expression) statement interleaved with raw asm
//! was silently skipped by `UnsafeWalker`'s raw-instruction pass. Its real
//! instructions were only synthesized later by `emit_pending_unsafe_bodies`,
//! which runs after EVERY unsafe block in the whole file has already
//! consumed the shared `emission_order` counter — so the call always sorted
//! to the very end of `.text` (even after its own function's `ret`),
//! regardless of where it appeared in the source. Worse, a label placed
//! immediately before such a call-expression had nothing to alias to in the
//! raw-instruction stream, so it silently resolved to whatever raw
//! instruction happened to come later — corrupting `jcc`/`jmp` fixup
//! targets in exactly the "miscompiles in some contexts" shape #1270
//! reports.
//!
//! Fixture: tests/build-emit/cmp_jcc_1270_repro.pdx
//!   mark_a();
//!   mov rax, 0x30; mov rdx, 0x30; cmp rax, rdx; je logp_ok;
//!   mov rax, 0xDEAD;
//!   logp_ok:
//!   mark_b();
//!   ret

use object::{Object, ObjectSection};

use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Locate the first occurrence of `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Build the fixture and return the encoded `.text` *section* bytes only
/// (not the whole ELF file — headers/symtab/strtab bytes would pollute a
/// naive opcode scan).
fn build_text_bytes() -> Vec<u8> {
    let input = build_emit("cmp_jcc_1270_repro.pdx");
    let out = run_build(input);
    out.assert_ok();
    let file_bytes = out.artifact_bytes();
    let file = object::File::parse(&*file_bytes).expect("output should parse as an object file");
    let text = file.section_by_name(".text").expect(".text section should exist");
    text.data().expect("`.text` section should have data").to_vec()
}

/// The two call-expression statements (`mark_a()`, `mark_b()`) must appear
/// at their true source positions relative to the raw asm — NOT both
/// shoved to the end of `.text` after the raw `cmp`/`je` sequence (and,
/// pre-fix, after the function's own `ret`).
#[test]
fn calls_interleave_at_true_source_positions() {
    let bytes = build_text_bytes();

    // mov rax, 0x30 -> 48 C7 C0 30 00 00 00
    let mov_rax = find(&bytes, &[0x48, 0xC7, 0xC0, 0x30, 0x00, 0x00, 0x00])
        .expect("mov rax, 0x30 should be encoded");
    // cmp rax, rdx -> 48 39 D0
    let cmp = find(&bytes, &[0x48, 0x39, 0xD0]).expect("cmp rax, rdx should be encoded");
    // ret -> C3
    let ret = bytes.iter().position(|&b| b == 0xC3).expect("ret should be encoded");

    // CALL rel32 -> E8 <disp32>; scan only opcode positions (not the disp32
    // payload bytes, which may themselves contain 0xE8).
    let mut call_offsets = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0xE8 && i + 5 <= bytes.len() {
            call_offsets.push(i);
            i += 5;
        } else {
            i += 1;
        }
    }
    assert_eq!(call_offsets.len(), 2, "expected exactly 2 CALL (0xE8) sites, got {call_offsets:?} in {bytes:02x?}");

    // mark_a() precedes the raw asm; mark_b() follows it — neither call may
    // sort to after `ret` (the pre-fix bug), and the first call must land
    // before the `mov rax, 0x30` / `cmp` sequence while the second lands
    // after it.
    assert!(
        call_offsets[0] < mov_rax,
        "mark_a() call (0x{:x}) should precede `mov rax, 0x30` (0x{:x})",
        call_offsets[0],
        mov_rax
    );
    assert!(
        call_offsets[1] > cmp,
        "mark_b() call (0x{:x}) should follow `cmp rax, rdx` (0x{:x})",
        call_offsets[1],
        cmp
    );
    assert!(
        call_offsets[1] < ret,
        "mark_b() call (0x{:x}) must precede `ret` (0x{:x}) — pre-#1270-fix it sorted AFTER ret as dead code",
        call_offsets[1],
        ret
    );
}

/// The `je logp_ok` fixup must resolve to the position immediately
/// preceding the `mark_b()` call — not to `ret` or anywhere else. Before
/// the fix, the `logp_ok:` label (with nothing but a StmtExpr after it in
/// the raw-instruction stream) silently lost its alias to `mark_b()` and
/// resolved to whatever raw instruction happened to follow instead.
#[test]
fn je_target_lands_immediately_before_mark_b_call() {
    let bytes = build_text_bytes();

    // je rel32 -> 0F 84 <disp32 LE>
    let je_opcode = find(&bytes, &[0x0F, 0x84]).expect("je rel32 should be encoded");
    let disp_bytes = &bytes[je_opcode + 2..je_opcode + 6];
    let disp = i32::from_le_bytes([disp_bytes[0], disp_bytes[1], disp_bytes[2], disp_bytes[3]]);
    let je_instr_end = je_opcode + 6; // rel32 is relative to the end of this instruction
    let target = (je_instr_end as i64 + disp as i64) as usize;

    let mut call_offsets = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0xE8 && i + 5 <= bytes.len() {
            call_offsets.push(i);
            i += 5;
        } else {
            i += 1;
        }
    }
    let mark_b_call = call_offsets[1];

    assert!(
        target <= mark_b_call && target > je_opcode,
        "je target (0x{target:x}) should land between je (0x{je_opcode:x}) and the mark_b() call (0x{mark_b_call:x})"
    );

    let fail_path = find(&bytes, &[0x48, 0xC7, 0xC0, 0xAD, 0xDE, 0x00, 0x00])
        .expect("fail-path mov rax, 0xDEAD should be encoded");
    assert!(
        target != fail_path,
        "je must not target the fail-path `mov rax, 0xDEAD` — that's the branch-not-taken code"
    );
}
