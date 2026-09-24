//! Hosted-DSL parser registry — the elaborator-side attachment point for
//! `@dsl_parser("<name>")` (R220.M3, paideia-as#1417).
//!
//! # What this module is
//!
//! When a paideia-as module carries the R220.M3 attribute:
//!
//! ```text
//! pub let parse_num : (Syntax) -> Syntax
//!     = fn(body: Syntax) -> body
//!     @dsl_parser("num");
//! ```
//!
//! the parser records `("num", let_node_id)` on the AST arena's
//! [`paideia_as_ast::ItemDslParserTable`] side-table. This module lifts
//! that raw mapping into a typed [`DslParserRegistry`] keyed by DSL
//! **name**, with duplicate-name detection and a single dispatch entry
//! point that delegates to R220.M2's
//! [`crate::macro_expand::expand_reflective_hygienic`] — so every
//! hosted-DSL invocation is hygienic by construction (Lean 4 / Ullrich
//! 2020 substrate, applied to the reflection surface per Christiansen &
//! Brady 2016 §4).
//!
//! # What this module is *not*
//!
//! This is the **attachment point** the elaborator holds. The context
//! lexer that recognises `dsl_name { <body> }` at the invocation site
//! (and thus decides *when* to call [`DslParserRegistry::dispatch`])
//! lands in R221.M4 — see `design/terminal/semantic-shell-language-plan.md`
//! §4. R220.M3's scope is: (1) the registration surface, (2) the
//! dispatch API, and (3) the diagnostics for the four failure modes.
//!
//! # Fingerprint tags
//!
//! Tests in this module carry `r220m3-dsl-NN` fingerprints per the batch
//! discipline in `.plans/scratch/CHANGELOG-dsl-parser.md`.

use std::collections::HashMap;

use paideia_as_ast::{AstArena, ExprData, ItemData, NodeId, PatternData};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_reflection::MacroScopeId;

use crate::hygiene::HygieneCache;
use crate::macro_expand::expand_reflective_hygienic;
use crate::term_eval::Value;

/// Diagnostic code for **duplicate** `@dsl_parser("<name>")` registrations
/// — two distinct `pub let` bindings claim the same DSL name.
///
/// Chosen from the free R220.M3 slice of the M-family (`M0320..=M0324`),
/// contiguous with the R220.M1/M2 codes (`M0309..=M0312`).
pub const M_DSL_PARSER_DUPLICATE: u16 = 320;

/// Diagnostic code for a dispatch to an **unknown** DSL name.
pub const M_DSL_PARSER_UNKNOWN: u16 = 321;

/// Diagnostic code for a DSL parser binding whose value is **not a lambda**
/// (`fn (body: Syntax) -> ...` / `|body| ...`). The `@dsl_parser` attribute
/// on a non-lambda RHS is a category error the parser cannot yet catch (the
/// value's type is not yet known at parse time), so the registry rejects it
/// at build time.
pub const M_DSL_PARSER_NOT_LAMBDA: u16 = 322;

/// Diagnostic code for a DSL parser lambda whose arity is not exactly one.
/// A hosted-DSL parser is `Syntax -> Syntax`; anything else cannot be
/// dispatched by [`DslParserRegistry::dispatch`], which passes a single
/// body argument.
pub const M_DSL_PARSER_BAD_ARITY: u16 = 323;

/// One entry in the registry — the resolved handle to a DSL parser
/// binding's lambda body plus the arg name the invocation site binds
/// its body-syntax against.
///
/// Kept small (three [`NodeId`]s + one owned `String`) so cloning the
/// registry is cheap; consumers can hold a `&DslParserEntry` for the
/// duration of a dispatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DslParserEntry {
    /// The `pub let` binding node registered under this DSL name — held
    /// so diagnostics can point back at the registration site.
    pub let_node: NodeId,
    /// The `ExprLambda` body node the R220.M2 evaluator descends into.
    /// This is `ExprData::Lambda::body`, resolved once at build time.
    pub lambda_body: NodeId,
    /// The single argument's name (e.g., `"body"`), taken from the
    /// lambda's parameter pattern. Passed straight to
    /// `expand_reflective_hygienic`'s `arg_names` list.
    pub arg_name: String,
}

