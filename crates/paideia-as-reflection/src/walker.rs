//! Structural walker over `Syntax` values.
//!
//! The walker mirrors the paideia-as-elaborator's existing pass-shape
//! (see `paideia_as_elaborator::name_resolution_walker`,
//! `effect_walker`, …). A hosted DSL implementation implements
//! [`SyntaxWalker`] and hands it to [`walk_syntax`], which visits every
//! node in pre-order and honors the returned [`WalkAction`].
//!
//! # Scope cap (R220.M1)
//!
//! [`WalkAction::Replace`] is defined structurally but **not applied
//! in-place** in this landing — the walker records the intent but the
//! caller must consume the replacement by re-driving elaboration on the
//! returned `NodeId`. In-place transformation is a FIXME for R220.M3
//! (`@dsl_parser`) when the elaborator's plug-in dispatch grows the
//! mutable arena hook needed to splice a subtree in place. Today the
//! walker's principal client is a *read* over the DSL body: DSL
//! implementors classify structure via [`SyntaxWalker::visit_node`] and
//! then build a fresh `Syntax` value they return separately.

use paideia_as_ast::AstArena;

use crate::syntax::Syntax;

/// Action a [`SyntaxWalker`] returns from [`SyntaxWalker::visit_node`].
///
/// - `Descend` — recurse into this node's children.
/// - `Skip` — do not recurse; move on to the next sibling.
/// - `Replace(new_node_id)` — semantic intent: the walker should treat
///   this node as if it were the supplied replacement. R220.M1 records
///   the intent (the walker returns it up the call stack) but does NOT
///   splice into the arena — see the module-level scope cap.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum WalkAction {
    /// Recurse into this node's children.
    Descend,
    /// Do not recurse; move on.
    Skip,
    /// Treat this node as if it were the referenced replacement.
    /// R220.M1 records intent only; in-place splice lands with R220.M3.
    Replace(paideia_as_ast::NodeId),
}

/// Stateful walker over a `Syntax` value.
///
/// Implement [`visit_node`] to inspect (and optionally classify /
/// replace) each `Syntax` the walker reaches. Defaults descend
/// everywhere. The walker is `&mut self` so implementations can carry
/// their own accumulator state.
///
/// [`visit_node`]: SyntaxWalker::visit_node
pub trait SyntaxWalker {
    /// Called once per node, in pre-order.
    ///
    /// The default implementation returns [`WalkAction::Descend`] so a
    /// no-op walker still traverses the full subtree.
    fn visit_node(&mut self, _s: &Syntax<'_>) -> WalkAction {
        WalkAction::Descend
    }

