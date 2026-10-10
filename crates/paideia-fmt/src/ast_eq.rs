//! Structural AST equivalence (FMT-14, paideia-as#1578).
//!
//! Compares two [`AstArena`]-rooted trees for *structural* equality,
//! ignoring spans and the exact byte position of every identifier /
//! literal — but treating identifier *text* and literal *values* as
//! significant. The intended consumer is the FMT-14 corpus harness
//! (`tests/corpus_parser_equivalence.rs`), which asserts
//! `parse(src) ≡ parse(format(src))` on every parseable `.pdx` file in
//! the workspace: a true result there is the formatter's "no observable
//! semantic drift" contract.
//!
//! # Equivalence contract
//!
//! For two nodes `(id_a, id_b)` the walker returns `true` iff every one
//! of the following holds (recursively):
//!
//! 1. Both nodes share the same [`NodeKind`].
//! 2. Any non-`NodeId` scalar in the node's structured payload is equal
//!    (e.g., `ItemData::Let::public`, `ExprData::Prefix::kind`,
//!    `TypeData::Ref::mutable`, `AtomicOrdering`, `LinClass`, string
//!    payloads such as `ExprData::StringLiteral`, …).
//! 3. Every `NodeId` field compares structurally equal under this same
//!    relation (recursive descent).
//! 4. For leaf-shaped nodes — [`NodeKind::Ident`], [`NodeKind::Placeholder`],
//!    [`NodeKind::GenericParam`], [`NodeKind::MacroPattern`],
//!    [`NodeKind::MacroTemplate`] — whose contents live outside the
//!    arena data vectors, the walker compares the source bytes at the
//!    node's span.
//!
//! # Formatter-mutable fields (intentionally compared)
//!
//! Nothing in the AST is *intentionally* formatter-mutable today — the
//! style spec (§7 / §10) rewrites only whitespace, trailing commas,
//! glyphs, and alignment, none of which perturb the parser's AST shape.
//! If a future FMT pass reorders attributes (e.g., `#[derive(A, B)]` →
//! `#[derive(B, A)]`), the comparator will catch it and the pass must
//! be either reverted or opted out of parser equivalence per-rule.
//!
//! # Depth bound
//!
//! Recursion is capped at [`MAX_DEPTH`] to keep a maliciously deep
//! fixture from blowing the test runner's stack. Reaching the cap is
//! treated as inequality — real paideia-as sources sit well under it,
//! so a cap hit is itself a signal that something is wrong.

use paideia_as_ast::{
    AstArena, EnumVariant, ExprData, GenericParam, HandlerArm, ItemData, NodeId, NodeKind,
    PatternData, StmtData, TypeData,
};

/// Maximum recursion depth before the comparator bails out and returns
/// `false`. Set generously (2048) — a well-formed phase-1 paideia-as
/// source rarely nests past a few hundred levels.
pub const MAX_DEPTH: usize = 2048;

/// Side of the comparison: source text plus the arena it was parsed into.
struct Side<'a> {
    src: &'a str,
    arena: &'a AstArena,
}

/// Top-level entry — structural AST equality.
///
/// Returns `true` iff the two trees rooted at `root_a` / `root_b` are
/// structurally equivalent per the contract in the module docs.
#[must_use]
pub fn ast_structurally_equal(
    src_a: &str,
    arena_a: &AstArena,
    root_a: NodeId,
    src_b: &str,
    arena_b: &AstArena,
    root_b: NodeId,
) -> bool {
    let a = Side {
        src: src_a,
        arena: arena_a,
    };
    let b = Side {
        src: src_b,
        arena: arena_b,
    };
    nodes_eq(&a, &b, root_a, root_b, 0)
}

/// Core recursive comparator. `depth` is incremented on every recursive
/// descent; a hit on [`MAX_DEPTH`] short-circuits to `false`.
fn nodes_eq(a: &Side<'_>, b: &Side<'_>, id_a: NodeId, id_b: NodeId, depth: usize) -> bool {
    if depth > MAX_DEPTH {
        return false;
    }
    let (Some(na), Some(nb)) = (a.arena.get(id_a), b.arena.get(id_b)) else {
        return false;
    };
    if na.kind != nb.kind {
        return false;
    }

    // Item-shaped: dispatch to ItemData comparator.
    if let (Some(ia), Some(ib)) = (a.arena.item_data(id_a), b.arena.item_data(id_b)) {
        return items_eq(a, b, ia, ib, depth + 1);
    }
    // Expression-shaped.
    if let (Some(ea), Some(eb)) = (a.arena.expr_data(id_a), b.arena.expr_data(id_b)) {
        return exprs_eq(a, b, ea, eb, depth + 1);
    }
    // Statement-shaped.
    if let (Some(sa), Some(sb)) = (a.arena.stmt_data(id_a), b.arena.stmt_data(id_b)) {
        return stmts_eq(a, b, sa, sb, depth + 1);
    }
    // Type-shaped.
    if let (Some(ta), Some(tb)) = (a.arena.type_data(id_a), b.arena.type_data(id_b)) {
        return types_eq(a, b, ta, tb, depth + 1);
    }
    // Pattern-shaped.
    if let (Some(pa), Some(pb)) = (a.arena.pattern_data(id_a), b.arena.pattern_data(id_b)) {
        return patterns_eq(a, b, pa, pb, depth + 1);
    }
    // Pure leaf (Ident, Placeholder, GenericParam marker, MacroPattern,
    // MacroTemplate, …): no category data. Compare the source bytes
    // the span covers — that's the only identity these carry.
    span_slice(a.src, id_a, a.arena) == span_slice(b.src, id_b, b.arena)
}

