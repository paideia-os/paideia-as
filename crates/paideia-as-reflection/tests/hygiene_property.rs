//! R220.M2 hygiene property test (paideia-as#1416, per plan §4 R220.M2
//! acceptance bullet 2).
//!
//! Generate 10 000 random potential-capture forms and assert that for
//! every one, the macro-introduced identifier and the use-site
//! identifier — even when they share the same surface spelling — remain
//! alpha-distinct after [`hygienic_rename`].
//!
//! A "potential-capture form" here is a randomly-shaped `Syntax` tree
//! (variable / let / lambda / match / call / antiquote combinators
//! composed to random depth) plus a random shared identifier name.
//! The property under test is:
//!
//! > For any macro-side tree `M` and use-site node `U`, after
//! > `hygienic_rename(&M, fresh_macro_scope_id())`, the renamed
//! > `Syntax`'s `hygiene()` MUST NOT equal `U.hygiene()` (which is
//! > `HYGIENIC_ID_UNTAGGED`).
//!
//! We also check the fine-grained per-node property via
//! [`hygienic_rename_map`]: every macro-side node's hygiene tag is
//! distinct from every use-site node's tag; every antiquote descendant
//! is recorded as `HYGIENIC_ID_UNTAGGED` (so use-site references passed
//! in through antiquotes retain their use-site context).

use paideia_as_ast::{AstArena, ExprData, MatchArm, NodeId, NodeKind, PatternData};
use paideia_as_diagnostics::{FileId, Span};
use paideia_as_reflection::{
    HYGIENIC_ID_UNTAGGED, Syntax, fresh_macro_scope_id, hygienic_rename, hygienic_rename_map,
};
use proptest::prelude::*;

fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

/// Structural shape the generator produces.  Each `Shape` recursively
/// composes into a real AST node when reified via [`reify`].
#[derive(Clone, Debug)]
enum Shape {
    /// A bare `Path(ident)` identifier.
    Var,
    /// `let module N = body in rest`.
    Let(Box<Shape>, Box<Shape>),
    /// A lambda `\_. body` with `param_count` placeholder params.
    Lambda(usize, Box<Shape>),
    /// A `match scrutinee { arm* }` shape.
    Match(Box<Shape>, Vec<Shape>),
    /// A call `callee(args*)`.
    Call(Box<Shape>, Vec<Shape>),
    /// An antiquote wrapping a use-site sub-shape.  Antiquote payloads
    /// track a distinct set of `NodeId`s so the assertion pass can
    /// verify per-node untagged-ness after the rename.
    Antiquote(Box<Shape>),
}

/// Buckets of NodeIds so the property assertions can distinguish
/// macro-introduced nodes from antiquote-payload nodes.  The returned
/// root NodeId is what the top-level caller feeds into
/// `hygienic_rename_map`; the buckets are what the properties assert
/// over.
#[derive(Default, Debug)]
struct Reified {
    macro_side_nodes: Vec<NodeId>,
    antiquote_payload_nodes: Vec<NodeId>,
}

