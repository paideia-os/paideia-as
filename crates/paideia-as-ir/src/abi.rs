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
//! DONE(PAS-DEBT-B3-007 Slice 1 / paideia-as#1520): SysV aggregate classifier
//! (`AggregateClass` + `classify_sysv_aggregate`) per SysV AMD64 psABI §3.2.3.
//! Foundation for the two return-value slices below.
//! DONE(PAS-DEBT-B3-007b / paideia-as#1543): MS x64 aggregate return
//! placement selector — `MsAggregateClass` + `classify_ms_aggregate` +
//! `MsReturnPlacement` + `ms_return_placement`. Aggregates fitting in a
//! single register (size ∈ {1,2,4,8}, or scalar float/double →
//! `Register(Xmm)`) go through RAX/XMM0; anything else uses a hidden RCX
//! sret pointer with the callee mirroring it back through RAX. The
//! instruction-sequence synthesiser lives alongside the SysV one in
//! `paideia_as_elaborator::aggregate_return` (`ms_caller_sret_prelude`,
//! `ms_caller_read_return_reg`, `ms_callee_load_return_reg`,
//! `ms_callee_sret_store`). Call-site wiring gates on the same upstream
//! return-record-layout side-table the SysV path waits on.
//! DONE(PAS-DEBT-B3-007c / paideia-as#1544): SysV aggregate return placement
//! selector — `SysvReturnPlacement` + `sysv_return_placement` consume a
//! classifier vector and resolve the register-pair (RAX / RDX / XMM0 / XMM1)
//! or Memory-sret shape per SysV AMD64 psABI §3.2.3. The instruction-sequence
//! synthesiser lives in `paideia_as_elaborator::aggregate_return` (byte-exact
//! epilogue/prelude helpers). End-to-end wiring at call sites gates on an
//! upstream return-record-layout side-table (tracked separately).

use crate::instruction::RegId;
use crate::let_meta::CallingConvention;
use crate::record_layout::RecordLayout;

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

/// SysV AMD64 psABI §3.2.3 aggregate-eightbyte class.
///
/// An aggregate value ≤ 16 bytes is split into up to two 8-byte "eightbytes",
/// each independently classified into one of the classes below. The vector of
/// per-eightbyte classes then drives register vs stack placement:
///
/// - `Integer`: eightbyte is passed/returned in a GPR (RDI-R9 for args; RAX,
///   or RDX:RAX for a 2-eightbyte pair, for returns).
/// - `SSE`: pure-float eightbyte, passed/returned in an XMM register.
/// - `Memory`: whole aggregate is passed on the stack / returned via a
///   caller-provided sret buffer.
/// - `ComplexX87`: x87 long-double + imaginary long-double (2 × 80-bit).
///   Emitted but never produced by the current classifier (paideia has no
///   `long double`); reserved for the psABI vocabulary.
/// - `Nothing`: no field lives in this eightbyte (padding / trailing tail).
///   Only appears as an intermediate value during merge; a real classifier
///   output always resolves to Integer/SSE/Memory (or is empty when the
///   whole aggregate is zero-sized).
///
/// Reference: System V Application Binary Interface, AMD64 Architecture
/// Processor Supplement, Draft Version 1.0 — §3.2.3 "Parameter Passing".
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum AggregateClass {
    /// No field occupies this eightbyte (intermediate merge state).
    Nothing,
    /// Passed/returned in a GPR.
    Integer,
    /// Pure-float eightbyte, passed/returned in an XMM register.
    SSE,
    /// x87 80-bit long-double eightbyte pair (reserved; unused today).
    ComplexX87,
    /// Passed/returned via memory (aggregate > 16 bytes or contains
    /// unaligned/misclassifiable fields).
    Memory,
}

