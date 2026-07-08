//! Shared integration-test harness for `paideia-as-encoder`.
//!
//! Extracted from the ~40 encoder integration tests that all repeated the
//! same "build an `Instruction`, run it through `encode_instruction`, then
//! compare bytes" boilerplate. Kept intentionally small — one helper covers
//! the byte-exact assertion path used by the majority of tests, the second
//! returns the collected [`EncodeStats`] for the smaller set of tests that
//! assert on relocation metadata as well.

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::instruction::Instruction;

/// Encode `inst` and return the raw bytes. Panics on encoder error so the
/// caller sees a helpful stack trace at the exact test that failed.
pub fn encode_bytes(inst: &Instruction) -> Vec<u8> {
    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(inst, &mut buf, &mut stats)
        .expect("common::encode_bytes: encoding failed");
    buf.as_slice().to_vec()
}
