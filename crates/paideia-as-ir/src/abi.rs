//! Named SysV x86-64 ABI register constants, register-role groupings, and
//! MS x64 calling-convention argument mapping.
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
//!
//! # MS x64 ABI mapping (issue #1007)
//!
//! This module provides a pure argument classification and slot-mapping layer
//! for MS x64 calling convention support. Callers (elaborator, later #1011)
//! classify their function parameters into `&[ArgClass]` and hand them to
//! `map_args` to get register/stack slot assignments.
//!
//! TODO(#1008): Float argument class and unified-bank slot advancement.
//! TODO(#1009): Aggregate type classification and layout-aware slot mapping.
//! TODO(#1011): MS hidden-pointer aggregate return value handling.
//! TODO(#1012): SysV RDX:RAX 128-bit return pair for large integers.

use crate::instruction::RegId;
use crate::let_meta::CallingConvention;

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

/// MS x64 integer/pointer arg registers in call order (RCX, RDX, R8, R9).
///
/// Used for Microsoft x64 calling convention (UEFI/Windows). The first 4
/// integer arguments are passed in these registers; the 5th+ argument spills
/// to the stack (with 32-byte shadow space below RSP).
pub const MS_ARG_REGS: [RegId; 4] = [RCX, RDX, R8, R9];

/// MS x64 shadow space on the stack (in bytes).
///
/// The caller reserves 32 bytes directly below the return address for the
/// callee to spill register arguments. This space is part of the caller's
/// frame and must be accounted for in stack offset calculations.
pub const MS_SHADOW_SPACE_BYTES: u32 = 32;

/// MS x64 caller-side shadow-space frame bump (in bytes).
///
/// The caller must decrement RSP by this amount before emitting arguments
/// and a CALL to an MS x64 ABI function. This includes the 32-byte shadow
/// space plus 8 bytes for the return address: (32 + 8) = 40.
///
/// MVP assumption: no caller-side prologue push before the shadow prelude.
/// When caller-side prologue push support is added, revisit this constant.
pub const MS_CALL_STACK_BUMP: u32 = 40;

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

/// Classification of a function argument for ABI mapping purposes.
///
/// An argument's class determines which register or stack slot it occupies
/// during a function call. Currently, only Integer class is implemented;
/// future phases will add Float, Vector, and Aggregate classes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum ArgClass {
    /// Integer or pointer argument (64-bit or narrower, zero/sign-extended
    /// at call site). Occupies one slot in the class-specific register pool.
    Integer,
}

/// A register or stack slot assigned to a function argument.
///
/// This type represents the target location for an argument value during
/// a function call, after applying the ABI's calling convention rules.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ArgSlot {
    /// Argument passed in a register.
    Reg(RegId),
    /// Argument passed on the stack.
    ///
    /// The offset is relative to the byte just above the return address.
    /// For MS x64, the shadow space (32 bytes) is included in this offset.
    /// For SysV, the offset starts at 0 for the 7th argument (first stack arg).
    Stack {
        /// Stack offset in bytes, relative to the byte just above the return address.
        offset: u32,
    },
}

/// A register or no-slot assigned to the function return value.
///
/// This type represents the target location for the function's return value,
/// after applying the ABI's calling convention rules.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ReturnSlot {
    /// Return value not present (void/unit return).
    None,
    /// Return value passed in a register.
    Reg(RegId),
}

/// Map a list of argument classifications to their ABI-specific slots.
///
/// Given a sequence of argument classes (e.g., `[Integer, Integer, Integer]`),
/// this function returns the corresponding slot assignments for the specified
/// calling convention. Slots are returned in argument order.
///
/// # SysV AMD64 (Unix/Linux) behavior:
/// - Arguments 0..6 map to RDI, RSI, RDX, RCX, R8, R9 respectively.
/// - Arguments 6+ map to stack starting at offset 0 (relative to return address).
/// - Stack offsets increment by 8 bytes per argument.
///
/// # MS x64 (Windows/UEFI) behavior:
/// - Arguments 0..4 map to RCX, RDX, R8, R9 respectively.
/// - Arguments 4+ map to stack starting at offset 32 (MS shadow space).
/// - Stack offsets increment by 8 bytes per argument.
#[must_use]
pub fn map_args(classes: &[ArgClass], cc: CallingConvention) -> Vec<ArgSlot> {
    match cc {
        CallingConvention::Sysv => {
            classes
                .iter()
                .enumerate()
                .map(|(i, _class)| {
                    if i < ARG_REGS.len() {
                        ArgSlot::Reg(ARG_REGS[i])
                    } else {
                        ArgSlot::Stack {
                            offset: ((i - ARG_REGS.len()) as u32) * 8,
                        }
                    }
                })
                .collect()
        }
        CallingConvention::Ms => {
            classes
                .iter()
                .enumerate()
                .map(|(i, _class)| {
                    if i < MS_ARG_REGS.len() {
                        ArgSlot::Reg(MS_ARG_REGS[i])
                    } else {
                        ArgSlot::Stack {
                            offset: ((i - MS_ARG_REGS.len()) as u32) * 8 + MS_SHADOW_SPACE_BYTES,
                        }
                    }
                })
                .collect()
        }
    }
}

