//! Regression test pinning the encoder facade and ELF-visible behavior.
//!
//! `push r15; pop r15` must serialise to `41 57 41 5f` regardless of
//! whether the encoder lives in emitter-elf or paideia-as-encoder.

use paideia_as_emitter_elf::{CodeBuffer, Reg64, pop_reg64, push_reg64};

#[test]
fn push_pop_r15_byte_sequence_unchanged() {
    let mut buf = CodeBuffer::new();
    push_reg64(&mut buf, Reg64::R15);
    pop_reg64(&mut buf, Reg64::R15);
    assert_eq!(buf.as_slice(), &[0x41, 0x57, 0x41, 0x5f]);
}
