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
        let ir_kind = map_node_kind(node.kind);
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
                    // Infix: op (usually a Var), lhs, rhs  (becomes App)
                    // App structure: [callee (the +), arg0 (lhs), arg1 (rhs)]
                    vec![*op, *lhs, *rhs]
                }
                ExprData::Prefix { op, expr } => {
                    // Prefix: op (unary operator), expr (argument)
                    // App structure: [callee (the op), arg0 (expr)]
                    vec![*op, *expr]
                }
                ExprData::Postfix { expr, op } => {
                    // Postfix: expr (argument), op (postfix operator)
                    // App structure: [callee (the op), arg0 (expr)]
                    vec![*op, *expr]
                }
                ExprData::Literal { .. } => {
                    // Literal: no children
                    Vec::new()
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
}