/// Classify a `RecordLayout` into its per-eightbyte SysV classes.
///
/// Implements the SysV AMD64 psABI §3.2.3 classification algorithm for
/// aggregates and their per-eightbyte return-value placement. Consumers of
/// this vector (planned in PAS-DEBT-B3-007b/c) select the return register
/// bank per eightbyte: `Integer` → RAX/RDX, `SSE` → XMM0/XMM1, `Memory` →
/// caller-allocated sret buffer.
///
/// Algorithm:
///
/// 1. If `layout.size > 16` → whole aggregate is `[Memory]`.
/// 2. Otherwise split into `ceil(size/8)` eightbytes (max 2 today), each
///    initialised to `Nothing`.
/// 3. For each field, locate its eightbyte(s) by `offset / 8` and
///    `(offset + size - 1) / 8`. If those two indices differ (a field
///    straddles an eightbyte boundary), the whole aggregate degrades to
///    `[Memory]` (psABI §3.2.3 clause 5.a — misaligned fields force
///    memory).
/// 4. Otherwise merge the field's class into that eightbyte using the
///    psABI §3.2.3 merge rule:
///    - `X + X = X`
///    - `Nothing + X = X`
///    - `Memory + anything = Memory`
///    - `Integer + anything (non-Memory) = Integer` (INTEGER wins over SSE)
///    - `SSE + SSE = SSE`
/// 5. If any eightbyte ended up `Memory`, degrade the whole result to
///    `[Memory]` (post-merge cleanup, psABI §3.2.3 clause 5.c).
///
/// Zero-sized aggregates return an empty vector.
///
/// Reference: System V Application Binary Interface, AMD64 Architecture
/// Processor Supplement, §3.2.3 "Parameter Passing", classification
/// algorithm and merge rule.
#[must_use]
pub fn classify_sysv_aggregate(layout: &RecordLayout) -> Vec<AggregateClass> {
    // §3.2.3 clause 5.a first bullet: aggregates > 16 bytes → MEMORY.
    if layout.size > 16 {
        return vec![AggregateClass::Memory];
    }
    if layout.size == 0 {
        return Vec::new();
    }

    let eightbyte_count = ((layout.size + 7) / 8) as usize;
    let mut classes = vec![AggregateClass::Nothing; eightbyte_count];

    for field in &layout.fields {
        if field.size == 0 {
            continue;
        }
        let lo = (field.offset / 8) as usize;
        let hi = ((field.offset + field.size as u64 - 1) / 8) as usize;
        if lo >= eightbyte_count || hi >= eightbyte_count {
            // Field lies outside declared aggregate extent — treat as
            // unaligned per §3.2.3 clause 5.a.
            return vec![AggregateClass::Memory];
        }
        if lo != hi {
            // §3.2.3 clause 5.a: field straddles an eightbyte boundary → MEMORY.
            return vec![AggregateClass::Memory];
        }
        let field_class = if field.is_float {
            AggregateClass::SSE
        } else {
            AggregateClass::Integer
        };
        classes[lo] = merge_classes(classes[lo], field_class);
    }

    // §3.2.3 clause 5.c: any MEMORY eightbyte demotes the whole aggregate.
    if classes.iter().any(|c| matches!(c, AggregateClass::Memory)) {
        return vec![AggregateClass::Memory];
    }

    // Eightbytes that stayed `Nothing` (trailing tail padding) collapse away.
    // The psABI treats those as absent from the return placement schedule.
    classes.retain(|c| !matches!(c, AggregateClass::Nothing));
    classes
}

/// Resolved SysV return-value placement for an aggregate.
///
/// Consumes the `Vec<AggregateClass>` produced by
/// [`classify_sysv_aggregate`] and reduces it to one of the six register
/// shapes the SysV AMD64 psABI §3.2.3 defines for return placement,
/// plus the Memory-sret shape and a `None` sentinel for zero-sized
/// aggregates. Downstream emitters branch on this enum rather than
/// re-classifying at each call site.
///
/// PAS-DEBT-B3-007c / paideia-as#1544.
///
/// # Register-pair conventions
/// - `IntSingle` — one eightbyte in `RAX`.
/// - `SseSingle` — one eightbyte in `XMM0`.
/// - `IntPair` — low eightbyte in `RAX`, high in `RDX`.
/// - `IntSse` — low in `RAX`, high in `XMM0`.
/// - `SseInt` — low in `XMM0`, high in `RAX`.
/// - `SsePair` — low in `XMM0`, high in `XMM1`.
/// - `Memory` — caller allocates a buffer, passes its address in `RDI`
///   as an implicit first argument (shifting the real args right by
///   one), and the callee must also return that pointer in `RAX`.
/// - `None` — void / zero-sized aggregate return; nothing to place.
///
/// The SSE placement rules follow psABI §3.2.3 Table 3.4 ("Merging the
/// classified registers"): the first `SSE` eightbyte uses `XMM0`, the
/// second `SSE` eightbyte uses `XMM1` — even in the mixed `[Integer,
/// SSE]` case where the low half is INTEGER (in `RAX`) and the high
/// half is SSE (still `XMM0`, per the psABI rule that XMM slots advance
/// independently of INTEGER slots).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum SysvReturnPlacement {
    /// Void / zero-size aggregate return. Callee touches nothing.
    None,
    /// Single INTEGER eightbyte in `RAX`.
    IntSingle,
    /// Single SSE eightbyte in `XMM0`.
    SseSingle,
    /// `[Integer, Integer]` → low eightbyte in `RAX`, high in `RDX`.
    IntPair,
    /// `[Integer, SSE]` → low eightbyte in `RAX`, high in `XMM0`.
    IntSse,
    /// `[SSE, Integer]` → low eightbyte in `XMM0`, high in `RAX`.
    SseInt,
    /// `[SSE, SSE]` → low eightbyte in `XMM0`, high in `XMM1`.
    SsePair,
    /// Aggregate returned via caller-provided sret buffer. Caller passes
    /// the buffer address in `RDI` as an implicit first arg; callee
    /// copies the aggregate into `[RDI]` and returns the same pointer in
    /// `RAX` per psABI §3.2.3.
    Memory,
}

