//! The unified [`SyntaxNode`] sum type spanning all three shell
//! sub-languages.
//!
//! # Why one enum, not three
//!
//! R222 (functor surface), R225 (unified HM checker), and R226
//! (Datalog evaluator) all consume the same AST vocabulary. If the
//! pipeline / Datalog / lambda surfaces had separate node types we
//! would need adapter enums at every boundary. One enum, dispatched
//! by the parser via [`crate::Context`] carried on [`NodeSpan`],
//! keeps the downstream code loop-shaped.
//!
//! # Variant grouping
//!
//! Variants are grouped by sub-language for readability; consumers
//! should not depend on the grouping.
//!
//! * **Pipeline**: `Cmd`, `Pipe`, `Seq`, `Redirect`, `Background`,
//!   `Group`.
//! * **Datalog**: `DatalogBlock`, `Atom`, `Rule`, `QVar`, `InterpVar`,
//!   `NotAtom`.
//! * **Lambda**: `Lambda`, `App`, `Var`, `Let`, `Match`, `BinOp`,
//!   `UnaryOp`, `FieldAccess`.
//! * **Literals & shared**: `RecordExpr`, `LitStr`, `LitInt`,
//!   `LitBool`, `Ident`.
//!
//! # Span discipline
//!
//! Every variant carries a [`NodeSpan`]. Constructors always compute
//! `span.union` over child spans so a diagnostic on a nested node can
//! resolve to a range that covers only its own text (not the parent's).

use crate::span::NodeSpan;

/// Redirection kind for pipeline stages.
///
/// The redirect target is itself a [`SyntaxNode`] (typically `LitStr`
/// or `Cmd` for `>(cmd)` process substitution — the latter deferred to
/// R222 but the AST shape is fixed here so the R222 evaluator adds
/// only semantics, not a new variant).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RedirectKind {
    /// `> file` — overwrite stdout.
    StdoutOverwrite,
    /// `>> file` — append stdout.
    StdoutAppend,
    /// `< file` — read stdin.
    StdinFrom,
    /// `2> file` — overwrite stderr.
    StderrOverwrite,
    /// `2>> file` — append stderr.
    StderrAppend,
    /// `&> file` — overwrite both. Kept because SH-D5 lists it as a
    /// convenience; the R222 evaluator lowers to two writes.
    BothOverwrite,
}

/// One field of a record literal. Split out because a record has an
/// arbitrary number of them and pairing `(String, SyntaxNode)` inline
/// on the variant would create a nested-tuple pattern the R226
/// pretty-matcher would then have to peel apart at every access.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordField {
    /// Field name (post-NFC).
    pub name: String,
    /// Field value expression.
    pub value: SyntaxNode,
    /// Span covering the whole `name: value` pair, not the value alone.
    pub span: NodeSpan,
}

/// One arm of a `match` expression. `pattern` is itself a
/// [`SyntaxNode`] — R221.M5 parses patterns using the same expression
/// grammar; R225's HM elaborator narrows to pattern-legal shapes
/// (`Var`, `LitInt`, `LitStr`, `LitBool`, `RecordExpr`, or `Ident`
/// as a wildcard). Nested pattern grammar is a follow-on milestone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchArm {
    /// Pattern (expression-shaped in R221.M5).
    pub pattern: SyntaxNode,
    /// Optional guard clause `pattern if cond => body`.
    pub guard: Option<SyntaxNode>,
    /// Arm body.
    pub body: SyntaxNode,
    /// Span from the pattern's leftmost byte through the body's
    /// rightmost byte.
    pub span: NodeSpan,
}

