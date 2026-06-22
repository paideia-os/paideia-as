//! AST → IR lowering scaffold: structural-only phase-1 lowering.
//!
//! This module implements the first-pass lowering from an AST arena to an IR
//! arena. Phase-1 is **intentionally structural only**: every AST node maps to
//! a corresponding IR node with default `LinClass::Unrestricted` and empty
//! `EffectRowId::EMPTY`. No type checking, no resolution, no transformation.
//!
//! The lowering preserves:
//! - **1-to-1 correspondence**: every AST node becomes exactly one IR node.
//! - **Stable indexing**: AST NodeId N ↔ IR IrNodeId N (both arenas index from 1).
//! - **Span propagation**: the IR node inherits the AST node's source span.
//! - **Node mapping**: `ast_to_ir` HashMap enables downstream passes to surface
//!   AST-level diagnostics from IR positions.
//!
//! # Lowering Table
//!
//! The following table shows how each AST `NodeKind` is mapped to an IR `IrKind`:
//!
//! | AST `NodeKind` | IR `IrKind` | Rationale |
//! |---|---|---|
//! | Ident | Var | Variable reference |
//! | ExprLiteral | Literal | Literal value |
//! | ExprPath | Var | Path resolves to variable or reference |
//! | ExprCall | App | Function application |
//! | ExprInfix | App | Infix operators are desugared to applications |
//! | ExprPrefix | App | Prefix operators are desugared to applications |
//! | ExprPostfix | App | Postfix operators are desugared to applications |
//! | ExprBlock | Action | Block is a sequence of statements (action) |
//! | ExprLambda | Lambda | Lambda abstraction |
//! | ExprMatch | Match | Match expression with pattern arms (phase-4-m1-002) |
//! | ExprIf | Branch | If-then-else conditional (phase-4-m1-004) |
//! | ExprLoop | Action | Loop placeholder; phase-1 does not model loop in IR |
//! | ExprActionBlock | Action | Action-marked block |
//! | ExprPerform | Perform | Effect operation invocation |
//! | ExprResume | App | Resume continuation (desugared to app; phase-1 placeholder) |
//! | ExprWithHandler | Handle | Handler installation |
//! | ExprHandlerValue | Action | Handler-value construction (phase-1 placeholder) |
//! | ExprUnsafe | Unsafe | Unsafe block escape hatch |
//! | StmtLet | Let | Let binding |
//! | StmtExpr | Action | Statement-as-action |
//! | StmtReturn | Action | Return placeholder; phase-1 does not model return in IR |
//! | StmtInstruction | RawInstruction | Assembly instruction with persisted mnemonic + operand shape |
//! | ExprArrayLit | ArrayLit | Array literal with element children |
//! | Module | Module | Module declaration |
//! | Signature | Module | Signature (module-like construct) |
//! | Structure | Module | Module body |
//! | Functor | Functor | Parameterized module |
//! | FunctorParam | Var | Functor parameter |
//! | Effect | Module | Effect declaration (module-like placeholder) |
//! | OpSig | Var | Operation signature within effect |
//! | Capability | Module | Capability declaration (module-like placeholder) |
//! | Let (item) | Let | Item-level let binding |
//! | Struct | Module | Struct type declaration (module-like placeholder) |
//! | Enum | Module | Enum type declaration (module-like placeholder) |
//! | UnsafeBlock | Unsafe | Unsafe block item |
//! | Placeholder | Placeholder | Unknown or deferred node |
//! | Other / unmatched | Placeholder | Fallback for unknown variants |
//!
//! # Phase-1 Design Rationale
//!
//! The coarse mapping is intentional: phase-1 preserves AST structure without
//! semantic analysis. Future PRs (phase-2+) will refine the IR with proper
//! representation of control flow (match, if, loop), return semantics, etc.
//! For now, placeholder categories like `Action` serve as buckets for
//! statements and complex expressions that will be elaborated later.

use paideia_as_ast::{AstArena, NodeId, NodeKind};
use paideia_as_ir::{IrArena, IrKind, IrNodeId};
use std::collections::HashMap;

/// Result of lowering: the IR arena + mapping table from AST to IR.
///
/// The mapping enables downstream passes to correlate IR nodes with their
/// original AST positions for diagnostic reporting and feedback.
#[derive(Debug)]
pub struct LoweringResult {
    /// The lowered IR arena.
    pub ir: IrArena,
    /// Mapping from AST NodeId to the IR NodeId it was lowered to.
    /// This is always a bijection in phase-1: every AST node maps to
    /// exactly one IR node.
    pub ast_to_ir: HashMap<NodeId, IrNodeId>,
}

