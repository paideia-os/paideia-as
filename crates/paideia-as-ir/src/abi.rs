//! Named SysV x86-64 ABI register constants and register-role groupings.
//!
//! # Motivation
//!
//! The elaborator scatters `RegId(7) // rdi` and `RegId(0) // rax` across
//! 100+ sites. The number and the comment are both correct today, but future
//! maintainers keep the comment stale (grep found several sites where a
//! trailing `// rdi` comment sits next to `RegId(6)` — which is actually
//! RSI). Named constants remove that class of drift and give us one place
//! to change if we ever port to a different ABI (e.g. Windows x64).
//!
//! # Contract
//!
//! Every constant here is a raw SysV register id. The scratch/arg role
//! groupings are the emitter's convention and match the existing
//! `LocalBindingTable` scratch pool at the time of introduction.
//!
//! Reg-id encoding follows Intel's x86-64 register index:
//! ```text
//!   0 → RAX    4 → RSP    8  → R8    12 → R12
//!   1 → RCX    5 → RBP    9  → R9    13 → R13
//!   2 → RDX    6 → RSI    10 → R10   14 → R14
//!   3 → RBX    7 → RDI    11 → R11   15 → R15
//! ```

use crate::instruction::RegId;

/// SysV integer-return register. First return slot for values that fit.
pub const RAX: RegId = RegId(0);
/// Second scratch caller-saved.
pub const RCX: RegId = RegId(1);
/// SysV arg-2. Second scratch after RAX in the SCRATCH pool.
pub const RDX: RegId = RegId(2);
/// Callee-saved; base for locals; not used by the emitter directly.
pub const RBX: RegId = RegId(3);
/// Stack pointer.
pub const RSP: RegId = RegId(4);
/// Base pointer. Callee-saved.
pub const RBP: RegId = RegId(5);
/// SysV arg-1.
pub const RSI: RegId = RegId(6);
/// SysV arg-0. Default scrutinee-pointer base register in the emit path.
pub const RDI: RegId = RegId(7);
/// SysV arg-4.
pub const R8: RegId = RegId(8);
/// SysV arg-5.
pub const R9: RegId = RegId(9);
/// Scratch (non-arg, caller-saved).
pub const R10: RegId = RegId(10);
/// Scratch (non-arg, caller-saved). Used as the fnptr-holder in
/// indirect-call sequences to avoid clobbering RDI-R9 during arg
/// marshalling.
pub const R11: RegId = RegId(11);
/// Callee-saved.
pub const R12: RegId = RegId(12);
/// Callee-saved. Used in tests that check the R13 SIB-escape encoding.
pub const R13: RegId = RegId(13);
/// Callee-saved.
pub const R14: RegId = RegId(14);
/// Callee-saved. Extended register used in extended-src tests.
pub const R15: RegId = RegId(15);

/// SysV integer/pointer arg registers in call order.
///
/// Callers marshal argument `i` into `ARG_REGS[i]` for `i < 6`; the 7th+
/// argument spills to the stack (not yet implemented in this emitter).
pub const ARG_REGS: [RegId; 6] = [RDI, RSI, RDX, RCX, R8, R9];

/// Return-value register.
pub const RET: RegId = RAX;

/// Scratch pool used by `LocalBindingTable` for `let` bindings inside a
/// function body. Matches the `[RAX, RCX, RDX, R8]` sequence hardcoded
/// today.
pub const SCRATCH: [RegId; 4] = [RAX, RCX, RDX, R8];

/// Extended scratch pool used by `lower_pattern` for nested pattern
/// leaf bindings (RAX is reserved for the enum discriminant during the
/// dispatch cascade). Matches the `[RCX, RDX, R8, R10, R11]` sequence
/// introduced in #987.
pub const PATTERN_SCRATCH: [RegId; 5] = [RCX, RDX, R8, R10, R11];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_constants_have_expected_ids() {
        assert_eq!(RAX.0, 0);
        assert_eq!(RCX.0, 1);
        assert_eq!(RDX.0, 2);
        assert_eq!(RSI.0, 6);
        assert_eq!(RDI.0, 7);
        assert_eq!(R8.0, 8);
        assert_eq!(R9.0, 9);
        assert_eq!(R10.0, 10);
        assert_eq!(R11.0, 11);
        assert_eq!(R13.0, 13);
        assert_eq!(R15.0, 15);
    }

    #[test]
    fn arg_regs_ordering_is_sysv() {
        assert_eq!(ARG_REGS, [RDI, RSI, RDX, RCX, R8, R9]);
        assert_eq!(ARG_REGS.len(), 6);
    }

    #[test]
    fn ret_is_rax() {
        assert_eq!(RET, RAX);
    }

    #[test]
    fn scratch_pool_matches_convention() {
        assert_eq!(SCRATCH, [RAX, RCX, RDX, R8]);
    }

    #[test]
    fn pattern_scratch_reserves_rax() {
        assert!(!PATTERN_SCRATCH.contains(&RAX));
        assert_eq!(PATTERN_SCRATCH, [RCX, RDX, R8, R10, R11]);
    }
}
