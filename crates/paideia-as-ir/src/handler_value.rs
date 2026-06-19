//! Handler-value representation for IR Handle nodes.
//!
//! Each `IrKind::Handle` node carries structural children (`[handler, body]`)
//! in the arena's `children_table`. This module provides a side-table
//! (`HandlerSideTable`) mapping Handle node ids to their full metadata:
//! effect being handled, operation implementations, return clause, and
//! finally clause.
//!
//! Phase-1: the IR builder populates this table; phase-2+ elaborators use
//! it to type-check and emit handlers. The side-table design keeps
//! `IrNodeData` at 48 bytes while allowing unbounded handler metadata.

use std::collections::HashMap;

use crate::IrArena;
use crate::node::IrNodeId;

/// Stable identifier for an effect (e.g., `Io`, `Mmio`).
///
/// This is a minimal re-export/alias for the effect id concept. The authoritative
/// definition lives in `paideia-as-effects::EffectId`, but we avoid a circular
/// dependency by defining it here as well. Both use the same underlying representation:
/// a non-zero u32 wrapper. Phase-1 elaborator interprets these ids via the global
/// effect registry (T6+).
///
/// This type is used in HandlerInfo to identify which effect a handler is installed for.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug)]
pub struct EffectId(pub u32);

/// Metadata for a Handle IR node.
///
/// Each Handle node (indexed by `IrNodeId`) has a corresponding `HandlerInfo`
/// in the side-table. Phase-1 captures the static structure of the handler:
/// which effect it handles, which operations it implements, whether it has a
/// return clause (for polymorphic returns in m3+), and whether it has a
/// finally clause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerInfo {
    /// The effect being handled (interned id from paideia-as-effects).
    pub effect: EffectId,

    /// Operation implementations: (op_name, handler_fn_id) pairs in
    /// declaration order. Each op_name identifies an operation on `effect`;
    /// handler_fn_id is the IrNodeId of the lambda or block implementing it.
    pub ops: Vec<(String, IrNodeId)>,

    /// Optional return-value handler (m3 polymorphic returns).
    /// If present, this IrNodeId refers to a lambda that handles the
    /// normal return case (non-effect path). Phase-1 sets this to None;
    /// m3 elaborator populates it.
    pub ret: Option<IrNodeId>,

    /// Optional finally clause body.
    /// If present, this IrNodeId refers to an action block that runs after
    /// the handler completes (effect or return). Runs regardless of the path
    /// taken. Phase-1 may set this; m3+ elaborator may extend it.
    pub finally: Option<IrNodeId>,
}

/// Side-table mapping Handle IrNodeIds to their metadata.
///
/// Parallels the arena's `children_table` pattern: uses a sparse vector
/// indexed by `IrNodeId.index()` so that lookups are O(1) and closely
/// packed with the node arena in memory.
///
/// Phase-1: populated by the IR builder as handlers are constructed.
/// Elaborators (phase-2+) read and mutate entries to populate type and
/// linearity information.
#[derive(Default, Debug, Clone)]
pub struct HandlerSideTable {
    /// Sparse table: `table[Handle.index()] = Some(HandlerInfo)`.
    /// Only Handle nodes have entries; other nodes don't.
    table: HashMap<IrNodeId, HandlerInfo>,
}

impl HandlerSideTable {
    /// Construct an empty handler side-table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) the metadata for a Handle node.
    ///
    /// Returns the previous entry if one existed; useful for debugging
    /// duplicate-handler errors.
    pub fn insert(&mut self, id: IrNodeId, info: HandlerInfo) -> Option<HandlerInfo> {
        self.table.insert(id, info)
    }

    /// Look up the metadata for a Handle node.
    ///
    /// Returns `None` if the node was never registered or is not a Handle node.
    #[must_use]
    pub fn get(&self, id: IrNodeId) -> Option<&HandlerInfo> {
        self.table.get(&id)
    }