/// Reduce a `classify_sysv_aggregate` output into a
/// [`SysvReturnPlacement`].
///
/// Handles the full range of shapes the classifier can produce today:
/// `[]` (zero-size), `[Integer]`, `[SSE]`, `[Integer, Integer]`,
/// `[Integer, SSE]`, `[SSE, Integer]`, `[SSE, SSE]`, `[Memory]`. The
/// psABI-reserved `ComplexX87` class (never produced by the current
/// classifier — paideia has no `long double`) degrades to `Memory` as a
/// safe conservative choice; the encoder cannot emit an x87 return pair
/// either way.
///
/// Panics only on internally-inconsistent input (a length > 2 without
/// the whole aggregate being `[Memory]`), which the classifier
/// guarantees is unreachable — `classify_sysv_aggregate` caps eightbyte
/// count at 2 for non-Memory aggregates (size ≤ 16).
///
/// PAS-DEBT-B3-007c / paideia-as#1544.
#[must_use]
pub fn sysv_return_placement(classes: &[AggregateClass]) -> SysvReturnPlacement {
    use AggregateClass::*;
    match classes {
        [] => SysvReturnPlacement::None,
        [Memory] => SysvReturnPlacement::Memory,
        [Integer] => SysvReturnPlacement::IntSingle,
        [SSE] => SysvReturnPlacement::SseSingle,
        [Integer, Integer] => SysvReturnPlacement::IntPair,
        [Integer, SSE] => SysvReturnPlacement::IntSse,
        [SSE, Integer] => SysvReturnPlacement::SseInt,
        [SSE, SSE] => SysvReturnPlacement::SsePair,
        // Nothing / ComplexX87 in the reduced output degrades to Memory
        // (conservative — encoder can't emit an x87 return pair, and
        // classify_sysv_aggregate is expected to have already stripped
        // trailing `Nothing`). Any other multi-element shape is an
        // internal inconsistency; treating it as Memory keeps the ABI
        // safe (worst case: an unnecessary sret round-trip).
        _ => SysvReturnPlacement::Memory,
    }
}

impl SysvReturnPlacement {
    /// True when this placement demands the caller to pass a hidden sret
    /// pointer in `RDI` (shifting real args to `RSI, RDX, RCX, R8, R9`).
    ///
    /// This is the single question a caller-side arg-marshalling loop
    /// asks before mapping arguments to registers.
    #[must_use]
    pub fn needs_hidden_sret(self) -> bool {
        matches!(self, SysvReturnPlacement::Memory)
    }

    /// True when at least one return eightbyte uses an XMM register
    /// (`XMM0` or `XMM1`). Emitters use this to decide whether to
    /// reserve/save an XMM before overwriting it.
    #[must_use]
    pub fn uses_xmm(self) -> bool {
        matches!(
            self,
            SysvReturnPlacement::SseSingle
                | SysvReturnPlacement::IntSse
                | SysvReturnPlacement::SseInt
                | SysvReturnPlacement::SsePair
        )
    }
}

// ============================================================================
// MS x64 aggregate classification (PAS-DEBT-B3-007b / paideia-as#1543)
// ============================================================================

/// MS x64 aggregate return-value register class.
///
/// Unlike SysV, MS x64 does **not** split aggregates across two return
/// registers: an aggregate either fits in a single register (size ∈
/// {1, 2, 4, 8}) and returns through that register, or it goes through
/// a caller-allocated sret buffer whose pointer is passed as an
/// implicit first argument in `RCX` (shifting the real args right by
/// one to `RDX`, `R8`, `R9`, then stack past the 32-byte shadow space).
///
/// The `Register` variant carries an [`MsRegClass`] that names which
/// register bank the value uses (`INTEGER → RAX`, `XMM → XMM0`). The
/// two-variant shape mirrors the binary caller-side decision (pass an
/// sret pointer in RCX or not?) — the register-class distinction is a
/// second, orthogonal decision consumed by the callee-side load helper.
///
/// PAS-DEBT-B3-007b / paideia-as#1543.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum MsAggregateClass {
    /// Aggregate fits in a single return register.
    Register(MsRegClass),
    /// Aggregate uses caller-allocated sret buffer (hidden `RCX`
    /// pointer at CALL time; callee mirrors that pointer back through
    /// `RAX` on RET per MS x64 ABI).
    Memory,
}

/// Which register bank a `Register`-classified MS aggregate uses.
///
/// PAS-DEBT-B3-007b / paideia-as#1543.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum MsRegClass {
    /// GPR: `RAX` for a register-sized aggregate return value.
    Int,
    /// XMM: `XMM0` for a scalar float/double return value.
    Xmm,
}

