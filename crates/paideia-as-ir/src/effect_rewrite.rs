//! Effect-handler rewrite pass per `custom-assembler.md` §6.3 +
//! `calling-convention.md` §4.2.
//!
//! Each `perform E.op(args)` is rewritten to an indirect call against
//! the handler table held in `R15`:
//!
//! 1. Load handler-table base from `R15`.
//! 2. Load the `E.op` slot at `[R15 + offset]`.
//! 3. Indirect-call the loaded pointer with the original arguments.
//!
//! Phase-1: the IR doesn't yet carry the operand list per node, so the
//! rewrite is exposed as a per-node helper that:
//!
//! - Takes an `IrPerform` node id.
//! - Allocates a new `App` node with the same span representing the
//!   indirect call.
//! - Records the handler-table offset for the operation in a side-table.
//!
//! `unsafe` blocks pass through unchanged per §9.4.
//!
//! Nested `with` blocks: we expose a small `WithRewrite` helper that
//! emits save/restore `App` nodes around the inner body's first/last
//! nodes. The actual register-allocation lowering is the emitter's
//! job (T8).

use std::collections::HashMap;

use crate::arena::IrArena;
use crate::node::{IrKind, IrNodeId};

/// Outcome of rewriting one `IrPerform`.
#[derive(Debug, Clone)]
pub struct PerformRewrite {
    /// New `App` node id replacing the original `Perform`.
    pub call: IrNodeId,
    /// Handler-table offset for the operation, in bytes.
    pub offset: u32,
}

/// Per-effect operation table. Phase-1 stores `(effect_name, op_name) →
/// byte offset`; the offset is `slot_index * 8` (each handler pointer
/// is 64 bits per the calling convention).
#[derive(Default, Debug, Clone)]
pub struct HandlerTable {
    /// Slot per `(effect, op)`. Insertion order determines offset.
    slots: HashMap<(String, String), u32>,
    /// Next slot index to assign.
    next_slot: u32,
}

impl HandlerTable {
    /// Construct an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up — or assign — the slot for `(effect, op)`.
    ///
    /// First call assigns a fresh slot at `next_slot * 8`; subsequent
    /// calls return the cached offset.
    pub fn slot_for(&mut self, effect: &str, op: &str) -> u32 {
        let key = (effect.to_owned(), op.to_owned());
        if let Some(&off) = self.slots.get(&key) {
            return off;
        }
        let off = self.next_slot * 8;
        self.next_slot += 1;
        self.slots.insert(key, off);
        off
    }

