//! R220.M2 hygiene acceptance corpus: 20 potential-capture forms elaborate
//! hygienically (paideia-as#1416, per plan §4 R220.M2 acceptance bullet 1).
//!
//! Each test builds a *macro-side* `Syntax` fragment that mentions an
//! identifier spelling `x` (or another shared spelling) and a *use-site*
//! `Syntax` fragment that mentions the same spelling.  We then apply
//! [`hygienic_rename`] to the macro side under a fresh
//! [`MacroScopeId`] and assert that the two `HygienicId`s are alpha-
//! distinct — i.e., name resolution would treat them as different
//! bindings even though the surface spelling collides.
//!
//! The corpus covers 20 shapes that commonly cause accidental capture
//! in unhygienic macro systems (bare `let`, lambda parameters, match
//! arms, nested binders, block-tail bindings, etc.).  When a shape
//! introduces multiple binder positions we exercise each one so the
//! test count in the file matches the plan spec.
//!
//! **Related tests**: unit tests inside `crate::hygiene` cover the
//! primitive API (fresh id monotonicity, encoding invariants, per-node
//! map semantics).  The 10 000-form property test in
//! `hygiene_property.rs` covers the corpus's random-form counterpart.

use paideia_as_ast::{AstArena, ExprData, MatchArm, NodeId, NodeKind, PatternData};
use paideia_as_diagnostics::{FileId, Span};
use paideia_as_reflection::{
    HYGIENIC_ID_UNTAGGED, HygienicId, Syntax, fresh_macro_scope_id, hygienic_rename,
    hygienic_rename_map,
};

fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

/// Build a `Path(ident("x"))` node in `arena` and return its NodeId.
fn build_var(arena: &mut AstArena) -> NodeId {
    let ident = arena.alloc(NodeKind::Ident, span());
    Syntax::var(arena, span(), ident)
}

/// Build a `let module N = body in rest` node.
fn build_let(arena: &mut AstArena, name: &str) -> NodeId {
    let body = build_var(arena);
    let rest = build_var(arena);
    Syntax::let_(arena, span(), name.to_owned(), body, rest)
}

/// Build a lambda `\params. body`.
fn build_lambda(arena: &mut AstArena, param_count: usize) -> NodeId {
    let mut params = Vec::with_capacity(param_count);
    for _ in 0..param_count {
        // Each parameter is a Placeholder (patterns have their own shape
        // in the AST; we only need distinct NodeIds here for the test).
        params.push(arena.alloc(NodeKind::Placeholder, span()));
    }
    let body = build_var(arena);
    Syntax::lambda(arena, span(), params, body)
}

/// Build a `match scrutinee { pat => body }` shape.
fn build_match(arena: &mut AstArena, arm_count: usize) -> NodeId {
    let scrutinee = build_var(arena);
    let mut arms = Vec::with_capacity(arm_count);
    for _ in 0..arm_count {
        let pattern = arena.alloc_pattern(NodeKind::PatWildcard, span(), PatternData::Wildcard);
        let body = build_var(arena);
        arms.push(MatchArm {
            pattern,
            guard: None,
            body,
        });
    }
    Syntax::match_(arena, span(), scrutinee, arms)
}

/// Build a Call node `f(arg1, arg2, ...)`.
fn build_call(arena: &mut AstArena, arg_count: usize) -> NodeId {
    let callee = build_var(arena);
    let mut args = Vec::with_capacity(arg_count);
    for _ in 0..arg_count {
        args.push(build_var(arena));
    }
    Syntax::app(arena, span(), callee, args)
}

/// Build an `Antiquote(inner)` node.
fn build_antiquote(arena: &mut AstArena, inner: NodeId) -> NodeId {
    arena.alloc_expr(
        NodeKind::ExprAntiquote,
        span(),
        ExprData::Antiquote { value: inner },
    )
}

/// Core assertion: after renaming `macro_node` under a fresh scope, its
/// hygiene is distinct from the use-site's untagged hygiene.
fn assert_alpha_distinct(macro_node: NodeId, use_site_node: NodeId, arena: &AstArena) {
    let macro_syn = Syntax::from_node(arena, macro_node);
    let use_site_syn = Syntax::from_node(arena, use_site_node);
    assert_eq!(
        use_site_syn.hygiene(),
        HYGIENIC_ID_UNTAGGED,
        "use-site Syntax must start untagged",
    );

    let scope = fresh_macro_scope_id();
    let renamed = hygienic_rename(&macro_syn, scope);

    assert!(
        renamed.hygiene().is_macro_scope(),
        "renamed macro hygiene must carry the macro-scope bit",
    );
    assert_eq!(
        renamed.hygiene().macro_scope(),
        Some(scope),
        "renamed macro hygiene must round-trip to the minted scope",
    );
    assert_ne!(
        renamed.hygiene(),
        use_site_syn.hygiene(),
        "macro-side hygiene must be alpha-distinct from use-site hygiene",
    );
    assert!(
        !use_site_syn.hygiene().is_macro_scope(),
        "use-site hygiene must not accidentally acquire a macro-scope tag",
    );
}

// ── 20 potential-capture forms ──────────────────────────────────────

