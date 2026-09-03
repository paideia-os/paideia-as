//! Linear-cap consumption verification across an `unsafe` block.
//!
//! v0.25-M1-003 (#1357). Linearity discipline for capabilities does not
//! stop at the `unsafe` boundary: every capability that enters an unsafe
//! block must be consumed **exactly once** inside it, and any capability
//! produced inside must be consumed before the block ends. The verifier
//! reifies the pre- and post-frames as [`CapSet`]s so callers (the
//! elaborator, and later the borrow-checker) can splice the block into
//! a whole-function analysis without re-walking its body.
//!
//! # Model
//!
//! An [`UnsafeBlock`] is a straight-line sequence of [`CapOp`]s
//! interleaved with pure computation the linearity pass does not care
//! about. The verifier walks that sequence with three ledgers:
//!
//! | ledger    | contents                                                          |
//! | --------- | ----------------------------------------------------------------- |
//! | `live`    | caps currently held (starts as `entry`)                           |
//! | `consumed`| caps that have been consumed (each cap at most once)              |
//! | `produced`| caps that were created inside the block                           |
//!
//! Each op mutates the ledgers; at end-of-block the invariants below are
//! checked and, if any fail, the first violation surfaces as a
//! [`LinearErr`].
//!
//! # Invariants
//!
//! 1. `Consume(c)` requires `c ∈ live`. Re-consuming a cap that has
//!    already been removed from `live` (whether it was in `entry` or
//!    freshly `Produce`d) is a [`LinearErr::DoubleConsumed`].
//! 2. `Produce(c)` adds `c` to `live` and records it in `produced`.
//!    Producing a cap that is already live is treated as a
//!    [`LinearErr::DoubleConsumed`] of the shadowed slot — the caller
//!    must consume the outstanding one first.
//! 3. On block exit, every `c ∈ entry` must appear in `consumed`
//!    (otherwise [`LinearErr::Unused`]) and every `c ∈ produced` that
//!    remains in `live` (i.e., was never consumed) is a
//!    [`LinearErr::Leaked`].
//!
//! # Diagnostic codes
//!
//! Codes `L0100`-`L0110` are reserved for the linearity pass. This
//! primitive lands `L0100`-`L0102`; the remaining eight are reserved
//! for M1-004 and later.
//!
//! | code    | variant                             |
//! | ------- | ----------------------------------- |
//! | `L0100` | [`LinearErr::Unused`]               |
//! | `L0101` | [`LinearErr::DoubleConsumed`]       |
//! | `L0102` | [`LinearErr::Leaked`]               |
//! | `L0103` | *reserved (M1-004: alias split)*    |
//! | `L0104` | *reserved (M1-004: cross-branch)*   |
//! | `L0105` | *reserved (borrow-checker join)*    |
//! | `L0106` | *reserved (region escape)*          |
//! | `L0107` | *reserved (session interleave)*     |
//! | `L0108` | *reserved (functor-instantiation)*  |
//! | `L0109` | *reserved (drop-glue conflict)*     |
//! | `L0110` | *reserved (verifier internal)*      |
//!
//! # Determinism
//!
//! The verifier walks `block.ops` in order and returns the **first**
//! `Consume`/`Produce` violation it sees. End-of-block errors
//! (`Unused`, `Leaked`) are surfaced in ascending [`CapId`] order so
//! two runs against the same input always produce byte-identical
//! diagnostics — a requirement for the pre-commit fingerprint hook.

#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use paideia_as_types::{CapId, CapSet};

/// A single capability-affecting operation inside an unsafe block.
///
/// The verifier only tracks operations that move capabilities; every
/// other statement inside the block is invisible to linearity and is
/// therefore not represented here.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum CapOp {
    /// Consume a capability, removing it from the live set.
    Consume(CapId),
    /// Produce a fresh capability, adding it to the live set. Producing
    /// a capability already live is rejected as `DoubleConsumed`.
    Produce(CapId),
}