    /// Number of slots assigned so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// `true` iff no slots have been assigned.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

/// Rewrite one `IrPerform` node into an indirect-call `IrApp`.
///
/// # Panics
///
/// Panics if `id`'s kind is not `IrKind::Perform`.
#[must_use]
pub fn rewrite_perform(
    arena: &mut IrArena,
    table: &mut HandlerTable,
    id: IrNodeId,
    effect: &str,
    op: &str,
) -> PerformRewrite {
    let node = arena[id];
    assert_eq!(
        node.kind,
        IrKind::Perform,
        "rewrite_perform: id must be a Perform node"
    );
    let offset = table.slot_for(effect, op);
    let call = arena.alloc(IrKind::App, node.span);
    PerformRewrite { call, offset }
}

/// `unsafe` blocks pass through unchanged per §9.4. This helper is a
/// no-op that returns the same id; provided for symmetry so the IR
/// walker can call it uniformly.
#[must_use]
pub fn rewrite_unsafe_passthrough(arena: &IrArena, id: IrNodeId) -> IrNodeId {
    debug_assert_eq!(arena[id].kind, IrKind::Unsafe);
    id
}

/// Save / restore nodes for nested `with` blocks.
#[derive(Debug, Clone, Copy)]
pub struct WithRewrite {
    /// Synthetic save-R15 node injected at the beginning of the body.
    pub save: IrNodeId,
    /// Synthetic restore-R15 node injected at the end.
    pub restore: IrNodeId,
}

/// Emit save / restore `App` nodes around the body of a `with` block.
///
/// Phase-1 produces synthetic `App` nodes claiming the body's span.
/// The emitter (T8) lowers them to the actual save/restore prologue
/// + epilogue per the calling convention.
#[must_use]
pub fn rewrite_with_save_restore(arena: &mut IrArena, body_id: IrNodeId) -> WithRewrite {
    let span = arena[body_id].span;
    let save = arena.alloc(IrKind::App, span);
    let restore = arena.alloc(IrKind::App, span);
    WithRewrite { save, restore }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{FileId, Span};

    fn span(byte_start: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), byte_start, 1)
    }

    // ── HandlerTable ─────────────────────────────────────────────────

    #[test]
    fn first_slot_is_offset_0() {
        let mut t = HandlerTable::new();
        assert_eq!(t.slot_for("Io", "read"), 0);
    }

    #[test]
    fn second_slot_is_offset_8() {
        let mut t = HandlerTable::new();
        let _ = t.slot_for("Io", "read");
        assert_eq!(t.slot_for("Io", "write"), 8);
    }

    #[test]
    fn same_op_returns_cached_offset() {
        let mut t = HandlerTable::new();
        let a = t.slot_for("Io", "read");
        let b = t.slot_for("Io", "read");
        assert_eq!(a, b);
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn different_effects_share_offset_space() {
        let mut t = HandlerTable::new();
        let io = t.slot_for("Io", "read");
        let mmio = t.slot_for("Mmio", "read");
        assert_ne!(io, mmio);
        assert_eq!(mmio, 8);
    }

    // ── AC bullet 1: three Performs → three App rewrites ─────────────

    #[test]
    fn three_performs_rewrite_to_three_apps_with_distinct_offsets() {
        let mut arena = IrArena::new();
        let mut table = HandlerTable::new();
        let p1 = arena.alloc(IrKind::Perform, span(0));
        let p2 = arena.alloc(IrKind::Perform, span(10));
        let p3 = arena.alloc(IrKind::Perform, span(20));
        let r1 = rewrite_perform(&mut arena, &mut table, p1, "Io", "read");
        let r2 = rewrite_perform(&mut arena, &mut table, p2, "Io", "write");
        let r3 = rewrite_perform(&mut arena, &mut table, p3, "Mmio", "read");
        assert_eq!(arena[r1.call].kind, IrKind::App);
        assert_eq!(arena[r2.call].kind, IrKind::App);
        assert_eq!(arena[r3.call].kind, IrKind::App);
        assert_eq!(r1.offset, 0);
        assert_eq!(r2.offset, 8);
        assert_eq!(r3.offset, 16);
    }

    #[test]
    fn rewrite_preserves_span() {
        let mut arena = IrArena::new();
        let mut table = HandlerTable::new();
        let s = span(42);
        let p = arena.alloc(IrKind::Perform, s);
        let r = rewrite_perform(&mut arena, &mut table, p, "E", "op");
        assert_eq!(arena[r.call].span, s);
    }

    // ── AC bullet 2: nested with → save/restore ──────────────────────

    #[test]
    fn nested_with_emits_save_and_restore() {
        let mut arena = IrArena::new();
        let body = arena.alloc(IrKind::Action, span(0));
        let outer = rewrite_with_save_restore(&mut arena, body);
        let inner = rewrite_with_save_restore(&mut arena, body);
        // Each call allocates a fresh save/restore pair → 4 new nodes.
        assert_ne!(outer.save, inner.save);
        assert_ne!(outer.restore, inner.restore);
        for id in [outer.save, outer.restore, inner.save, inner.restore] {
            assert_eq!(arena[id].kind, IrKind::App);
        }
    }

    // ── AC bullet 3: unsafe passthrough ──────────────────────────────

    #[test]
    fn unsafe_block_passes_through_unchanged() {
        let mut arena = IrArena::new();
        let u = arena.alloc(IrKind::Unsafe, span(0));
        let out = rewrite_unsafe_passthrough(&arena, u);
        assert_eq!(out, u);
    }

    // ── Edge ─────────────────────────────────────────────────────────

    #[test]
    #[should_panic(expected = "must be a Perform node")]
    fn rewrite_perform_panics_on_non_perform() {
        let mut arena = IrArena::new();
        let mut table = HandlerTable::new();
        let other = arena.alloc(IrKind::Var, span(0));
        let _ = rewrite_perform(&mut arena, &mut table, other, "E", "op");
    }

    #[test]
    fn handler_table_len_grows_only_on_new_slots() {
        let mut t = HandlerTable::new();
        let _ = t.slot_for("Io", "read");
        let _ = t.slot_for("Io", "read"); // cached
        let _ = t.slot_for("Io", "write"); // new
        assert_eq!(t.len(), 2);
        assert!(!t.is_empty());
    }
}