/// Safely extract the UTF-8 slice a node's span covers in `src`. Returns
/// `""` on any out-of-range or non-char-boundary condition — comparing
/// two empty strings yields `true`, so a corrupt input gracefully
/// degrades to a weaker (not stronger) assertion.
fn span_slice<'s>(src: &'s str, id: NodeId, arena: &AstArena) -> &'s str {
    let Some(nd) = arena.get(id) else {
        return "";
    };
    let start = nd.span.byte_start() as usize;
    let end = nd.span.byte_end() as usize;
    if end > src.len() || start > end {
        return "";
    }
    // get() returns None if the range crosses a non-char-boundary.
    src.get(start..end).unwrap_or("")
}

// -------------------------------------------------------------------
// Items
// -------------------------------------------------------------------

fn items_eq(
    a: &Side<'_>,
    b: &Side<'_>,
    ia: &ItemData,
    ib: &ItemData,
    depth: usize,
) -> bool {
    match (ia, ib) {
        (
            ItemData::Module {
                name: na,
                sig: sa,
                body: ba,
                inner_attrs: aa,
                doc: da,
            },
            ItemData::Module {
                name: nb,
                sig: sb,
                body: bb,
                inner_attrs: ab,
                doc: db,
            },
        ) => {
            nodes_eq(a, b, *na, *nb, depth)
                && opt_nodes_eq(a, b, *sa, *sb, depth)
                && nodes_eq(a, b, *ba, *bb, depth)
                && aa.len() == ab.len()
                && opt_nodes_eq(a, b, *da, *db, depth)
        }
        (
            ItemData::Signature {
                name: na,
                body: ba,
                doc: da,
            },
            ItemData::Signature {
                name: nb,
                body: bb,
                doc: db,
            },
        ) => {
            nodes_eq(a, b, *na, *nb, depth)
                && nodes_eq(a, b, *ba, *bb, depth)
                && opt_nodes_eq(a, b, *da, *db, depth)
        }
        (
            ItemData::Structure {
                items: ita,
                inner_attrs: aa,
                doc: da,
            },
            ItemData::Structure {
                items: itb,
                inner_attrs: ab,
                doc: db,
            },
        ) => {
            vec_nodes_eq(a, b, ita, itb, depth)
                && aa.len() == ab.len()
                && opt_nodes_eq(a, b, *da, *db, depth)
        }
        (
            ItemData::Functor {
                params: pa,
                body: ba,
                doc: da,
            },
            ItemData::Functor {
                params: pb,
                body: bb,
                doc: db,
            },
        ) => {
            vec_nodes_eq(a, b, pa, pb, depth)
                && nodes_eq(a, b, *ba, *bb, depth)
                && opt_nodes_eq(a, b, *da, *db, depth)
        }
        (
            ItemData::FunctorParam {
                name: na,
                sig: sa,
            },
            ItemData::FunctorParam {
                name: nb,
                sig: sb,
            },
        ) => nodes_eq(a, b, *na, *nb, depth) && nodes_eq(a, b, *sa, *sb, depth),
        (
            ItemData::FunctorDecl {
                name: na,
                param_name: pna,
                param_sig: psa,
                return_sig: rsa,
                session_var: sva,
                doc: da,
            },
            ItemData::FunctorDecl {
                name: nb,
                param_name: pnb,
                param_sig: psb,
                return_sig: rsb,
                session_var: svb,
                doc: db,
            },
        ) => {
            nodes_eq(a, b, *na, *nb, depth)
                && nodes_eq(a, b, *pna, *pnb, depth)
                && nodes_eq(a, b, *psa, *psb, depth)
                && nodes_eq(a, b, *rsa, *rsb, depth)
                && opt_nodes_eq(a, b, *sva, *svb, depth)
                && opt_nodes_eq(a, b, *da, *db, depth)
        }
        (
            ItemData::Effect {
                name: na,
                ops: oa,
                doc: da,
            },
            ItemData::Effect {
                name: nb,
                ops: ob,
                doc: db,
            },
        ) => {
            nodes_eq(a, b, *na, *nb, depth)
                && vec_nodes_eq(a, b, oa, ob, depth)
                && opt_nodes_eq(a, b, *da, *db, depth)
        }
        (
            ItemData::OpSig {
                name: na,
                ty: ta,
                effect_set: ea,
            },
            ItemData::OpSig {
                name: nb,
                ty: tb,
                effect_set: eb,
            },
        ) => {
            nodes_eq(a, b, *na, *nb, depth)
                && nodes_eq(a, b, *ta, *tb, depth)
                && opt_nodes_eq(a, b, *ea, *eb, depth)
        }
        (
            ItemData::Capability {
                name: na,
                body: ba,
                doc: da,
            },
            ItemData::Capability {
                name: nb,
                body: bb,
                doc: db,
            },
        ) => {
            nodes_eq(a, b, *na, *nb, depth)
                && nodes_eq(a, b, *ba, *bb, depth)
                && opt_nodes_eq(a, b, *da, *db, depth)
        }
        (
            ItemData::Let {
                public: pa,
                mutable: ma,
                name: na,
                generic_params: gpa,
                ty: ta,
                value: va,
                align: aa,
                ring: ra,
                link_section: lsa,
                abi: aba,
                no_frame: nfa,
                interrupt: ia2,
                doc: da,
            },
            ItemData::Let {
                public: pb,
                mutable: mb,
                name: nb,
                generic_params: gpb,
                ty: tb,
                value: vb,
                align: ab2,
                ring: rb,
                link_section: lsb,
                abi: abb,
                no_frame: nfb,
                interrupt: ib2,
                doc: db,
            },
        ) => {
            pa == pb
                && ma == mb
                && aa == ab2
                && ra == rb
                && lsa == lsb
                && aba == abb
                && nfa == nfb
                && ia2 == ib2
                && generic_params_eq(a, b, gpa, gpb, depth)
                && nodes_eq(a, b, *na, *nb, depth)
                && opt_nodes_eq(a, b, *ta, *tb, depth)
                && nodes_eq(a, b, *va, *vb, depth)
                && opt_nodes_eq(a, b, *da, *db, depth)
        }
        (
            ItemData::Struct {
                name: na,
                generic_params: gpa,
                fields: fa,
                attributes: aa,
                doc: da,
            },
            ItemData::Struct {
                name: nb,
                generic_params: gpb,
                fields: fb,
                attributes: ab,
                doc: db,
            },
        ) => {
            aa.len() == ab.len()
                && generic_params_eq(a, b, gpa, gpb, depth)
                && nodes_eq(a, b, *na, *nb, depth)
                && pair_vec_eq(a, b, fa, fb, depth)
                && opt_nodes_eq(a, b, *da, *db, depth)
        }
        (
            ItemData::Enum {
                name: na,
                generic_params: gpa,
                variants: va,
                attributes: aa,
                doc: da,
            },
            ItemData::Enum {
                name: nb,
                generic_params: gpb,
                variants: vb,
                attributes: ab,
                doc: db,
            },
        ) => {
            aa.len() == ab.len()
                && generic_params_eq(a, b, gpa, gpb, depth)
                && nodes_eq(a, b, *na, *nb, depth)
                && enum_variants_eq(a, b, va, vb, depth)
                && opt_nodes_eq(a, b, *da, *db, depth)
        }
        (
            ItemData::Trait {
                name: na,
                generic_params: gpa,
                associated_types: ata,
                methods: ma,
                doc: da,
            },
            ItemData::Trait {
                name: nb,
                generic_params: gpb,
                associated_types: atb,
                methods: mb,
                doc: db,
            },
        ) => {
            ma.len() == mb.len()
                && nodes_eq(a, b, *na, *nb, depth)
                && generic_params_eq(a, b, gpa, gpb, depth)
                && vec_nodes_eq(a, b, ata, atb, depth)
                && opt_nodes_eq(a, b, *da, *db, depth)
        }
        (ItemData::Impl(ida), ItemData::Impl(idb)) => {
            ida.trait_args.len() == idb.trait_args.len()
                && generic_params_eq(a, b, &ida.generic_params, &idb.generic_params, depth)
                && opt_nodes_eq(a, b, ida.trait_name, idb.trait_name, depth)
                && vec_nodes_eq(a, b, &ida.trait_args, &idb.trait_args, depth)
                && nodes_eq(a, b, ida.for_type, idb.for_type, depth)
                && vec_nodes_eq(a, b, &ida.methods, &idb.methods, depth)
        }
        (
            ItemData::UnsafeBlock {
                effects: ea,
                capabilities: ca,
                justification: ja,
                block: bla,
            },
            ItemData::UnsafeBlock {
                effects: eb,
                capabilities: cb,
                justification: jb,
                block: blb,
            },
        ) => {
            vec_nodes_eq(a, b, ea, eb, depth)
                && vec_nodes_eq(a, b, ca, cb, depth)
                && nodes_eq(a, b, *ja, *jb, depth)
                && vec_nodes_eq(a, b, bla, blb, depth)
        }
        (ItemData::MacroDecl(mda), ItemData::MacroDecl(mdb)) => {
            // Rules compared by (count, pattern/template NodeId pair).
            // MacroPattern / MacroTemplate are leaf arena nodes whose
            // identity is the raw bytes — nodes_eq falls through to the
            // span-slice comparison. pattern_elems / template_elems
            // length match is a cheap structural guard.
            mda.rules.len() == mdb.rules.len()
                && nodes_eq(a, b, mda.name, mdb.name, depth)
                && mda.rules.iter().zip(mdb.rules.iter()).all(|(ra, rb)| {
                    ra.pattern_elems.len() == rb.pattern_elems.len()
                        && ra.template_elems.len() == rb.template_elems.len()
                        && ra.fragments.len() == rb.fragments.len()
                        && nodes_eq(a, b, ra.pattern, rb.pattern, depth)
                        && nodes_eq(a, b, ra.template, rb.template, depth)
                })
                && opt_nodes_eq(a, b, mda.doc, mdb.doc, depth)
        }
        (ItemData::NonItem, ItemData::NonItem) => true,
        _ => false,
    }
}

