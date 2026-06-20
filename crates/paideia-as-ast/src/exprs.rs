//! Expression-specific structured data (§8 Expr grammar).
//!
//! [`ExprData`] is an enum carrying the semantic payload for expression nodes.
//! Each variant represents a category of expression as specified in the EBNF:
//! LambdaExpr, ActionBlock, WithHandlerExpr, UnsafeExpr, InfixExpr, PrefixExpr,
//! PostfixExpr, LiteralExpr, IdentifierExpr, CallExpr, BlockExpr, MatchExpr,
//! IfExpr, LoopExpr.

use crate::NodeId;
use paideia_as_diagnostics::Span;

/// Generic parameter declaration.
///
/// Represents a single type parameter in a generic function or type declaration.
/// Example: `T` in `fn foo<T: Trait>(x: T)`, or `U: Clone` in `fn bar<U: Clone>`.
#[derive(Clone, Debug)]
pub struct GenericParam {
    /// Parameter name (Ident node).
    pub name: NodeId,
    /// Trait bounds (type-name/path nodes for trait bounds).
    /// Empty if no bounds specified.
    pub bounds: Vec<NodeId>,
}

/// Structured payload for expression nodes.
///
/// Each variant corresponds to a top-level expression kind as specified in
/// §8 of the syntax reference. Child `NodeId` fields point to other nodes
/// in the arena.
#[derive(Clone, Debug)]
pub enum ExprData {
    /// `fn/λ <T, U> params -> body` or `|x, y| body`.
    ///
    /// A lambda expression. The `pipe_form` flag distinguishes between
    /// `fn` / `λ` style (false) and `|...|` style (true).
    /// Generic parameters are only valid in `fn` style (phase 4).
    Lambda {
        /// Generic parameters (type parameters with optional bounds).
        /// Empty for pipe-form and non-generic functions.
        generic_params: Vec<GenericParam>,
        /// Parameter nodes (Pattern : Type pairs).
        params: Vec<NodeId>,
        /// Body expression.
        body: NodeId,
        /// `true` if pipe form `|x| ...`, `false` if `fn (x: T) ...`.
        pipe_form: bool,
    },

    /// `action !{eff} @{caps} { stmts }`.
    ///
    /// Action block with optional effect and capability annotations.
    ActionBlock {
        /// Optional effect set (TypeEffectRow node).
        effects: Option<NodeId>,
        /// Optional capability set (Vec<Ident> wrapped as a node).
        capabilities: Option<NodeId>,
        /// Statement nodes in the block body.
        body: Vec<NodeId>,
    },

    /// `with handler-expr handle name block`.
    ///
    /// Exception handler expression: `with` introduces a handler that
    /// catches exceptions bound by name in the block.
    WithHandler {
        /// Handler expression.
        handler: NodeId,
        /// Binding name (Ident node).
        bind: NodeId,
        /// Block expression to be protected by the handler.
        block: NodeId,
        /// Optional `finally => body` clause appearing as the final element of
        /// the handled block. `None` for legacy callers; `Some(expr)` when present.
        finally: Option<NodeId>,
    },

    /// `unsafe { effects: …, capabilities: …, justification: …, block: … }`.
    ///
    /// Unsafe escape hatch with mandatory justification and capability/effect
    /// escaping.
    Unsafe {
        /// Effect names being escaped.
        effects: Vec<NodeId>,
        /// Capability names being escaped.
        capabilities: Vec<NodeId>,
        /// Justification string literal.
        justification: NodeId,
        /// Statement nodes in the block.
        block: Vec<NodeId>,
    },

    /// `lhs op rhs`.
    ///
    /// Infix binary operator. The operator precedence and associativity
    /// resolution is the parser's responsibility; this node just records
    /// the raw operands and operator.
    Infix {
        /// Left operand.
        lhs: NodeId,
        /// Operator token (Ident or operator node).
        op: NodeId,
        /// Right operand.
        rhs: NodeId,
    },

    /// `op expr`.
    ///
    /// Prefix unary operator (e.g., `-x`, `!x`, `&x`).
    Prefix {
        /// Operator token.
        op: NodeId,
        /// Operand expression.
        expr: NodeId,
    },

    /// `expr op`.
    ///
    /// Postfix unary operator (e.g., `x.field`, `x[idx]`, `x?`, `x()`).
    Postfix {
        /// Base expression.
        expr: NodeId,
        /// Operator token (Ident for field access, or punctuation for indexing, etc.).
        op: NodeId,
    },

    /// Literal expression.
    ///
    /// Wraps a literal node (Int, Float, Char, String, Byte, ByteString,
    /// Unit, Bool).
    Literal {
        /// Literal node.
        lit: NodeId,
    },

    /// `path::to::name` or simple `name`.
    ///
    /// Path expression (qualified identifier).
    Path {
        /// Path segments (Ident nodes).
        segments: Vec<NodeId>,
    },