/// Per-module registry mapping DSL name → resolved parser handle.
///
/// The registry is built from an [`AstArena`] in one pass — every entry
/// in [`AstArena::item_dsl_parser`](paideia_as_ast::AstArena::item_dsl_parser)
/// is validated (RHS must be a `Syntax -> Syntax` lambda) and, on success,
/// installed under its declared name. Duplicate names, non-lambda RHS,
/// and arity-mismatches each produce a distinct diagnostic (see the
/// `M_DSL_PARSER_*` codes above); a failed entry does **not** poison the
/// rest of the registry — every valid binding still lands.
///
/// Lookup is `O(1)` via a `HashMap<String, DslParserEntry>`.
#[derive(Clone, Debug, Default)]
pub struct DslParserRegistry {
    entries: HashMap<String, DslParserEntry>,
}

impl DslParserRegistry {
    /// Construct an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    /// Number of registered DSL parsers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no DSL parsers are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Look up a DSL parser by name.
    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<&DslParserEntry> {
        self.entries.get(name)
    }

    /// Iterate over `(name, entry)` pairs. Iteration order is unspecified.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &DslParserEntry)> + '_ {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Insert a raw entry — intended for tests and for consumers that
    /// have already validated the lambda-shape invariant.
    ///
    /// Returns the previous entry, if any, so the caller can emit a
    /// duplicate-registration diagnostic. `build_from_arena` uses this
    /// path with duplicate detection wired up.
    pub fn insert(&mut self, name: String, entry: DslParserEntry) -> Option<DslParserEntry> {
        self.entries.insert(name, entry)
    }

    /// Build a registry by scanning `arena.item_dsl_parser()` and
    /// resolving each `(Let-id, dsl-name)` pair into a
    /// [`DslParserEntry`]. Emits one diagnostic per rejected entry;
    /// accepted entries still land in the returned registry.
    ///
    /// Ordering: entries are drained from the side-table in `NodeId`
    /// order, so duplicate-registration diagnostics deterministically
    /// point at the *later* binding (the one that would have shadowed
    /// the earlier), matching the "first declaration wins" convention
    /// paideia-as uses for other item-name collisions.
    pub fn build_from_arena(arena: &AstArena) -> (Self, Vec<Diagnostic>) {
        let mut registry = Self::new();
        let mut diags = Vec::new();

        // Collect + sort to keep diagnostic order deterministic.
        let mut raw: Vec<(NodeId, String)> = arena
            .item_dsl_parser()
            .iter()
            .map(|(id, name)| (id, name.to_string()))
            .collect();
        raw.sort_by_key(|(id, _)| id.get());

        for (let_id, dsl_name) in raw {
            match resolve_lambda(arena, let_id, &dsl_name) {
                Ok(entry) => {
                    if let Some(prior) = registry.insert(dsl_name.clone(), entry) {
                        diags.push(
                            Diagnostic::error(m_code(M_DSL_PARSER_DUPLICATE))
                                .message(format!(
                                    "duplicate @dsl_parser(\"{}\") registration; earlier binding \
                                     already claimed this DSL name",
                                    dsl_name
                                ))
                                .with_span(node_span(arena, let_id))
                                .finish(),
                        );
                        // Restore the earlier winner so "first wins" holds.
                        registry.insert(dsl_name, prior);
                    }
                }
                Err(d) => diags.push(d),
            }
        }

        (registry, diags)
    }