/// A straight-line unsafe block, expressed as its capability-affecting
/// operations in source order.
///
/// Branching and looping are M1-004's concern; this primitive covers
/// the straight-line case that M2 lowerings emit for MMIO and DMA
/// intrinsics.
#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub struct UnsafeBlock {
    /// Cap operations in evaluation order.
    pub ops: Vec<CapOp>,
}

impl UnsafeBlock {
    /// Construct an empty block.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Construct a block from a list of ops.
    #[must_use]
    pub fn from_ops(ops: Vec<CapOp>) -> Self {
        Self { ops }
    }
}

/// Verifier context.
///
/// Currently opaque — the M1-003 verifier does not consult any
/// enclosing state. `LinearCx` exists so that M1-004+ can plumb in
/// borrow-checker state, region graphs, and session envs without
/// re-signing every call site.
#[derive(Clone, Debug, Default)]
pub struct LinearCx {
    // Placeholder for future extension (region graph, borrow ledger,
    // session env). Kept private so downstream callers cannot depend
    // on the empty shape.
    _priv: (),
}

impl LinearCx {
    /// Fresh empty context.
    #[must_use]
    pub fn new() -> Self {
        Self { _priv: () }
    }
}

/// Linearity violations reported by [`verify_unsafe_block`].
///
/// Each variant carries the single [`CapId`] the diagnostic refers to
/// so the diagnostics layer can render `L01xx c<id>` labels without
/// looking anything up.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum LinearErr {
    /// An entry cap was never referenced by any `Consume` op.
    ///
    /// Diagnostic `L0100`.
    Unused(CapId),
    /// A `Consume` op targeted a cap that was not live at that point —
    /// either it had already been consumed, or it was never in the
    /// live set to begin with. Both cases share the same diagnostic
    /// because from the verifier's ledger they are indistinguishable
    /// (the ledger only knows what is live *now*).
    ///
    /// Diagnostic `L0101`.
    DoubleConsumed(CapId),
    /// A cap produced inside the block was still live at end-of-block.
    ///
    /// Diagnostic `L0102`.
    Leaked(CapId),
}

impl LinearErr {
    /// Stable diagnostic code (`L0100`..`L0110`).
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            LinearErr::Unused(_) => "L0100",
            LinearErr::DoubleConsumed(_) => "L0101",
            LinearErr::Leaked(_) => "L0102",
        }
    }

    /// The capability the diagnostic is about.
    #[must_use]
    pub fn cap(self) -> CapId {
        match self {
            LinearErr::Unused(c) | LinearErr::DoubleConsumed(c) | LinearErr::Leaked(c) => c,
        }
    }
}

impl core::fmt::Display for LinearErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LinearErr::Unused(c) => write!(f, "L0100: cap {c} entered unsafe block unused"),
            LinearErr::DoubleConsumed(c) => {
                write!(f, "L0101: cap {c} consumed while not live")
            }
            LinearErr::Leaked(c) => {
                write!(f, "L0102: cap {c} produced inside unsafe block was not consumed")
            }
        }
    }
}

impl std::error::Error for LinearErr {}