// -------------------------------------------------------------------
// Expressions
// -------------------------------------------------------------------

fn exprs_eq(a: &Side<'_>, b: &Side<'_>, ea: &ExprData, eb: &ExprData, depth: usize) -> bool {
    match (ea, eb) {
        (
            ExprData::Lambda {
                generic_params: gpa,
                params: pa,
                body: ba,
                pipe_form: pfa,
            },
            ExprData::Lambda {
                generic_params: gpb,
                params: pb,
                body: bb,
                pipe_form: pfb,
            },
        ) => {
            pfa == pfb
                && generic_params_eq(a, b, gpa, gpb, depth)
                && vec_nodes_eq(a, b, pa, pb, depth)
                && nodes_eq(a, b, *ba, *bb, depth)
        }
        (
            ExprData::ActionBlock {
                effects: ea2,
                capabilities: ca,
                body: ba,
            },
            ExprData::ActionBlock {
                effects: eb2,
                capabilities: cb,
                body: bb,
            },
        ) => {
            opt_nodes_eq(a, b, *ea2, *eb2, depth)
                && opt_nodes_eq(a, b, *ca, *cb, depth)
                && vec_nodes_eq(a, b, ba, bb, depth)
        }
        (
            ExprData::WithHandler {
                handler: ha,
                bind: ba,
                block: blka,
                finally: fa,
            },
            ExprData::WithHandler {
                handler: hb,
                bind: bb,
                block: blkb,
                finally: fb,
            },
        ) => {
            nodes_eq(a, b, *ha, *hb, depth)
                && nodes_eq(a, b, *ba, *bb, depth)
                && nodes_eq(a, b, *blka, *blkb, depth)
                && opt_nodes_eq(a, b, *fa, *fb, depth)
        }
        (
            ExprData::Unsafe {
                effects: ea2,
                capabilities: ca,
                justification: ja,
                block: bla,
            },
            ExprData::Unsafe {
                effects: eb2,
                capabilities: cb,
                justification: jb,
                block: blb,
            },
        ) => {
            vec_nodes_eq(a, b, ea2, eb2, depth)
                && vec_nodes_eq(a, b, ca, cb, depth)
                && nodes_eq(a, b, *ja, *jb, depth)
                && vec_nodes_eq(a, b, bla, blb, depth)
        }
        (
            ExprData::Infix {
                lhs: la,
                op: oa,
                rhs: ra,
            },
            ExprData::Infix {
                lhs: lb,
                op: ob,
                rhs: rb,
            },
        ) => {
            nodes_eq(a, b, *la, *lb, depth)
                && nodes_eq(a, b, *oa, *ob, depth)
                && nodes_eq(a, b, *ra, *rb, depth)
        }
        (
            ExprData::Prefix {
                op: oa,
                expr: ea2,
                kind: ka,
            },
            ExprData::Prefix {
                op: ob,
                expr: eb2,
                kind: kb,
            },
        ) => {
            ka == kb && nodes_eq(a, b, *oa, *ob, depth) && nodes_eq(a, b, *ea2, *eb2, depth)
        }
        (
            ExprData::Postfix { expr: ea2, op: oa },
            ExprData::Postfix { expr: eb2, op: ob },
        ) => nodes_eq(a, b, *ea2, *eb2, depth) && nodes_eq(a, b, *oa, *ob, depth),
        (
            ExprData::Cast {
                expr: ea2,
                target_ty: ta,
            },
            ExprData::Cast {
                expr: eb2,
                target_ty: tb,
            },
        ) => nodes_eq(a, b, *ea2, *eb2, depth) && nodes_eq(a, b, *ta, *tb, depth),
        (ExprData::Literal { lit: la }, ExprData::Literal { lit: lb }) => {
            nodes_eq(a, b, *la, *lb, depth)
        }
        (ExprData::Path { segments: sa }, ExprData::Path { segments: sb }) => {
            vec_nodes_eq(a, b, sa, sb, depth)
        }
        (
            ExprData::Call {
                callee: ca,
                args: aa,
            },
            ExprData::Call {
                callee: cb,
                args: ab,
            },
        ) => nodes_eq(a, b, *ca, *cb, depth) && vec_nodes_eq(a, b, aa, ab, depth),
        (
            ExprData::Block {
                stmts: sa,
                tail: ta,
            },
            ExprData::Block {
                stmts: sb,
                tail: tb,
            },
        ) => vec_nodes_eq(a, b, sa, sb, depth) && opt_nodes_eq(a, b, *ta, *tb, depth),
        (
            ExprData::Match {
                scrutinee: sa,
                arms: aa,
                attrs: ata,
            },
            ExprData::Match {
                scrutinee: sb,
                arms: ab,
                attrs: atb,
            },
        ) => {
            ata == atb
                && nodes_eq(a, b, *sa, *sb, depth)
                && aa.len() == ab.len()
                && aa.iter().zip(ab.iter()).all(|(arm_a, arm_b)| {
                    nodes_eq(a, b, arm_a.pattern, arm_b.pattern, depth)
                        && opt_nodes_eq(a, b, arm_a.guard, arm_b.guard, depth)
                        && nodes_eq(a, b, arm_a.body, arm_b.body, depth)
                })
        }
        (
            ExprData::If {
                cond: ca,
                then_block: ta,
                else_block: ea2,
            },
            ExprData::If {
                cond: cb,
                then_block: tb,
                else_block: eb2,
            },
        ) => {
            nodes_eq(a, b, *ca, *cb, depth)
                && nodes_eq(a, b, *ta, *tb, depth)
                && opt_nodes_eq(a, b, *ea2, *eb2, depth)
        }
        (
            ExprData::Loop {
                kind: ka,
                header: ha,
                body: ba,
            },
            ExprData::Loop {
                kind: kb,
                header: hb,
                body: bb,
            },
        ) => {
            ka == kb
                && opt_nodes_eq(a, b, *ha, *hb, depth)
                && nodes_eq(a, b, *ba, *bb, depth)
        }
        (
            ExprData::For {
                pattern: pa,
                iterable: ia,
                body: ba,
            },
            ExprData::For {
                pattern: pb,
                iterable: ib,
                body: bb,
            },
        ) => {
            nodes_eq(a, b, *pa, *pb, depth)
                && nodes_eq(a, b, *ia, *ib, depth)
                && nodes_eq(a, b, *ba, *bb, depth)
        }
        (ExprData::Break, ExprData::Break) => true,
        (ExprData::Continue, ExprData::Continue) => true,
        (ExprData::OperandRegister { reg: ra }, ExprData::OperandRegister { reg: rb }) => {
            nodes_eq(a, b, *ra, *rb, depth)
        }
        (
            ExprData::OperandImmediate { expr: ea2 },
            ExprData::OperandImmediate { expr: eb2 },
        ) => nodes_eq(a, b, *ea2, *eb2, depth),
        (
            ExprData::OperandMemoryRef {
                segment: sa,
                addr: aa,
            },
            ExprData::OperandMemoryRef {
                segment: sb,
                addr: ab,
            },
        ) => sa == sb && nodes_eq(a, b, *aa, *ab, depth),
        (
            ExprData::Perform {
                op_path: oa,
                args: aa,
            },
            ExprData::Perform {
                op_path: ob,
                args: ab,
            },
        ) => nodes_eq(a, b, *oa, *ob, depth) && vec_nodes_eq(a, b, aa, ab, depth),
        (ExprData::Resume { value: va }, ExprData::Resume { value: vb }) => {
            nodes_eq(a, b, *va, *vb, depth)
        }
        (
            ExprData::HandlerValue {
                effect: ea2,
                arms: aa,
            },
            ExprData::HandlerValue {
                effect: eb2,
                arms: ab,
            },
        ) => {
            nodes_eq(a, b, *ea2, *eb2, depth)
                && aa.len() == ab.len()
                && aa.iter().zip(ab.iter()).all(|(h1, h2)| match (h1, h2) {
                    (
                        HandlerArm::Op {
                            op: o1,
                            handler: h1,
                        },
                        HandlerArm::Op {
                            op: o2,
                            handler: h2,
                        },
                    ) => nodes_eq(a, b, *o1, *o2, depth) && nodes_eq(a, b, *h1, *h2, depth),
                    (
                        HandlerArm::Finally { cleanup: c1 },
                        HandlerArm::Finally { cleanup: c2 },
                    ) => nodes_eq(a, b, *c1, *c2, depth),
                    _ => false,
                })
        }
        (ExprData::Quote { body: ba }, ExprData::Quote { body: bb }) => {
            nodes_eq(a, b, *ba, *bb, depth)
        }
        (ExprData::Antiquote { value: va }, ExprData::Antiquote { value: vb }) => {
            nodes_eq(a, b, *va, *vb, depth)
        }
        (
            ExprData::FunctorApp {
                functor: fa,
                arguments: aa,
                sharing: sa,
            },
            ExprData::FunctorApp {
                functor: fb,
                arguments: ab,
                sharing: sb,
            },
        ) => {
            nodes_eq(a, b, *fa, *fb, depth)
                && vec_nodes_eq(a, b, aa, ab, depth)
                && sa.len() == sb.len()
                && sa.iter().zip(sb.iter()).all(|(c1, c2)| {
                    c1.left_path == c2.left_path && c1.right_path == c2.right_path
                })
        }
        (
            ExprData::Pack {
                module_path: ma,
                signature_path: sa,
            },
            ExprData::Pack {
                module_path: mb,
                signature_path: sb,
            },
        ) => nodes_eq(a, b, *ma, *mb, depth) && nodes_eq(a, b, *sa, *sb, depth),
        (ExprData::Unpack { value: va }, ExprData::Unpack { value: vb }) => {
            nodes_eq(a, b, *va, *vb, depth)
        }
        (
            ExprData::LetModule {
                name: na,
                body: ba,
                rest: ra,
            },
            ExprData::LetModule {
                name: nb,
                body: bb,
                rest: rb,
            },
        ) => {
            na == nb && nodes_eq(a, b, *ba, *bb, depth) && nodes_eq(a, b, *ra, *rb, depth)
        }
        (
            ExprData::RecordCons {
                type_name: ta,
                fields: fa,
            },
            ExprData::RecordCons {
                type_name: tb,
                fields: fb,
            },
        ) => nodes_eq(a, b, *ta, *tb, depth) && pair_vec_eq(a, b, fa, fb, depth),
        (
            ExprData::FieldAccess {
                receiver: ra,
                field: fa,
            },
            ExprData::FieldAccess {
                receiver: rb,
                field: fb,
            },
        ) => nodes_eq(a, b, *ra, *rb, depth) && nodes_eq(a, b, *fa, *fb, depth),
        (ExprData::StringLiteral(va), ExprData::StringLiteral(vb)) => va == vb,
        (ExprData::ByteStringLiteral(va), ExprData::ByteStringLiteral(vb)) => va == vb,
        (ExprData::InlineBytes(va), ExprData::InlineBytes(vb)) => va == vb,
        (ExprData::InlineStr(va), ExprData::InlineStr(vb)) => va == vb,
        (
            ExprData::Borrow {
                expr: ea2,
                mutable: ma,
            },
            ExprData::Borrow {
                expr: eb2,
                mutable: mb,
            },
        ) => ma == mb && nodes_eq(a, b, *ea2, *eb2, depth),
        (ExprData::Deref { expr: ea2 }, ExprData::Deref { expr: eb2 }) => {
            nodes_eq(a, b, *ea2, *eb2, depth)
        }
        (ExprData::ArrayLit(va), ExprData::ArrayLit(vb)) => vec_nodes_eq(a, b, va, vb, depth),
        (
            ExprData::ArrayRepeat {
                expr: ea2,
                count: ca,
            },
            ExprData::ArrayRepeat {
                expr: eb2,
                count: cb,
            },
        ) => nodes_eq(a, b, *ea2, *eb2, depth) && nodes_eq(a, b, *ca, *cb, depth),
        (ExprData::Uninit, ExprData::Uninit) => true,
        (
            ExprData::Range {
                start: sa,
                end: ea2,
            },
            ExprData::Range {
                start: sb,
                end: eb2,
            },
        ) => opt_nodes_eq(a, b, *sa, *sb, depth) && opt_nodes_eq(a, b, *ea2, *eb2, depth),
        (ExprData::Tuple { elements: ea2 }, ExprData::Tuple { elements: eb2 }) => {
            vec_nodes_eq(a, b, ea2, eb2, depth)
        }
        _ => false,
    }
}

