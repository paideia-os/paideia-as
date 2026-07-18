//! paideia-as-emit: stable public API for emitting x86_64 instructions at runtime.
//!
//! This crate provides the primary entry point for JIT compilers and runtime
//! environments that need to emit individual x86_64 instructions into a buffer.
//!
//! The API is designed to be stable and consumed by paideia-os phase-10 (WASM jail)
//! and paideia-as self-hosting. It wraps the internal encoder's single-instruction
//! path and enforces two load-bearing invariants:
//!
//! 1. **No partial emissions**: on error, the buffer is rolled back to its
//!    pre-call state using `truncate()`.
//! 2. **No unresolved relocations**: operands that would produce relocations
//!    (SymbolRef, LabelRef, MemRipRelSym, MemSymIndexed) are rejected at
//!    pre-flight, before the encoder runs.

pub use paideia_as_encoder::CodeBuffer;
pub use paideia_as_runtime::Instruction;
pub use paideia_as_runtime::Mnemonic;
pub use paideia_as_runtime::Operand;
pub use paideia_as_runtime::{
    resolve_symbols, DuplicateLabel, LabelMap, ResolveError, ResolvePolicy, SymbolTable,
};

use paideia_as_encoder::{encode_instruction, EncodeError, EncodeStats};

/// Errors returned by [`emit_instruction`].
///
/// This enum is `#[non_exhaustive]` so that future paideia-as releases
/// may add variants without a semver bump on downstream users. Match on it
/// with a wildcard arm.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EmitError {
    /// The instruction's operand count did not match its mnemonic.
    OperandCount {
        /// The mnemonic that failed the arity check.
        mnemonic: Mnemonic,
        /// Number of operands the mnemonic accepts.
        expected: usize,
        /// Number of operands supplied.
        got: usize,
    },

    /// The instruction's operand shape (register / immediate / memory form
    /// combination) did not match its mnemonic's supported forms.
    OperandShape {
        /// The mnemonic that failed the shape check.
        mnemonic: Mnemonic,
    },

    /// An operand contained a value that cannot be encoded — for example, a
    /// [`Operand::Var`] that was not resolved to a concrete register before
    /// emit, or a register id outside the 0..15 range.
    InvalidOperand,

    /// The instruction, mnemonic form, or CPU-feature requirement is not
    /// supported by this build of the encoder.
    Unsupported,

    /// The instruction referenced an unresolved external symbol, label, or
    /// symbol-relative memory operand. `emit_instruction` emits only
    /// self-contained instructions; route these through the
    /// pa-r20-006 `resolve_symbols` API instead.
    UnresolvedRelocation,
}

impl core::fmt::Display for EmitError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EmitError::OperandCount {
                mnemonic,
                expected,
                got,
            } => write!(
                f,
                "operand count mismatch for {:?}: expected {}, got {}",
                mnemonic, expected, got
            ),
            EmitError::OperandShape { mnemonic } => {
                write!(f, "operand shape mismatch for {:?}", mnemonic)
            }
            EmitError::InvalidOperand => f.write_str("invalid operand for emit"),
            EmitError::Unsupported => {
                f.write_str("mnemonic or operand form unsupported in this build")
            }
            EmitError::UnresolvedRelocation => f.write_str(
                "instruction requires symbol/label resolution; use resolve_symbols",
            ),
        }
    }
}

impl std::error::Error for EmitError {}

impl From<EncodeError> for EmitError {
    fn from(e: EncodeError) -> Self {
        use paideia_as_encoder::EncodeError as E;
        match e {
            E::OperandCount {
                mnemonic,
                expected,
                got,
            } => EmitError::OperandCount {
                mnemonic,
                expected,
                got,
            },
            E::OperandShape { mnemonic } => EmitError::OperandShape { mnemonic },
            E::InvalidOperand(_msg) => EmitError::InvalidOperand,
            E::Unsupported(_msg) => EmitError::Unsupported,
        }
    }
}

/// Emit a single instruction's bytes into `buf`.
///
/// On `Ok(())`, the buffer's `bytes` vector has been extended with a fully
/// self-contained encoding of `ins`. The buffer's prior contents are
/// untouched.
///
/// On `Err(_)`, the buffer is left **exactly as it was on entry** (length and
/// contents restored via truncate). Callers may safely reuse `buf` after an
/// error without needing to reset it themselves.
///
/// # Errors
///
/// See [`EmitError`] for the full taxonomy. In particular, this function
/// refuses instructions whose operands would produce relocations
/// (`Operand::SymbolRef`, `Operand::LabelRef`, `Operand::MemRipRelSym`,
/// `Operand::MemSymIndexed`) — those must be routed through a future
/// `resolve_symbols` API.
///
/// # Stability
///
/// This function is part of the paideia-as stable public API.
/// Signature changes require a major-version bump; adding variants to
/// [`EmitError`] is compatible because the enum is `#[non_exhaustive]`.
pub fn emit_instruction(
    buf: &mut CodeBuffer,
    ins: Instruction,
) -> Result<(), EmitError> {
    // 1. Pre-flight: reject Var operands and reloc-producing operands here so
    //    that we never let the encoder write placeholder bytes we'd have to
    //    roll back. Cheap: single pass over inst.operands.
    for op in &ins.operands {
        if matches!(op, Operand::Var { .. }) {
            return Err(EmitError::InvalidOperand);
        }
        if matches!(
            op,
            Operand::SymbolRef { .. }
                | Operand::LabelRef { .. }
                | Operand::MemRipRelSym { .. }
                | Operand::MemSymIndexed { .. }
        ) {
            return Err(EmitError::UnresolvedRelocation);
        }
    }

    // 2. Snapshot buffer length for rollback on encoder failure.
    let saved_len = buf.bytes.len();

    // 3. Delegate to encoder. Discard EncodeStats (throwaway) and check
    //    EncodeOutput has no reloc/label fixups (belt-and-braces given §1
    //    pre-flight, but reloc emit can happen at inner-encoder level from
    //    encoding hints we did not statically catch — better safe).
    let mut stats = EncodeStats::new();
    match encode_instruction(&ins, buf, &mut stats) {
        Ok(output) => {
            if !output.reloc_sites.is_empty() || !output.label_fixups.is_empty() {
                buf.bytes.truncate(saved_len);
                return Err(EmitError::UnresolvedRelocation);
            }
            Ok(())
        }
        Err(e) => {
            buf.bytes.truncate(saved_len);
            Err(EmitError::from(e))
        }
    }
}