/// Verify a straight-line unsafe block against the incoming cap frame.
///
/// Returns the **post-frame** on success — always the empty set, since
/// every cap must be consumed exactly once inside the block. Returning
/// a `CapSet` (rather than `()`) lets callers compose this verifier
/// with sequential-composition rules in M1-004 without a special case.
///
/// See the module-level docs for the exact invariants and diagnostic
/// codes.
///
/// # Errors
///
/// Returns the **first** violation encountered while walking
/// `block.ops`; if the walk completes without a per-op error, unused
/// entry caps are reported in ascending [`CapId`] order, followed by
/// leaked produced caps in the same order.
pub fn verify_unsafe_block(
    block: &UnsafeBlock,
    entry: &CapSet,
    _cx: &LinearCx,
) -> Result<CapSet, LinearErr> {
    // Snapshot the entry frame into a mutable ledger. `BTreeSet` gives
    // us both O(log n) membership *and* the deterministic iteration
    // order the AC requires for end-of-block diagnostics.
    let entry_caps: BTreeSet<CapId> = entry.as_slice().iter().copied().collect();
    let mut live: BTreeSet<CapId> = entry_caps.clone();
    let mut consumed: BTreeSet<CapId> = BTreeSet::new();
    let mut produced: BTreeSet<CapId> = BTreeSet::new();

    for op in &block.ops {
        match *op {
            CapOp::Consume(c) => {
                if !live.remove(&c) {
                    // Either already-consumed (present in `consumed`)
                    // or never held. Both surface as DoubleConsumed:
                    // the ledger only knows what is live now, and the
                    // caller supplied `entry`, so anything not in the
                    // current live set is by definition a duplicate
                    // consume of a slot we no longer hold.
                    return Err(LinearErr::DoubleConsumed(c));
                }
                consumed.insert(c);
            }
            CapOp::Produce(c) => {
                if live.contains(&c) {
                    // Producing a cap already live would create two
                    // simultaneous slots for the same identity — from
                    // a linearity standpoint that shadows the older
                    // slot, and the shadowed one can never be
                    // consumed. We reject rather than silently drop.
                    return Err(LinearErr::DoubleConsumed(c));
                }
                live.insert(c);
                produced.insert(c);
            }
        }
    }

    // End-of-block: entry caps that were never consumed are `Unused`.
    // BTreeSet's iterator yields in ascending order → deterministic.
    for c in &entry_caps {
        if !consumed.contains(c) {
            return Err(LinearErr::Unused(*c));
        }
    }

    // ... then produced caps that are still live are `Leaked`.
    for c in &produced {
        if live.contains(c) {
            return Err(LinearErr::Leaked(*c));
        }
    }

    // Ok path: live is empty by construction (every entry cap was
    // consumed, and every produced cap that stayed live would have
    // triggered Leaked above). Rebuild it as a `CapSet` for the
    // caller.
    Ok(CapSet::from_ids(live.into_iter().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cid(n: u32) -> CapId {
        CapId::new(n).expect("test cap ids are non-zero")
    }

    fn cs(ids: &[CapId]) -> CapSet {
        CapSet::from_ids(ids.to_vec())
    }

    // ---- AC-mandated tests ------------------------------------------------

    #[test]
    fn single_cap_consumed() {
        let entry = cs(&[cid(1)]);
        let block = UnsafeBlock::from_ops(vec![CapOp::Consume(cid(1))]);
        let post = verify_unsafe_block(&block, &entry, &LinearCx::new())
            .expect("single consume is well-formed");
        assert!(post.is_empty(), "post-frame must be empty after consumption");
    }

    #[test]
    fn double_consume_detected() {
        let entry = cs(&[cid(1)]);
        let block = UnsafeBlock::from_ops(vec![CapOp::Consume(cid(1)), CapOp::Consume(cid(1))]);
        let err = verify_unsafe_block(&block, &entry, &LinearCx::new())
            .expect_err("second consume must fail");
        assert_eq!(err, LinearErr::DoubleConsumed(cid(1)));
        assert_eq!(err.code(), "L0101");
    }

    #[test]
    fn leak_out_detected() {
        // Cap produced inside the block but never consumed → Leaked.
        let entry = cs(&[]);
        let block = UnsafeBlock::from_ops(vec![CapOp::Produce(cid(9))]);
        let err = verify_unsafe_block(&block, &entry, &LinearCx::new())
            .expect_err("unbalanced produce must leak");
        assert_eq!(err, LinearErr::Leaked(cid(9)));
        assert_eq!(err.code(), "L0102");
    }

    #[test]
    fn alias_not_consumed() {
        // Two entry caps, only one consumed → the other is Unused.
        let entry = cs(&[cid(1), cid(2)]);
        let block = UnsafeBlock::from_ops(vec![CapOp::Consume(cid(1))]);
        let err = verify_unsafe_block(&block, &entry, &LinearCx::new())
            .expect_err("aliased-but-unused cap must be reported");
        assert_eq!(err, LinearErr::Unused(cid(2)));
        assert_eq!(err.code(), "L0100");
    }

    // ---- Additional coverage ---------------------------------------------

    #[test]
    fn empty_block_empty_entry_is_ok() {
        let post = verify_unsafe_block(&UnsafeBlock::empty(), &CapSet::empty(), &LinearCx::new())
            .expect("no caps, no ops, no obligations");
        assert!(post.is_empty());
    }

    #[test]
    fn produce_then_consume_balances() {
        let entry = cs(&[]);
        let block = UnsafeBlock::from_ops(vec![CapOp::Produce(cid(3)), CapOp::Consume(cid(3))]);
        let post = verify_unsafe_block(&block, &entry, &LinearCx::new())
            .expect("balanced produce/consume is well-formed");
        assert!(post.is_empty());
    }

    #[test]
    fn consume_of_never_held_cap_is_double_consume() {
        // Cap never entered the block and was never produced inside →
        // treated identically to double-consume (the ledger only sees
        // "not live").
        let entry = cs(&[cid(1)]);
        let block = UnsafeBlock::from_ops(vec![CapOp::Consume(cid(7))]);
        let err = verify_unsafe_block(&block, &entry, &LinearCx::new())
            .expect_err("consuming an unheld cap must fail");
        assert_eq!(err, LinearErr::DoubleConsumed(cid(7)));
    }

    #[test]
    fn produce_of_already_live_cap_is_rejected() {
        let entry = cs(&[cid(4)]);
        let block = UnsafeBlock::from_ops(vec![CapOp::Produce(cid(4))]);
        let err = verify_unsafe_block(&block, &entry, &LinearCx::new())
            .expect_err("shadowing produce must fail");
        assert_eq!(err, LinearErr::DoubleConsumed(cid(4)));
    }

    #[test]
    fn multiple_unused_caps_report_lowest_id_first() {
        // Deterministic order: BTreeSet ascending. cid(2) < cid(5)
        // so cid(2) is the reported one.
        let entry = cs(&[cid(2), cid(5), cid(8)]);
        let block = UnsafeBlock::from_ops(vec![CapOp::Consume(cid(8))]);
        let err = verify_unsafe_block(&block, &entry, &LinearCx::new()).expect_err("has unused");
        assert_eq!(err, LinearErr::Unused(cid(2)));
    }

    #[test]
    fn unused_takes_precedence_over_leak() {
        // Entry cap unused + produced cap leaked → Unused reported first.
        let entry = cs(&[cid(1)]);
        let block = UnsafeBlock::from_ops(vec![CapOp::Produce(cid(9))]);
        let err = verify_unsafe_block(&block, &entry, &LinearCx::new()).expect_err("both fail");
        assert_eq!(err, LinearErr::Unused(cid(1)));
    }

    #[test]
    fn error_display_carries_diagnostic_code() {
        assert!(format!("{}", LinearErr::Unused(cid(1))).starts_with("L0100"));
        assert!(format!("{}", LinearErr::DoubleConsumed(cid(1))).starts_with("L0101"));
        assert!(format!("{}", LinearErr::Leaked(cid(1))).starts_with("L0102"));
    }

    #[test]
    fn error_cap_accessor_round_trips() {
        assert_eq!(LinearErr::Unused(cid(11)).cap(), cid(11));
        assert_eq!(LinearErr::DoubleConsumed(cid(12)).cap(), cid(12));
        assert_eq!(LinearErr::Leaked(cid(13)).cap(), cid(13));
    }

    #[test]
    fn all_diagnostic_codes_are_in_reserved_range() {
        for err in [
            LinearErr::Unused(cid(1)),
            LinearErr::DoubleConsumed(cid(1)),
            LinearErr::Leaked(cid(1)),
        ] {
            let code = err.code();
            assert!(code.starts_with('L'));
            let n: u32 = code[1..].parse().expect("numeric suffix");
            assert!((100..=110).contains(&n), "code {code} outside L0100-L0110");
        }
    }
}