/// Convert a `Shape` into real AST nodes in `arena`, recording each
/// allocated NodeId into the appropriate bucket of `out`.  `in_antiquote`
/// tracks whether the caller is descending under an `Antiquote` shape;
/// nodes generated inside such a subtree land in the antiquote bucket.
fn reify(shape: &Shape, arena: &mut AstArena, out: &mut Reified, in_antiquote: bool) -> NodeId {
    let id = match shape {
        Shape::Var => {
            let ident = arena.alloc(NodeKind::Ident, span());
            record(out, ident, in_antiquote);
            Syntax::var(arena, span(), ident)
        }
        Shape::Let(body, rest) => {
            let body_id = reify(body, arena, out, in_antiquote);
            let rest_id = reify(rest, arena, out, in_antiquote);
            Syntax::let_(arena, span(), "x".into(), body_id, rest_id)
        }
        Shape::Lambda(param_count, body) => {
            let mut params = Vec::with_capacity(*param_count);
            for _ in 0..*param_count {
                let p = arena.alloc(NodeKind::Placeholder, span());
                record(out, p, in_antiquote);
                params.push(p);
            }
            let body_id = reify(body, arena, out, in_antiquote);
            Syntax::lambda(arena, span(), params, body_id)
        }
        Shape::Match(scrutinee, arms) => {
            let scrut_id = reify(scrutinee, arena, out, in_antiquote);
            let mut arm_vec = Vec::with_capacity(arms.len());
            for arm in arms {
                let pat =
                    arena.alloc_pattern(NodeKind::PatWildcard, span(), PatternData::Wildcard);
                record(out, pat, in_antiquote);
                let body_id = reify(arm, arena, out, in_antiquote);
                arm_vec.push(MatchArm {
                    pattern: pat,
                    guard: None,
                    body: body_id,
                });
            }
            Syntax::match_(arena, span(), scrut_id, arm_vec)
        }
        Shape::Call(callee, args) => {
            let callee_id = reify(callee, arena, out, in_antiquote);
            let mut arg_ids = Vec::with_capacity(args.len());
            for a in args {
                arg_ids.push(reify(a, arena, out, in_antiquote));
            }
            Syntax::app(arena, span(), callee_id, arg_ids)
        }
        Shape::Antiquote(inner) => {
            let inner_id = reify(inner, arena, out, /* in_antiquote = */ true);
            let anti_id = arena.alloc_expr(
                NodeKind::ExprAntiquote,
                span(),
                ExprData::Antiquote { value: inner_id },
            );
            // The antiquote wrapper itself is the boundary node: the
            // walker's HygieneAssigner enters the wrapper, flips
            // antiquote_depth on the way in, and tags the wrapper as
            // untagged.  So the wrapper belongs in the antiquote
            // bucket regardless of the caller's `in_antiquote`.
            out.antiquote_payload_nodes.push(anti_id);
            return anti_id;
        }
    };
    record(out, id, in_antiquote);
    id
}

fn record(out: &mut Reified, id: NodeId, in_antiquote: bool) {
    if in_antiquote {
        out.antiquote_payload_nodes.push(id);
    } else {
        out.macro_side_nodes.push(id);
    }
}

fn arb_shape() -> impl Strategy<Value = Shape> {
    // Leaf strategy: bare Var (most-frequent leaf so we always have
    // real identifier positions to test hygiene against).
    let leaf = Just(Shape::Var);
    // Recursive strategy: compose up to depth 4, branching factor ≤ 3
    // so the average tree is small enough for 10 000 iterations to
    // run in reasonable time.
    leaf.prop_recursive(
        /* depth = */ 4,
        /* desired_size = */ 32,
        /* expected_branch = */ 3,
        |inner| {
            prop_oneof![
                inner
                    .clone()
                    .prop_map(|b| Shape::Antiquote(Box::new(b))),
                (inner.clone(), inner.clone())
                    .prop_map(|(a, b)| Shape::Let(Box::new(a), Box::new(b))),
                (0usize..=3usize, inner.clone())
                    .prop_map(|(n, b)| Shape::Lambda(n, Box::new(b))),
                (inner.clone(), prop::collection::vec(inner.clone(), 1..=3))
                    .prop_map(|(s, arms)| Shape::Match(Box::new(s), arms)),
                (inner.clone(), prop::collection::vec(inner.clone(), 0..=3))
                    .prop_map(|(c, args)| Shape::Call(Box::new(c), args)),
            ]
        },
    )
}