    /// `f(args)`.
    ///
    /// Function call expression.
    Call {
        /// Callee expression.
        callee: NodeId,
        /// Argument expressions.
        args: Vec<NodeId>,
    },

    /// `{ stmts; expr? }`.
    ///
    /// Block expression: statements followed by an optional tail expression.
    Block {
        /// Statement nodes.
        stmts: Vec<NodeId>,
        /// Optional final expression (no semicolon).
        tail: Option<NodeId>,
    },

    /// `match scrutinee { arms }`.
    ///
    /// Match expression with pattern arms.
    Match {
        /// Scrutinee expression.
        scrutinee: NodeId,
        /// Match arms (pattern, optional guard, body).
        arms: Vec<MatchArm>,
    },

    /// `if cond then else?`.
    ///
    /// Conditional expression.
    If {
        /// Condition expression.
        cond: NodeId,
        /// Then-branch block.
        then_block: NodeId,
        /// Optional else-branch block.
        else_block: Option<NodeId>,
    },

    /// `loop block` or `while cond block` or `for pat in iter block`.
    ///
    /// Loop expression. The kind disambiguates between infinite, conditional,
    /// and iterative loops.
    Loop {
        /// Loop kind (Loop, While, For).
        kind: LoopKind,
        /// Optional header (condition for While, or pattern+iter for For).
        header: Option<NodeId>,
        /// Loop body.
        body: NodeId,
    },

    /// Register operand (for assembly).
    ///
    /// Represents a register name (e.g., `rax`, `r8`).
    OperandRegister {
        /// Register identifier.
        reg: NodeId,
    },

    /// Immediate operand (for assembly).
    ///
    /// An immediate value or expression as an operand.
    OperandImmediate {
        /// Immediate expression.
        expr: NodeId,
    },

    /// Memory reference operand (for assembly).
    ///
    /// Memory addressing mode: `[addr]` or complex addressing.
    OperandMemoryRef {
        /// Address expression.
        addr: NodeId,
    },

    /// `perform Effect::op(args)`.
    ///
    /// Perform expression: invokes an effect operation with arguments.
    Perform {
        /// Path to the effect operation (an ExprPath node).
        op_path: NodeId,
        /// Argument expressions.
        args: Vec<NodeId>,
    },

    /// `resume value`.
    ///
    /// Resume expression: phase-1 has no context check.
    Resume {
        /// Value expression.
        value: NodeId,
    },

    /// `handle Effect { op ... ; finally => ... }`.
    ///
    /// Handler-value construction: bundles operation handlers and an optional
    /// cleanup handler into a single first-class value. This is distinct from
    /// `with handler-expr handle Effect { ... }` (which installs a handler).
    HandlerValue {
        /// Effect name being handled (Ident or Path node).
        effect: NodeId,
        /// Handler arms (Op | Finally variants).
        arms: Vec<HandlerArm>,
    },

    /// `quote { ... }` (code quotation).
    ///
    /// Captures the abstract syntax tree of the quoted expression as data,
    /// enabling code-as-data and metaprogramming patterns.
    Quote {
        /// The quoted expression body.
        body: NodeId,
    },

    /// `~(...)` (antiquotation).
    ///
    /// Splices a computed value into a quoted expression. Only valid inside
    /// a `quote { ... }` block.
    Antiquote {
        /// The value expression to splice.
        value: NodeId,
    },

    /// `F(M)(N) sharing (M::t = N::t, ...)`.
    ///
    /// Functor application: applies a functor to one or more module arguments
    /// with optional sharing constraints on types.
    FunctorApp {
        /// Functor name (ExprPath node).
        functor: NodeId,
        /// Module arguments (Vec of ExprPath nodes, one per `(M)` group).
        arguments: Vec<NodeId>,
        /// Sharing constraints (type equality specifications).
        sharing: Vec<SharingConstraint>,
    },

    /// `pack M : S`.
    ///
    /// Pack expression: packs a module value `M` according to signature `S`.
    Pack {
        /// Module path being packed (ExprPath node).
        module_path: NodeId,
        /// Signature path constraint (ExprPath node).
        signature_path: NodeId,
    },

    /// `unpack v`.
    ///
    /// Unpack expression: extracts a module value from a packed value `v`.
    Unpack {
        /// Packed value expression.
        value: NodeId,
    },

    /// `let module N = unpack v in <expr>`.
    ///
    /// Let-module binding: binds a module name `N` to an unpacked value,
    /// with a continuation `rest` that uses the bound name.
    LetModule {
        /// Binding name (module identifier string).
        name: String,
        /// RHS of `=` — an unpack expression (ExprUnpack node).
        body: NodeId,
        /// Continuation `in <expr>` — the rest expression.
        rest: NodeId,
    },

    /// `TypeName { field1: expr1, field2: expr2, ... }`.
    ///
    /// Record constructor expression: instantiates a record type with field values.
    RecordCons {
        /// Type name (Ident node).
        type_name: NodeId,
        /// Record fields: each is (field_name_node, field_value_node).
        fields: Vec<(NodeId, NodeId)>,
    },