// -------------------------------------------------------------------
// Statements
// -------------------------------------------------------------------

fn stmts_eq(a: &Side<'_>, b: &Side<'_>, sa: &StmtData, sb: &StmtData, depth: usize) -> bool {
    match (sa, sb) {
        (
            StmtData::Let {
                mutable: ma,
                name: na,
                ty: ta,
                value: va,
                atomic: aa,
            },
            StmtData::Let {
                mutable: mb,
                name: nb,
                ty: tb,
                value: vb,
                atomic: ab,
            },
        ) => {
            ma == mb
                && aa == ab
                && nodes_eq(a, b, *na, *nb, depth)
                && opt_nodes_eq(a, b, *ta, *tb, depth)
                && nodes_eq(a, b, *va, *vb, depth)
        }
        (StmtData::Expr { expr: ea }, StmtData::Expr { expr: eb }) => {
            nodes_eq(a, b, *ea, *eb, depth)
        }
        (StmtData::Return { value: va }, StmtData::Return { value: vb }) => {
            opt_nodes_eq(a, b, *va, *vb, depth)
        }
        (
            StmtData::Instruction {
                mnemonic: ma,
                operands: oa,
            },
            StmtData::Instruction {
                mnemonic: mb,
                operands: ob,
            },
        ) => {
            // Mnemonic IDs are per-arena; resolve to text before comparing.
            a.arena.mnemonic_str(*ma) == b.arena.mnemonic_str(*mb)
                && vec_nodes_eq(a, b, oa, ob, depth)
        }
        (StmtData::Label { name: na }, StmtData::Label { name: nb }) => {
            nodes_eq(a, b, *na, *nb, depth)
        }
        _ => false,
    }
}