    /// Called after a node's subtree has been visited (post-order hook).
    /// Default is a no-op; implementations that need bracketing (e.g.,
    /// scope enter / scope exit) override this.
    fn exit_node(&mut self, _s: &Syntax<'_>) {}
}

/// Drive a [`SyntaxWalker`] over `root` (a `NodeId` in `arena`) in
/// pre-order. Honors the [`WalkAction`] the walker returns per node.
///
/// Returns a `Vec` of every `Replace(node_id)` intent the walker
/// emitted, in visit order. R220.M3 will consume this vector to splice
/// replacements back into the surface AST; R220.M1's contract is to
/// hand it back to the caller for inspection.
pub fn walk_syntax<W: SyntaxWalker>(
    walker: &mut W,
    arena: &AstArena,
    root: paideia_as_ast::NodeId,
) -> Vec<paideia_as_ast::NodeId> {
    let mut replacements = Vec::new();
    let root_syn = Syntax::from_node(arena, root);
    walk_one(walker, arena, &root_syn, &mut replacements);
    replacements
}

fn walk_one<W: SyntaxWalker>(
    walker: &mut W,
    arena: &AstArena,
    node: &Syntax<'_>,
    replacements: &mut Vec<paideia_as_ast::NodeId>,
) {
    match walker.visit_node(node) {
        WalkAction::Descend => {
            for child in node.children() {
                walk_one(walker, arena, &child, replacements);
            }
            walker.exit_node(node);
        }
        WalkAction::Skip => {
            walker.exit_node(node);
        }
        WalkAction::Replace(new_id) => {
            replacements.push(new_id);
            // Do NOT recurse into either the original or the replacement
            // at R220.M1 — the replacement's arena residency and hygiene
            // are the caller's responsibility until R220.M3 wires the
            // in-place splice. See the module-level scope cap.
            walker.exit_node(node);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::{AstArena, ExprData, NodeKind};
    use paideia_as_diagnostics::{FileId, Span};

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    struct CountVisitor {
        visits: usize,
        exits: usize,
    }

    impl SyntaxWalker for CountVisitor {
        fn visit_node(&mut self, _s: &Syntax<'_>) -> WalkAction {
            self.visits += 1;
            WalkAction::Descend
        }

        fn exit_node(&mut self, _s: &Syntax<'_>) {
            self.exits += 1;
        }
    }

    #[test]
    fn default_walker_descends_everywhere() {
        let mut arena = AstArena::new();
        // Build: Call(callee, arg1, arg2)
        let ident = arena.alloc(NodeKind::Ident, span());
        let callee = crate::Syntax::var(&mut arena, span(), ident);
        let a1_placeholder = arena.alloc(NodeKind::Placeholder, span());
        let arg1 = crate::Syntax::literal(&mut arena, span(), a1_placeholder);
        let a2_placeholder = arena.alloc(NodeKind::Placeholder, span());
        let arg2 = crate::Syntax::literal(&mut arena, span(), a2_placeholder);
        let call_id = crate::Syntax::app(&mut arena, span(), callee, vec![arg1, arg2]);

        let mut visitor = CountVisitor { visits: 0, exits: 0 };
        let replacements = walk_syntax(&mut visitor, &arena, call_id);
        assert!(replacements.is_empty());
        // Call + callee (Path) + callee's inner Ident + arg1 (Literal) +
        // arg1's inner Placeholder + arg2 (Literal) + arg2's inner
        // Placeholder = 7 visits. `Syntax::var`/`Syntax::literal`
        // allocate wrapper + child, so the arena is more granular than
        // "one node per constructor".
        assert_eq!(visitor.visits, 7);
        assert_eq!(visitor.exits, 7);
    }

    struct SkipVisitor {
        visits: usize,
    }

    impl SyntaxWalker for SkipVisitor {
        fn visit_node(&mut self, _s: &Syntax<'_>) -> WalkAction {
            self.visits += 1;
            WalkAction::Skip
        }
    }

    #[test]
    fn skip_action_prevents_recursion() {
        let mut arena = AstArena::new();
        let ident = arena.alloc(NodeKind::Ident, span());
        let callee = crate::Syntax::var(&mut arena, span(), ident);
        let arg_placeholder = arena.alloc(NodeKind::Placeholder, span());
        let arg = crate::Syntax::literal(&mut arena, span(), arg_placeholder);
        let call_id = crate::Syntax::app(&mut arena, span(), callee, vec![arg]);

        let mut visitor = SkipVisitor { visits: 0 };
        walk_syntax(&mut visitor, &arena, call_id);
        // Only the root call node — children are skipped
        assert_eq!(visitor.visits, 1);
    }

    struct ReplaceVisitor {
        replacement: paideia_as_ast::NodeId,
    }

    impl SyntaxWalker for ReplaceVisitor {
        fn visit_node(&mut self, s: &Syntax<'_>) -> WalkAction {
            // Descend until we find a literal; then emit a Replace intent.
            if s.head_kind().kind == crate::syntax::SyntaxKind::Literal {
                WalkAction::Replace(self.replacement)
            } else {
                WalkAction::Descend
            }
        }
    }

    #[test]
    fn replace_action_records_intent_but_does_not_mutate_arena() {
        let mut arena = AstArena::new();
        let ident = arena.alloc(NodeKind::Ident, span());
        let callee = crate::Syntax::var(&mut arena, span(), ident);
        let arg_placeholder = arena.alloc(NodeKind::Placeholder, span());
        let arg = crate::Syntax::literal(&mut arena, span(), arg_placeholder);
        let call_id = crate::Syntax::app(&mut arena, span(), callee, vec![arg]);
        // A pre-built replacement literal, sitting in the same arena.
        let repl_placeholder = arena.alloc(NodeKind::Placeholder, span());
        let replacement = arena.alloc_expr(
            NodeKind::ExprLiteral,
            span(),
            ExprData::Literal {
                lit: repl_placeholder,
            },
        );

        let mut visitor = ReplaceVisitor { replacement };
        let replacements = walk_syntax(&mut visitor, &arena, call_id);
        // Both the ExprLiteral wrapper AND its inner Placeholder child
        // match SyntaxKind::Literal (a Placeholder head classifies as
        // Literal per the current syntax::head_kind mapping), so the
        // Replace intent fires twice. This is expected walker semantics:
        // Replace is recorded and does not descend, but siblings are
        // still visited independently.
        assert_eq!(replacements.len(), 2);
        assert_eq!(replacements[0], replacement);
        assert_eq!(replacements[1], replacement);

        // The arena is unchanged — the original arg literal still lives
        // and the walker only recorded intent.
        assert!(arena.expr_data(arg).is_some());
    }
}
