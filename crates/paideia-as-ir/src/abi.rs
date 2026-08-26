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
//! DONE(#1008 / paideia-os #1333, paideia-as#1333): `ArgClass::Float` landed
//! for SysV (independent int/float register counters, per §3.2.3). MS x64's
//! *unified*-bank slot advancement (single register-index counter spanning
//! both classes) remains a follow-up — see `map_args`'s MS x64 note.
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

/// XMM0 (compact `RegId` 53 — see `RegId` doc comment).
pub const XMM0: RegId = RegId(53);
/// XMM1.
pub const XMM1: RegId = RegId(54);
/// XMM2.
pub const XMM2: RegId = RegId(55);
/// XMM3.
pub const XMM3: RegId = RegId(56);
/// XMM4.
pub const XMM4: RegId = RegId(57);
/// XMM5.
pub const XMM5: RegId = RegId(58);
/// XMM6.
pub const XMM6: RegId = RegId(59);
/// XMM7.
pub const XMM7: RegId = RegId(60);

/// SysV float/double arg registers in call order (paideia-os #1333,
/// paideia-as#1333). Per SysV AMD64 ABI §3.2.3: the first 8 `SSE` class
/// arguments (float/double) go in XMM0–XMM7, independent of the integer
/// arg-register count/index (float and integer args advance separate
/// counters). The 9th+ float argument spills to the stack (not yet
/// implemented in this emitter, matching `ARG_REGS`'s integer 7th+ gap).
pub const XMM_ARG_REGS: [RegId; 8] = [XMM0, XMM1, XMM2, XMM3, XMM4, XMM5, XMM6, XMM7];

/// SysV float/double return register (paideia-os #1333, paideia-as#1333).
/// Per SysV AMD64 ABI §3.2.3: a scalar `SSE`-class return value comes back
/// in XMM0 (both ABIs; MS x64 also returns floats in XMM0).
pub const XMM_RET: RegId = XMM0;

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

/// #1192: Padding added to MS shadow-space bump when the scratch-save count
/// is odd, restoring RSP ≡ 0 mod 16 at CALL as required by MS ABI.
/// When scratch_save_set.len() % 2 == 1, use MS_CALL_STACK_BUMP + MS_CALL_STACK_BUMP_ODD_PAD
/// instead of MS_CALL_STACK_BUMP alone.
pub const MS_CALL_STACK_BUMP_ODD_PAD: u32 = 8;

/// #1195: pad added to paideia→SysV cross-ABI call sequence when the
/// scratch-save count is even, restoring RSP ≡ 0 mod 16 at CALL as required
/// by SysV ABI. bridge_saves contributes 2 pushes for cross-ABI calls;
/// combined with entry RSP ≡ 8 mod 16, even scratch counts leave RSP = 8
/// mod 16 at CALL (misaligned). Adding 8 shifts alignment back to 0 mod 16.
/// N odd cases are already aligned by parity — no bump needed.
pub const SYSV_CALL_ALIGN_PAD: u32 = 8;

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

/// Registers saved by the caller-side bridge when crossing paideia ↔ MS/SysV
/// ABI boundaries. Per calling-convention.md §11.5: R15 (env) + R14 (effect).
/// MS ABI already preserves R12-R15 so paideia doesn't need to double-save
/// against MS callee clobber; the save is against a hypothetical bug and
/// against future paideia semantic-tag additions.
///
/// LIFO ordering: push R15 first, then R14; pop R14 first, then R15.
pub const PAIDEIA_BRIDGE_SAVE: [RegId; 2] = [R15, R14];