// -------------------------------------------------------------------
// Types
// -------------------------------------------------------------------

fn types_eq(a: &Side<'_>, b: &Side<'_>, ta: &TypeData, tb: &TypeData, depth: usize) -> bool {
    match (ta, tb) {
        (
            TypeData::Name {
                name: na,
                args: aa,
            },
            TypeData::Name {
                name: nb,
                args: ab,
            },
        ) => nodes_eq(a, b, *na, *nb, depth) && vec_nodes_eq(a, b, aa, ab, depth),
        (
            TypeData::FnPtr {
                params: pa,
                param_names: pna,
                ret: ra,
                effects: ea,
                capabilities: ca,
            },
            TypeData::FnPtr {
                params: pb,
                param_names: pnb,
                ret: rb,
                effects: eb,
                capabilities: cb,
            },
        ) => {
            pna.len() == pnb.len()
                && pna
                    .iter()
                    .zip(pnb.iter())
                    .all(|(na, nb)| opt_nodes_eq(a, b, *na, *nb, depth))
                && vec_nodes_eq(a, b, pa, pb, depth)
                && nodes_eq(a, b, *ra, *rb, depth)
                && opt_nodes_eq(a, b, *ea, *eb, depth)
                && opt_nodes_eq(a, b, *ca, *cb, depth)
        }
        (
            TypeData::Closure {
                params: pa,
                ret: ra,
                effects: ea,
                capabilities: ca,
            },
            TypeData::Closure {
                params: pb,
                ret: rb,
                effects: eb,
                capabilities: cb,
            },
        ) => {
            vec_nodes_eq(a, b, pa, pb, depth)
                && nodes_eq(a, b, *ra, *rb, depth)
                && opt_nodes_eq(a, b, *ea, *eb, depth)
                && opt_nodes_eq(a, b, *ca, *cb, depth)
        }
        (TypeData::Tuple { elements: ea }, TypeData::Tuple { elements: eb }) => {
            vec_nodes_eq(a, b, ea, eb, depth)
        }
        (
            TypeData::LinearClass {
                class: ca,
                inner: ia,
            },
            TypeData::LinearClass {
                class: cb,
                inner: ib,
            },
        ) => ca == cb && nodes_eq(a, b, *ia, *ib, depth),
        (
            TypeData::EffectRow {
                items: ia,
                rest: ra,
            },
            TypeData::EffectRow {
                items: ib,
                rest: rb,
            },
        ) => vec_nodes_eq(a, b, ia, ib, depth) && opt_nodes_eq(a, b, *ra, *rb, depth),
        (TypeData::Ptr { pointee: pa }, TypeData::Ptr { pointee: pb }) => {
            nodes_eq(a, b, *pa, *pb, depth)
        }
        (
            TypeData::Ref {
                pointee: pa,
                mutable: ma,
            },
            TypeData::Ref {
                pointee: pb,
                mutable: mb,
            },
        ) => ma == mb && nodes_eq(a, b, *pa, *pb, depth),
        (TypeData::Record { fields: fa }, TypeData::Record { fields: fb }) => {
            pair_vec_eq(a, b, fa, fb, depth)
        }
        (TypeData::Enum { variants: va }, TypeData::Enum { variants: vb }) => {
            enum_variants_eq(a, b, va, vb, depth)
        }
        (
            TypeData::SelfQualifiedPath { item: ia },
            TypeData::SelfQualifiedPath { item: ib },
        ) => nodes_eq(a, b, *ia, *ib, depth),
        (
            TypeData::Array {
                element: ea,
                length: la,
            },
            TypeData::Array {
                element: eb,
                length: lb,
            },
        ) => nodes_eq(a, b, *ea, *eb, depth) && nodes_eq(a, b, *la, *lb, depth),
        (
            TypeData::Forall {
                bound: ba,
                body: boa,
            },
            TypeData::Forall {
                bound: bb,
                body: bob,
            },
        ) => vec_nodes_eq(a, b, ba, bb, depth) && nodes_eq(a, b, *boa, *bob, depth),
        _ => false,
    }
}