    /// Look up (mutable) the metadata for a Handle node.
    ///
    /// Allows elaborators to mutate the handler's metadata (e.g., update
    /// linearity or effect-row information) without cloning.
    pub fn get_mut(&mut self, id: IrNodeId) -> Option<&mut HandlerInfo> {
        self.table.get_mut(&id)
    }

    /// Number of handlers registered in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.table.len()
    }

    /// `true` iff no handlers are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }
}

/// Pretty-print a handler's metadata.
///
/// Returns a human-readable debug string showing the effect, operations,
/// return clause presence, and finally clause presence. Useful for
/// debugging and snapshot tests.
///
/// # Arguments
///
/// * `info` - The handler metadata to format.
/// * `arena` - The arena (used to look up node kinds if needed in future phases).
///
/// # Example
///
/// ```ignore
/// let handler_info = HandlerInfo {
///     effect: EffectId::new(1).unwrap(),
///     ops: vec![("read".to_string(), i1), ("write".to_string(), i2)],
///     ret: Some(i3),
///     finally: None,
/// };
/// let s = pretty_handler(&handler_info, &arena);
/// // Outputs something like:
/// // Handler<1>:
/// //   ops:
/// //     - read: i1
/// //     - write: i2
/// //   ret: Some(i3)
/// //   finally: None
/// ```
#[must_use]
pub fn pretty_handler(info: &HandlerInfo, _arena: &IrArena) -> String {
    let mut s = String::new();
    s.push_str(&format!("Handler<{}>:\n", info.effect.0));
    s.push_str("  ops:\n");
    for (op_name, op_id) in &info.ops {
        s.push_str(&format!("    - {}: {}\n", op_name, op_id));
    }
    // Format option as Some(id) or None, using Display for the IrNodeId
    if let Some(ret_id) = info.ret {
        s.push_str(&format!("  ret: Some({})\n", ret_id));
    } else {
        s.push_str("  ret: None\n");
    }
    if let Some(finally_id) = info.finally {
        s.push_str(&format!("  finally: Some({})\n", finally_id));
    } else {
        s.push_str("  finally: None\n");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{FileId, SourceMap, Span, VecSink};

    use crate::node::IrKind;
    use crate::walker::{IrWalker, walk};
    use crate::walker_ctx::WalkerCtx;

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    // ── HandlerSideTable tests ──────────────────────────────────────────

    #[test]
    fn handler_side_table_insert_and_get() {
        let mut table = HandlerSideTable::new();
        let handle_id = IrNodeId::new(1).unwrap();
        let op_id = IrNodeId::new(2).unwrap();

        let info = HandlerInfo {
            effect: EffectId(42),
            ops: vec![("read".to_string(), op_id)],
            ret: None,
            finally: None,
        };

        // Insert and verify
        table.insert(handle_id, info.clone());
        let retrieved = table.get(handle_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().effect, EffectId(42));
        assert_eq!(retrieved.unwrap().ops.len(), 1);
        assert_eq!(retrieved.unwrap().ops[0].0, "read");
        assert_eq!(retrieved.unwrap().ops[0].1, op_id);
        assert_eq!(retrieved.unwrap().ret, None);
        assert_eq!(retrieved.unwrap().finally, None);

        // Verify get on unset node returns None
        let unset_id = IrNodeId::new(999).unwrap();
        assert_eq!(table.get(unset_id), None);
    }

    #[test]
    fn handler_side_table_round_trips_through_clone() {
        let mut table = HandlerSideTable::new();

        // Insert multiple handlers
        for i in 1u32..=3 {
            let handle_id = IrNodeId::new(i).unwrap();
            let op_id = IrNodeId::new(100 + i).unwrap();
            let info = HandlerInfo {
                effect: EffectId(i * 10),
                ops: vec![(format!("op{}", i), op_id)],
                ret: if i == 1 { Some(op_id) } else { None },
                finally: if i == 2 { Some(op_id) } else { None },
            };
            table.insert(handle_id, info);
        }

        // Clone and verify all entries are present
        let cloned = table.clone();
        assert_eq!(cloned.len(), 3);

        for i in 1u32..=3 {
            let handle_id = IrNodeId::new(i).unwrap();
            let info = cloned.get(handle_id);
            assert!(info.is_some());
            let info_unwrap = info.unwrap();
            assert_eq!(info_unwrap.effect, EffectId(i * 10));
            assert_eq!(info_unwrap.ops.len(), 1);
            assert_eq!(info_unwrap.ops[0].0, format!("op{}", i));
            if i == 1 {
                assert!(info_unwrap.ret.is_some());
            } else {
                assert_eq!(info_unwrap.ret, None);
            }
            if i == 2 {
                assert!(info_unwrap.finally.is_some());
            } else {
                assert_eq!(info_unwrap.finally, None);
            }
        }

        // Verify clone independence: mutate original, cloned should be unchanged
        let new_handle = IrNodeId::new(4).unwrap();
        table.insert(
            new_handle,
            HandlerInfo {
                effect: EffectId(999),
                ops: vec![],
                ret: None,
                finally: None,
            },
        );
        assert_eq!(table.len(), 4);
        assert_eq!(cloned.len(), 3);
        assert_eq!(cloned.get(new_handle), None);
    }

    #[test]
    fn pretty_handler_for_2_op_handler_matches_expected() {
        let mut arena = IrArena::new();
        let op1_id = arena.alloc(IrKind::Lambda, span());
        let op2_id = arena.alloc(IrKind::Lambda, span());
        let finally_id = arena.alloc(IrKind::Action, span());

        let info = HandlerInfo {
            effect: EffectId(7),
            ops: vec![("read".to_string(), op1_id), ("write".to_string(), op2_id)],
            ret: None,
            finally: Some(finally_id),
        };

        let formatted = pretty_handler(&info, &arena);

        // Verify the formatted string contains all expected parts
        assert!(
            formatted.contains("Handler<7>"),
            "formatted output should contain 'Handler<7>', but got: {}",
            formatted
        );
        assert!(formatted.contains("ops:"));
        assert!(formatted.contains("read:"));
        assert!(formatted.contains("write:"));
        assert!(formatted.contains(&format!("{}", op1_id)));
        assert!(formatted.contains(&format!("{}", op2_id)));
        assert!(formatted.contains("ret: None"));
        assert!(formatted.contains(&format!("finally: Some({})", finally_id)));

        // Snapshot-style check: verify format is multi-line and structured
        let lines: Vec<&str> = formatted.lines().collect();
        assert!(
            lines.len() >= 5,
            "expected at least 5 lines in formatted output"
        );
        assert!(lines[0].contains("Handler<7>"));
    }

    #[test]
    fn walker_visits_handle_children_in_order() {
        // Build a Handle node with handler and body children
        let mut arena = IrArena::new();
        let handler_impl = arena.alloc(IrKind::Lambda, span());
        let body = arena.alloc(IrKind::Action, span());
        let handle = arena.alloc_with_children(IrKind::Handle, span(), [handler_impl, body]);

        // Recording walker to capture visit order
        struct RecordingWalker {
            visits: Vec<(VisitPhase, IrKind)>,
        }

        #[derive(Debug, Copy, Clone, Eq, PartialEq)]
        enum VisitPhase {
            Pre,
            Post,
        }

        impl IrWalker for RecordingWalker {
            fn pre_visit(
                &mut self,
                _id: IrNodeId,
                node: &crate::node::IrNodeData,
                _arena: &IrArena,
                _ctx: &mut WalkerCtx<'_>,
            ) {
                self.visits.push((VisitPhase::Pre, node.kind));
            }

            fn post_visit(
                &mut self,
                _id: IrNodeId,
                node: &crate::node::IrNodeData,
                _arena: &IrArena,
                _ctx: &mut WalkerCtx<'_>,
            ) {
                self.visits.push((VisitPhase::Post, node.kind));
            }
        }

        let mut walker = RecordingWalker { visits: Vec::new() };
        let sm = SourceMap::new();
        let mut sink = VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);

        walk(&mut walker, &arena, handle, &mut ctx);

        // Verify the visit order matches the documented Handle child semantics
        // Expected: Pre(Handle), Pre(Lambda), Post(Lambda), Pre(Action), Post(Action), Post(Handle)
        assert_eq!(
            walker.visits.len(),
            6,
            "Handle with 2 children should produce 6 visits"
        );
        assert_eq!(
            walker.visits[0],
            (VisitPhase::Pre, IrKind::Handle),
            "first visit should be Pre(Handle)"
        );
        assert_eq!(
            walker.visits[1],
            (VisitPhase::Pre, IrKind::Lambda),
            "second visit should be Pre(handler)"
        );
        assert_eq!(
            walker.visits[2],
            (VisitPhase::Post, IrKind::Lambda),
            "third visit should be Post(handler)"
        );
        assert_eq!(
            walker.visits[3],
            (VisitPhase::Pre, IrKind::Action),
            "fourth visit should be Pre(body)"
        );
        assert_eq!(
            walker.visits[4],
            (VisitPhase::Post, IrKind::Action),
            "fifth visit should be Post(body)"
        );
        assert_eq!(
            walker.visits[5],
            (VisitPhase::Post, IrKind::Handle),
            "last visit should be Post(Handle)"
        );
    }

    // ── Edge cases and additional coverage ───────────────────────────────

    #[test]
    fn handler_side_table_empty_by_default() {
        let table = HandlerSideTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());
    }

    #[test]
    fn handler_side_table_get_mut_allows_mutation() {
        let mut table = HandlerSideTable::new();
        let handle_id = IrNodeId::new(1).unwrap();
        let op_id = IrNodeId::new(2).unwrap();

        let info = HandlerInfo {
            effect: EffectId(42),
            ops: vec![("read".to_string(), op_id)],
            ret: None,
            finally: None,
        };

        table.insert(handle_id, info);

        // Mutate via get_mut
        if let Some(info_mut) = table.get_mut(handle_id) {
            info_mut.ret = Some(IrNodeId::new(99).unwrap());
        }

        // Verify mutation took effect
        let retrieved = table.get(handle_id).unwrap();
        assert_eq!(retrieved.ret, Some(IrNodeId::new(99).unwrap()));
    }

    #[test]
    fn pretty_handler_with_all_fields_populated() {
        let mut arena = IrArena::new();
        let ret_id = arena.alloc(IrKind::Lambda, span());
        let finally_id = arena.alloc(IrKind::Action, span());
        let op1_id = arena.alloc(IrKind::Lambda, span());
        let op2_id = arena.alloc(IrKind::Lambda, span());
        let op3_id = arena.alloc(IrKind::Lambda, span());

        let info = HandlerInfo {
            effect: EffectId(99),
            ops: vec![
                ("op_a".to_string(), op1_id),
                ("op_b".to_string(), op2_id),
                ("op_c".to_string(), op3_id),
            ],
            ret: Some(ret_id),
            finally: Some(finally_id),
        };

        let formatted = pretty_handler(&info, &arena);

        // All fields should be present
        assert!(
            formatted.contains("Handler<99>"),
            "formatted output should contain 'Handler<99>', but got: {}",
            formatted
        );
        assert!(formatted.contains("op_a:"));
        assert!(formatted.contains("op_b:"));
        assert!(formatted.contains("op_c:"));
        assert!(formatted.contains(&format!("ret: Some({})", ret_id)));
        assert!(formatted.contains(&format!("finally: Some({})", finally_id)));
    }

    #[test]
    fn handler_info_clone_independence() {
        let op_id = IrNodeId::new(42).unwrap();
        let info1 = HandlerInfo {
            effect: EffectId(1),
            ops: vec![("op".to_string(), op_id)],
            ret: None,
            finally: None,
        };

        let mut info2 = info1.clone();
        info2.effect = EffectId(999);
        info2.ops.push(("op2".to_string(), op_id));

        // Verify original is unchanged
        assert_eq!(info1.effect, EffectId(1));
        assert_eq!(info1.ops.len(), 1);

        // Verify clone has changes
        assert_eq!(info2.effect, EffectId(999));
        assert_eq!(info2.ops.len(), 2);
    }
}