/// Lower an entire AST arena to an IR arena.
///
/// This function walks the AST arena in NodeId order (1, 2, 3, …) and
/// allocates one IR node per AST node using the lowering table above.
/// Every IR node starts with:
/// - `lin_class = LinClass::Unrestricted` (default)
/// - `effect_row = EffectRowId::EMPTY` (default)
/// - `span` copied from the AST node
///
/// The 1-to-1 correspondence and stable indexing mean that AST NodeId N
/// always maps to IR IrNodeId N, preserving arena structure.
///
/// # Panics
///
/// This function does not panic on malformed input. Even unknown NodeKind
/// variants are mapped to `IrKind::Placeholder`.
///
/// # Returns
///
/// A `LoweringResult` containing the IR arena and the AST-to-IR mapping.
#[must_use]
pub fn lower_ast_to_ir(ast: &AstArena) -> LoweringResult {
    let mut ir = IrArena::with_capacity(ast.len());
    let mut ast_to_ir = HashMap::with_capacity(ast.len());

    // First pass: allocate all IR nodes (without children).
    for i in 0..ast.len() {
        // NodeId and IrNodeId both index from 1.
        let ast_id = NodeId::new((i + 1) as u32).expect("non-zero node id");
        let node = &ast[ast_id];
        let mut ir_kind = map_node_kind(node.kind);
        // Phase 7 m4-001: prefix `~` lowers to a dedicated `IrKind::BitNot`
        // rather than the generic `App`. `map_node_kind` only sees the AST
        // node kind, so refine the bucket here where the `PrefixOp` payload is
        // available. `!`/`-`/other keep the generic `App` mapping.
        if node.kind == paideia_as_ast::NodeKind::ExprPrefix {
            if let Some(paideia_as_ast::ExprData::Prefix {
                kind: paideia_as_ast::PrefixOp::BitNot,
                ..
            }) = ast.expr_data(ast_id)
            {
                ir_kind = IrKind::BitNot;
            }
        }
        // Phase 7 m5-001 & m5-002: l-value assignment detection.
        // Detect three patterns and lower them to Store instead of App:
        // 1. a[i] = value (m5-001): LHS is ExprCall with 1 argument
        // 2. *p = value (m5-002): LHS is ExprDeref
        // 3. (*p).f = value (m5-002): LHS is ExprPostfix(FieldAccess) where expr is ExprDeref
        if node.kind == paideia_as_ast::NodeKind::ExprInfix {
            if let Some(paideia_as_ast::ExprData::Infix { op, lhs, .. }) = ast.expr_data(ast_id) {
                let op_node = &ast[*op];
                // Operator "=" is always 1 byte
                if op_node.span.byte_len() == 1 {
                    // Check if LHS is an l-value expression
                    let is_lvalue = if let Some(paideia_as_ast::ExprData::Call { args, .. }) =
                        ast.expr_data(*lhs)
                    {
                        // Pattern 1: a[i] = value
                        args.len() == 1
                    } else if let Some(paideia_as_ast::ExprData::Deref { .. }) = ast.expr_data(*lhs)
                    {
                        // Pattern 2: *p = value
                        true
                    } else if let Some(paideia_as_ast::ExprData::Postfix { expr, .. }) =
                        ast.expr_data(*lhs)
                    {
                        // Pattern 3: (*p).f = value (field access on a deref)
                        if let Some(paideia_as_ast::ExprData::Deref { .. }) = ast.expr_data(*expr) {
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if is_lvalue {
                        ir_kind = IrKind::Store;
                    }
                }
            }
        }
        let ir_id = ir.alloc(ir_kind, node.span);
        ast_to_ir.insert(ast_id, ir_id);
    }

    // Second pass: transfer structure (children) from AST to IR.
    // This ensures that IR nodes have the same parent-child relationships as the AST.
    for i in 0..ast.len() {
        let ast_id = NodeId::new((i + 1) as u32).expect("non-zero node id");
        let ir_id = ast_to_ir[&ast_id];
        let ast_node = &ast[ast_id];

        // Debug: print the structure for Lambda nodes
        if ast_node.kind == paideia_as_ast::NodeKind::ExprLambda {
            eprintln!(
                "[lower_ast_to_ir] Lambda AST node {} -> IR {}",
                ast_id.get(),
                ir_id.get()
            );
        }

        // Extract children from AST based on node kind/data.
        let ast_children: Vec<NodeId> = if let Some(expr_data) = ast.expr_data(ast_id) {
            use paideia_as_ast::ExprData;
            match expr_data {
                ExprData::Lambda { body, .. } => {
                    // Lambda: body is a single child node
                    vec![*body]
                }
                ExprData::Call { callee, args } => {
                    // Call (App): callee + all arguments
                    let mut children = vec![*callee];
                    children.extend(args.iter().copied());
                    children
                }
                ExprData::Infix { op, lhs, rhs } => {
                    // Check if this is an l-value assignment.
                    // Phase 7 m5-001 & m5-002: if this lowered to Store, rearrange children.
                    // Store expects [addr, index_or_unused, value].
                    let store_children = if let Some(ir_node) = ir.get(ir_id) {
                        if ir_node.kind == IrKind::Store {
                            // Try Pattern 1: a[i] = value (ExprCall on LHS)
                            if let Some(paideia_as_ast::ExprData::Call { callee, args }) =
                                ast.expr_data(*lhs)
                            {
                                if args.len() == 1 {
                                    // callee is the base, args[0] is the index, rhs is the value
                                    Some(vec![*callee, args[0], *rhs])
                                } else {
                                    None
                                }
                            }
                            // Try Pattern 2: *p = value (ExprDeref on LHS)
                            else if let Some(paideia_as_ast::ExprData::Deref { expr }) =
                                ast.expr_data(*lhs)
                            {
                                // For deref store, children are [pointer, unused, value]
                                Some(vec![*expr, *op, *rhs])
                            }
                            // Try Pattern 3: (*p).f = value (ExprPostfix(FieldAccess) on LHS)
                            else if let Some(paideia_as_ast::ExprData::Postfix {
                                expr: field_expr,
                                ..
                            }) = ast.expr_data(*lhs)
                            {
                                if let Some(paideia_as_ast::ExprData::Deref { expr: ptr }) =
                                    ast.expr_data(*field_expr)
                                {
                                    // For field-access-of-deref store, children are [pointer, unused, value]
                                    Some(vec![*ptr, *op, *rhs])
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    // Use the special Store children if found, otherwise fall back to regular infix
                    match store_children {
                        Some(children) => children,
                        None => vec![*op, *lhs, *rhs],
                    }
                }
                ExprData::Prefix { op, expr, kind } => {
                    // Prefix `~` (BitNot) lowers to `IrKind::BitNot`, whose
                    // sole child is the operand (no operator node). All other
                    // prefix operators desugar to `App` with structure
                    // [callee (the op), arg0 (expr)].
                    match kind {
                        paideia_as_ast::PrefixOp::BitNot => vec![*expr],
                        _ => vec![*op, *expr],
                    }
                }
                ExprData::Postfix { expr, op } => {
                    // Postfix: expr (argument), op (postfix operator)
                    // App structure: [callee (the op), arg0 (expr)]
                    vec![*op, *expr]
                }
                ExprData::Cast { expr, .. } => {
                    // Cast lowers to IrKind::Cast with a single child (the
                    // operand). The target type is metadata recorded out-of-band
                    // in the CastSideTable, not a structural child.
                    vec![*expr]
                }
                ExprData::Literal { .. } => {
                    // Literal: no children
                    Vec::new()
                }
                ExprData::Block { stmts, tail } => {
                    // Block: all statements + optional tail expression
                    let mut children = stmts.iter().copied().collect::<Vec<_>>();
                    if let Some(tail_expr) = tail {
                        children.push(*tail_expr);
                    }
                    children
                }
                ExprData::ActionBlock {
                    effects,
                    capabilities,
                    body,
                } => {
                    // ActionBlock: optional effects/capabilities + all statements
                    let mut children = Vec::new();
                    if let Some(eff) = effects {
                        children.push(*eff);
                    }
                    if let Some(cap) = capabilities {
                        children.push(*cap);
                    }
                    children.extend(body.iter().copied());
                    children
                }
                ExprData::ArrayLit(elements) => {
                    // ArrayLit: all element expressions as children
                    elements.clone()
                }
                // TODO: Add Path, Ident, and other expression types as needed
                // _ => Vec::new(),
                _ => Vec::new(),
            }
        } else if let Some(item_data) = ast.item_data(ast_id) {
            use paideia_as_ast::ItemData;
            match item_data {
                ItemData::Let { value, .. } => {
                    // Let binding: value is the single child (the RHS)
                    vec![*value]
                }
                ItemData::Structure { items, .. } => {
                    // Structure: all items are children
                    items.clone()
                }
                ItemData::Module { body, .. } => {
                    // Module: body is the single child
                    vec![*body]
                }
                _ => Vec::new(),
            }
        } else if let Some(stmt_data) = ast.stmt_data(ast_id) {
            use paideia_as_ast::StmtData;
            match stmt_data {
                StmtData::Let {
                    name, ty, value, ..
                } => {
                    // Statement Let: name + type (opt) + value
                    let mut children = vec![*name, *value];
                    if let Some(t) = ty {
                        children.push(*t);
                    }
                    children
                }
                StmtData::Expr { expr } => {
                    // Statement Expr: the expression
                    vec![*expr]
                }
                StmtData::Return { value } => {
                    // Statement Return: optional return value
                    value.iter().copied().collect()
                }
                StmtData::Instruction { operands, .. } => {
                    // Assembly instruction: operands are children
                    operands.clone()
                }
                StmtData::Label { name } => {
                    // Label: name is a child
                    vec![*name]
                }
            }
        } else {
            Vec::new()
        };

        // Transfer children to IR using children_mut.
        if !ast_children.is_empty() {
            if let Some(ir_children) = ir.children_mut(ir_id) {
                for ast_child_id in ast_children {
                    if let Some(ir_child_id) = ast_to_ir.get(&ast_child_id) {
                        ir_children.push(*ir_child_id);
                    }
                }
            }
        }
    }

    LoweringResult { ir, ast_to_ir }
}

/// Map an AST NodeKind to an IR IrKind per the lowering table.
///
/// This function is the heart of phase-1 structural lowering. The mapping
/// is deliberately coarse: every Placeholder, Module, Action, Var category
/// is a bucket for nodes that will be refined with proper IR variants in
/// later PRs.
fn map_node_kind(kind: NodeKind) -> IrKind {
    match kind {
        // Identifiers and references
        NodeKind::Ident | NodeKind::ExprPath => IrKind::Var,

        // Literals
        NodeKind::ExprLiteral => IrKind::Literal,

        // Operators (all desugared to applications)
        NodeKind::ExprInfix | NodeKind::ExprPrefix | NodeKind::ExprPostfix => IrKind::App,

        // Cast `expr as type` lowers to a dedicated IrKind::Cast (Phase 7 m4-002).
        // The target type is recorded separately in the CastSideTable; the emit
        // pass chooses movsx/movzx/mov per the source and destination widths.
        NodeKind::ExprCast => IrKind::Cast,

        // Function application
        NodeKind::ExprCall => IrKind::App,

        // Lambda abstraction
        NodeKind::ExprLambda => IrKind::Lambda,

        // Handler installation
        NodeKind::ExprWithHandler => IrKind::Handle,

        // Effect operations
        NodeKind::ExprPerform => IrKind::Perform,

        // Resume expressions (phase-1 placeholder)
        NodeKind::ExprResume => IrKind::App,

        // Handler-value construction (phase-1: placeholder mapped to Action)
        // TODO: phase-2 will introduce a dedicated IrKind::HandlerValue when elaborator
        // validates handler arm coverage and parameter binding.
        NodeKind::ExprHandlerValue => IrKind::Action,

        // Unsafe block escape hatch
        NodeKind::ExprUnsafe => IrKind::Unsafe,

        // Blocks and sequences (all mapped to Action placeholders for phase-1)
        NodeKind::ExprBlock | NodeKind::ExprActionBlock | NodeKind::StmtExpr => IrKind::Action,

        // Assembly instruction: mnemonic + operands persisted through lowering
        NodeKind::StmtInstruction => IrKind::RawInstruction,

        // Control flow: phase-4-m1-002 adds Match to IR; phase-4-m1-004 adds Branch
        NodeKind::ExprMatch => IrKind::Match,
        NodeKind::ExprIf => IrKind::Branch,

        // Control flow placeholders (phase-1 does not model these in IR yet)
        NodeKind::ExprLoop | NodeKind::StmtReturn => IrKind::Action,

        // Array literal (Phase 8 m2-002): sequence of element expressions.
        // cmd_build walks children, packs to bytes per element width.
        NodeKind::ExprArrayLit => IrKind::ArrayLit,

        // Let bindings
        NodeKind::StmtLet | NodeKind::Let => IrKind::Let,

        // Module-like constructs (items and declarations)
        NodeKind::Module
        | NodeKind::Signature
        | NodeKind::Structure
        | NodeKind::Effect
        | NodeKind::Capability
        | NodeKind::Struct
        | NodeKind::Enum => IrKind::Module,

        // Functor (parameterized module)
        NodeKind::Functor => IrKind::Functor,

        // Functor parameters and operation signatures (mapped to Var)
        NodeKind::FunctorParam | NodeKind::OpSig => IrKind::Var,

        // Unsafe block item
        NodeKind::UnsafeBlock => IrKind::Unsafe,

        // Placeholders and unknown nodes
        NodeKind::Placeholder => IrKind::Placeholder,

        // TODO: phase-4-m4-005 — Borrow / BorrowMut / Deref:
        // When AST types ExprBorrow and ExprBorrowMut are added (phase-4-m5 or later),
        // lower ExprBorrow → IrKind::Borrow with BorrowSideTable.insert(id, BorrowMeta { ... }).
        // Lower ExprBorrowMut → IrKind::BorrowMut with BorrowSideTable.insert(id, BorrowMeta { ... }).
        // Lower ExprDeref → IrKind::Deref.
        // Real wiring with borrow checker activates in phase-4-m6.

        // Operands (OperandRegister, OperandImmediate, OperandMemoryRef)
        // These do not appear as top-level nodes in phase-1, but map to Var
        // as a conservative default.
        NodeKind::OperandRegister | NodeKind::OperandImmediate | NodeKind::OperandMemoryRef => {
            IrKind::Var
        }

        // Types (TypeName, TypeArrow, TypeTuple, TypeLinearClass, TypeEffectRow)
        // These are not lowered to IR in phase-1 (they stay in the type table).
        // If they appear as top-level nodes, map to Placeholder.
        NodeKind::TypeName
        | NodeKind::TypeArrow
        | NodeKind::TypeTuple
        | NodeKind::TypeLinearClass
        | NodeKind::TypeEffectRow => IrKind::Placeholder,

        // Patterns (PatWildcard, PatIdent, PatLiteral, etc.)
        // These are not lowered to IR in phase-1 (they stay in the pattern table).
        // If they appear as top-level nodes, map to Placeholder.
        NodeKind::PatWildcard
        | NodeKind::PatIdent
        | NodeKind::PatLiteral
        | NodeKind::PatTuple
        | NodeKind::PatStruct
        | NodeKind::PatEnumVariant
        | NodeKind::PatOr
        | NodeKind::PatBinding => IrKind::Placeholder,

        // Wildcard for future variants added to NodeKind after phase-1.
        _ => IrKind::Placeholder,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::{ExprData, StmtData};
    use paideia_as_diagnostics::FileId;

    fn span() -> paideia_as_diagnostics::Span {
        paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn lower_empty_arena() {
        let ast = AstArena::new();
        let result = lower_ast_to_ir(&ast);
        assert_eq!(result.ir.len(), 0);
        assert!(result.ast_to_ir.is_empty());
    }

    #[test]
    fn lower_single_placeholder() {
        let mut ast = AstArena::new();
        let _id = ast.alloc(NodeKind::Placeholder, span());
        let result = lower_ast_to_ir(&ast);
        assert_eq!(result.ir.len(), 1);
        assert_eq!(result.ast_to_ir.len(), 1);
    }

    #[test]
    fn lower_let_plus() {
        // Build: let x = 1 + 2
        // This tests AC bullet 1: lowering a small AST manually.
        let mut ast = AstArena::new();

        // Allocate IntLit nodes for "1" and "2".
        let lit_one_id = ast.alloc(NodeKind::ExprLiteral, span());
        let lit_two_id = ast.alloc(NodeKind::ExprLiteral, span());

        // Allocate Ident node for "+" (the operator).
        let op_plus_id = ast.alloc(NodeKind::Ident, span());

        // Allocate ExprInfix: 1 + 2
        let infix_id = ast.alloc_expr(
            NodeKind::ExprInfix,
            span(),
            ExprData::Infix {
                lhs: lit_one_id,
                op: op_plus_id,
                rhs: lit_two_id,
            },
        );

        // Allocate Ident node for "x".
        let name_x_id = ast.alloc(NodeKind::Ident, span());

        // Allocate StmtLet: let x = 1 + 2
        let let_stmt_id = ast.alloc_stmt(
            NodeKind::StmtLet,
            span(),
            StmtData::Let {
                mutable: false,
                name: name_x_id,
                ty: None,
                value: infix_id,
            },
        );

        // Lower the AST.
        let result = lower_ast_to_ir(&ast);

        // Verify the IR contains a Let, Literal nodes, and App.
        assert_eq!(result.ir.len(), 6);
        assert_eq!(result.ast_to_ir.len(), 6);

        // Check mappings are present.
        assert!(result.ast_to_ir.contains_key(&lit_one_id));
        assert!(result.ast_to_ir.contains_key(&lit_two_id));
        assert!(result.ast_to_ir.contains_key(&op_plus_id));
        assert!(result.ast_to_ir.contains_key(&infix_id));
        assert!(result.ast_to_ir.contains_key(&name_x_id));
        assert!(result.ast_to_ir.contains_key(&let_stmt_id));

        // Verify the IR node kinds match the lowering table.
        let lit_one_ir = result.ast_to_ir[&lit_one_id];
        let lit_two_ir = result.ast_to_ir[&lit_two_id];
        let op_plus_ir = result.ast_to_ir[&op_plus_id];
        let infix_ir = result.ast_to_ir[&infix_id];
        let name_x_ir = result.ast_to_ir[&name_x_id];
        let let_stmt_ir = result.ast_to_ir[&let_stmt_id];

        assert_eq!(result.ir[lit_one_ir].kind, IrKind::Literal);
        assert_eq!(result.ir[lit_two_ir].kind, IrKind::Literal);
        assert_eq!(result.ir[op_plus_ir].kind, IrKind::Var);
        assert_eq!(result.ir[infix_ir].kind, IrKind::App);
        assert_eq!(result.ir[name_x_ir].kind, IrKind::Var);
        assert_eq!(result.ir[let_stmt_ir].kind, IrKind::Let);
    }

    #[test]
    fn lower_span_preservation() {
        // AC bullet 2: every AST node's span is preserved in its IR counterpart.
        let mut ast = AstArena::new();

        // Allocate a few nodes with the test span.
        let id1 = ast.alloc(NodeKind::Ident, span());
        let id2 = ast.alloc(NodeKind::ExprLiteral, span());
        let id3 = ast.alloc(NodeKind::StmtLet, span());

        let result = lower_ast_to_ir(&ast);

        // Verify spans match.
        let ir1 = result.ast_to_ir[&id1];
        let ir2 = result.ast_to_ir[&id2];
        let ir3 = result.ast_to_ir[&id3];

        assert_eq!(result.ir[ir1].span, span());
        assert_eq!(result.ir[ir2].span, span());
        assert_eq!(result.ir[ir3].span, span());
    }

    #[test]
    fn lower_does_not_panic_on_arena() {
        // AC bullet 4: lowering should not panic on a variety of node kinds.
        let mut ast = AstArena::new();

        // Allocate a mix of different node kinds.
        ast.alloc(NodeKind::Placeholder, span());
        ast.alloc(NodeKind::Ident, span());
        ast.alloc(NodeKind::Module, span());
        ast.alloc(NodeKind::Functor, span());
        ast.alloc(NodeKind::ExprLambda, span());
        ast.alloc(NodeKind::ExprCall, span());
        ast.alloc(NodeKind::ExprBlock, span());
        ast.alloc(NodeKind::ExprMatch, span());
        ast.alloc(NodeKind::StmtLet, span());
        ast.alloc(NodeKind::ExprUnsafe, span());

        // This should not panic.
        let result = lower_ast_to_ir(&ast);
        assert_eq!(result.ir.len(), 10);
    }

    #[test]
    fn lower_preserves_kind_count() {
        // Assert that the number of IR nodes equals the number of AST nodes.
        let mut ast = AstArena::new();

        for _ in 0..50 {
            ast.alloc(NodeKind::Placeholder, span());
        }

        let result = lower_ast_to_ir(&ast);
        assert_eq!(result.ir.len(), ast.len());
    }

    #[test]
    fn ast_to_ir_mapping_is_complete() {
        // Assert that every NodeId in the AST has an entry in the mapping.
        let mut ast = AstArena::new();

        for _ in 0..20 {
            ast.alloc(NodeKind::Ident, span());
        }

        let result = lower_ast_to_ir(&ast);

        for i in 0..ast.len() {
            let id = NodeId::new((i + 1) as u32).unwrap();
            assert!(result.ast_to_ir.contains_key(&id));
        }
    }

    #[test]
    fn lower_preserves_default_lin_class_and_effect_row() {
        // Verify that all IR nodes start with Unrestricted and empty effect row.
        let mut ast = AstArena::new();
        ast.alloc(NodeKind::Placeholder, span());
        ast.alloc(NodeKind::ExprLambda, span());
        ast.alloc(NodeKind::StmtLet, span());

        let result = lower_ast_to_ir(&ast);

        for i in 0..result.ir.len() {
            let ir_id = IrNodeId::new((i + 1) as u32).unwrap();
            assert_eq!(
                result.ir[ir_id].lin_class,
                paideia_as_ir::LinClass::Unrestricted
            );
            assert_eq!(
                result.ir[ir_id].effect_row,
                paideia_as_ir::EffectRowId::EMPTY
            );
        }
    }

    #[test]
    fn lower_stable_indexing() {
        // Verify that AST NodeId N ↔ IR IrNodeId N (both index from 1).
        let mut ast = AstArena::new();

        let id1 = ast.alloc(NodeKind::Ident, span());
        let id2 = ast.alloc(NodeKind::ExprLambda, span());
        let id3 = ast.alloc(NodeKind::StmtLet, span());

        let result = lower_ast_to_ir(&ast);

        // NodeId 1 should map to IrNodeId 1.
        assert_eq!(result.ast_to_ir[&id1].get(), 1);
        // NodeId 2 should map to IrNodeId 2.
        assert_eq!(result.ast_to_ir[&id2].get(), 2);
        // NodeId 3 should map to IrNodeId 3.
        assert_eq!(result.ast_to_ir[&id3].get(), 3);
    }

    #[test]
    fn lower_mapping_is_correct_bijection() {
        // For each AST node, the mapped IR node should have matching kind.
        let mut ast = AstArena::new();

        // Carefully construct test data with known mappings.
        ast.alloc(NodeKind::Ident, span()); // Should map to Var
        ast.alloc(NodeKind::ExprLiteral, span()); // Should map to Literal
        ast.alloc(NodeKind::ExprCall, span()); // Should map to App
        ast.alloc(NodeKind::Module, span()); // Should map to Module

        let result = lower_ast_to_ir(&ast);

        let id1 = NodeId::new(1).unwrap();
        let id2 = NodeId::new(2).unwrap();
        let id3 = NodeId::new(3).unwrap();
        let id4 = NodeId::new(4).unwrap();

        assert_eq!(result.ir[result.ast_to_ir[&id1]].kind, IrKind::Var);
        assert_eq!(result.ir[result.ast_to_ir[&id2]].kind, IrKind::Literal);
        assert_eq!(result.ir[result.ast_to_ir[&id3]].kind, IrKind::App);
        assert_eq!(result.ir[result.ast_to_ir[&id4]].kind, IrKind::Module);
    }

    #[test]
    fn lower_stmt_instruction_to_raw_instruction() {
        // AC: lower `mov rax, 1` StmtInstruction; assert single IrKind::RawInstruction;
        // assert ast_to_ir[ir_node_id] == original_node_id.
        let mut ast = AstArena::new();

        // Allocate operand nodes: "rax" and "1"
        let rax_id = ast.alloc(NodeKind::OperandRegister, span());
        let one_id = ast.alloc(NodeKind::ExprLiteral, span());

        // Allocate the StmtInstruction: mnemonic_id=0 (stub), operands=[rax, 1]
        let instr_id = ast.alloc_stmt(
            NodeKind::StmtInstruction,
            span(),
            StmtData::Instruction {
                mnemonic: 0, // Stub: real mnemonic interning happens in parser/elaborator
                operands: vec![rax_id, one_id],
            },
        );

        // Lower the AST.
        let result = lower_ast_to_ir(&ast);

        // Verify we have 3 IR nodes: OperandRegister, ExprLiteral, StmtInstruction.
        assert_eq!(result.ir.len(), 3);
        assert_eq!(result.ast_to_ir.len(), 3);

        // Verify the StmtInstruction AST node maps to a RawInstruction IR node.
        let ir_instr_id = result.ast_to_ir[&instr_id];
        assert_eq!(result.ir[ir_instr_id].kind, IrKind::RawInstruction);

        // Verify round-trip: ast_to_ir[ir_instr_id] resolves back to instr_id.
        // This tests the bijection invariant: AST NodeId N ↔ IR IrNodeId N.
        assert_eq!(ir_instr_id.get() as u32, instr_id.get());

        // Verify operand nodes are also lowered correctly.
        let ir_rax_id = result.ast_to_ir[&rax_id];
        let ir_one_id = result.ast_to_ir[&one_id];
        assert_eq!(result.ir[ir_rax_id].kind, IrKind::Var); // OperandRegister -> Var
        assert_eq!(result.ir[ir_one_id].kind, IrKind::Literal); // ExprLiteral -> Literal
    }

    #[test]
    fn lower_array_assign_produces_store() {
        // Phase 7 m5-001: array-index assignment `a[i] = value`.
        // Build: a[i] = x
        // This tests that an assignment to an indexed expression lowers to IrKind::Store
        // instead of IrKind::App.
        let mut ast = AstArena::new();

        // Allocate base variable: a
        let base_var_id = ast.alloc(NodeKind::Ident, span());

        // Allocate index variable: i
        let index_var_id = ast.alloc(NodeKind::Ident, span());

        // Allocate ExprCall: a[i]
        // Indexing is represented as Call with 1 argument
        let index_expr_id = ast.alloc_expr(
            NodeKind::ExprCall,
            span(),
            ExprData::Call {
                callee: base_var_id,
                args: vec![index_var_id],
            },
        );

        // Allocate value variable: x
        let value_var_id = ast.alloc(NodeKind::Ident, span());

        // Allocate the operator node (=) - a Placeholder with 1-byte span
        let assign_op_id = ast.alloc(NodeKind::Placeholder, span());

        // Allocate ExprInfix: a[i] = x
        let assign_expr_id = ast.alloc_expr(
            NodeKind::ExprInfix,
            span(),
            ExprData::Infix {
                lhs: index_expr_id,
                op: assign_op_id,
                rhs: value_var_id,
            },
        );

        // Lower the AST.
        let result = lower_ast_to_ir(&ast);

        // Verify the assignment lowered to Store instead of App.
        let assign_ir_id = result.ast_to_ir[&assign_expr_id];
        assert_eq!(
            result.ir[assign_ir_id].kind,
            IrKind::Store,
            "Array assignment should lower to Store"
        );

        // Verify children are rearranged to [base, index, value]
        let children = result.ir.children(assign_ir_id);
        assert_eq!(children.len(), 3, "Store should have 3 children");

        // Children should map to base_var, index_var, value_var
        let base_child_id = children[0];
        let index_child_id = children[1];
        let value_child_id = children[2];

        let base_ir_id = result.ast_to_ir[&base_var_id];
        let index_ir_id = result.ast_to_ir[&index_var_id];
        let value_ir_id = result.ast_to_ir[&value_var_id];

        assert_eq!(base_child_id, base_ir_id);
        assert_eq!(index_child_id, index_ir_id);
        assert_eq!(value_child_id, value_ir_id);
    }

    #[test]
    fn lower_regular_assign_produces_app() {
        // Verify that regular assignment (not to an index) still lowers to App.
        // Build: x = 5
        let mut ast = AstArena::new();

        // Allocate variable: x
        let var_x_id = ast.alloc(NodeKind::Ident, span());

        // Allocate literal: 5
        let lit_5_id = ast.alloc(NodeKind::ExprLiteral, span());

        // Allocate the operator node (=)
        let assign_op_id = ast.alloc(NodeKind::Placeholder, span());

        // Allocate ExprInfix: x = 5 (not an indexed assignment)
        let assign_expr_id = ast.alloc_expr(
            NodeKind::ExprInfix,
            span(),
            ExprData::Infix {
                lhs: var_x_id,
                op: assign_op_id,
                rhs: lit_5_id,
            },
        );

        // Lower the AST.
        let result = lower_ast_to_ir(&ast);

        // Verify the assignment lowered to App (regular operator desugaring)
        let assign_ir_id = result.ast_to_ir[&assign_expr_id];
        assert_eq!(
            result.ir[assign_ir_id].kind,
            IrKind::App,
            "Regular assignment should lower to App"
        );
    }

    #[test]
    fn lower_deref_assign_produces_store() {
        // Phase 7 m5-002: pointer-deref assignment `*p = value`.
        // Build: *p = x
        let mut ast = AstArena::new();

        // Allocate pointer variable: p
        let ptr_var_id = ast.alloc(NodeKind::Ident, span());

        // Allocate ExprDeref: *p
        let deref_expr_id = ast.alloc_expr(
            NodeKind::ExprDeref,
            span(),
            ExprData::Deref { expr: ptr_var_id },
        );

        // Allocate value variable: x
        let value_var_id = ast.alloc(NodeKind::Ident, span());

        // Allocate the operator node (=)
        let assign_op_id = ast.alloc(NodeKind::Placeholder, span());

        // Allocate ExprInfix: *p = x
        let assign_expr_id = ast.alloc_expr(
            NodeKind::ExprInfix,
            span(),
            ExprData::Infix {
                lhs: deref_expr_id,
                op: assign_op_id,
                rhs: value_var_id,
            },
        );

        // Lower the AST.
        let result = lower_ast_to_ir(&ast);

        // Verify the assignment lowered to Store
        let assign_ir_id = result.ast_to_ir[&assign_expr_id];
        assert_eq!(
            result.ir[assign_ir_id].kind,
            IrKind::Store,
            "Deref assignment should lower to Store"
        );

        // Verify children are [pointer, unused, value]
        let children = result.ir.children(assign_ir_id);
        assert_eq!(children.len(), 3, "Store should have 3 children");

        let ptr_child_id = children[0];
        let value_child_id = children[2];

        let ptr_ir_id = result.ast_to_ir[&ptr_var_id];
        let value_ir_id = result.ast_to_ir[&value_var_id];

        assert_eq!(ptr_child_id, ptr_ir_id);
        assert_eq!(value_child_id, value_ir_id);
    }

    #[test]
    fn lower_field_deref_assign_produces_store() {
        // Phase 7 m5-002: field-access-of-deref assignment `(*p).field = value`.
        // Build: (*p).field = x
        let mut ast = AstArena::new();

        // Allocate pointer variable: p
        let ptr_var_id = ast.alloc(NodeKind::Ident, span());

        // Allocate ExprDeref: *p
        let deref_expr_id = ast.alloc_expr(
            NodeKind::ExprDeref,
            span(),
            ExprData::Deref { expr: ptr_var_id },
        );

        // Allocate field name: "field"
        let field_op_id = ast.alloc(NodeKind::Ident, span());

        // Allocate ExprPostfix: (*p).field
        let field_access_id = ast.alloc_expr(
            NodeKind::ExprPostfix,
            span(),
            ExprData::Postfix {
                expr: deref_expr_id,
                op: field_op_id,
            },
        );

        // Allocate value variable: x
        let value_var_id = ast.alloc(NodeKind::Ident, span());

        // Allocate the operator node (=)
        let assign_op_id = ast.alloc(NodeKind::Placeholder, span());

        // Allocate ExprInfix: (*p).field = x
        let assign_expr_id = ast.alloc_expr(
            NodeKind::ExprInfix,
            span(),
            ExprData::Infix {
                lhs: field_access_id,
                op: assign_op_id,
                rhs: value_var_id,
            },
        );

        // Lower the AST.
        let result = lower_ast_to_ir(&ast);

        // Verify the assignment lowered to Store
        let assign_ir_id = result.ast_to_ir[&assign_expr_id];
        assert_eq!(
            result.ir[assign_ir_id].kind,
            IrKind::Store,
            "Field-deref assignment should lower to Store"
        );

        // Verify children are [pointer, unused, value]
        let children = result.ir.children(assign_ir_id);
        assert_eq!(children.len(), 3, "Store should have 3 children");

        let ptr_child_id = children[0];
        let value_child_id = children[2];

        let ptr_ir_id = result.ast_to_ir[&ptr_var_id];
        let value_ir_id = result.ast_to_ir[&value_var_id];

        assert_eq!(ptr_child_id, ptr_ir_id);
        assert_eq!(value_child_id, value_ir_id);
    }

    #[test]
    fn lower_array_literal_produces_array_lit() {
        // Phase 8 m2-002: array literal `[expr0, expr1, ...]` lowers to IrKind::ArrayLit
        // with element children.
        let mut ast = AstArena::new();

        // Allocate element expressions (3 literals)
        let elem0_id = ast.alloc(NodeKind::ExprLiteral, span());
        let elem1_id = ast.alloc(NodeKind::ExprLiteral, span());
        let elem2_id = ast.alloc(NodeKind::ExprLiteral, span());

        // Allocate ExprArrayLit: [elem0, elem1, elem2]
        let array_lit_id = ast.alloc_expr(
            NodeKind::ExprArrayLit,
            span(),
            ExprData::ArrayLit(vec![elem0_id, elem1_id, elem2_id]),
        );

        // Lower the AST.
        let result = lower_ast_to_ir(&ast);

        // Verify we have 4 IR nodes: 3 Literals + 1 ArrayLit.
        assert_eq!(result.ir.len(), 4);

        // Verify the ArrayLit AST node maps to an ArrayLit IR node.
        let ir_array_id = result.ast_to_ir[&array_lit_id];
        assert_eq!(
            result.ir[ir_array_id].kind,
            IrKind::ArrayLit,
            "ArrayLit should lower to IrKind::ArrayLit"
        );

        // Verify children order is preserved.
        let children = result.ir.children(ir_array_id);
        assert_eq!(children.len(), 3, "ArrayLit should have 3 element children");

        let elem0_ir = result.ast_to_ir[&elem0_id];
        let elem1_ir = result.ast_to_ir[&elem1_id];
        let elem2_ir = result.ast_to_ir[&elem2_id];

        assert_eq!(children[0], elem0_ir);
        assert_eq!(children[1], elem1_ir);
        assert_eq!(children[2], elem2_ir);

        // Verify all element children are Literals.
        assert_eq!(result.ir[elem0_ir].kind, IrKind::Literal);
        assert_eq!(result.ir[elem1_ir].kind, IrKind::Literal);
        assert_eq!(result.ir[elem2_ir].kind, IrKind::Literal);
    }

    #[test]
    fn lower_empty_array_literal() {
        // Phase 8 m2-002: empty array literal `[]` lowers to ArrayLit with no children.
        let mut ast = AstArena::new();

        // Allocate ExprArrayLit: []
        let array_lit_id = ast.alloc_expr(
            NodeKind::ExprArrayLit,
            span(),
            ExprData::ArrayLit(vec![]),
        );

        // Lower the AST.
        let result = lower_ast_to_ir(&ast);

        // Verify we have 1 IR node.
        assert_eq!(result.ir.len(), 1);

        // Verify the ArrayLit IR node exists and has no children.
        let ir_array_id = result.ast_to_ir[&array_lit_id];
        assert_eq!(result.ir[ir_array_id].kind, IrKind::ArrayLit);

        let children = result.ir.children(ir_array_id);
        assert_eq!(
            children.len(),
            0,
            "Empty ArrayLit should have no children"
        );
    }
}
