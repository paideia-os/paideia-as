//! Administrative Normal Form (ANF) pass per `custom-assembler.md` §6.2.
//!
//! ANF makes every nontrivial subexpression a named binding. After ANF
//! every function call's arguments are atomic — variables or literals.
//!
//! Phase-1 implementation: the IR arena is currently a flat list of
//! nodes without explicit parent-child links (those land in a future
//! IR refinement). The ANF *primitive* exposed here operates on the
//! per-call shape: given an operand list and an arena, it lifts each
//! non-atomic operand into a fresh `Let`-bound `Var`, returning the
//! prefix of let-bindings and the rewritten operand vector. Downstream
//! passes call this primitive on every call/binop/app site to produce
//! the ANF form bottom-up.
//!
//! A node is **atomic** iff its `IrKind` is `Var` or `Literal`. Every
//! other kind is non-atomic and gets a fresh `Let` binding around its
//! original computation. The new `Var` reference (the binding's
//! occurrence) replaces the original operand in the call site.

use crate::arena::IrArena;
use crate::handler_value::HandlerInfo;
use crate::node::{IrKind, IrNodeId};

/// Result of ANF-normalising an operand list.
#[derive(Debug, Clone)]
pub struct AnfRewrite {
    /// Let-binding ids prepended (in order) before the rewritten use.
    /// Each entry is the `IrKind::Let` node introduced for a hoisted
    /// non-atomic operand. The caller threads these into its block /
    /// statement list before the use.
    pub bindings: Vec<IrNodeId>,
    /// Rewritten operand list: same length as the input, with every
    /// non-atomic id replaced by a `Var` referring to its `Let`.
    pub operands: Vec<IrNodeId>,
}

/// Returns `true` iff `kind` is an ANF-atomic kind.
///
/// Phase-1 atomic kinds: `Var`, `Literal`. Everything else (Module,
/// Functor, Let, Lambda, App, Perform, Handle, Action, Unsafe,
/// Placeholder) is non-atomic and gets hoisted.
#[must_use]
pub fn is_atomic(kind: IrKind) -> bool {
    matches!(kind, IrKind::Var | IrKind::Literal)
}

/// Normalize a call's operand list to ANF.
///
/// For each operand:
/// - If atomic (`Var` or `Literal`): pass through unchanged.
/// - Otherwise: allocate one `Let` binding spanning the operand's span;
///   allocate one `Var` referring to it; substitute the new `Var` for
///   the operand. The `Let` is appended to `bindings`; the `Var` to
///   `operands`.
///
/// The function is **idempotent**: applying it twice yields the same
/// `AnfRewrite` (the second pass sees `Var` operands and short-circuits).
#[must_use]
pub fn normalise_operands(arena: &mut IrArena, operands: &[IrNodeId]) -> AnfRewrite {
    let mut bindings = Vec::new();
    let mut rewritten = Vec::with_capacity(operands.len());

    for &id in operands {
        let node = arena[id];
        if is_atomic(node.kind) {
            rewritten.push(id);
            continue;
        }
        // Allocate a Let binding spanning the operand's span; allocate
        // a Var for its referent. The Let "wraps" the original node
        // semantically; physically it just claims the span — the
        // actual nested IR is the caller's responsibility once IR
        // child-pointer wiring lands.
        let let_id = arena.alloc(IrKind::Let, node.span);
        let var_id = arena.alloc(IrKind::Var, node.span);
        bindings.push(let_id);
        rewritten.push(var_id);
    }

    AnfRewrite {
        bindings,
        operands: rewritten,
    }
}

/// Normalise a Perform node: hoist its argument operands into Let
/// bindings if any are non-atomic.
///
/// Returns `AnfRewrite { bindings, operands }` where `operands` are the
/// (possibly-rewritten) argument NodeIds and `bindings` are the
/// fresh Let nodes the caller must prepend before the Perform.
#[must_use]
pub fn normalise_perform_args(arena: &mut IrArena, arg_operands: &[IrNodeId]) -> AnfRewrite {
    normalise_operands(arena, arg_operands)
}

/// Normalise a Resume node: hoist the resume value if non-atomic.
///
/// Returns `AnfRewrite { bindings, operands }` where `operands`
/// contains the (possibly-rewritten) resume value and `bindings` are
/// the fresh Let nodes the caller must prepend.
#[must_use]
pub fn normalise_resume_value(arena: &mut IrArena, value: IrNodeId) -> AnfRewrite {
    normalise_operands(arena, &[value])
}