/// Classify a `RecordLayout` for MS x64 return placement.
///
/// Rules per Microsoft x64 ABI:
///
/// 1. A single-field aggregate whose sole field is a scalar
///    float/double (`is_float=true`, size ∈ {4, 8}, no tail padding)
///    is `Register(Xmm)` — returned in `XMM0`. Mirrors the plain
///    scalar-float-return path (a bare `f32`/`f64` returns in `XMM0`
///    on both ABIs; MS treats a single-float wrapper struct
///    equivalently for the SSE-return decision).
/// 2. Any other aggregate whose total size is 1, 2, 4, or 8 bytes is
///    `Register(Int)` — returned in `RAX`. This includes multi-field
///    aggregates (e.g. `{ u32 a; u32 b }`) and mixed-scalar aggregates
///    (e.g. `{ f32 a; f32 b }`), which MS returns in `RAX` even though
///    both fields are float — the MS ABI's `XMM0` slot is reserved for
///    a *single* scalar float/double, not for aggregates.
/// 3. All other sizes (0, 3, 5, 6, 7, or > 8) are `Memory` — returned
///    through the hidden `RCX` sret buffer.
///
/// Zero-size aggregates classify as `Memory` here as a benign default;
/// callers that need to distinguish void returns from real
/// zero-size-aggregate returns should use
/// [`ms_return_placement_from_layout`], which maps `size == 0` to
/// [`MsReturnPlacement::None`].
///
/// Note: MS x64 differs from SysV in that it does **not** split
/// aggregates across two return registers. There is no `RegisterPair`
/// class here.
///
/// PAS-DEBT-B3-007b / paideia-as#1543.
#[must_use]
pub fn classify_ms_aggregate(layout: &RecordLayout) -> MsAggregateClass {
    // Special case: single scalar float/double field → XMM0. Only
    // fires when the field completely fills the aggregate (no padding,
    // no siblings) so a tagged wrapper doesn't accidentally get an SSE
    // return where MS ABI expects RAX.
    if layout.fields.len() == 1
        && layout.fields[0].is_float
        && matches!(layout.size, 4 | 8)
        && layout.fields[0].size as u64 == layout.size
        && layout.fields[0].offset == 0
    {
        return MsAggregateClass::Register(MsRegClass::Xmm);
    }
    match layout.size {
        1 | 2 | 4 | 8 => MsAggregateClass::Register(MsRegClass::Int),
        _ => MsAggregateClass::Memory,
    }
}

/// Resolved MS x64 return-value placement for an aggregate.
///
/// Mirrors [`SysvReturnPlacement`] in role — a small enum consumed by
/// call-site emitters — but has fewer variants because MS x64 never
/// splits aggregates across two return registers.
///
/// * `None` — void / zero-size aggregate return; callee touches
///   nothing.
/// * `IntSingle` — `RAX` (all register-sized aggregates and scalar
///   integers).
/// * `XmmSingle` — `XMM0` (scalar float/double aggregate and bare
///   float/double return).
/// * `Memory` — hidden `RCX` sret pointer; callee copies the aggregate
///   into `[RCX]` and returns that same pointer in `RAX`.
///
/// PAS-DEBT-B3-007b / paideia-as#1543.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum MsReturnPlacement {
    /// Void / zero-size aggregate return. Callee touches nothing.
    None,
    /// Single INTEGER-class return in `RAX`.
    IntSingle,
    /// Single SSE-class return in `XMM0`.
    XmmSingle,
    /// Aggregate returned via caller-provided sret buffer whose pointer
    /// is passed as an implicit first argument in `RCX`; callee copies
    /// the aggregate into `[RCX]` and returns the same pointer in
    /// `RAX` per MS x64 ABI.
    Memory,
}

/// Reduce an [`MsAggregateClass`] into an [`MsReturnPlacement`].
///
/// Does not model the zero-size aggregate case; use
/// [`ms_return_placement_from_layout`] for the layout-driven entry
/// point that maps `size == 0` to [`MsReturnPlacement::None`].
///
/// PAS-DEBT-B3-007b / paideia-as#1543.
#[must_use]
pub fn ms_return_placement(class: MsAggregateClass) -> MsReturnPlacement {
    match class {
        MsAggregateClass::Register(MsRegClass::Int) => MsReturnPlacement::IntSingle,
        MsAggregateClass::Register(MsRegClass::Xmm) => MsReturnPlacement::XmmSingle,
        MsAggregateClass::Memory => MsReturnPlacement::Memory,
    }
}