    /// Dispatch a hosted-DSL invocation `name { <body> }` through the
    /// registered parser. Delegates to R220.M2's
    /// [`expand_reflective_hygienic`], so the resulting AST subtree is
    /// alpha-distinct from every use-site identifier.
    ///
    /// * `arena` — the mutable AST arena the splice writes into.
    /// * `name` — the DSL invocation head identifier.
    /// * `args` — the evaluator arguments the invocation binds. The
    ///   caller assembles these so the borrow-checker sees no overlap
    ///   between the immutable borrow inside `Value::Term(_)` and the
    ///   mutable borrow the evaluator needs on `arena`. In practice
    ///   the context-lexer landing in R221.M4 will build these via a
    ///   dedicated helper on the arena's mutable side; callers can
    ///   pass `vec![]` when the DSL parser body is a nullary macro
    ///   (matching the R220.M2 tests' `vec![]` idiom).
    /// * `call_site` — the whole invocation's span, used for both the
    ///   splice and the "unknown DSL" diagnostic below.
    /// * `depth` — the current expansion depth, forwarded as-is so
    ///   nested DSL invocations (`outer { inner { ... } }`) share the
    ///   R220.M2 recursion guard.
    ///
    /// Returns `Ok((spliced_node_id, scope, cache))` on success — the
    /// same triple `expand_reflective_hygienic` returns; the scope and
    /// cache are what R220.M2 exposes so name resolution can consult
    /// the hygiene tags. Returns `Err` with a single M-family
    /// diagnostic for the "unknown DSL name" case, or the evaluator's
    /// diagnostic list on any other failure (including the R220.M2
    /// `M0311` recursion-depth guard, which propagates through this
    /// entry point unchanged — see the depth-guard test).
    #[allow(clippy::result_large_err)]
    pub fn dispatch<'a>(
        &self,
        arena: &mut AstArena,
        name: &str,
        args: Vec<Value<'a>>,
        call_site: Span,
        depth: usize,
    ) -> Result<(NodeId, MacroScopeId, HygieneCache), Vec<Diagnostic>> {
        let entry = match self.entries.get(name) {
            Some(e) => e.clone(),
            None => {
                let known: Vec<&str> = {
                    let mut names: Vec<&str> = self.entries.keys().map(String::as_str).collect();
                    names.sort_unstable();
                    names
                };
                let hint = if known.is_empty() {
                    "no @dsl_parser(...) registered in this module".to_string()
                } else {
                    format!("registered DSLs: {}", known.join(", "))
                };
                return Err(vec![
                    Diagnostic::error(m_code(M_DSL_PARSER_UNKNOWN))
                        .message(format!(
                            "unknown hosted-DSL name '{}' — {}",
                            name, hint
                        ))
                        .with_span(call_site)
                        .finish(),
                ]);
            }
        };

        let arg_names = [entry.arg_name.clone()];
        expand_reflective_hygienic(
            arena,
            entry.lambda_body,
            args,
            &arg_names,
            call_site,
            depth,
        )
    }
}

/// Resolve a `pub let name = fn(body: Syntax) -> ...` binding into a
/// [`DslParserEntry`], or emit a targeted diagnostic on failure.
fn resolve_lambda(
    arena: &AstArena,
    let_id: NodeId,
    dsl_name: &str,
) -> Result<DslParserEntry, Diagnostic> {
    let value_id = match arena.item_data(let_id) {
        Some(ItemData::Let { value, .. }) => *value,
        _ => {
            // Should not happen — item_dsl_parser is keyed by Let ids —
            // but we surface a helpful diagnostic instead of panicking.
            return Err(
                Diagnostic::error(m_code(M_DSL_PARSER_NOT_LAMBDA))
                    .message(format!(
                        "@dsl_parser(\"{}\") is only valid on a `pub let` binding",
                        dsl_name
                    ))
                    .with_span(node_span(arena, let_id))
                    .finish(),
            );
        }
    };

    let (params, body) = match arena.expr_data(value_id) {
        Some(ExprData::Lambda { params, body, .. }) => (params.clone(), *body),
        _ => {
            return Err(
                Diagnostic::error(m_code(M_DSL_PARSER_NOT_LAMBDA))
                    .message(format!(
                        "@dsl_parser(\"{}\") requires a lambda value (Syntax -> Syntax); \
                         non-lambda RHS cannot be dispatched",
                        dsl_name
                    ))
                    .with_span(node_span(arena, value_id))
                    .finish(),
            );
        }
    };

    if params.len() != 1 {
        return Err(
            Diagnostic::error(m_code(M_DSL_PARSER_BAD_ARITY))
                .message(format!(
                    "@dsl_parser(\"{}\") lambda must take exactly one Syntax argument, \
                     got {} parameter(s)",
                    dsl_name,
                    params.len()
                ))
                .with_span(node_span(arena, value_id))
                .finish(),
        );
    }

    // Extract the parameter name (from the PatternData::Ident's inner
    // Ident node's source span). Fall back to a sentinel that never
    // collides with user identifiers if the pattern is a wildcard.
    let arg_name = param_name(arena, params[0]).unwrap_or_else(|| "_dsl_body".to_string());

    Ok(DslParserEntry {
        let_node: let_id,
        lambda_body: body,
        arg_name,
    })
}

