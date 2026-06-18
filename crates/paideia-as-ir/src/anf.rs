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
}