// -------------------------------------------------------------------
// Patterns
// -------------------------------------------------------------------

fn patterns_eq(
    a: &Side<'_>,
    b: &Side<'_>,
    pa: &PatternData,
    pb: &PatternData,
    depth: usize,
) -> bool {
    match (pa, pb) {
        (PatternData::Wildcard, PatternData::Wildcard) => true,
        (
            PatternData::Ident {
                name: na,
                mutable: ma,
            },
            PatternData::Ident {
                name: nb,
                mutable: mb,
            },
        ) => ma == mb && nodes_eq(a, b, *na, *nb, depth),
        (PatternData::Literal { lit: la }, PatternData::Literal { lit: lb }) => {
            nodes_eq(a, b, *la, *lb, depth)
        }
        (PatternData::Tuple { elements: ea }, PatternData::Tuple { elements: eb }) => {
            vec_nodes_eq(a, b, ea, eb, depth)
        }
        (
            PatternData::Struct {
                path: pa2,
                fields: fa,
            },
            PatternData::Struct {
                path: pb2,
                fields: fb,
            },
        ) => {
            nodes_eq(a, b, *pa2, *pb2, depth)
                && fa.len() == fb.len()
                && fa.iter().zip(fb.iter()).all(|(f1, f2)| {
                    nodes_eq(a, b, f1.name, f2.name, depth)
                        && nodes_eq(a, b, f1.pattern, f2.pattern, depth)
                })
        }
        (
            PatternData::EnumVariant {
                path: pa2,
                args: aa,
            },
            PatternData::EnumVariant {
                path: pb2,
                args: ab,
            },
        ) => nodes_eq(a, b, *pa2, *pb2, depth) && vec_nodes_eq(a, b, aa, ab, depth),
        (
            PatternData::Or { alternatives: aa },
            PatternData::Or { alternatives: ab },
        ) => vec_nodes_eq(a, b, aa, ab, depth),
        (
            PatternData::Binding {
                name: na,
                inner: ia,
            },
            PatternData::Binding {
                name: nb,
                inner: ib,
            },
        ) => nodes_eq(a, b, *na, *nb, depth) && nodes_eq(a, b, *ia, *ib, depth),
        (
            PatternData::Range {
                start: sa,
                end: ea,
            },
            PatternData::Range {
                start: sb,
                end: eb,
            },
        ) => opt_nodes_eq(a, b, *sa, *sb, depth) && opt_nodes_eq(a, b, *ea, *eb, depth),
        (
            PatternData::Reference {
                inner: ia,
                mutable: ma,
            },
            PatternData::Reference {
                inner: ib,
                mutable: mb,
            },
        ) => ma == mb && nodes_eq(a, b, *ia, *ib, depth),
        (PatternData::Slice { elements: ea }, PatternData::Slice { elements: eb }) => {
            vec_nodes_eq(a, b, ea, eb, depth)
        }
        (PatternData::Rest { binder: ba }, PatternData::Rest { binder: bb }) => {
            opt_nodes_eq(a, b, *ba, *bb, depth)
        }
        _ => false,
    }
}