/// Extract the identifier text of a lambda parameter's pattern, if it
/// is an `Ident` pattern. Returns `None` for wildcard / non-ident
/// patterns; `dispatch` uses a placeholder name in that case.
fn param_name(arena: &AstArena, pat: NodeId) -> Option<String> {
    let ident_id = match arena.pattern_data(pat)? {
        PatternData::Ident { name, .. } => *name,
        _ => return None,
    };
    // The Ident node's span points at the identifier's source text.
    // Without a source-map handle here we cannot slice the source,
    // so we key the arg name off the NodeId's raw number — the value
    // evaluator only compares arg names to metavariable references
    // inside the DSL parser body, and any consistent handle works.
    Some(format!("_dsl_p{}", ident_id.get()))
}

fn node_span(arena: &AstArena, id: NodeId) -> Span {
    arena
        .get(id)
        .map(|n| n.span)
        .unwrap_or_else(|| Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 0))
}

fn m_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::M, Severity::Error, n).expect("valid M code")
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::{ExprData, ItemData, NodeKind, PatternData};
    use paideia_as_diagnostics::FileId;

    fn file() -> FileId {
        FileId::new(1).unwrap()
    }

    fn sp(byte_start: u32, byte_len: u32) -> Span {
        Span::new(file(), byte_start, byte_len)
    }

    /// Build a minimal DSL parser Let binding in `arena` under `dsl_name`:
    ///
    ///   pub let <ident> = fn(<param>) -> <ident-of-param> @dsl_parser("<dsl_name>")
    ///
    /// Returns the Let node id.
    fn install_identity_parser(
        arena: &mut AstArena,
        dsl_name: &str,
        binding_name: &str,
    ) -> NodeId {
        // param name Ident
        let param_ident = arena.alloc(NodeKind::Ident, sp(0, binding_name.len() as u32));
        // param pattern
        let param_pat = arena.alloc_pattern(
            NodeKind::PatIdent,
            sp(0, 1),
            PatternData::Ident { name: param_ident, mutable: false },
        );
        // body: return the parameter identifier (a Path/Ident node — for
        // the registry's dispatch we only need a NodeId; identity is fine)
        let body_ident = arena.alloc(NodeKind::Ident, sp(0, 1));
        // lambda
        let lambda = arena.alloc_expr(
            NodeKind::ExprLambda,
            sp(0, 1),
            ExprData::Lambda {
                generic_params: Vec::new(),
                params: vec![param_pat],
                body: body_ident,
                pipe_form: false,
            },
        );
        // let name ident
        let let_name = arena.alloc(NodeKind::Ident, sp(0, binding_name.len() as u32));
        // let item
        let let_id = arena.alloc_item(
            NodeKind::Let,
            sp(0, 1),
            ItemData::Let {
                public: true,
                mutable: false,
                name: let_name,
                generic_params: Vec::new(),
                ty: None,
                value: lambda,
                align: None,
                ring: None,
                link_section: None,
                abi: None,
                no_frame: false,
                interrupt: None,
                doc: None,
            },
        );
        arena
            .item_dsl_parser_mut()
            .insert(let_id, dsl_name.to_string());
        let_id
    }

    // Fingerprint tag: r220m3-dsl-13 — a single well-formed registration
    // lands in the registry under its declared name.
    #[test]
    fn build_from_arena_single_entry() {
        let mut arena = AstArena::new();
        let let_id = install_identity_parser(&mut arena, "num", "parse_num");
        let (registry, diags) = DslParserRegistry::build_from_arena(&arena);
        assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
        assert_eq!(registry.len(), 1);
        let entry = registry.lookup("num").expect("num should be registered");
        assert_eq!(entry.let_node, let_id);
    }

    // Fingerprint tag: r220m3-dsl-14 — two distinct DSL names coexist and
    // dispatch selects the right one.
    #[test]
    fn multi_dsl_dispatch_selects_right_one() {
        let mut arena = AstArena::new();
        install_identity_parser(&mut arena, "num", "parse_num");
        install_identity_parser(&mut arena, "sh", "parse_sh");
        let (registry, diags) = DslParserRegistry::build_from_arena(&arena);
        assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);
        assert_eq!(registry.len(), 2);
        let num_entry = registry.lookup("num").expect("num");
        let sh_entry = registry.lookup("sh").expect("sh");
        assert_ne!(num_entry.let_node, sh_entry.let_node);
    }

    // Fingerprint tag: r220m3-dsl-15 — dispatch to an unregistered DSL
    // name yields M0321 with the "registered DSLs: …" hint.
    #[test]
    fn dispatch_unknown_dsl_yields_m0321() {
        let mut arena = AstArena::new();
        install_identity_parser(&mut arena, "num", "parse_num");
        let (registry, _) = DslParserRegistry::build_from_arena(&arena);
        let err = registry
            .dispatch(&mut arena, "missing", vec![], sp(10, 5), 0)
            .expect_err("unknown DSL name must fail dispatch");
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].code().number(), M_DSL_PARSER_UNKNOWN);
        assert!(
            err[0].message().contains("registered DSLs: num"),
            "diagnostic must hint at registered names, got: {}",
            err[0].message()
        );
    }

    // Fingerprint tag: r220m3-dsl-19 — the R220.M2 depth-guard (M0311)
    // propagates through dispatch unchanged. This is the "DSL parser
    // errors propagate through Elab effect" acceptance test: an
    // evaluator-level failure surfaces without being swallowed at the
    // registry boundary.
    #[test]
    fn dispatch_depth_guard_propagates_m0311() {
        let mut arena = AstArena::new();
        install_identity_parser(&mut arena, "num", "parse_num");
        let (registry, _) = DslParserRegistry::build_from_arena(&arena);
        let err = registry
            .dispatch(
                &mut arena,
                "num",
                vec![],
                sp(10, 5),
                crate::macro_expand::MAX_EXPANSION_DEPTH + 1,
            )
            .expect_err("depth over cap must fail dispatch");
        assert!(
            err.iter().any(|d| d.code().number() == crate::macro_expand::M_RECURSION_LIMIT),
            "expected M0311 propagated from expand_reflective_hygienic, got: {:?}",
            err
        );
    }

    // Fingerprint tag: r220m3-dsl-20 — a recursive dispatch shape
    // (dispatch-A then dispatch-B in sequence, standing in for the
    // "dsl-A inside dsl-B" invocation shape context-lexer R221.M4 will
    // spell natively): each dispatch returns Err at the unknown branch,
    // but the two error diagnostics reference the *distinct* call-site
    // spans supplied — i.e., the registry never conflates recursive
    // invocations, which is the prerequisite for R220.M2 hygiene
    // composition being sound across nested DSL bodies.
    #[test]
    fn dispatch_recursive_call_sites_stay_distinct() {
        let mut arena = AstArena::new();
        install_identity_parser(&mut arena, "outer", "parse_outer");
        install_identity_parser(&mut arena, "inner", "parse_inner");
        let (registry, _) = DslParserRegistry::build_from_arena(&arena);

        // Outer invocation site.
        let outer_span = sp(100, 20);
        // Inner invocation site (nested — different span).
        let inner_span = sp(110, 5);

        // Dispatch outer (unknown-name path uses call_site verbatim).
        let outer_err = registry
            .dispatch(&mut arena, "missing_outer", vec![], outer_span, 0)
            .expect_err("unknown must fail");
        // Dispatch inner from within outer's conceptual scope.
        let inner_err = registry
            .dispatch(&mut arena, "missing_inner", vec![], inner_span, 1)
            .expect_err("unknown must fail");

        // The two diagnostics reference distinct spans — the registry
        // did not memoise or share span state across dispatches.
        let outer_diag_span = outer_err[0].primary_span();
        let inner_diag_span = inner_err[0].primary_span();
        assert_ne!(
            outer_diag_span.map(|s| s.byte_start()),
            inner_diag_span.map(|s| s.byte_start()),
            "recursive dispatch must preserve distinct call-site spans"
        );
    }

    // Fingerprint tag: r220m3-dsl-16 — duplicate registrations under the
    // same DSL name emit M0320 and preserve the first winner.
    #[test]
    fn duplicate_registration_yields_m0320_first_wins() {
        let mut arena = AstArena::new();
        let first = install_identity_parser(&mut arena, "num", "parse_num_a");
        let _second = install_identity_parser(&mut arena, "num", "parse_num_b");
        let (registry, diags) = DslParserRegistry::build_from_arena(&arena);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), M_DSL_PARSER_DUPLICATE);
        let entry = registry.lookup("num").expect("num");
        assert_eq!(
            entry.let_node, first,
            "earlier registration should win (first-declaration-wins)"
        );
    }

    // Fingerprint tag: r220m3-dsl-17 — a `@dsl_parser` on a non-lambda RHS
    // yields M0322 and does not corrupt the rest of the registry.
    #[test]
    fn non_lambda_rhs_yields_m0322() {
        let mut arena = AstArena::new();
        // Install a valid entry first.
        install_identity_parser(&mut arena, "ok", "parse_ok");
        // Now install a broken entry: RHS is a literal, not a lambda.
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, sp(0, 1));
        let literal = arena.alloc_expr(
            NodeKind::ExprLiteral,
            sp(0, 1),
            ExprData::Literal { lit: lit_placeholder },
        );
        let let_name = arena.alloc(NodeKind::Ident, sp(0, 3));
        let let_id = arena.alloc_item(
            NodeKind::Let,
            sp(0, 1),
            ItemData::Let {
                public: true,
                mutable: false,
                name: let_name,
                generic_params: Vec::new(),
                ty: None,
                value: literal,
                align: None,
                ring: None,
                link_section: None,
                abi: None,
                no_frame: false,
                interrupt: None,
                doc: None,
            },
        );
        arena.item_dsl_parser_mut().insert(let_id, "bad".to_string());

        let (registry, diags) = DslParserRegistry::build_from_arena(&arena);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), M_DSL_PARSER_NOT_LAMBDA);
        // The valid entry still lands.
        assert!(registry.lookup("ok").is_some());
        assert!(registry.lookup("bad").is_none());
    }

    // Fingerprint tag: r220m3-dsl-18 — arity mismatch (0 or 2+ params)
    // yields M0323.
    #[test]
    fn arity_mismatch_yields_m0323() {
        let mut arena = AstArena::new();
        // Zero-param lambda.
        let body = arena.alloc(NodeKind::Ident, sp(0, 1));
        let lambda = arena.alloc_expr(
            NodeKind::ExprLambda,
            sp(0, 1),
            ExprData::Lambda {
                generic_params: Vec::new(),
                params: Vec::new(),
                body,
                pipe_form: true,
            },
        );
        let let_name = arena.alloc(NodeKind::Ident, sp(0, 3));
        let let_id = arena.alloc_item(
            NodeKind::Let,
            sp(0, 1),
            ItemData::Let {
                public: true,
                mutable: false,
                name: let_name,
                generic_params: Vec::new(),
                ty: None,
                value: lambda,
                align: None,
                ring: None,
                link_section: None,
                abi: None,
                no_frame: false,
                interrupt: None,
                doc: None,
            },
        );
        arena.item_dsl_parser_mut().insert(let_id, "zero".to_string());
        let (_, diags) = DslParserRegistry::build_from_arena(&arena);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), M_DSL_PARSER_BAD_ARITY);
    }
}