    /// `receiver.field`.
    ///
    /// Field access expression: accesses a named field of a record or struct.
    FieldAccess {
        /// Receiver expression.
        receiver: NodeId,
        /// Field name (Ident node).
        field: NodeId,
    },
}

/// A single arm in a match expression.
///
/// Consists of a pattern, optional guard, and body expression.
#[derive(Copy, Clone, Debug)]
pub struct MatchArm {
    /// Pattern to match.
    pub pattern: NodeId,
    /// Optional guard condition.
    pub guard: Option<NodeId>,
    /// Arm body expression.
    pub body: NodeId,
}

/// A single arm in a handler-value expression.
///
/// Either an operation handler (`op name => body`) or a finally handler (`finally => body`).
#[derive(Copy, Clone, Debug)]
pub enum HandlerArm {
    /// Operation handler: `op name => expr`.
    Op {
        /// Operation name (Ident node).
        op: NodeId,
        /// Handler expression.
        handler: NodeId,
    },
    /// Finally handler: `finally => expr`.
    Finally {
        /// Cleanup expression.
        cleanup: NodeId,
    },
}

/// A sharing constraint in a functor application.
///
/// Specifies type equality between two module paths in the context of
/// functor application, e.g., `M::t = N::t`.
#[derive(Clone, Debug)]
pub struct SharingConstraint {
    /// Left-hand side path segments (e.g., ["M", "t"]).
    pub left_path: Vec<String>,
    /// Right-hand side path segments (e.g., ["N", "t"]).
    pub right_path: Vec<String>,
    /// Source span of the constraint.
    pub span: Span,
}

/// Kind of loop construct.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum LoopKind {
    /// Infinite loop: `loop block`.
    Loop,
    /// Conditional loop: `while cond block`.
    While,
    /// Iterative loop: `for pat in iter block`.
    For,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expr_lambda_constructs() {
        let param = NodeId::new(1).unwrap();
        let body = NodeId::new(2).unwrap();
        let expr = ExprData::Lambda {
            generic_params: vec![],
            params: vec![param],
            body,
            pipe_form: false,
        };
        match expr {
            ExprData::Lambda {
                generic_params,
                params,
                body: b,
                pipe_form,
            } => {
                assert!(generic_params.is_empty());
                assert_eq!(params.len(), 1);
                assert_eq!(params[0], param);
                assert_eq!(b, body);
                assert!(!pipe_form);
            }
            _ => panic!("expected Lambda variant"),
        }
    }

    #[test]
    fn expr_call_constructs() {
        let callee = NodeId::new(1).unwrap();
        let arg1 = NodeId::new(2).unwrap();
        let arg2 = NodeId::new(3).unwrap();
        let expr = ExprData::Call {
            callee,
            args: vec![arg1, arg2],
        };
        match expr {
            ExprData::Call { callee: c, args } => {
                assert_eq!(c, callee);
                assert_eq!(args.len(), 2);
            }
            _ => panic!("expected Call variant"),
        }
    }

    #[test]
    fn expr_match_constructs() {
        let scrutinee = NodeId::new(1).unwrap();
        let pat = NodeId::new(2).unwrap();
        let body = NodeId::new(3).unwrap();
        let arm = MatchArm {
            pattern: pat,
            guard: None,
            body,
        };
        let expr = ExprData::Match {
            scrutinee,
            arms: vec![arm],
        };
        match expr {
            ExprData::Match { scrutinee: s, arms } => {
                assert_eq!(s, scrutinee);
                assert_eq!(arms.len(), 1);
                assert_eq!(arms[0].pattern, pat);
            }
            _ => panic!("expected Match variant"),
        }
    }

    #[test]
    fn expr_block_constructs() {
        let stmt = NodeId::new(1).unwrap();
        let tail = NodeId::new(2).unwrap();
        let expr = ExprData::Block {
            stmts: vec![stmt],
            tail: Some(tail),
        };
        match expr {
            ExprData::Block { stmts, tail: t } => {
                assert_eq!(stmts.len(), 1);
                assert_eq!(t, Some(tail));
            }
            _ => panic!("expected Block variant"),
        }
    }

    #[test]
    fn expr_if_constructs() {
        let cond = NodeId::new(1).unwrap();
        let then_b = NodeId::new(2).unwrap();
        let else_b = NodeId::new(3).unwrap();
        let expr = ExprData::If {
            cond,
            then_block: then_b,
            else_block: Some(else_b),
        };
        match expr {
            ExprData::If {
                cond: c,
                then_block,
                else_block,
            } => {
                assert_eq!(c, cond);
                assert_eq!(then_block, then_b);
                assert_eq!(else_block, Some(else_b));
            }
            _ => panic!("expected If variant"),
        }
    }

    #[test]
    fn loop_kind_variants_exist() {
        let _loop_kind = LoopKind::Loop;
        let _while_kind = LoopKind::While;
        let _for_kind = LoopKind::For;
    }
}