/// Map an argument classification to its return slot.
///
/// Given a return value's argument class and calling convention, this function
/// returns the slot where the return value should be placed by the callee.
///
/// # Both SysV and MS x64:
/// - Integer class returns in RAX.
#[must_use]
pub fn map_return(class: ArgClass, _cc: CallingConvention) -> ReturnSlot {
    match class {
        ArgClass::Integer => ReturnSlot::Reg(RAX),
    }
}

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

    // ============================================================================
    // SysV x64 argument mapping tests (5 tests)
    // ============================================================================

    #[test]
    fn map_args_sysv_zero_args_returns_empty() {
        let classes = [];
        let slots = map_args(&classes, CallingConvention::Sysv);
        assert_eq!(slots, []);
    }

    #[test]
    fn map_args_sysv_one_integer_in_rdi() {
        let classes = [ArgClass::Integer];
        let slots = map_args(&classes, CallingConvention::Sysv);
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0], ArgSlot::Reg(RDI));
    }

    #[test]
    fn map_args_sysv_six_integers_fill_arg_regs() {
        let classes = [
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
        ];
        let slots = map_args(&classes, CallingConvention::Sysv);
        assert_eq!(slots.len(), 6);
        assert_eq!(slots[0], ArgSlot::Reg(RDI));
        assert_eq!(slots[1], ArgSlot::Reg(RSI));
        assert_eq!(slots[2], ArgSlot::Reg(RDX));
        assert_eq!(slots[3], ArgSlot::Reg(RCX));
        assert_eq!(slots[4], ArgSlot::Reg(R8));
        assert_eq!(slots[5], ArgSlot::Reg(R9));
    }

    #[test]
    fn map_args_sysv_seventh_integer_stacks_at_offset_zero() {
        let classes = [
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
        ];
        let slots = map_args(&classes, CallingConvention::Sysv);
        assert_eq!(slots.len(), 7);
        // First 6 in registers
        assert_eq!(slots[0], ArgSlot::Reg(RDI));
        assert_eq!(slots[5], ArgSlot::Reg(R9));
        // 7th on stack at offset 0
        assert_eq!(slots[6], ArgSlot::Stack { offset: 0 });
    }

    #[test]
    fn map_args_sysv_eighth_integer_stacks_at_offset_eight() {
        let classes = [
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
        ];
        let slots = map_args(&classes, CallingConvention::Sysv);
        assert_eq!(slots.len(), 8);
        // First 6 in registers
        assert_eq!(slots[5], ArgSlot::Reg(R9));
        // 7th and 8th on stack
        assert_eq!(slots[6], ArgSlot::Stack { offset: 0 });
        assert_eq!(slots[7], ArgSlot::Stack { offset: 8 });
    }

    // ============================================================================
    // MS x64 argument mapping tests (5 tests)
    // ============================================================================

    #[test]
    fn map_args_ms_zero_args_returns_empty() {
        let classes = [];
        let slots = map_args(&classes, CallingConvention::Ms);
        assert_eq!(slots, []);
    }

    #[test]
    fn map_args_ms_one_integer_in_rcx() {
        let classes = [ArgClass::Integer];
        let slots = map_args(&classes, CallingConvention::Ms);
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0], ArgSlot::Reg(RCX));
    }

    #[test]
    fn map_args_ms_four_integers_fill_arg_regs() {
        let classes = [
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
        ];
        let slots = map_args(&classes, CallingConvention::Ms);
        assert_eq!(slots.len(), 4);
        assert_eq!(slots[0], ArgSlot::Reg(RCX));
        assert_eq!(slots[1], ArgSlot::Reg(RDX));
        assert_eq!(slots[2], ArgSlot::Reg(R8));
        assert_eq!(slots[3], ArgSlot::Reg(R9));
    }

    #[test]
    fn map_args_ms_fifth_integer_stacks_at_offset_thirty_two() {
        let classes = [
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
        ];
        let slots = map_args(&classes, CallingConvention::Ms);
        assert_eq!(slots.len(), 5);
        // First 4 in registers
        assert_eq!(slots[0], ArgSlot::Reg(RCX));
        assert_eq!(slots[3], ArgSlot::Reg(R9));
        // 5th on stack at offset 32 (shadow space)
        assert_eq!(slots[4], ArgSlot::Stack { offset: 32 });
    }

    #[test]
    fn map_args_ms_sixth_integer_stacks_at_offset_forty() {
        let classes = [
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
            ArgClass::Integer,
        ];
        let slots = map_args(&classes, CallingConvention::Ms);
        assert_eq!(slots.len(), 6);
        // First 4 in registers
        assert_eq!(slots[3], ArgSlot::Reg(R9));
        // 5th and 6th on stack
        assert_eq!(slots[4], ArgSlot::Stack { offset: 32 });
        assert_eq!(slots[5], ArgSlot::Stack { offset: 40 });
    }

    // ============================================================================
    // Return value mapping tests (2 tests)
    // ============================================================================

    #[test]
    fn map_return_integer_in_rax_both_ccs() {
        let sysv_return = map_return(ArgClass::Integer, CallingConvention::Sysv);
        let ms_return = map_return(ArgClass::Integer, CallingConvention::Ms);

        assert_eq!(sysv_return, ReturnSlot::Reg(RAX));
        assert_eq!(ms_return, ReturnSlot::Reg(RAX));
    }

    #[test]
    fn map_return_ms_matches_sysv_for_integer() {
        // Parity test: both ABIs return integers in RAX.
        let integer_class = ArgClass::Integer;
        let sysv_slot = map_return(integer_class, CallingConvention::Sysv);
        let ms_slot = map_return(integer_class, CallingConvention::Ms);

        assert_eq!(sysv_slot, ms_slot);
        assert_eq!(sysv_slot, ReturnSlot::Reg(RAX));
    }

    // ============================================================================
    // Constants and shape regression tests (4 tests)
    // ============================================================================

    #[test]
    fn ms_arg_regs_ordering_is_rcx_rdx_r8_r9() {
        assert_eq!(MS_ARG_REGS, [RCX, RDX, R8, R9]);
        assert_eq!(MS_ARG_REGS.len(), 4);
        assert_eq!(MS_ARG_REGS[0], RCX);
        assert_eq!(MS_ARG_REGS[1], RDX);
        assert_eq!(MS_ARG_REGS[2], R8);
        assert_eq!(MS_ARG_REGS[3], R9);
    }

    #[test]
    fn sysv_arg_regs_ordering_unchanged() {
        // Regression fence: ARG_REGS must remain in SysV order.
        assert_eq!(ARG_REGS, [RDI, RSI, RDX, RCX, R8, R9]);
        assert_eq!(ARG_REGS.len(), 6);
    }

    #[test]
    fn ms_shadow_space_is_thirty_two_bytes() {
        assert_eq!(MS_SHADOW_SPACE_BYTES, 32);
    }

    #[test]
    fn ms_and_sysv_arg_reg_pools_disjoint_except_rdx_rcx_r8() {
        // Document the register overlap between MS and SysV arg pools.
        // MS: [RCX, RDX, R8, R9]
        // SysV: [RDI, RSI, RDX, RCX, R8, R9]
        // Overlap: RCX, RDX, R8, R9 (all MS regs are also SysV regs)
        // Disjoint: RDI, RSI are SysV-only.

        let ms_set: std::collections::HashSet<_> = MS_ARG_REGS.iter().copied().collect();
        let sysv_set: std::collections::HashSet<_> = ARG_REGS.iter().copied().collect();

        for &reg in &ms_set {
            assert!(sysv_set.contains(&reg), "MS register {:?} not in SysV pool", reg);
        }

        let sysv_only: std::collections::HashSet<_> =
            sysv_set.difference(&ms_set).copied().collect();
        assert_eq!(sysv_only.len(), 2, "Expected exactly 2 SysV-only registers");
        assert!(sysv_only.contains(&RDI), "RDI should be SysV-only");
        assert!(sysv_only.contains(&RSI), "RSI should be SysV-only");
    }
}