/// Normalise a handler-body op impl: hoist its return value if
/// non-atomic. The handler's op-impl bodies are processed individually.
///
/// Returns `AnfRewrite { bindings, operands }` where `operands`
/// contains the (possibly-rewritten) op body and `bindings` are
/// the fresh Let nodes the caller must prepend.
#[must_use]
pub fn normalise_handler_op_body(arena: &mut IrArena, op_body: IrNodeId) -> AnfRewrite {
    normalise_operands(arena, &[op_body])
}

/// Normalise a `finally` clause body: same shape as op body — hoist
/// the return value if non-atomic so the join point is fed by an
/// atom.
///
/// Returns `AnfRewrite { bindings, operands }` where `operands`
/// contains the (possibly-rewritten) finally body and `bindings` are
/// the fresh Let nodes the caller must prepend.
#[must_use]
pub fn normalise_finally_clause(arena: &mut IrArena, finally_body: IrNodeId) -> AnfRewrite {
    normalise_operands(arena, &[finally_body])
}

/// Normalise a full Handle node's payload. For each op in the
/// HandlerInfo, normalise its impl body. Also normalise the ret
/// and finally clauses if present.
///
/// Returns `(bindings, rewritten_handler_info)` where `bindings` are
/// the collected Let nodes from normalising all op bodies, ret, and
/// finally clauses. The caller must thread these Let nodes before the
/// Handle. The `rewritten_handler_info` is a clone of the original with
/// its op bodies, ret, and finally body replaced by rewritten IrNodeIds.
#[must_use]
pub fn normalise_handler(
    arena: &mut IrArena,
    handler_info: &HandlerInfo,
) -> (Vec<IrNodeId>, HandlerInfo) {
    let mut all_bindings = Vec::new();
    let mut rewritten_ops = Vec::new();

    // Normalise each op body.
    for (op_name, op_body_id) in &handler_info.ops {
        let rewrite = normalise_handler_op_body(arena, *op_body_id);
        all_bindings.extend(rewrite.bindings);
        // Rewrite captured the (possibly hoisted) op body.
        let rewritten_body_id = if rewrite.operands.is_empty() {
            *op_body_id
        } else {
            rewrite.operands[0]
        };
        rewritten_ops.push((op_name.clone(), rewritten_body_id));
    }

    // Normalise ret clause if present.
    let rewritten_ret = if let Some(ret_id) = handler_info.ret {
        let rewrite = normalise_operands(arena, &[ret_id]);
        all_bindings.extend(rewrite.bindings);
        if rewrite.operands.is_empty() {
            Some(ret_id)
        } else {
            Some(rewrite.operands[0])
        }
    } else {
        None
    };

    // Normalise finally clause if present.
    let rewritten_finally = if let Some(finally_id) = handler_info.finally {
        let rewrite = normalise_finally_clause(arena, finally_id);
        all_bindings.extend(rewrite.bindings);
        if rewrite.operands.is_empty() {
            Some(finally_id)
        } else {
            Some(rewrite.operands[0])
        }
    } else {
        None
    };

    let rewritten_info = HandlerInfo {
        effect: handler_info.effect,
        ops: rewritten_ops,
        ret: rewritten_ret,
        finally: rewritten_finally,
    };

    (all_bindings, rewritten_info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{FileId, Span};

    fn span(byte_start: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), byte_start, 1)
    }

    // ── Atomicity ────────────────────────────────────────────────────

    #[test]
    fn var_and_literal_are_atomic() {
        assert!(is_atomic(IrKind::Var));
        assert!(is_atomic(IrKind::Literal));
    }

    #[test]
    fn other_kinds_are_not_atomic() {
        for k in [
            IrKind::Module,
            IrKind::Functor,
            IrKind::Let,
            IrKind::Lambda,
            IrKind::App,
            IrKind::Perform,
            IrKind::Handle,
            IrKind::Action,
            IrKind::Unsafe,
            IrKind::Placeholder,
        ] {
            assert!(!is_atomic(k));
        }
    }

    // ── AC bullet 1: f(g(1) + h(2)) ──────────────────────────────────

    #[test]
    fn anf_lifts_non_atomic_operands() {
        // Build operands [App(g(1)), App(h(2))]. After ANF, the
        // rewrite should contain two Let bindings and two Var operands.
        let mut arena = IrArena::new();
        let g_call = arena.alloc(IrKind::App, span(0));
        let h_call = arena.alloc(IrKind::App, span(10));
        let rewrite = normalise_operands(&mut arena, &[g_call, h_call]);
        assert_eq!(rewrite.bindings.len(), 2);
        assert_eq!(rewrite.operands.len(), 2);
        for &id in &rewrite.operands {
            assert_eq!(arena[id].kind, IrKind::Var);
        }
        for &id in &rewrite.bindings {
            assert_eq!(arena[id].kind, IrKind::Let);
        }
    }

    #[test]
    fn anf_passes_atomic_operands_unchanged() {
        let mut arena = IrArena::new();
        let v = arena.alloc(IrKind::Var, span(0));
        let l = arena.alloc(IrKind::Literal, span(10));
        let rewrite = normalise_operands(&mut arena, &[v, l]);
        assert!(rewrite.bindings.is_empty());
        assert_eq!(rewrite.operands, vec![v, l]);
    }

    #[test]
    fn anf_mixed_operands_hoists_only_non_atomic() {
        let mut arena = IrArena::new();
        let v = arena.alloc(IrKind::Var, span(0));
        let app = arena.alloc(IrKind::App, span(10));
        let l = arena.alloc(IrKind::Literal, span(20));
        let rewrite = normalise_operands(&mut arena, &[v, app, l]);
        assert_eq!(rewrite.bindings.len(), 1);
        assert_eq!(rewrite.operands.len(), 3);
        assert_eq!(arena[rewrite.operands[0]].kind, IrKind::Var); // v unchanged
        assert_eq!(arena[rewrite.operands[1]].kind, IrKind::Var); // hoisted
        assert_eq!(arena[rewrite.operands[2]].kind, IrKind::Literal); // unchanged
    }

    // ── AC bullet 2: idempotence ─────────────────────────────────────

    #[test]
    fn anf_is_idempotent() {
        let mut arena = IrArena::new();
        let app = arena.alloc(IrKind::App, span(0));
        let first = normalise_operands(&mut arena, &[app]);
        // Second pass over the rewritten operands.
        let second = normalise_operands(&mut arena, &first.operands);
        assert!(second.bindings.is_empty());
        assert_eq!(second.operands, first.operands);
    }

    // ── AC bullet 3: span preservation ───────────────────────────────

    #[test]
    fn synthesized_let_and_var_inherit_operand_span() {
        let mut arena = IrArena::new();
        let s = span(42);
        let app = arena.alloc(IrKind::App, s);
        let rewrite = normalise_operands(&mut arena, &[app]);
        assert_eq!(arena[rewrite.bindings[0]].span, s);
        assert_eq!(arena[rewrite.operands[0]].span, s);
    }

    // ── Edge cases ───────────────────────────────────────────────────

    #[test]
    fn empty_operand_list_yields_empty_rewrite() {
        let mut arena = IrArena::new();
        let rewrite = normalise_operands(&mut arena, &[]);
        assert!(rewrite.bindings.is_empty());
        assert!(rewrite.operands.is_empty());
    }

    #[test]
    fn order_preserved_for_multiple_hoists() {
        let mut arena = IrArena::new();
        let a = arena.alloc(IrKind::App, span(0));
        let b = arena.alloc(IrKind::Action, span(10));
        let c = arena.alloc(IrKind::Handle, span(20));
        let rewrite = normalise_operands(&mut arena, &[a, b, c]);
        assert_eq!(rewrite.bindings.len(), 3);
        // The let bindings span the same byte-starts as the originals.
        assert_eq!(arena[rewrite.bindings[0]].span.byte_start(), 0);
        assert_eq!(arena[rewrite.bindings[1]].span.byte_start(), 10);
        assert_eq!(arena[rewrite.bindings[2]].span.byte_start(), 20);
    }

    #[test]
    fn each_hoist_allocates_two_new_nodes() {
        let mut arena = IrArena::new();
        let baseline = arena.len();
        let app = arena.alloc(IrKind::App, span(0));
        assert_eq!(arena.len(), baseline + 1);
        let _ = normalise_operands(&mut arena, &[app]);
        // One Let + one Var added per hoisted operand.
        assert_eq!(arena.len(), baseline + 3);
    }

    // ── Perform + Resume + Handler ───────────────────────────────────

    #[test]
    fn anf_perform_with_atomic_arg_no_hoist() {
        let mut arena = IrArena::new();
        let v = arena.alloc(IrKind::Var, span(0));
        let rewrite = normalise_perform_args(&mut arena, &[v]);
        assert!(rewrite.bindings.is_empty());
        assert_eq!(rewrite.operands, vec![v]);
    }

    #[test]
    fn anf_perform_with_non_atomic_arg_hoists() {
        let mut arena = IrArena::new();
        let app = arena.alloc(IrKind::App, span(0));
        let rewrite = normalise_perform_args(&mut arena, &[app]);
        assert_eq!(rewrite.bindings.len(), 1);
        assert_eq!(rewrite.operands.len(), 1);
        assert_eq!(arena[rewrite.bindings[0]].kind, IrKind::Let);
        assert_eq!(arena[rewrite.operands[0]].kind, IrKind::Var);
    }

    #[test]
    fn anf_resume_with_atomic_value_no_hoist() {
        let mut arena = IrArena::new();
        let v = arena.alloc(IrKind::Var, span(0));
        let rewrite = normalise_resume_value(&mut arena, v);
        assert!(rewrite.bindings.is_empty());
        assert_eq!(rewrite.operands.len(), 1);
        assert_eq!(rewrite.operands[0], v);
    }

    #[test]
    fn anf_resume_with_non_atomic_value_hoists() {
        let mut arena = IrArena::new();
        let app = arena.alloc(IrKind::App, span(0));
        let rewrite = normalise_resume_value(&mut arena, app);
        assert_eq!(rewrite.bindings.len(), 1);
        assert_eq!(rewrite.operands.len(), 1);
        assert_eq!(arena[rewrite.bindings[0]].kind, IrKind::Let);
        assert_eq!(arena[rewrite.operands[0]].kind, IrKind::Var);
    }

    #[test]
    fn anf_handler_op_body_atomic_no_hoist() {
        let mut arena = IrArena::new();
        let v = arena.alloc(IrKind::Var, span(0));
        let rewrite = normalise_handler_op_body(&mut arena, v);
        assert!(rewrite.bindings.is_empty());
        assert_eq!(rewrite.operands.len(), 1);
        assert_eq!(rewrite.operands[0], v);
    }

    #[test]
    fn anf_handler_op_body_non_atomic_hoists() {
        let mut arena = IrArena::new();
        let action = arena.alloc(IrKind::Action, span(0));
        let rewrite = normalise_handler_op_body(&mut arena, action);
        assert_eq!(rewrite.bindings.len(), 1);
        assert_eq!(rewrite.operands.len(), 1);
        assert_eq!(arena[rewrite.bindings[0]].kind, IrKind::Let);
        assert_eq!(arena[rewrite.operands[0]].kind, IrKind::Var);
    }

    #[test]
    fn anf_finally_clause_non_atomic_hoists() {
        let mut arena = IrArena::new();
        let app = arena.alloc(IrKind::App, span(0));
        let rewrite = normalise_finally_clause(&mut arena, app);
        assert_eq!(rewrite.bindings.len(), 1);
        assert_eq!(rewrite.operands.len(), 1);
        assert_eq!(arena[rewrite.bindings[0]].kind, IrKind::Let);
        assert_eq!(arena[rewrite.operands[0]].kind, IrKind::Var);
    }

    #[test]
    fn anf_normalise_handler_processes_all_ops() {
        use crate::handler_value::{EffectId, HandlerInfo};

        let mut arena = IrArena::new();
        let op1_body = arena.alloc(IrKind::App, span(0));
        let op2_body = arena.alloc(IrKind::Action, span(10));

        let info = HandlerInfo {
            effect: EffectId(42),
            ops: vec![("op1".to_string(), op1_body), ("op2".to_string(), op2_body)],
            ret: None,
            finally: None,
        };

        let (bindings, rewritten) = normalise_handler(&mut arena, &info);

        // Both op bodies are non-atomic, so we expect 2 bindings.
        assert_eq!(bindings.len(), 2);
        for binding_id in &bindings {
            assert_eq!(arena[*binding_id].kind, IrKind::Let);
        }

        // Ops should be rewritten with Var references.
        assert_eq!(rewritten.ops.len(), 2);
        assert_eq!(rewritten.ops[0].0, "op1");
        assert_eq!(rewritten.ops[1].0, "op2");
        assert_eq!(arena[rewritten.ops[0].1].kind, IrKind::Var);
        assert_eq!(arena[rewritten.ops[1].1].kind, IrKind::Var);

        // Effect should be unchanged.
        assert_eq!(rewritten.effect, EffectId(42));
        assert_eq!(rewritten.ret, None);
        assert_eq!(rewritten.finally, None);
    }

    #[test]
    fn anf_normalise_handler_with_ret_and_finally() {
        use crate::handler_value::{EffectId, HandlerInfo};

        let mut arena = IrArena::new();
        let op_body = arena.alloc(IrKind::App, span(0));
        let ret_body = arena.alloc(IrKind::App, span(10));
        let finally_body = arena.alloc(IrKind::Action, span(20));

        let info = HandlerInfo {
            effect: EffectId(7),
            ops: vec![("read".to_string(), op_body)],
            ret: Some(ret_body),
            finally: Some(finally_body),
        };

        let (bindings, rewritten) = normalise_handler(&mut arena, &info);

        // Op body + ret + finally all non-atomic => 3 bindings.
        assert_eq!(bindings.len(), 3);

        // Verify all are Let bindings.
        for binding_id in &bindings {
            assert_eq!(arena[*binding_id].kind, IrKind::Let);
        }

        // Verify op, ret, finally are all rewritten to Vars.
        assert_eq!(arena[rewritten.ops[0].1].kind, IrKind::Var);
        assert!(rewritten.ret.is_some());
        assert_eq!(arena[rewritten.ret.unwrap()].kind, IrKind::Var);
        assert!(rewritten.finally.is_some());
        assert_eq!(arena[rewritten.finally.unwrap()].kind, IrKind::Var);
    }

    #[test]
    fn anf_idempotence_on_handler_bodies() {
        use crate::handler_value::{EffectId, HandlerInfo};

        let mut arena = IrArena::new();
        let op_body = arena.alloc(IrKind::App, span(0));

        let info = HandlerInfo {
            effect: EffectId(1),
            ops: vec![("op".to_string(), op_body)],
            ret: None,
            finally: None,
        };

        let (first_bindings, first_rewritten) = normalise_handler(&mut arena, &info);
        assert_eq!(first_bindings.len(), 1);

        // Apply normalise_handler again to the rewritten info.
        let (second_bindings, _second_rewritten) = normalise_handler(&mut arena, &first_rewritten);

        // Second pass should produce no additional bindings (op body is now a Var).
        assert!(second_bindings.is_empty());
    }

    #[test]
    fn anf_handler_with_atomic_ops_produces_no_bindings() {
        use crate::handler_value::{EffectId, HandlerInfo};

        let mut arena = IrArena::new();
        let v1 = arena.alloc(IrKind::Var, span(0));
        let v2 = arena.alloc(IrKind::Var, span(10));

        let info = HandlerInfo {
            effect: EffectId(99),
            ops: vec![("op1".to_string(), v1), ("op2".to_string(), v2)],
            ret: None,
            finally: None,
        };

        let (bindings, rewritten) = normalise_handler(&mut arena, &info);

        // Both op bodies are Vars (atomic), so no bindings.
        assert!(bindings.is_empty());

        // Ops should remain unchanged.
        assert_eq!(rewritten.ops[0].1, v1);
        assert_eq!(rewritten.ops[1].1, v2);
    }

    #[test]
    fn anf_normalise_perform_delegates_to_operands() {
        let mut arena = IrArena::new();
        let app = arena.alloc(IrKind::App, span(0));
        let v = arena.alloc(IrKind::Var, span(10));

        // normalise_perform_args with mixed operands
        let rewrite = normalise_perform_args(&mut arena, &[app, v]);
        assert_eq!(rewrite.bindings.len(), 1);
        assert_eq!(rewrite.operands.len(), 2);
        assert_eq!(arena[rewrite.operands[0]].kind, IrKind::Var); // hoisted app
        assert_eq!(rewrite.operands[1], v); // v unchanged
    }

    #[test]
    fn anf_resume_normalise_single_operand() {
        let mut arena = IrArena::new();
        let app = arena.alloc(IrKind::App, span(0));
        let rewrite = normalise_resume_value(&mut arena, app);
        assert_eq!(rewrite.bindings.len(), 1);
        assert_eq!(rewrite.operands.len(), 1);
        assert_eq!(arena[rewrite.bindings[0]].kind, IrKind::Let);
        assert_eq!(arena[rewrite.operands[0]].kind, IrKind::Var);
    }
}