/// Layout-driven entry point that composes [`classify_ms_aggregate`]
/// with [`ms_return_placement`] and handles the zero-size aggregate
/// case (→ [`MsReturnPlacement::None`]).
///
/// This is the single-call helper an MS x64 emitter integration will
/// invoke at a call site or a callee epilogue once the upstream
/// return-record-layout side-table lands.
///
/// PAS-DEBT-B3-007b / paideia-as#1543.
#[must_use]
pub fn ms_return_placement_from_layout(layout: &RecordLayout) -> MsReturnPlacement {
    if layout.size == 0 {
        return MsReturnPlacement::None;
    }
    ms_return_placement(classify_ms_aggregate(layout))
}

impl MsReturnPlacement {
    /// True when the caller must pass a hidden sret pointer in `RCX`
    /// (shifting real args right by one to `RDX`, `R8`, `R9`, then
    /// stack past the 32-byte shadow space).
    #[must_use]
    pub fn needs_hidden_sret(self) -> bool {
        matches!(self, MsReturnPlacement::Memory)
    }

    /// True when the return path uses `XMM0` (i.e. the `XmmSingle`
    /// shape). Callers use this to decide whether to reserve/save
    /// `XMM0` before overwriting it.
    #[must_use]
    pub fn uses_xmm(self) -> bool {
        matches!(self, MsReturnPlacement::XmmSingle)
    }
}