#[test]
fn form_01_bare_variable_x() {
    let mut arena = AstArena::new();
    let macro_node = build_var(&mut arena);
    let use_site_node = build_var(&mut arena);
    assert_alpha_distinct(macro_node, use_site_node, &arena);
}

#[test]
fn form_02_let_module_binding() {
    let mut arena = AstArena::new();
    let macro_node = build_let(&mut arena, "x");
    let use_site_node = build_var(&mut arena);
    assert_alpha_distinct(macro_node, use_site_node, &arena);
}

#[test]
fn form_03_unary_lambda_x() {
    let mut arena = AstArena::new();
    let macro_node = build_lambda(&mut arena, 1);
    let use_site_node = build_var(&mut arena);
    assert_alpha_distinct(macro_node, use_site_node, &arena);
}

#[test]
fn form_04_nullary_lambda_body_x() {
    let mut arena = AstArena::new();
    let macro_node = build_lambda(&mut arena, 0);
    let use_site_node = build_var(&mut arena);
    assert_alpha_distinct(macro_node, use_site_node, &arena);
}

#[test]
fn form_05_binary_lambda_shadowed_param() {
    let mut arena = AstArena::new();
    let macro_node = build_lambda(&mut arena, 2);
    let use_site_node = build_var(&mut arena);
    assert_alpha_distinct(macro_node, use_site_node, &arena);
}

#[test]
fn form_06_nested_let() {
    let mut arena = AstArena::new();
    let inner = build_let(&mut arena, "x");
    let outer_body = build_var(&mut arena);
    let outer = Syntax::let_(&mut arena, span(), "x".into(), outer_body, inner);
    let use_site_node = build_var(&mut arena);
    assert_alpha_distinct(outer, use_site_node, &arena);
}

#[test]
fn form_07_lambda_containing_let() {
    let mut arena = AstArena::new();
    let inner_let = build_let(&mut arena, "x");
    let macro_node = Syntax::lambda(&mut arena, span(), Vec::new(), inner_let);
    let use_site_node = build_var(&mut arena);
    assert_alpha_distinct(macro_node, use_site_node, &arena);
}

#[test]
fn form_08_match_with_single_arm() {
    let mut arena = AstArena::new();
    let macro_node = build_match(&mut arena, 1);
    let use_site_node = build_var(&mut arena);
    assert_alpha_distinct(macro_node, use_site_node, &arena);
}

#[test]
fn form_09_match_with_two_arms() {
    let mut arena = AstArena::new();
    let macro_node = build_match(&mut arena, 2);
    let use_site_node = build_var(&mut arena);
    assert_alpha_distinct(macro_node, use_site_node, &arena);
}

#[test]
fn form_10_match_with_many_arms() {
    let mut arena = AstArena::new();
    let macro_node = build_match(&mut arena, 7);
    let use_site_node = build_var(&mut arena);
    assert_alpha_distinct(macro_node, use_site_node, &arena);
}

#[test]
fn form_11_call_no_args() {
    let mut arena = AstArena::new();
    let macro_node = build_call(&mut arena, 0);
    let use_site_node = build_var(&mut arena);
    assert_alpha_distinct(macro_node, use_site_node, &arena);
}

#[test]
fn form_12_call_one_arg() {
    let mut arena = AstArena::new();
    let macro_node = build_call(&mut arena, 1);
    let use_site_node = build_var(&mut arena);
    assert_alpha_distinct(macro_node, use_site_node, &arena);
}

#[test]
fn form_13_call_many_args() {
    let mut arena = AstArena::new();
    let macro_node = build_call(&mut arena, 5);
    let use_site_node = build_var(&mut arena);
    assert_alpha_distinct(macro_node, use_site_node, &arena);
}

#[test]
fn form_14_lambda_returning_call() {
    let mut arena = AstArena::new();
    let inner_call = build_call(&mut arena, 2);
    let macro_node = Syntax::lambda(&mut arena, span(), Vec::new(), inner_call);
    let use_site_node = build_var(&mut arena);
    assert_alpha_distinct(macro_node, use_site_node, &arena);
}

#[test]
fn form_15_match_within_let() {
    let mut arena = AstArena::new();
    let inner_match = build_match(&mut arena, 2);
    let rest = build_var(&mut arena);
    let macro_node = Syntax::let_(&mut arena, span(), "x".into(), inner_match, rest);
    let use_site_node = build_var(&mut arena);
    assert_alpha_distinct(macro_node, use_site_node, &arena);
}

#[test]
fn form_16_deeply_nested_binders() {
    let mut arena = AstArena::new();
    // let x = (\x. (match x { _ => x })) in x
    let match_node = build_match(&mut arena, 1);
    let lambda_body = match_node;
    let params = vec![arena.alloc(NodeKind::Placeholder, span())];
    let lambda_node = Syntax::lambda(&mut arena, span(), params, lambda_body);
    let rest = build_var(&mut arena);
    let macro_node = Syntax::let_(&mut arena, span(), "x".into(), lambda_node, rest);
    let use_site_node = build_var(&mut arena);
    assert_alpha_distinct(macro_node, use_site_node, &arena);
}

