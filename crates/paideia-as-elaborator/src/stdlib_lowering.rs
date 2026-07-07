//! Stdlib trait method → mnemonic sequence lowering.
//!
//! PA-r16-007-backtrack (#1036): a hardcoded registry that maps
//! `(trait_name, method_name)` pairs to the IR instruction sequences
//! they should lower to. Consulted by emit_call before its normal SysV
//! call-marshalling.
//!
//! Scope: PauseOps::spin_hint() only in v0.16. Follow-up issues track
//! PerCpuOps, MmioOps, BytesOps, ChecksumOps retrofits.

use paideia_as_ir::{SmallVec, instruction::{InstrMode, Instruction, Mnemonic}};

/// Look up the lowering recipe for `(trait_name, method_name)`.
/// Returns None if the pair is not a known stdlib trait method,
/// signalling emit_call should fall through to normal call emission.
///
/// The returned Vec<Instruction> is spliced in place of the call —
/// no arg-marshalling, no `call target`, no `ret`.
#[must_use]
pub fn lower_stdlib_method(
    trait_name: &str,
    method_name: &str,
    mode: InstrMode,
) -> Option<Vec<Instruction>> {
    match (trait_name, method_name) {
        ("PauseOps", "spin_hint") => Some(vec![Instruction {
            mnemonic: Mnemonic::Pause,
            operands: SmallVec::new(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode,
        }]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_ops_spin_hint_returns_pause_mnemonic() {
        let insts = lower_stdlib_method("PauseOps", "spin_hint", InstrMode::Mode64)
            .expect("pause recipe should exist");
        assert_eq!(insts.len(), 1);
        assert_eq!(insts[0].mnemonic, Mnemonic::Pause);
        assert!(insts[0].operands.is_empty());
    }

    #[test]
    fn unknown_trait_returns_none() {
        assert!(lower_stdlib_method("UnknownTrait", "some_method", InstrMode::Mode64).is_none());
    }

    #[test]
    fn known_trait_unknown_method_returns_none() {
        assert!(lower_stdlib_method("PauseOps", "nonexistent", InstrMode::Mode64).is_none());
    }
}