/// Merge two class assignments for the same eightbyte per SysV §3.2.3.
///
/// The rule is asymmetric only between `Memory` (absorbs everything) and
/// `Integer` (absorbs everything non-Memory); other pairs are commutative.
fn merge_classes(a: AggregateClass, b: AggregateClass) -> AggregateClass {
    use AggregateClass::*;
    match (a, b) {
        (x, y) if x == y => x,
        (Nothing, x) | (x, Nothing) => x,
        (Memory, _) | (_, Memory) => Memory,
        (Integer, _) | (_, Integer) => Integer,
        // SSE + ComplexX87 is not producible from paideia types today
        // (no x87 long-double); fall through to Memory as a conservative
        // pin per psABI clause 5.d (X87UP without X87 → SSE, but any mix
        // of SSE with ComplexX87 fields is degraded to Memory here since
        // the encoder cannot emit an x87 return pair).
        _ => Memory,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record_layout::FieldLayout;

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

    // ============================================================================
    // SysV aggregate classifier (PAS-DEBT-B3-007 Slice 1 / paideia-as#1520)
    // ============================================================================

    /// `{ x: u64 }` → single INTEGER eightbyte.
    #[test]
    fn classify_sysv_single_u64_is_integer() {
        let layout = RecordLayout::new(
            8,
            8,
            vec![FieldLayout { offset: 0, size: 8, signed: false, is_float: false }],
        );
        assert_eq!(classify_sysv_aggregate(&layout), vec![AggregateClass::Integer]);
    }

    /// `{ x: u64, y: u64 }` → two INTEGER eightbytes (RDX:RAX return pair).
    #[test]
    fn classify_sysv_pair_u64_is_integer_integer() {
        let layout = RecordLayout::new(
            16,
            8,
            vec![
                FieldLayout { offset: 0, size: 8, signed: false, is_float: false },
                FieldLayout { offset: 8, size: 8, signed: false, is_float: false },
            ],
        );
        assert_eq!(
            classify_sysv_aggregate(&layout),
            vec![AggregateClass::Integer, AggregateClass::Integer]
        );
    }

    /// `{ x: f32, y: f32 }` → single SSE eightbyte (both floats fit in one).
    #[test]
    fn classify_sysv_two_f32_in_one_eightbyte_is_sse() {
        let layout = RecordLayout::new(
            8,
            4,
            vec![
                FieldLayout { offset: 0, size: 4, signed: false, is_float: true },
                FieldLayout { offset: 4, size: 4, signed: false, is_float: true },
            ],
        );
        assert_eq!(classify_sysv_aggregate(&layout), vec![AggregateClass::SSE]);
    }

    /// 24-byte aggregate → MEMORY (single-slot vec, whole aggregate on stack /
    /// via sret buffer).
    #[test]
    fn classify_sysv_over_sixteen_bytes_is_memory() {
        let layout = RecordLayout::new(
            24,
            8,
            vec![
                FieldLayout { offset: 0, size: 8, signed: false, is_float: false },
                FieldLayout { offset: 8, size: 8, signed: false, is_float: false },
                FieldLayout { offset: 16, size: 8, signed: false, is_float: false },
            ],
        );
        assert_eq!(classify_sysv_aggregate(&layout), vec![AggregateClass::Memory]);
    }

    /// Mixed int + float in the same eightbyte → INTEGER (INTEGER wins per
    /// psABI §3.2.3 merge rule).
    #[test]
    fn classify_sysv_int_float_in_same_eightbyte_integer_wins() {
        let layout = RecordLayout::new(
            8,
            4,
            vec![
                FieldLayout { offset: 0, size: 4, signed: false, is_float: false },
                FieldLayout { offset: 4, size: 4, signed: false, is_float: true },
            ],
        );
        assert_eq!(classify_sysv_aggregate(&layout), vec![AggregateClass::Integer]);
    }

    /// Field straddling an eightbyte boundary → MEMORY (§3.2.3 clause 5.a).
    /// Regression: a u64 field at odd offset 4 crosses the 0..8 / 8..16 line.
    #[test]
    fn classify_sysv_straddling_field_is_memory() {
        let layout = RecordLayout::new(
            16,
            1,
            vec![FieldLayout { offset: 4, size: 8, signed: false, is_float: false }],
        );
        assert_eq!(classify_sysv_aggregate(&layout), vec![AggregateClass::Memory]);
    }

    /// Zero-size aggregate → empty class vec (no eightbyte to place).
    #[test]
    fn classify_sysv_zero_size_returns_empty() {
        let layout = RecordLayout::new(0, 1, vec![]);
        assert_eq!(classify_sysv_aggregate(&layout), Vec::<AggregateClass>::new());
    }

    /// Boundary at exactly 16 bytes → still classified (not MEMORY).
    /// Two 8-byte integers give `[Integer, Integer]`.
    #[test]
    fn classify_sysv_exactly_sixteen_bytes_still_split() {
        let layout = RecordLayout::new(
            16,
            8,
            vec![
                FieldLayout { offset: 0, size: 8, signed: false, is_float: false },
                FieldLayout { offset: 8, size: 8, signed: false, is_float: false },
            ],
        );
        let classes = classify_sysv_aggregate(&layout);
        assert_eq!(classes.len(), 2);
        assert_eq!(classes[0], AggregateClass::Integer);
        assert_eq!(classes[1], AggregateClass::Integer);
    }

    /// Int in first eightbyte, float in second → `[Integer, SSE]` mixed return.
    /// This is the shape B3-007c will need for return-pair register selection.
    #[test]
    fn classify_sysv_int_then_float_yields_integer_sse() {
        let layout = RecordLayout::new(
            16,
            8,
            vec![
                FieldLayout { offset: 0, size: 8, signed: false, is_float: false },
                FieldLayout { offset: 8, size: 8, signed: false, is_float: true },
            ],
        );
        assert_eq!(
            classify_sysv_aggregate(&layout),
            vec![AggregateClass::Integer, AggregateClass::SSE]
        );
    }

    // ============================================================================
    // SysvReturnPlacement classifier consumer (PAS-DEBT-B3-007c / paideia-as#1544)
    // ============================================================================

    #[test]
    fn placement_zero_size_is_none() {
        assert_eq!(sysv_return_placement(&[]), SysvReturnPlacement::None);
    }

    #[test]
    fn placement_single_integer_is_rax() {
        assert_eq!(
            sysv_return_placement(&[AggregateClass::Integer]),
            SysvReturnPlacement::IntSingle
        );
    }

    #[test]
    fn placement_single_sse_is_xmm0() {
        assert_eq!(
            sysv_return_placement(&[AggregateClass::SSE]),
            SysvReturnPlacement::SseSingle
        );
    }

    #[test]
    fn placement_integer_pair_is_rdx_rax_pair() {
        // The canonical B3-007c shape: `{ u64 lo; u64 hi }` — low in RAX,
        // high in RDX per psABI §3.2.3.
        assert_eq!(
            sysv_return_placement(&[AggregateClass::Integer, AggregateClass::Integer]),
            SysvReturnPlacement::IntPair
        );
    }

    #[test]
    fn placement_integer_sse_is_rax_xmm0() {
        assert_eq!(
            sysv_return_placement(&[AggregateClass::Integer, AggregateClass::SSE]),
            SysvReturnPlacement::IntSse
        );
    }

    #[test]
    fn placement_sse_integer_is_xmm0_rax() {
        assert_eq!(
            sysv_return_placement(&[AggregateClass::SSE, AggregateClass::Integer]),
            SysvReturnPlacement::SseInt
        );
    }

    #[test]
    fn placement_sse_pair_is_xmm0_xmm1() {
        // `{ f64 x; f64 y }` — low in XMM0, high in XMM1 per psABI §3.2.3.
        assert_eq!(
            sysv_return_placement(&[AggregateClass::SSE, AggregateClass::SSE]),
            SysvReturnPlacement::SsePair
        );
    }

    #[test]
    fn placement_memory_flags_hidden_sret() {
        let placement = sysv_return_placement(&[AggregateClass::Memory]);
        assert_eq!(placement, SysvReturnPlacement::Memory);
        assert!(placement.needs_hidden_sret());
        assert!(!placement.uses_xmm());
    }

    #[test]
    fn placement_register_shapes_never_flag_hidden_sret() {
        for placement in [
            SysvReturnPlacement::None,
            SysvReturnPlacement::IntSingle,
            SysvReturnPlacement::SseSingle,
            SysvReturnPlacement::IntPair,
            SysvReturnPlacement::IntSse,
            SysvReturnPlacement::SseInt,
            SysvReturnPlacement::SsePair,
        ] {
            assert!(
                !placement.needs_hidden_sret(),
                "placement {:?} unexpectedly requested a hidden sret",
                placement
            );
        }
    }

    #[test]
    fn placement_uses_xmm_flags_every_sse_shape() {
        for placement in [
            SysvReturnPlacement::SseSingle,
            SysvReturnPlacement::IntSse,
            SysvReturnPlacement::SseInt,
            SysvReturnPlacement::SsePair,
        ] {
            assert!(placement.uses_xmm(), "placement {:?} missed uses_xmm", placement);
        }
        for placement in [
            SysvReturnPlacement::None,
            SysvReturnPlacement::IntSingle,
            SysvReturnPlacement::IntPair,
            SysvReturnPlacement::Memory,
        ] {
            assert!(!placement.uses_xmm(), "placement {:?} falsely uses_xmm", placement);
        }
    }

    /// End-to-end: run the classifier on `{ u64, f64 }` and then reduce it
    /// with `sysv_return_placement` — pins that the two functions compose
    /// so downstream emitters can chain them without an intermediate
    /// hand-massage of the class vector.
    #[test]
    fn placement_composes_with_classifier_int_then_float() {
        let layout = RecordLayout::new(
            16,
            8,
            vec![
                FieldLayout { offset: 0, size: 8, signed: false, is_float: false },
                FieldLayout { offset: 8, size: 8, signed: false, is_float: true },
            ],
        );
        let classes = classify_sysv_aggregate(&layout);
        assert_eq!(sysv_return_placement(&classes), SysvReturnPlacement::IntSse);
    }

    // ============================================================================
    // MS x64 aggregate classifier (PAS-DEBT-B3-007b / paideia-as#1543)
    // ============================================================================

    /// Single u64 field → RAX (register-sized integer aggregate).
    #[test]
    fn classify_ms_single_u64_is_register_int() {
        let layout = RecordLayout::new(
            8,
            8,
            vec![FieldLayout { offset: 0, size: 8, signed: false, is_float: false }],
        );
        assert_eq!(
            classify_ms_aggregate(&layout),
            MsAggregateClass::Register(MsRegClass::Int)
        );
    }

    /// Single f64 field → XMM0 (MS scalar-float return path).
    #[test]
    fn classify_ms_single_f64_is_register_xmm() {
        let layout = RecordLayout::new(
            8,
            8,
            vec![FieldLayout { offset: 0, size: 8, signed: false, is_float: true }],
        );
        assert_eq!(
            classify_ms_aggregate(&layout),
            MsAggregateClass::Register(MsRegClass::Xmm)
        );
    }

    /// Single f32 field (size 4, fills the aggregate) → XMM0.
    #[test]
    fn classify_ms_single_f32_is_register_xmm() {
        let layout = RecordLayout::new(
            4,
            4,
            vec![FieldLayout { offset: 0, size: 4, signed: false, is_float: true }],
        );
        assert_eq!(
            classify_ms_aggregate(&layout),
            MsAggregateClass::Register(MsRegClass::Xmm)
        );
    }

    /// `{ u32 a; u32 b }` — 8 bytes with two integer fields → RAX.
    #[test]
    fn classify_ms_pair_u32_is_register_int() {
        let layout = RecordLayout::new(
            8,
            4,
            vec![
                FieldLayout { offset: 0, size: 4, signed: false, is_float: false },
                FieldLayout { offset: 4, size: 4, signed: false, is_float: false },
            ],
        );
        assert_eq!(
            classify_ms_aggregate(&layout),
            MsAggregateClass::Register(MsRegClass::Int)
        );
    }

    /// `{ f32 a; f32 b }` — 8 bytes with two float fields → RAX
    /// (MS returns aggregate float pairs in RAX; only single-scalar
    /// float aggregates go to XMM0).
    #[test]
    fn classify_ms_two_f32_in_one_reg_is_register_int() {
        let layout = RecordLayout::new(
            8,
            4,
            vec![
                FieldLayout { offset: 0, size: 4, signed: false, is_float: true },
                FieldLayout { offset: 4, size: 4, signed: false, is_float: true },
            ],
        );
        assert_eq!(
            classify_ms_aggregate(&layout),
            MsAggregateClass::Register(MsRegClass::Int)
        );
    }

    /// Non-power-of-2 aggregate size (3 bytes) → Memory (sret via RCX).
    #[test]
    fn classify_ms_size_3_is_memory() {
        let layout = RecordLayout::new(
            3,
            1,
            vec![
                FieldLayout { offset: 0, size: 1, signed: false, is_float: false },
                FieldLayout { offset: 1, size: 1, signed: false, is_float: false },
                FieldLayout { offset: 2, size: 1, signed: false, is_float: false },
            ],
        );
        assert_eq!(classify_ms_aggregate(&layout), MsAggregateClass::Memory);
    }

    /// 16-byte aggregate → Memory. Unlike SysV, MS does not split into
    /// a register pair at 16 bytes — anything over 8 goes through sret.
    #[test]
    fn classify_ms_size_16_is_memory() {
        let layout = RecordLayout::new(
            16,
            8,
            vec![
                FieldLayout { offset: 0, size: 8, signed: false, is_float: false },
                FieldLayout { offset: 8, size: 8, signed: false, is_float: false },
            ],
        );
        assert_eq!(classify_ms_aggregate(&layout), MsAggregateClass::Memory);
    }

    /// 24-byte aggregate → Memory (mirror of SysV's oversized case).
    #[test]
    fn classify_ms_size_24_is_memory() {
        let layout = RecordLayout::new(
            24,
            8,
            vec![
                FieldLayout { offset: 0, size: 8, signed: false, is_float: false },
                FieldLayout { offset: 8, size: 8, signed: false, is_float: false },
                FieldLayout { offset: 16, size: 8, signed: false, is_float: false },
            ],
        );
        assert_eq!(classify_ms_aggregate(&layout), MsAggregateClass::Memory);
    }

    // ============================================================================
    // MsReturnPlacement selector (PAS-DEBT-B3-007b / paideia-as#1543)
    // ============================================================================

    #[test]
    fn ms_return_placement_int_maps_to_rax() {
        assert_eq!(
            ms_return_placement(MsAggregateClass::Register(MsRegClass::Int)),
            MsReturnPlacement::IntSingle
        );
    }

    #[test]
    fn ms_return_placement_xmm_maps_to_xmm0() {
        assert_eq!(
            ms_return_placement(MsAggregateClass::Register(MsRegClass::Xmm)),
            MsReturnPlacement::XmmSingle
        );
    }

    #[test]
    fn ms_return_placement_memory_flags_hidden_sret() {
        let placement = ms_return_placement(MsAggregateClass::Memory);
        assert_eq!(placement, MsReturnPlacement::Memory);
        assert!(placement.needs_hidden_sret());
        assert!(!placement.uses_xmm());
    }

    /// Zero-size aggregate at the layout-entry point → None.
    #[test]
    fn ms_return_placement_from_layout_zero_size_is_none() {
        let layout = RecordLayout::new(0, 1, vec![]);
        assert_eq!(
            ms_return_placement_from_layout(&layout),
            MsReturnPlacement::None
        );
    }

    /// End-to-end: 24-byte aggregate → Memory placement. Pins that the
    /// three-layer chain (RecordLayout → classify_ms_aggregate →
    /// ms_return_placement) composes through the from-layout wrapper.
    #[test]
    fn ms_return_placement_from_layout_composes_with_classifier() {
        let layout = RecordLayout::new(
            24,
            8,
            vec![
                FieldLayout { offset: 0, size: 8, signed: false, is_float: false },
                FieldLayout { offset: 8, size: 8, signed: false, is_float: false },
                FieldLayout { offset: 16, size: 8, signed: false, is_float: false },
            ],
        );
        assert_eq!(
            ms_return_placement_from_layout(&layout),
            MsReturnPlacement::Memory
        );
    }

    /// `uses_xmm` fires only on the XmmSingle placement — everything
    /// else is false.
    #[test]
    fn ms_placement_uses_xmm_only_for_xmm_single() {
        for placement in [
            MsReturnPlacement::None,
            MsReturnPlacement::IntSingle,
            MsReturnPlacement::Memory,
        ] {
            assert!(
                !placement.uses_xmm(),
                "MS placement {:?} falsely uses_xmm",
                placement
            );
        }
        assert!(MsReturnPlacement::XmmSingle.uses_xmm());
    }

    /// `needs_hidden_sret` fires only on the Memory placement.
    #[test]
    fn ms_placement_needs_hidden_sret_only_for_memory() {
        for placement in [
            MsReturnPlacement::None,
            MsReturnPlacement::IntSingle,
            MsReturnPlacement::XmmSingle,
        ] {
            assert!(
                !placement.needs_hidden_sret(),
                "MS placement {:?} unexpectedly requested a hidden sret",
                placement
            );
        }
        assert!(MsReturnPlacement::Memory.needs_hidden_sret());
    }

    /// End-to-end: scalar-f64 aggregate goes through the XmmSingle
    /// placement via the layout entry point.
    #[test]
    fn ms_return_placement_from_layout_scalar_f64_is_xmm_single() {
        let layout = RecordLayout::new(
            8,
            8,
            vec![FieldLayout { offset: 0, size: 8, signed: false, is_float: true }],
        );
        assert_eq!(
            ms_return_placement_from_layout(&layout),
            MsReturnPlacement::XmmSingle
        );
    }
}