/// Determine which registers the caller must save/restore when crossing
/// an ABI boundary. Returns:
/// - `&PAIDEIA_BRIDGE_SAVE` if caller is paideia (None) and callee is MS or explicit SysV
/// - `&[]` (empty slice) for all other cross-ABI or intra-ABI cases
///
/// The caller is responsible for saving/restoring these registers via
/// inline push/pop bookends around the call sequence.
#[must_use]
pub fn bridge_save_set(
    caller_abi: Option<CallingConvention>,
    callee_abi: Option<CallingConvention>,
) -> &'static [RegId] {
    match (caller_abi, callee_abi) {
        // Paideia (None) calling MS: save R15, R14
        (None, Some(CallingConvention::Ms)) => &PAIDEIA_BRIDGE_SAVE,
        // Paideia (None) calling explicit SysV: save R15, R14
        (None, Some(CallingConvention::Sysv)) => &PAIDEIA_BRIDGE_SAVE,
        // All other cases: no bridge save
        _ => &[],
    }
}

/// Classification of a function argument for ABI mapping purposes.
///
/// An argument's class determines which register or stack slot it occupies
/// during a function call. Integer and Float are implemented; future phases
/// will add Vector and Aggregate classes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum ArgClass {
    /// Integer or pointer argument (64-bit or narrower, zero/sign-extended
    /// at call site). Occupies one slot in the class-specific register pool.
    Integer,
    /// Scalar float/double argument (`f32`/`f64`) — paideia-os #1333,
    /// paideia-as#1333. Occupies one slot in the XMM register pool,
    /// advancing independently of the Integer-class counter (SysV AMD64
    /// ABI §3.2.3: float and integer args are classified and counted
    /// separately, so `f(i64, f64, i64)` maps to `RDI, XMM0, RSI`, not
    /// `RDI, XMM0, RDX`).
    Float,
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
///
/// # Float class (paideia-os #1333, paideia-as#1333):
/// `Float`-class arguments draw from `XMM_ARG_REGS`/`MS_ARG_REGS`' float
/// counterpart using an independent counter from `Integer`-class arguments
/// (SysV AMD64 ABI §3.2.3). Stack-spilled arguments (of either class) still
/// consume stack slots in original left-to-right argument order.
///
/// MS x64 note: the real MS x64 ABI shares a *single* unified register-index
/// counter across int/float args (`arg[i]` always uses the i-th slot of
/// whichever register bank matches its class, e.g. `f(i64, f64)` → RCX,
/// XMM1 — not XMM0). This function does not yet model that unified-index
/// rule for MS; MS float marshalling beyond 1-2 args is a follow-up.
#[must_use]
pub fn map_args(classes: &[ArgClass], cc: CallingConvention) -> Vec<ArgSlot> {
    match cc {
        CallingConvention::Sysv => {
            let mut int_used = 0usize;
            let mut float_used = 0usize;
            let mut stack_idx = 0usize;
            classes
                .iter()
                .map(|class| match class {
                    ArgClass::Integer => {
                        if int_used < ARG_REGS.len() {
                            let slot = ArgSlot::Reg(ARG_REGS[int_used]);
                            int_used += 1;
                            slot
                        } else {
                            let slot = ArgSlot::Stack { offset: (stack_idx as u32) * 8 };
                            stack_idx += 1;
                            slot
                        }
                    }
                    ArgClass::Float => {
                        if float_used < XMM_ARG_REGS.len() {
                            let slot = ArgSlot::Reg(XMM_ARG_REGS[float_used]);
                            float_used += 1;
                            slot
                        } else {
                            let slot = ArgSlot::Stack { offset: (stack_idx as u32) * 8 };
                            stack_idx += 1;
                            slot
                        }
                    }
                })
                .collect()
        }
        CallingConvention::Ms => {
            classes
                .iter()
                .enumerate()
                .map(|(i, class)| {
                    let reg_pool_len = MS_ARG_REGS.len();
                    if i < reg_pool_len {
                        match class {
                            ArgClass::Integer => ArgSlot::Reg(MS_ARG_REGS[i]),
                            ArgClass::Float => ArgSlot::Reg(XMM_ARG_REGS[i]),
                        }
                    } else {
                        ArgSlot::Stack {
                            offset: ((i - reg_pool_len) as u32) * 8 + MS_SHADOW_SPACE_BYTES,
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
/// - Float class returns in XMM0 (paideia-os #1333, paideia-as#1333; SysV
///   AMD64 ABI §3.2.3 and MS x64 both use XMM0 for a scalar float/double
///   return value).
#[must_use]
pub fn map_return(class: ArgClass, _cc: CallingConvention) -> ReturnSlot {
    match class {
        ArgClass::Integer => ReturnSlot::Reg(RAX),
        ArgClass::Float => ReturnSlot::Reg(XMM_RET),
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

    // ============================================================================
    // Float-class ABI mapping tests (paideia-os #1333, paideia-as#1333)
    // ============================================================================

    #[test]
    fn xmm_arg_regs_ordering_is_xmm0_through_xmm7() {
        assert_eq!(
            XMM_ARG_REGS,
            [XMM0, XMM1, XMM2, XMM3, XMM4, XMM5, XMM6, XMM7]
        );
        assert_eq!(XMM0.0, 53);
        assert_eq!(XMM7.0, 60);
    }

    #[test]
    fn map_args_sysv_one_float_in_xmm0() {
        let classes = [ArgClass::Float];
        let slots = map_args(&classes, CallingConvention::Sysv);
        assert_eq!(slots, [ArgSlot::Reg(XMM0)]);
    }

    #[test]
    fn map_args_sysv_int_and_float_counters_are_independent() {
        // f(i64, f64, i64) -> RDI, XMM0, RSI — the float arg does NOT
        // consume an integer register slot.
        let classes = [ArgClass::Integer, ArgClass::Float, ArgClass::Integer];
        let slots = map_args(&classes, CallingConvention::Sysv);
        assert_eq!(slots, [ArgSlot::Reg(RDI), ArgSlot::Reg(XMM0), ArgSlot::Reg(RSI)]);
    }

    #[test]
    fn map_args_sysv_ninth_float_spills_to_stack() {
        let classes = [ArgClass::Float; 9];
        let slots = map_args(&classes, CallingConvention::Sysv);
        assert_eq!(slots.len(), 9);
        assert_eq!(slots[7], ArgSlot::Reg(XMM7));
        assert_eq!(slots[8], ArgSlot::Stack { offset: 0 });
    }

    #[test]
    fn map_args_sysv_mixed_stack_spill_shares_offset_counter() {
        // 7 integers (6 in regs, 1 on stack) + 1 float that also spills
        // (all 8 XMM regs already busy from a prior call shape isn't modeled
        // here; this test spills the float because 9 floats already filled
        // XMM0-7 - instead we directly check that Integer and Float spills
        // share one running stack-offset counter in argument order).
        let classes = [
            ArgClass::Integer, ArgClass::Integer, ArgClass::Integer, ArgClass::Integer,
            ArgClass::Integer, ArgClass::Integer, ArgClass::Integer, // 7th int spills
            ArgClass::Float, ArgClass::Float, ArgClass::Float, ArgClass::Float,
            ArgClass::Float, ArgClass::Float, ArgClass::Float, ArgClass::Float, // 8 floats fill XMM0-7
            ArgClass::Float, // 9th float spills
        ];
        let slots = map_args(&classes, CallingConvention::Sysv);
        assert_eq!(slots[6], ArgSlot::Stack { offset: 0 }); // 7th int
        assert_eq!(slots[15], ArgSlot::Stack { offset: 8 }); // 9th float, after the int's stack slot
    }

    #[test]
    fn map_return_float_in_xmm0_both_ccs() {
        let sysv_return = map_return(ArgClass::Float, CallingConvention::Sysv);
        let ms_return = map_return(ArgClass::Float, CallingConvention::Ms);
        assert_eq!(sysv_return, ReturnSlot::Reg(XMM0));
        assert_eq!(ms_return, ReturnSlot::Reg(XMM0));
    }

    #[test]
    fn map_args_ms_first_float_in_xmm0() {
        let classes = [ArgClass::Float];
        let slots = map_args(&classes, CallingConvention::Ms);
        assert_eq!(slots, [ArgSlot::Reg(XMM0)]);
    }
}