/// The unified AST node for all three shell sub-languages.
///
/// See the module doc for the variant grouping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyntaxNode {
    // ---- Pipeline stages -----------------------------------------
    /// A command invocation: `name arg1 arg2 …`. `name` is boxed so
    /// path-like `bin/tool` (parsed as a `FieldAccess` chain in R222)
    /// slots in without a wrapper variant.
    Cmd {
        /// Command name / callable.
        name: Box<SyntaxNode>,
        /// Positional arguments in source order.
        args: Vec<SyntaxNode>,
        /// Span covering `name` through the last `arg`.
        span: NodeSpan,
    },
    /// A pipeline connective: `lhs | rhs`. Right-associates so
    /// `a | b | c` parses as `Pipe(a, Pipe(b, c))` — the R222 evaluator
    /// walks left-to-right by descending the right spine.
    Pipe {
        /// Left-hand-side stage.
        lhs: Box<SyntaxNode>,
        /// Right-hand-side pipeline (possibly another `Pipe`).
        rhs: Box<SyntaxNode>,
        /// Span covering the `|` itself and both sides.
        span: NodeSpan,
    },
    /// A sequence: `a; b; c`. Distinct from `Pipe` because `;` starts
    /// a fresh pipeline (fresh stdin from the terminal) rather than
    /// threading the previous stage's stdout.
    Seq {
        /// Sequenced items in source order.
        items: Vec<SyntaxNode>,
        /// Span from first item through last.
        span: NodeSpan,
    },
    /// A redirection: `stage > file` or friends. `target` is a
    /// [`SyntaxNode`] to leave room for R222's process substitution.
    Redirect {
        /// The stage whose stream is being redirected.
        source: Box<SyntaxNode>,
        /// Redirection kind (see [`RedirectKind`]).
        kind: RedirectKind,
        /// Target expression (typically `LitStr` in R221.M5).
        target: Box<SyntaxNode>,
        /// Span covering source, operator, and target.
        span: NodeSpan,
    },
    /// A backgrounded stage: `stage &`. The `&` shows up only at
    /// pipeline top-level; nested inside `{ }` or `( )` it is a syntax
    /// error at R221.M5 (recovered as `UnexpectedOp`).
    Background {
        /// The stage being detached.
        inner: Box<SyntaxNode>,
        /// Span covering `inner` and the trailing `&`.
        span: NodeSpan,
    },
    /// A parenthesised sub-expression `( … )`. Distinct from `Cmd`
    /// because the R229 colorer, R225 elaborator and R222 evaluator
    /// all want to know that the grouping was explicit (parens are
    /// preserved by the pretty-printer for round-trip fidelity).
    Group {
        /// The wrapped expression.
        inner: Box<SyntaxNode>,
        /// Span covering the opening `(` through the closing `)`.
        span: NodeSpan,
    },

    // ---- Datalog -----------------------------------------------
    /// A `datalog { … }` block. Items are `Atom` (fact), `Rule`, or
    /// `NotAtom` (negated goal). Parser strips the surrounding `datalog {`
    /// and `}`; the block's span still covers them.
    DatalogBlock {
        /// Ordered items inside the block.
        items: Vec<SyntaxNode>,
        /// Span from `datalog` through the closing `}`.
        span: NodeSpan,
    },
    /// A Datalog atom `pred(arg1, arg2, …)`. Arity is `args.len()`;
    /// tests and the R226 evaluator both key off it.
    Atom {
        /// Predicate name (post-NFC).
        pred: String,
        /// Argument terms.
        args: Vec<SyntaxNode>,
        /// Span from the predicate name's first byte through the
        /// closing `)`.
        span: NodeSpan,
    },
    /// A Datalog rule `head => body_atom, body_atom, …`. `body` is
    /// modelled as a `Vec<SyntaxNode>` rather than nested `And` nodes;
    /// R226 iterates through it directly.
    Rule {
        /// Rule head atom.
        head: Box<SyntaxNode>,
        /// Rule body conjuncts.
        body: Vec<SyntaxNode>,
        /// Span from head's first byte through the trailing `.`.
        span: NodeSpan,
    },
    /// A Datalog logic variable `?name`.
    QVar {
        /// Variable name (post-NFC).
        name: String,
        /// Span covering the `?` and the name.
        span: NodeSpan,
    },
    /// A pipeline-value interpolation `$name` inside a Datalog block.
    /// The R226 evaluator resolves the `name` against the enclosing
    /// pipeline's binding environment.
    InterpVar {
        /// Interpolated name (post-NFC).
        name: String,
        /// Span covering the `$` and the name.
        span: NodeSpan,
    },
    /// A negated Datalog goal `not atom(…)`. R226 evaluates with
    /// stratified negation; the parser recognizes any `not` prefixed to
    /// an `Atom` (nested `not not` is an error at R221.M5).
    NotAtom {
        /// The atom being negated.
        inner: Box<SyntaxNode>,
        /// Span covering `not` and the inner atom.
        span: NodeSpan,
    },

    // ---- Lambda -----------------------------------------------
    /// A lambda expression `{ |params| body }` or `\params -> body`.
    /// Both surface forms produce the same AST shape — the pretty-
    /// printer picks its output form by whether the enclosing context
    /// is `Pipeline` (uses the `{|…|…}` form so it embeds cleanly in a
    /// pipeline stage) or `Lambda` (uses the more terse `\… -> …`).
    Lambda {
        /// Parameter names in declaration order.
        params: Vec<String>,
        /// Lambda body expression.
        body: Box<SyntaxNode>,
        /// Span from the opening `{` or `\` through the closing `}`
        /// or expression end.
        span: NodeSpan,
    },
    /// Function application `func arg1 arg2 …` in a lambda context.
    /// Kept separate from `Cmd` because R225's HM checker generates
    /// different constraints for currying (App) vs. process-invocation
    /// (Cmd).
    App {
        /// Function expression.
        func: Box<SyntaxNode>,
        /// Argument expressions.
        args: Vec<SyntaxNode>,
        /// Span from `func` through the last `arg`.
        span: NodeSpan,
    },
    /// A variable reference in a lambda context. In Pipeline context
    /// this would appear as an `Ident` inside `Cmd::name`; the
    /// distinction is context-driven.
    Var {
        /// Variable name (post-NFC).
        name: String,
        /// Span covering the name.
        span: NodeSpan,
    },
    /// A `let name = value in body` binding.
    Let {
        /// Bound name (post-NFC).
        name: String,
        /// Bound value expression.
        value: Box<SyntaxNode>,
        /// Body of the binding.
        body: Box<SyntaxNode>,
        /// Span from `let` through the body's last byte.
        span: NodeSpan,
    },
    /// A `match scrutinee { arm1, arm2, … }` expression. Arms are
    /// [`MatchArm`] values (see that type's doc).
    Match {
        /// The scrutinee expression.
        scrutinee: Box<SyntaxNode>,
        /// Ordered match arms.
        arms: Vec<MatchArm>,
        /// Span from `match` through the closing `}`.
        span: NodeSpan,
    },
    /// An infix binary operator: arithmetic (`+`, `-`, `*`, `/`),
    /// comparison (`<`, `<=`, `>`, `>=`, `==`, `!=`), or logical
    /// (`and`, `or`).
    BinOp {
        /// Operator glyph or keyword.
        op: String,
        /// Left operand.
        lhs: Box<SyntaxNode>,
        /// Right operand.
        rhs: Box<SyntaxNode>,
        /// Span covering both operands and the operator.
        span: NodeSpan,
    },
    /// A prefix unary operator: `not`, `-`. The `-` prefix is
    /// distinguished from binary `-` by parse position (start of an
    /// expression vs. between operands).
    UnaryOp {
        /// Operator glyph or keyword.
        op: String,
        /// Operand.
        inner: Box<SyntaxNode>,
        /// Span covering the operator and operand.
        span: NodeSpan,
    },
    /// Field access `base.field`. In Pipeline context this parses as
    /// `Ident . Ident` at the lexer level; the parser folds adjacent
    /// `Ident Dot Ident` runs into a `FieldAccess` chain.
    FieldAccess {
        /// The base expression whose field is being accessed.
        base: Box<SyntaxNode>,
        /// Field name (post-NFC).
        field: String,
        /// Span from `base`'s first byte through `field`'s last.
        span: NodeSpan,
    },

    // ---- Literals & shared -----------------------------------------
    /// A record literal `{ a: 1, b: "s" }`. Distinct from `Lambda`
    /// (also `{`-opened) by field-name-followed-by-colon lookahead —
    /// the parser peeks 2 tokens after `{` to disambiguate.
    RecordExpr {
        /// Field name/value pairs in source order.
        fields: Vec<RecordField>,
        /// Span from `{` through `}`.
        span: NodeSpan,
    },
    /// A string literal (interpolation-free). Escape sequences are
    /// preserved unresolved — R225's elaborator resolves them.
    LitStr {
        /// Raw string content (post-NFC, escapes unresolved).
        value: String,
        /// Span covering the quotes and content.
        span: NodeSpan,
    },
    /// A signed integer literal. Overflow (`> i64::MAX`) is a parse
    /// error; R221.M5 does not model bignums.
    LitInt {
        /// The parsed integer value.
        value: i64,
        /// Span covering the digits (and optional leading `-`).
        span: NodeSpan,
    },
    /// A boolean literal `true` / `false`. Parsed from bare
    /// identifiers at parser-boundary (the lexer emits them as
    /// `Ident`).
    LitBool {
        /// The parsed boolean.
        value: bool,
        /// Span covering the identifier.
        span: NodeSpan,
    },
    /// A bare identifier where the parser could not determine (or does
    /// not need to determine) a more specific role. Used for command
    /// arguments in Pipeline context (`ls foo` → args: [Ident("foo")])
    /// and for Datalog term names (constants like `alice` in
    /// `parent(alice, bob)`).
    Ident {
        /// Identifier text (post-NFC).
        name: String,
        /// Span covering the identifier.
        span: NodeSpan,
    },
}

