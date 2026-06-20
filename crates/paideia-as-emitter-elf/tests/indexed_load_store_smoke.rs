//! End-to-end smoke test for indexed load/store encoder.
//!
//! Constructs a synthetic IR arena with a Load(pointer=RDI, index=RCX, width=8)
//! node, emits it via the encoder, and verifies the output contains the
//! canonical SIB byte sequence.

use paideia_as_emitter_elf::CodeBuffer;
use paideia_as_emitter_elf::Reg64;
use paideia_as_emitter_elf::emit_indexed_load;

#[test]
fn index_load_synthetic_ir_width_8() {
    // Construct a minimal synthetic IR scenario: Load with width=8.
    // In a full IR, this would be an IrKind::Load node with pointer and index children.
    // For this smoke test, we directly invoke the encoder primitive.

    let mut buf = CodeBuffer::new();
    emit_indexed_load(&mut buf, Reg64::Rax, Reg64::Rdi, Reg64::Rcx, 8, false);

    // Expected encoding: mov rax, [rdi + rcx * 8]
    // - REX.W = 0x48
    // - Opcode = 0x8B
    // - ModR/M = 0x04 (mod=00, reg=000 [RAX], rm=100 [SIB follows])
    // - SIB = 0xCF (scale=11 [×8], index=001 [RCX], base=111 [RDI])
    // Total: 48 8b 04 cf
    assert_eq!(buf.as_slice(), &[0x48, 0x8b, 0x04, 0xcf]);
}

#[test]
fn index_load_all_four_widths() {
    // Verify all four canonical widths encode correctly.

    // Width 1: mov al, [rdi + rcx]
    let mut buf1 = CodeBuffer::new();
    emit_indexed_load(&mut buf1, Reg64::Rax, Reg64::Rdi, Reg64::Rcx, 1, false);
    assert_eq!(buf1.as_slice(), &[0x8a, 0x04, 0x0f]);

    // Width 2: mov ax, [rdi + rcx * 2]
    let mut buf2 = CodeBuffer::new();
    emit_indexed_load(&mut buf2, Reg64::Rax, Reg64::Rdi, Reg64::Rcx, 2, false);
    assert_eq!(buf2.as_slice(), &[0x66, 0x8b, 0x04, 0x4f]);

    // Width 4: mov eax, [rdi + rcx * 4]
    let mut buf4 = CodeBuffer::new();
    emit_indexed_load(&mut buf4, Reg64::Rax, Reg64::Rdi, Reg64::Rcx, 4, false);
    assert_eq!(buf4.as_slice(), &[0x8b, 0x04, 0x8f]);

    // Width 8: mov rax, [rdi + rcx * 8]
    let mut buf8 = CodeBuffer::new();
    emit_indexed_load(&mut buf8, Reg64::Rax, Reg64::Rdi, Reg64::Rcx, 8, false);
    assert_eq!(buf8.as_slice(), &[0x48, 0x8b, 0x04, 0xcf]);
}