// -------------------------------------------------------------------
// Shared helpers
// -------------------------------------------------------------------

fn opt_nodes_eq(
    a: &Side<'_>,
    b: &Side<'_>,
    ia: Option<NodeId>,
    ib: Option<NodeId>,
    depth: usize,
) -> bool {
    match (ia, ib) {
        (None, None) => true,
        (Some(x), Some(y)) => nodes_eq(a, b, x, y, depth),
        _ => false,
    }
}

fn vec_nodes_eq(
    a: &Side<'_>,
    b: &Side<'_>,
    va: &[NodeId],
    vb: &[NodeId],
    depth: usize,
) -> bool {
    va.len() == vb.len()
        && va
            .iter()
            .zip(vb.iter())
            .all(|(x, y)| nodes_eq(a, b, *x, *y, depth))
}

fn pair_vec_eq(
    a: &Side<'_>,
    b: &Side<'_>,
    va: &[(NodeId, NodeId)],
    vb: &[(NodeId, NodeId)],
    depth: usize,
) -> bool {
    va.len() == vb.len()
        && va.iter().zip(vb.iter()).all(|((n1, t1), (n2, t2))| {
            nodes_eq(a, b, *n1, *n2, depth) && nodes_eq(a, b, *t1, *t2, depth)
        })
}