impl SyntaxNode {
    /// The node's own span. Delegated to the variant's `span` field.
    /// A macro would compress this but the exhaustive match keeps
    /// `#[deny(unreachable_patterns)]` honest as the enum grows.
    pub fn span(&self) -> NodeSpan {
        match self {
            SyntaxNode::Cmd { span, .. }
            | SyntaxNode::Pipe { span, .. }
            | SyntaxNode::Seq { span, .. }
            | SyntaxNode::Redirect { span, .. }
            | SyntaxNode::Background { span, .. }
            | SyntaxNode::Group { span, .. }
            | SyntaxNode::DatalogBlock { span, .. }
            | SyntaxNode::Atom { span, .. }
            | SyntaxNode::Rule { span, .. }
            | SyntaxNode::QVar { span, .. }
            | SyntaxNode::InterpVar { span, .. }
            | SyntaxNode::NotAtom { span, .. }
            | SyntaxNode::Lambda { span, .. }
            | SyntaxNode::App { span, .. }
            | SyntaxNode::Var { span, .. }
            | SyntaxNode::Let { span, .. }
            | SyntaxNode::Match { span, .. }
            | SyntaxNode::BinOp { span, .. }
            | SyntaxNode::UnaryOp { span, .. }
            | SyntaxNode::FieldAccess { span, .. }
            | SyntaxNode::RecordExpr { span, .. }
            | SyntaxNode::LitStr { span, .. }
            | SyntaxNode::LitInt { span, .. }
            | SyntaxNode::LitBool { span, .. }
            | SyntaxNode::Ident { span, .. } => *span,
        }
    }
}