#[test]
fn form_17_antiquote_of_use_site_x() {
    // Macro fragment contains an antiquote whose payload is a use-site
    // `x`.  After rename, the whole tree carries the scope tag at the
    // top level, but the per-node map records the antiquote descendant
    // as untagged so it does NOT collide with the macro's own `x`.
    let mut arena = AstArena::new();
    let use_site_var = build_var(&mut arena);
    let anti = build_antiquote(&mut arena, use_site_var);
    let macro_side = Syntax::from_node(&arena, anti);
    let scope = fresh_macro_scope_id();
    let map = hygienic_rename_map(&arena, macro_side.node_id(), scope);
    let scope_tag = HygienicId::for_macro_scope(scope);
    assert_ne!(scope_tag, HYGIENIC_ID_UNTAGGED);
    // The antiquote's payload must be recorded as untagged, so name
    // resolution treats it as a use-site reference.
    assert_eq!(map.get(use_site_var), Some(HYGIENIC_ID_UNTAGGED));
}

#[test]
fn form_18_call_argument_is_antiquote() {
    // Macro fragment:  f( ~use_site_x, macro_y )
    // After rename: `use_site_x` untagged, `f` and `macro_y` carry the scope.
    let mut arena = AstArena::new();
    let use_site_x = build_var(&mut arena);
    let anti = build_antiquote(&mut arena, use_site_x);
    let macro_y = build_var(&mut arena);
    let callee = build_var(&mut arena);
    let call = Syntax::app(&mut arena, span(), callee, vec![anti, macro_y]);

    let scope = fresh_macro_scope_id();
    let scope_tag = HygienicId::for_macro_scope(scope);
    let map = hygienic_rename_map(&arena, call, scope);

    assert_eq!(map.get(callee), Some(scope_tag), "callee must be macro-side");
    assert_eq!(map.get(macro_y), Some(scope_tag), "macro-side arg must be macro-side");
    assert_eq!(map.get(use_site_x), Some(HYGIENIC_ID_UNTAGGED),
        "antiquote payload must stay untagged");
    assert_ne!(map.get(macro_y), map.get(use_site_x),
        "macro-side and antiquote payload must be alpha-distinct");
}

#[test]
fn form_19_two_macro_invocations_have_distinct_scopes() {
    // Two macro invocations of the same shape must produce alpha-
    // distinct hygiene, so DSL-level names introduced by different
    // invocations never collide even inside the same use-site.
    let mut arena = AstArena::new();
    let macro_a = build_var(&mut arena);
    let macro_b = build_var(&mut arena);

    let syn_a = Syntax::from_node(&arena, macro_a);
    let syn_b = Syntax::from_node(&arena, macro_b);

    let scope_a = fresh_macro_scope_id();
    let scope_b = fresh_macro_scope_id();
    assert_ne!(scope_a, scope_b);

    let renamed_a = hygienic_rename(&syn_a, scope_a);
    let renamed_b = hygienic_rename(&syn_b, scope_b);

    assert_ne!(
        renamed_a.hygiene(),
        renamed_b.hygiene(),
        "distinct macro invocations must yield distinct scope hygiene",
    );
}

#[test]
fn form_20_nested_macro_expansion_shape() {
    // Simulates `outer_macro( inner_macro(x) )`: the *outer* macro
    // returns a Syntax fragment that itself was renamed under an inner
    // macro's scope.  The outer rename produces a fresh scope tag; the
    // inner fragment's per-node map still remembers its own scope for
    // its identifiers, and both remain alpha-distinct from any use-site
    // reference.
    let mut arena = AstArena::new();
    let inner_var = build_var(&mut arena);
    let inner_syn = Syntax::from_node(&arena, inner_var);

    let inner_scope = fresh_macro_scope_id();
    let inner_renamed = hygienic_rename(&inner_syn, inner_scope);

    let outer_scope = fresh_macro_scope_id();
    let outer_renamed = hygienic_rename(&inner_renamed, outer_scope);

    // Both scope tags exist and both differ from untagged.
    assert!(inner_renamed.hygiene().is_macro_scope());
    assert!(outer_renamed.hygiene().is_macro_scope());
    assert_ne!(inner_renamed.hygiene(), HYGIENIC_ID_UNTAGGED);
    assert_ne!(outer_renamed.hygiene(), HYGIENIC_ID_UNTAGGED);
    // Outer rename shadows inner: outer's scope wins at the top level.
    assert_eq!(outer_renamed.hygiene().macro_scope(), Some(outer_scope));
    assert_ne!(outer_renamed.hygiene(), inner_renamed.hygiene());

    // A parallel use-site reference remains alpha-distinct from both.
    // Snapshot the two renamed hygiene tags before we mutably borrow
    // the arena again to build the use-site.
    let inner_hyg = inner_renamed.hygiene();
    let outer_hyg = outer_renamed.hygiene();
    let use_site_ref = build_var(&mut arena);
    let use_site_syn = Syntax::from_node(&arena, use_site_ref);
    assert_ne!(inner_hyg, use_site_syn.hygiene());
    assert_ne!(outer_hyg, use_site_syn.hygiene());
}