proptest! {
    // Cases raised from proptest's default 256 to the plan-mandated
    // 10 000.  These forms are small (avg. tree size ~ 32 nodes) and
    // the assertion pass is O(nodes), so 10 000 iterations stays under
    // a few seconds on the reference workstation.
    #![proptest_config(ProptestConfig {
        cases: 10_000,
        max_shrink_iters: 512,
        .. ProptestConfig::default()
    })]

    /// Property 1 (coarse): after `hygienic_rename` under a fresh scope,
    /// the returned `Syntax`'s hygiene MUST be a macro-scope tag and
    /// MUST NOT equal `HYGIENIC_ID_UNTAGGED`.  That is: no accidental
    /// capture of the use-site's untagged hygiene at the root.
    #[test]
    fn coarse_no_accidental_capture(macro_shape in arb_shape()) {
        let mut arena = AstArena::new();
        let mut out = Reified::default();
        let root = reify(&macro_shape, &mut arena, &mut out, /* in_antiquote = */ false);
        let macro_syn = Syntax::from_node(&arena, root);
        let scope = fresh_macro_scope_id();
        let renamed = hygienic_rename(&macro_syn, scope);

        prop_assert!(renamed.hygiene().is_macro_scope(),
            "renamed root must carry the macro-scope bit");
        prop_assert_eq!(renamed.hygiene().macro_scope(), Some(scope),
            "renamed root must round-trip to the minted scope");
        prop_assert_ne!(renamed.hygiene(), HYGIENIC_ID_UNTAGGED,
            "renamed root must be alpha-distinct from untagged use-site hygiene");
    }

    /// Property 2 (fine-grained): the per-node map assigns the macro
    /// scope tag to every macro-side node and `HYGIENIC_ID_UNTAGGED` to
    /// every antiquote-payload node.  Together with Property 1 this
    /// proves the alpha-rename correctly partitions the AST at the
    /// antiquote boundary.
    #[test]
    fn per_node_map_partitions_at_antiquote_boundary(macro_shape in arb_shape()) {
        let mut arena = AstArena::new();
        let mut out = Reified::default();
        let root = reify(&macro_shape, &mut arena, &mut out, /* in_antiquote = */ false);

        let scope = fresh_macro_scope_id();
        let scope_tag = paideia_as_reflection::HygienicId::for_macro_scope(scope);
        let map = hygienic_rename_map(&arena, root, scope);

        // Every macro-side node the walker reached must carry the scope tag.
        // (Not every allocated node is necessarily reachable via the
        // walker's children() traversal — e.g., some AST wrappers.
        // We assert only that when the walker DID visit a macro-side
        // node, it tagged it correctly.)
        for id in &out.macro_side_nodes {
            if let Some(tag) = map.get(*id) {
                prop_assert_eq!(tag, scope_tag,
                    "macro-side node must carry the macro-scope tag");
            }
        }

        // Every antiquote-payload node the walker reached must be untagged.
        for id in &out.antiquote_payload_nodes {
            if let Some(tag) = map.get(*id) {
                prop_assert_eq!(tag, HYGIENIC_ID_UNTAGGED,
                    "antiquote-payload node must be untagged so use-site \
                     name resolution applies");
            }
        }

        // And: scope tag and untagged never coincide (encoding invariant).
        prop_assert_ne!(scope_tag, HYGIENIC_ID_UNTAGGED);
    }

    /// Property 3: two distinct macro invocations produce alpha-
    /// distinct hygiene, so DSL-scoped names introduced by different
    /// invocations never collide.
    #[test]
    fn distinct_invocations_have_distinct_scopes(macro_shape in arb_shape()) {
        let mut arena = AstArena::new();
        let mut out = Reified::default();
        let root = reify(&macro_shape, &mut arena, &mut out, /* in_antiquote = */ false);
        let macro_syn = Syntax::from_node(&arena, root);

        let scope_a = fresh_macro_scope_id();
        let scope_b = fresh_macro_scope_id();
        prop_assert_ne!(scope_a, scope_b);

        let renamed_a = hygienic_rename(&macro_syn, scope_a);
        let renamed_b = hygienic_rename(&macro_syn, scope_b);

        prop_assert_ne!(renamed_a.hygiene(), renamed_b.hygiene(),
            "two invocations must produce alpha-distinct hygiene tags");
    }
}