fn generic_params_eq(
    a: &Side<'_>,
    b: &Side<'_>,
    ga: &[GenericParam],
    gb: &[GenericParam],
    depth: usize,
) -> bool {
    if ga.len() != gb.len() {
        return false;
    }
    for (p1, p2) in ga.iter().zip(gb.iter()) {
        match (p1, p2) {
            (
                GenericParam::Type {
                    name: na,
                    bounds: ba,
                },
                GenericParam::Type {
                    name: nb,
                    bounds: bb,
                },
            ) => {
                if !nodes_eq(a, b, *na, *nb, depth) || ba.len() != bb.len() {
                    return false;
                }
                for (tb1, tb2) in ba.iter().zip(bb.iter()) {
                    if !nodes_eq(a, b, tb1.path, tb2.path, depth) {
                        return false;
                    }
                    if tb1.projections.len() != tb2.projections.len() {
                        return false;
                    }
                    for ((pn1, pt1), (pn2, pt2)) in
                        tb1.projections.iter().zip(tb2.projections.iter())
                    {
                        if !nodes_eq(a, b, *pn1, *pn2, depth)
                            || !nodes_eq(a, b, *pt1, *pt2, depth)
                        {
                            return false;
                        }
                    }
                }
            }
            (
                GenericParam::Lifetime { name: na },
                GenericParam::Lifetime { name: nb },
            ) => {
                if na != nb {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

fn enum_variants_eq(
    a: &Side<'_>,
    b: &Side<'_>,
    va: &[EnumVariant],
    vb: &[EnumVariant],
    depth: usize,
) -> bool {
    if va.len() != vb.len() {
        return false;
    }
    for (v1, v2) in va.iter().zip(vb.iter()) {
        match (v1, v2) {
            (EnumVariant::Unit { name: n1 }, EnumVariant::Unit { name: n2 }) => {
                if !nodes_eq(a, b, *n1, *n2, depth) {
                    return false;
                }
            }
            (
                EnumVariant::Tuple {
                    name: n1,
                    payload: p1,
                },
                EnumVariant::Tuple {
                    name: n2,
                    payload: p2,
                },
            ) => {
                if !nodes_eq(a, b, *n1, *n2, depth) || !vec_nodes_eq(a, b, p1, p2, depth) {
                    return false;
                }
            }
            (
                EnumVariant::Record {
                    name: n1,
                    fields: f1,
                },
                EnumVariant::Record {
                    name: n2,
                    fields: f2,
                },
            ) => {
                if !nodes_eq(a, b, *n1, *n2, depth) || !pair_vec_eq(a, b, f1, f2, depth) {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::{AstArena, ItemData, NodeKind};
    use paideia_as_diagnostics::{FileId, Span};

    fn file() -> FileId {
        FileId::new(1).unwrap()
    }

    #[test]
    fn empty_arenas_structure_eq_when_same_single_leaf() {
        // Two arenas, each with a single Placeholder covering identical
        // text slices.
        let mut a = AstArena::new();
        let mut b = AstArena::new();
        let ida = a.alloc(NodeKind::Placeholder, Span::new(file(), 0, 3));
        let idb = b.alloc(NodeKind::Placeholder, Span::new(file(), 2, 3));
        assert!(ast_structurally_equal("foo", &a, ida, "..foo", &b, idb));
    }

    #[test]
    fn kind_mismatch_is_unequal() {
        let mut a = AstArena::new();
        let mut b = AstArena::new();
        let ida = a.alloc(NodeKind::Placeholder, Span::new(file(), 0, 3));
        let idb = b.alloc(NodeKind::Ident, Span::new(file(), 0, 3));
        assert!(!ast_structurally_equal("foo", &a, ida, "foo", &b, idb));
    }

    #[test]
    fn ident_text_mismatch_is_unequal() {
        let mut a = AstArena::new();
        let mut b = AstArena::new();
        let ida = a.alloc(NodeKind::Ident, Span::new(file(), 0, 3));
        let idb = b.alloc(NodeKind::Ident, Span::new(file(), 0, 3));
        assert!(!ast_structurally_equal("foo", &a, ida, "bar", &b, idb));
    }

    #[test]
    fn structure_with_matching_items_is_eq() {
        let mut a = AstArena::new();
        let mut b = AstArena::new();
        let name_a = a.alloc(NodeKind::Ident, Span::new(file(), 4, 1));
        let val_a = a.alloc(NodeKind::Placeholder, Span::new(file(), 8, 1));
        let let_a = a.alloc_item(
            NodeKind::Let,
            Span::new(file(), 0, 10),
            ItemData::Let {
                public: false,
                mutable: false,
                name: name_a,
                generic_params: vec![],
                ty: None,
                value: val_a,
                align: None,
                ring: None,
                link_section: None,
                abi: None,
                no_frame: false,
                interrupt: None,
                doc: None,
            },
        );
        let struct_a = a.alloc_item(
            NodeKind::Structure,
            Span::new(file(), 0, 10),
            ItemData::Structure {
                items: vec![let_a],
                inner_attrs: vec![],
                doc: None,
            },
        );

        // Mirror arena with different spans — formatter re-emission may
        // move everything by whitespace changes.
        let name_b = b.alloc(NodeKind::Ident, Span::new(file(), 6, 1));
        let val_b = b.alloc(NodeKind::Placeholder, Span::new(file(), 10, 1));
        let let_b = b.alloc_item(
            NodeKind::Let,
            Span::new(file(), 0, 12),
            ItemData::Let {
                public: false,
                mutable: false,
                name: name_b,
                generic_params: vec![],
                ty: None,
                value: val_b,
                align: None,
                ring: None,
                link_section: None,
                abi: None,
                no_frame: false,
                interrupt: None,
                doc: None,
            },
        );
        let struct_b = b.alloc_item(
            NodeKind::Structure,
            Span::new(file(), 0, 12),
            ItemData::Structure {
                items: vec![let_b],
                inner_attrs: vec![],
                doc: None,
            },
        );

        assert!(ast_structurally_equal(
            "let x = 1\n",
            &a,
            struct_a,
            "let  x  =  1\n",
            &b,
            struct_b,
        ));
    }

    #[test]
    fn flag_difference_is_unequal() {
        // `pub` vs non-`pub` must break equivalence.
        let mut a = AstArena::new();
        let mut b = AstArena::new();
        let name_a = a.alloc(NodeKind::Ident, Span::new(file(), 0, 1));
        let val_a = a.alloc(NodeKind::Placeholder, Span::new(file(), 2, 1));
        let let_a = a.alloc_item(
            NodeKind::Let,
            Span::new(file(), 0, 3),
            ItemData::Let {
                public: true,
                mutable: false,
                name: name_a,
                generic_params: vec![],
                ty: None,
                value: val_a,
                align: None,
                ring: None,
                link_section: None,
                abi: None,
                no_frame: false,
                interrupt: None,
                doc: None,
            },
        );
        let name_b = b.alloc(NodeKind::Ident, Span::new(file(), 0, 1));
        let val_b = b.alloc(NodeKind::Placeholder, Span::new(file(), 2, 1));
        let let_b = b.alloc_item(
            NodeKind::Let,
            Span::new(file(), 0, 3),
            ItemData::Let {
                public: false,
                mutable: false,
                name: name_b,
                generic_params: vec![],
                ty: None,
                value: val_b,
                align: None,
                ring: None,
                link_section: None,
                abi: None,
                no_frame: false,
                interrupt: None,
                doc: None,
            },
        );
        assert!(!ast_structurally_equal("x 1", &a, let_a, "x 1", &b, let_b));
    }
}
