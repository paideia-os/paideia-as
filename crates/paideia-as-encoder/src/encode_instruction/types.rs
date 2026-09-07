//! Public encoder types: statistics, errors, relocation sites, fixups,
//! and the per-instruction encoding output.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes; every item remains
//! reachable at `encode_instruction::…` via `pub use` in `mod.rs`.

use super::*;

#[derive(Debug, Clone, Copy, Default)]
pub struct EncodeStats {
    /// Number of instructions tightened (used shorter encoding form).
    pub tightened: usize,
    /// Total number of instructions encoded.
    pub total: usize,
}

impl EncodeStats {
    /// Create a new empty stats structure.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a tightening event.
    pub fn record_tightening(&mut self) {
        self.tightened += 1;
    }

    /// Increment total instruction count.
    pub fn record_instruction(&mut self) {
        self.total += 1;
    }
}

#[derive(Debug, thiserror::Error)]
/// Errors that can occur during instruction encoding.
pub enum EncodeError {
    /// Operand count mismatch for a mnemonic.
    #[error("operand mismatch for {mnemonic:?}: expected {expected}, got {got}")]
    OperandCount {
        /// The mnemonic that had the operand count mismatch.
        mnemonic: Mnemonic,
        /// Expected operand count.
        expected: usize,
        /// Actual operand count.
        got: usize,
    },
    /// Operand shape mismatch for a mnemonic.
    #[error("operand shape mismatch for {mnemonic:?}")]
    OperandShape {
        /// The mnemonic that had the operand shape mismatch.
        mnemonic: Mnemonic,
    },
    /// Invalid operand value (e.g., RSP as SIB index).
    #[error("invalid operand: {0}")]
    InvalidOperand(&'static str),
    /// Feature not yet supported by the encoder.
    #[error("unsupported: {0}")]
    Unsupported(&'static str),
}

/// Kind of relocation for a symbol reference.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum RelocKind {
    /// PC-relative 32-bit relocation (x86_64 R_X86_64_PC32).
    PcRel32,
    /// PLT 32-bit relocation (x86_64 R_X86_64_PLT32).
    Plt32,
    /// Absolute 32-bit relocation (x86_64 R_X86_64_32).
    /// PA10-006a: used for ljmp imm32:imm16 direct form with symbol reference.
    Abs32,
    /// Absolute 64-bit relocation (x86_64 R_X86_64_64).
    Abs64,
}

/// A relocation site in the encoded instruction stream.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct RelocSite {
    /// Byte offset into the instruction stream where the relocation applies.
    pub byte_offset: u32,
    /// Name of the symbol being referenced.
    pub symbol: String,
    /// Kind of relocation to apply.
    pub kind: RelocKind,
    /// Addend to apply to the symbol address.
    pub addend: i32,
}

/// Phase 6 m4-003: A label fixup site in the encoded instruction stream.
/// Records where a Jcc or Jmp instruction references a label (forward or backward),
/// allowing the linker to patch the rel32 displacement after all labels are resolved.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct LabelFixup {
    /// Byte offset into the instruction stream where the rel32 placeholder is located.
    pub byte_offset: u32,
    /// Name of the target label.
    pub label_name: String,
    /// Addend to apply to the label offset (typically 0).
    pub addend: i32,
    /// Size of the instruction (5 for jmp, 6 for jcc).
    pub instruction_size: u32,
}

/// Output from encoding an instruction, including relocation sites and label fixups.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EncodeOutput {
    /// Relocation sites to be processed by the linker.
    pub reloc_sites: Vec<RelocSite>,
    /// Label fixup sites for Jcc/Jmp instructions (phase 6 m4-003).
    pub label_fixups: Vec<LabelFixup>,
}

impl EncodeOutput {
    /// Create a new empty output.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a relocation site to the output.
    pub fn add_reloc(&mut self, site: RelocSite) {
        self.reloc_sites.push(site);
    }

    /// Phase 6 m4-003: Add a label fixup site to the output.
    pub fn add_label_fixup(&mut self, fixup: LabelFixup) {
        self.label_fixups.push(fixup);
    }
}
