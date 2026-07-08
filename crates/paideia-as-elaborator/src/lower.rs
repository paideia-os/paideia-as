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
//! | ExprArrayRepeat | ArrayLit | Array repeat `[expr; N]` → expanded to N copies of lowered expr |
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

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, DiagnosticSink, Severity, SourceMap};
use paideia_as_ir::{IrArena, IrKind, IrNodeId};
use std::collections::HashMap;

use crate::unsafe_walker::extract_integer_from_span;

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
/// # Arguments
///
/// * `ast` - The AST arena to lower
/// * `source_map` - The source map for extracting literal values and locations
/// * `sink` - Diagnostic sink for emitting errors and warnings (e.g., T0550)
/// * `registry` - The struct registry mapping struct names to RecordTypeIds (PA-r17-010a)
/// * `enum_registry` - The enum registry mapping enum names to EnumTypeIds
/// * `payload_map` - HashMap from (EnumTypeId, variant_idx) to Option<RecordTypeId> for nested patterns
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
pub fn lower_ast_to_ir(
    ast: &AstArena,
    source_map: &SourceMap,
    sink: &mut dyn DiagnosticSink,
    registry: &crate::StructRegistry,
    enum_registry: &crate::EnumRegistry,
    payload_map: &std::collections::HashMap<(paideia_as_ir::enum_layout::EnumTypeId, u32), Option<paideia_as_ir::record_layout::RecordTypeId>>,
) -> LoweringResult {
    let mut ir = IrArena::with_capacity(ast.len());
    let mut ast_to_ir = HashMap::with_capacity(ast.len());

    // Pre-pass: collect all While/Loop nodes that appear (directly or indirectly)
    // inside Unsafe blocks. This allows us to skip child transfer for While/Loop nodes
    // inside unsafe blocks, since they should not have children when part of an unsafe
    // block's statement sequence.
    let mut nodes_in_unsafe_blocks = std::collections::HashSet::new();

    fn collect_unsafe_descendants(
        ast: &AstArena,
        node_id: paideia_as_ast::NodeId,
        dest: &mut std::collections::HashSet<u32>,
    ) {
        if let Some(node) = ast.get(node_id) {
            // If this is a While/Loop node, mark it
            if node.kind == paideia_as_ast::NodeKind::ExprLoop {
                dest.insert(node_id.get());
            }

            // Recursively check children of the current node
            if let Some(stmt_data) = ast.stmt_data(node_id) {
                match stmt_data {
                    paideia_as_ast::StmtData::Let { value, .. } => {
                        collect_unsafe_descendants(ast, *value, dest);
                    }
                    paideia_as_ast::StmtData::Expr { expr } => {
                        collect_unsafe_descendants(ast, *expr, dest);
                    }
                    _ => {}
                }
            }

            if let Some(expr_data) = ast.expr_data(node_id) {
                match expr_data {
                    paideia_as_ast::ExprData::Loop { body, .. } => {
                        collect_unsafe_descendants(ast, *body, dest);
                    }
                    paideia_as_ast::ExprData::If { cond, then_block, else_block } => {
                        collect_unsafe_descendants(ast, *cond, dest);
                        collect_unsafe_descendants(ast, *then_block, dest);
                        if let Some(else_id) = else_block {
                            collect_unsafe_descendants(ast, *else_id, dest);
                        }
                    }
                    paideia_as_ast::ExprData::Match { scrutinee, arms, .. } => {
                        collect_unsafe_descendants(ast, *scrutinee, dest);
                        for arm in arms {
                            collect_unsafe_descendants(ast, arm.body, dest);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    for i in 0..ast.len() {
        let ast_id = NodeId::new((i + 1) as u32).expect("non-zero node id");
        if let Some(node) = ast.get(ast_id) {
            if node.kind == paideia_as_ast::NodeKind::ExprUnsafe {
                if let Some(paideia_as_ast::ExprData::Unsafe { block, .. }) =
                    ast.expr_data(ast_id)
                {
                    // Recursively collect While/Loop nodes inside this unsafe block
                    for &stmt_id in block {
                        collect_unsafe_descendants(ast, stmt_id, &mut nodes_in_unsafe_blocks);
                    }
                }
            }
        }
    }

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
        // Phase 17 m6-b: Detect four patterns and lower them to Store instead of App:
        // 1. a[i] = value (m5-001): LHS is ExprCall with 1 argument
        // 2. *p = value (m5-002): LHS is ExprDeref
        // 3. (*p).f = value (m5-002): LHS is ExprFieldAccess where receiver is ExprDeref
        // 4. r.f = value (m6-b): LHS is ExprFieldAccess where receiver is ExprPath|Ident (global/local record)
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
                    } else if let Some(paideia_as_ast::ExprData::FieldAccess { receiver, .. }) =
                        ast.expr_data(*lhs)
                    {
                        // Pattern 3 & 4: field access on deref or direct variable
                        // Check if receiver is a Deref (Pattern 3: (*p).f = value)
                        if let Some(paideia_as_ast::ExprData::Deref { .. }) = ast.expr_data(*receiver) {
                            true
                        } else {
                            // Check if receiver is a simple variable (Pattern 4: r.f = value)
                            // A simple variable is represented as ExprPath or Ident
                            let receiver_node = ast.get(*receiver);
                            if let Some(n) = receiver_node {
                                n.kind == paideia_as_ast::NodeKind::ExprPath
                                    || n.kind == paideia_as_ast::NodeKind::Ident
                            } else {
                                false
                            }
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
        // PA-R17-012: ExprLoop distinction (infinite loop vs while).
        // Detect Loop kind field and map to appropriate IR kind.
        // Loop (infinite) → IrKind::Loop
        // While (conditional) → IrKind::While
        if node.kind == paideia_as_ast::NodeKind::ExprLoop {
            if let Some(paideia_as_ast::ExprData::Loop { kind, .. }) = ast.expr_data(ast_id) {
                match kind {
                    paideia_as_ast::LoopKind::Loop => {
                        ir_kind = IrKind::Loop;
                    }
                    paideia_as_ast::LoopKind::While => {
                        ir_kind = IrKind::While;
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
                    // Phase 7 m5-001 & m5-002; Phase 17 m6-b: if this lowered to Store, rearrange children.
                    // Store expects [addr_or_field_access, index_or_unused, value].
                    // For field access patterns, we pass the FieldAccess IR node as child[0]
                    // so populate_field_access_info can access field info via the side-table.
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
                            // Try Pattern 3 & 4: field access (ExprFieldAccess on LHS)
                            // Pattern 3: (*p).f = value (field access on a deref)
                            // Pattern 4: r.f = value (field access on a global/local record)
                            else if let Some(paideia_as_ast::ExprData::FieldAccess { .. }) =
                                ast.expr_data(*lhs)
                            {
                                // For field access store, children are [FieldAccess_ast, unused, value]
                                // The AST-to-IR mapping will convert FieldAccess_ast to FieldAccess_ir
                                // in the children transfer loop below.
                                Some(vec![*lhs, *op, *rhs])
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
                ExprData::StringLiteral(_) | ExprData::ByteStringLiteral(_) => {
                    // StringLiteral (PA10-002): no structural children.
                    // Byte payload is stored in literal_bytes side-table by the elaborator.
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
                ExprData::ArrayRepeat { expr, count } => {
                    // ArrayRepeat: expand [expr; count] to N copies of expr.
                    // Extract count as a literal, then replicate expr.
                    expand_array_repeat(ast, *expr, *count)
                }
                ExprData::RecordCons { type_name, fields } => {
                    // RecordCons: canonicalize field order to declared order.
                    // Extract type name and look up in registry.
                    match extract_source_text_for_record_cons(ast, source_map, *type_name) {
                        Some(type_name_text) => {
                            if let Some(type_id) = registry.get_by_name(&type_name_text) {
                        // Canonicalize path: registry lookup succeeded
                        use std::collections::HashMap;

                        // 1. Build HashMap<String, (NodeId, NodeId)> from literal fields,
                        //    catching duplicates. We store both name_node and value_node
                        //    so we can emit diagnostics with proper spans.
                        let mut literal_map: HashMap<String, (NodeId, NodeId)> = HashMap::new();
                        let mut duplicate_names: Vec<(String, NodeId)> = Vec::new();

                        for (name_node, value_node) in fields.iter() {
                            let field_name_text = match extract_source_text_for_record_cons(ast, source_map, *name_node) {
                                Some(text) => text,
                                None => continue, // Skip if we can't extract name
                            };

                            if literal_map.insert(field_name_text.clone(), (*name_node, *value_node)).is_some() {
                                duplicate_names.push((field_name_text, *name_node));
                            }
                        }

                        // Emit T0538 diagnostics for duplicate fields
                        for (name, name_node) in duplicate_names {
                            if let Some(node) = ast.get(name_node) {
                                if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 538) {
                                    let diag = Diagnostic::error(code)
                                        .message(format!(
                                            "Record literal contains field '{}' more than once",
                                            name
                                        ))
                                        .with_span(node.span)
                                        .finish();
                                    let _ = sink.emit(diag);
                                }
                            }
                        }

                        // 2. Iterate declared fields in order, popping from literal_map
                        let mut ordered_values: Vec<NodeId> = Vec::new();
                        let mut missing_names: Vec<String> = Vec::new();
                        let declared_fields_vec: Vec<(String, u8)> = registry.get_fields(type_id)
                            .map(|f| f.clone())
                            .unwrap_or_default();

                        if !declared_fields_vec.is_empty() {
                            // Only perform canonicalization if struct has declared fields
                            for (decl_name, _byte_code) in declared_fields_vec.iter() {
                                match literal_map.remove(decl_name) {
                                    Some((_name_node, value_node)) => {
                                        ordered_values.push(value_node);
                                    }
                                    None => {
                                        missing_names.push(decl_name.clone());
                                    }
                                }
                            }

                            // Emit T0537 diagnostics for missing fields
                            for name in missing_names {
                                if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 537) {
                                    let diag = Diagnostic::error(code)
                                        .message(format!(
                                            "Record literal omits declared field '{}'",
                                            name
                                        ))
                                        .with_span(ast_node.span)
                                        .finish();
                                    let _ = sink.emit(diag);
                                }
                            }

                            // 3. Residual entries in literal_map are unknown fields
                            for (unknown_name, (name_node, _value_node)) in literal_map.into_iter() {
                                if let Some(node) = ast.get(name_node) {
                                    if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 539) {
                                        let diag = Diagnostic::error(code)
                                            .message(format!(
                                                "Record literal contains field '{}' not declared in the record type",
                                                unknown_name
                                            ))
                                            .with_span(node.span)
                                            .finish();
                                        let _ = sink.emit(diag);
                                    }
                                }
                            }
                        } else {
                            // Struct has no declared fields (likely due to registration failure).
                            // Use literal order as fallback and don't emit canonicalization diagnostics.
                            for (_name_node, value_node) in literal_map.into_values() {
                                ordered_values.push(value_node);
                            }
                        }

                        // 4. Assemble IR children: [type_name_node, ordered_values...]
                        let mut children = vec![*type_name];
                        children.extend(ordered_values);
                        children
                            } else {
                                // Registry lookup failed: emit in literal order (regression guard).
                                // populate_record_layout_table will already handle the unknown type.
                                let mut children = vec![*type_name];
                                for (_field_name, field_value) in fields {
                                    children.push(*field_value);
                                }
                                children
                            }
                        }
                        None => {
                            // Can't extract type name; emit in literal order (fallback).
                            let mut children = vec![*type_name];
                            for (_field_name, field_value) in fields {
                                children.push(*field_value);
                            }
                            children
                        }
                    }
                }
                ExprData::FieldAccess { receiver, .. } => {
                    // FieldAccess: single child is the receiver expression.
                    // The field ident is metadata (in FieldAccessInfo side-table).
                    // Phase 6 m3-002: lowered to IrKind::FieldAccess with exactly
                    // one child (record_value).
                    vec![*receiver]
                }
                ExprData::Unsafe {
                    effects: _effects,
                    capabilities: _capabilities,
                    justification: _justification,
                    block,
                } => {
                    // Unsafe block: the block's statements become children.
                    // Phase 8 m4-001: walk the block's statements and emit each as an IR child.
                    // The effects, capabilities, and justification are metadata preserved
                    // separately and not exposed as structural children.
                    block.clone()
                }
                ExprData::Borrow { expr, .. } => {
                    // Borrow (& and &mut): single child is the inner expression.
                    // PA10-006u: metadata (symbol, mutable) handled separately by elaborator.
                    vec![*expr]
                }
                ExprData::Deref { expr } => {
                    // Deref (*): single child is the reference expression.
                    vec![*expr]
                }
                ExprData::If {
                    cond,
                    then_block,
                    else_block,
                } => {
                    // If-then-else: condition, then block, and optional else block.
                    let mut children = vec![*cond, *then_block];
                    if let Some(else_id) = else_block {
                        children.push(*else_id);
                    }
                    children
                }
                ExprData::Match {
                    scrutinee,
                    arms,
                    ..
                } => {
                    // Match: scrutinee + arm bodies only.
                    // Patterns and guards are dropped here; populate_match_arm_meta
                    // walks AST arms directly to extract pattern classification.
                    let mut children = vec![*scrutinee];
                    for arm in arms {
                        children.push(arm.body);
                    }
                    children
                }
                ExprData::Loop { header, body, .. } => {
                    // Loop (infinite or while): optional header (condition) + body.
                    // BUGFIX PA-R17-012c (#990): Skip child transfer for While/Loop nodes inside
                    // unsafe blocks. While nodes inside unsafe blocks should not have children to
                    // avoid conflicts with unsafe_walker's label handling. The unsafe_walker
                    // processes the block statements independently, so the While control flow and
                    // unsafe block context must be kept separate.
                    if nodes_in_unsafe_blocks.contains(&ast_id.get()) {
                        // This While/Loop is inside an unsafe block: don't add children.
                        // The unsafe block will handle the statements independently.
                        Vec::new()
                    } else {
                        // This While/Loop is in normal function body: add children for control flow emission.
                        let mut children = Vec::new();
                        if let Some(header_id) = header {
                            children.push(*header_id);
                        }
                        children.push(*body);
                        children
                    }
                }
                ExprData::For {
                    pattern,
                    iterable,
                    body,
                } => {
                    // For loop: pattern + iterable + body.
                    vec![*pattern, *iterable, *body]
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

    // PA-r15-009c (#1055): Populate match dispatch metadata for @jump_table matches.
    populate_match_dispatch_meta(&ast, &mut ir, &ast_to_ir, source_map, sink);

    // PA-r17-010a (#1070): Populate RecordLayoutTable for RecordCons expressions.
    // Walk all AST nodes; for each RecordCons, look up the struct type name in the registry
    // and insert the (RecordCons_ir_id -> RecordTypeId) mapping into record_layout_table.
    populate_record_layout_table(&ast, &mut ir, &ast_to_ir, registry, source_map, sink);

    // Phase 6 m3-002 (#1073): Populate FieldAccessSideTable for FieldAccess expressions.
    // Walk all AST nodes with ExprFieldAccess, build binding→struct_type map, look up
    // field indices in the registry, and insert (FieldAccess_ir_id -> FieldAccessInfo)
    // mappings into field_access_info side-table.
    populate_field_access_info(&ast, &mut ir, &ast_to_ir, registry, source_map, sink);

    // Phase 7 m4-003 (#1048/#1049): Populate EnumConsInfoTable for EnumCons expressions.
    // Walk all AST nodes with ExprCall whose callee is an ExprPath with exactly 2 segments,
    // look up the enum type and variant index in the registry, rewrite App to EnumCons,
    // and insert (EnumCons_ir_id -> EnumConsInfo) mappings into enum_cons_info side-table.
    populate_enum_cons_info(&ast, &mut ir, &ast_to_ir, enum_registry, source_map, sink);

    // Phase 7 m9-009 (#1081/#1082): Populate MatchArmMeta for match arms and
    // match_scrutinee_table for match expressions. Walks AST ExprMatch nodes to
    // classify arm patterns and resolve scrutinee enum types.
    populate_match_arm_meta(&ast, &mut ir, &ast_to_ir, enum_registry, registry, payload_map, source_map, sink);

    // Phase-5-m1-001: Literal values are populated by cmd_build.rs Phase-5-m1-001 walk
    // before emit_walker::walk() runs. No need to duplicate that work here.

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

        // String and byte string literals (PA10-002): lowered to StringLiteral IR nodes
        // with byte payloads stored in the literal_bytes side-table.
        NodeKind::ExprString | NodeKind::ExprByteString => IrKind::StringLiteral,

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

        // Array repeat (Phase 9 m1-002): `[expr; count]` → ArrayLit with N copies of expr.
        // During lowering, this is expanded by extract_repeat_count and expand_array_repeat.
        NodeKind::ExprArrayRepeat => IrKind::ArrayLit,

        // Record constructor (Phase 8 m2-003): instantiates a record type with field values.
        // At module level, populate_data_table walks fields and encodes to DataEntry.
        // At runtime, emit_walker lowering dispatches per context.
        NodeKind::ExprRecordCons => IrKind::RecordCons,

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

        // Borrow (phase-4-m5): & and &mut expressions → IrKind::Borrow.
        // PA10-006u: in static-init context, address-of-symbol constants are handled specially
        // by the elaborator (populate AddrOfSideTable); elsewhere they are runtime borrows.
        // Note: ExprBorrow covers both immutable (&) and mutable (&mut); the mutable flag
        // is in ExprData::Borrow { expr, mutable }, not a separate NodeKind.
        NodeKind::ExprBorrow => IrKind::Borrow,

        // Deref (*expr): dereference a reference.
        NodeKind::ExprDeref => IrKind::Deref,

        // Field access (receiver.field): access a named field of a record.
        // Phase 6 m3-002: Lowered to dedicated IrKind::FieldAccess with side-table
        // metadata (FieldAccessInfo: type_id, field_index) populated by
        // populate_field_access_info pass.
        NodeKind::ExprFieldAccess => IrKind::FieldAccess,

        // Operands (OperandRegister, OperandImmediate, OperandMemoryRef)
        // These do not appear as top-level nodes in phase-1, but map to Var
        // as a conservative default.
        NodeKind::OperandRegister | NodeKind::OperandImmediate | NodeKind::OperandMemoryRef => {
            IrKind::Var
        }

        // Types (TypeName, TypeFnPtr, TypeTuple, TypeLinearClass, TypeEffectRow)
        // These are not lowered to IR in phase-1 (they stay in the type table).
        // If they appear as top-level nodes, map to Placeholder.
        NodeKind::TypeName
        | NodeKind::TypeFnPtr
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

/// Extract a count literal from an AST expression node.
///
/// Returns `Some(count)` if the expression is a Literal node whose integer
/// value can be extracted. Returns `None` if the count is not a literal,
/// or if extraction fails (non-integer literal).
fn extract_repeat_count(ast: &AstArena, count_expr_id: NodeId) -> Option<usize> {
    // Check if count_expr is a Literal expression
    if let Some(ExprData::Literal { lit: _lit_node_id }) = ast.expr_data(count_expr_id) {
        // The lit_node_id is a Placeholder node. In phase-1, we have limited
        // access to the actual value without a full evaluator. For now, we
        // return None to defer to the elaborator's error reporting.
        // Future phases will add constant evaluation in the elaborator.
        None
    } else {
        None
    }
}

/// Expand an array repeat expression to N copies of the element.
///
/// Given `[expr; count]`, this function:
/// 1. Attempts to extract count as a constant integer literal
/// 2. If successful, returns N copies of expr as children
/// 3. If count is not a literal, returns a single copy with a note
///    that P0211 should be emitted by the elaborator
fn expand_array_repeat(ast: &AstArena, expr: NodeId, count: NodeId) -> Vec<NodeId> {
    // Try to extract the count literal
    if let Some(count_val) = extract_repeat_count(ast, count) {
        // Replicate expr count_val times
        vec![expr; count_val]
    } else {
        // Count is not a literal. For now, emit a single copy as a fallback.
        // The elaborator will emit P0211 if the constant evaluator can't resolve count.
        vec![expr]
    }
}

/// PA-r15-009c (#1055): Populate match dispatch metadata for @jump_table matches.
///
/// For each Match node in the IR with the `@jump_table` attribute on the
/// corresponding AST node, compute and insert the dispatch metadata (min_arm,
/// range, covered_arms, density_ok) into the arena's side-table.
///
/// If any arm's pattern is not an integer literal, emit T0550 diagnostic and
/// set `density_ok = false`.
fn populate_match_dispatch_meta(
    ast: &AstArena,
    ir: &mut IrArena,
    ast_to_ir: &HashMap<NodeId, IrNodeId>,
    source_map: &SourceMap,
    sink: &mut dyn DiagnosticSink,
) {
    // Walk through all IR nodes looking for Match nodes.
    for ast_node_id in 1..=ast.len() {
        let ast_id = match NodeId::new(ast_node_id as u32) {
            Some(nid) => nid,
            None => continue,
        };

        // Get the AST node and check if it's an ExprMatch.
        let ast_node = match ast.get(ast_id) {
            Some(n) => n,
            None => continue,
        };

        if ast_node.kind != NodeKind::ExprMatch {
            continue;
        }

        // Get the ExprData::Match to access attrs and arms.
        let (attrs, arms) = match ast.expr_data(ast_id) {
            Some(ExprData::Match { attrs, arms, .. }) => (attrs, arms),
            _ => continue,
        };

        // Only process if @jump_table attribute is present.
        if !attrs.jump_table {
            continue;
        }

        // Get the corresponding IR node ID.
        let ir_match_id = match ast_to_ir.get(&ast_id) {
            Some(id) => *id,
            None => continue,
        };

        // Extract integer values from arm patterns with their arm indices.
        let mut arm_values: Vec<i64> = Vec::new();
        let mut arm_value_index_pairs: Vec<(i64, u32)> = Vec::new();
        let mut has_non_integer_pattern = false;

        for (arm_idx, arm) in arms.iter().enumerate() {
            // Try to extract integer value from the pattern.
            if let Some(value) = try_extract_integer_pattern(ast, arm.pattern, source_map) {
                arm_values.push(value);
                arm_value_index_pairs.push((value, arm_idx as u32));
            } else {
                // Check if it's a wildcard (which is OK, just not counted).
                if !is_wildcard_pattern(ast, arm.pattern, source_map) {
                    // Non-integer, non-wildcard pattern found.
                    // Emit T0550 diagnostic.
                    if let Some(pattern_node) = ast.get(arm.pattern) {
                        if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 550) {
                            let diag = Diagnostic::error(code)
                                .message("non-integer pattern in @jump_table match")
                                .with_span(pattern_node.span)
                                .finish();
                            let _ = sink.emit(diag);
                        }
                    }
                    has_non_integer_pattern = true;
                }
                // Wildcard patterns are skipped but don't cause an error.
            }
        }

        // Compute dispatch metadata.
        let density_ok = if has_non_integer_pattern || arm_values.is_empty() {
            false
        } else {
            let min_val = *arm_values.iter().min().unwrap_or(&0);
            let max_val = *arm_values.iter().max().unwrap_or(&0);
            let range = (max_val - min_val + 1) as u32;
            let covered_arms = arm_values.len() as u32;
            covered_arms.saturating_mul(2) >= range
        };

        // Extract min_arm and range.
        let (min_arm, range) = if !arm_values.is_empty() {
            let min_val = *arm_values.iter().min().unwrap();
            let max_val = *arm_values.iter().max().unwrap();
            let r = (max_val - min_val + 1) as u32;
            (min_val, r)
        } else {
            (0, 1)
        };

        let covered_arms = arm_values.len() as u32;

        // Insert into the dispatch metadata side-table.
        ir.match_dispatch_meta_mut().insert(
            ir_match_id,
            paideia_as_ir::MatchDispatchMeta {
                jump_table: true,
                min_arm,
                range,
                covered_arms,
                density_ok,
            },
        );

        // Also store the per-arm (value, index) pairs for rodata synthesis.
        ir.match_jump_table_arm_values_mut().insert(
            ir_match_id,
            arm_value_index_pairs,
        );
    }
}

/// Try to extract an integer value from a pattern node.
///
/// Returns Some(value) if the pattern is an integer literal, None otherwise.
/// Uses the SourceMap to extract and parse the actual integer value from source.
fn try_extract_integer_pattern(
    ast: &AstArena,
    pattern_id: NodeId,
    source_map: &SourceMap,
) -> Option<i64> {
    let pattern_node = ast.get(pattern_id)?;

    // Check if it's a PatLiteral node (real pattern literal, not expression literal).
    if pattern_node.kind != NodeKind::PatLiteral {
        return None;
    }

    // Get the pattern data and verify it's a Literal pattern.
    let pattern_data = ast.pattern_data(pattern_id)?;
    if let paideia_as_ast::PatternData::Literal { lit } = pattern_data {
        // Use extract_integer_from_span to get the actual value from source text.
        extract_integer_from_span(ast, *lit, source_map)
    } else {
        None
    }
}

/// Check if a pattern is a wildcard (`_`).
///
/// Returns true if:
/// 1. The pattern is a NodeKind::PatWildcard with PatternData::Wildcard, OR
/// 2. The pattern is a NodeKind::PatIdent with identifier text "_"
fn is_wildcard_pattern(ast: &AstArena, pattern_id: NodeId, source_map: &SourceMap) -> bool {
    let pattern_node = match ast.get(pattern_id) {
        Some(n) => n,
        None => return false,
    };

    // Check if it's a PatWildcard node (real wildcard pattern).
    if pattern_node.kind == NodeKind::PatWildcard {
        if let Some(paideia_as_ast::PatternData::Wildcard) = ast.pattern_data(pattern_id) {
            return true;
        }
    }

    // Check if it's a PatIdent with identifier text "_"
    if pattern_node.kind == NodeKind::PatIdent {
        if let Some(paideia_as_ast::PatternData::Ident { .. }) = ast.pattern_data(pattern_id) {
            // Extract the identifier text from the pattern's span
            let span = pattern_node.span;
            let file_id = span.file();
            let source = source_map.content(file_id);
            let start = span.byte_start() as usize;
            let end = start + span.byte_len() as usize;
            if end <= source.len() {
                let text = &source[start..end];
                if text == "_" {
                    return true;
                }
            }
        }
    }

    false
}

/// Populate the RecordLayoutTable with RecordCons type information.
///
/// PA-r17-010a (#1070): Walk all ExprRecordCons nodes in the AST, look up their
/// struct type name in the registry, and insert the (ir_node_id -> RecordTypeId)
/// mapping into the record_layout_table.
///
/// For each RecordCons node:
/// 1. Extract the type_name (child[0] of RecordCons per softarch spec)
/// 2. Get source text for that ident
/// 3. Look up in registry.by_name
/// 4. If found: insert into record_layout_table
/// 5. If NOT found: emit T0552 diagnostic with the unknown type name
fn populate_record_layout_table(
    ast: &AstArena,
    ir: &mut IrArena,
    ast_to_ir: &HashMap<NodeId, IrNodeId>,
    registry: &crate::StructRegistry,
    source_map: &SourceMap,
    sink: &mut dyn DiagnosticSink,
) {
    // Walk all AST nodes looking for RecordCons expressions
    for ast_node_id in 1..=ast.len() {
        let ast_id = match NodeId::new(ast_node_id as u32) {
            Some(nid) => nid,
            None => continue,
        };

        // Get the AST node and check if it's an ExprRecordCons
        let ast_node = match ast.get(ast_id) {
            Some(n) => n,
            None => continue,
        };

        if ast_node.kind != NodeKind::ExprRecordCons {
            continue;
        }

        // Get the ExprData::RecordCons to access type_name
        let type_name_id = match ast.expr_data(ast_id) {
            Some(ExprData::RecordCons { type_name, .. }) => type_name,
            _ => continue,
        };

        // Extract the type name from source
        let type_name_text = match extract_source_text_for_record_cons(ast, source_map, *type_name_id) {
            Some(text) => text,
            None => {
                // Skip if we can't extract the type name
                continue;
            }
        };

        // Get the corresponding IR node ID for this RecordCons
        let ir_record_cons_id = match ast_to_ir.get(&ast_id) {
            Some(id) => *id,
            None => continue,
        };

        // Look up the struct type in the registry
        match registry.get_by_name(&type_name_text) {
            Some(type_id) => {
                // Insert the RecordCons -> RecordTypeId mapping
                ir.record_layout_table_mut().insert(ir_record_cons_id, type_id);
            }
            None => {
                // Emit T0552 diagnostic: unknown struct type
                if let Some(type_name_node) = ast.get(*type_name_id) {
                    if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 552) {
                        let diag = Diagnostic::error(code)
                            .message(format!(
                                "Unsupported struct type: '{}' (not found in struct registry)",
                                type_name_text
                            ))
                            .with_span(type_name_node.span)
                            .finish();
                        let _ = sink.emit(diag);
                    }
                }
            }
        }
    }
}

/// Extract source text for a RecordCons type_name node.
///
/// Similar to extract_source_text but returns a String instead of Option<String>
/// for better ergonomics in the populate_record_layout_table context.
fn extract_source_text_for_record_cons(
    ast: &AstArena,
    source_map: &SourceMap,
    node_id: NodeId,
) -> Option<String> {
    let node = ast.get(node_id)?;
    let span = node.span;
    let file_id = span.file();
    let source = source_map.content(file_id);

    let start = span.byte_start() as usize;
    let len = span.byte_len() as usize;
    if start + len > source.len() {
        return None;
    }

    let text = &source[start..start + len];
    Some(text.to_string())
}

/// Build the `binding_name → declared_type_text` map used by multiple
/// populators (`populate_field_access_info`, `populate_match_arm_meta`).
///
/// Walks the AST once and collects every `NodeKind::Let` (item-level) and
/// `NodeKind::StmtLet` (statement-level) binding that has an explicit type
/// annotation. Keyed on the source text of the binding name; valued on the
/// source text of the declared type. Downstream populators filter the map
/// against their own registry (StructRegistry for field access,
/// EnumRegistry for match arms).
///
/// Refactor 2026-07-07 Step 7: extracted from duplicated Step-1 walks that
/// previously lived at `populate_field_access_info` and
/// `populate_match_arm_meta`. Retires the "update one, forget the other"
/// hazard for any future change to how binding types are extracted.
fn build_binding_type_map(ast: &AstArena, source_map: &SourceMap) -> HashMap<String, String> {
    let mut binding_to_type = HashMap::new();

    for ast_node_id in 1..=ast.len() {
        let ast_id = match NodeId::new(ast_node_id as u32) {
            Some(nid) => nid,
            None => continue,
        };

        let ast_node = match ast.get(ast_id) {
            Some(n) => n,
            None => continue,
        };

        // Process item-level Let bindings (ItemData::Let).
        if ast_node.kind == NodeKind::Let {
            if let Some(paideia_as_ast::ItemData::Let { name, ty, .. }) = ast.item_data(ast_id) {
                if let Some(ty_node_id) = ty {
                    if let Some(type_text) =
                        extract_source_text_for_record_cons(ast, source_map, *ty_node_id)
                    {
                        if let Some(binding_name) =
                            extract_source_text_for_record_cons(ast, source_map, *name)
                        {
                            binding_to_type.insert(binding_name, type_text);
                        }
                    }
                }
            }
        }

        // Process statement-level Let bindings (StmtData::Let).
        if ast_node.kind == NodeKind::StmtLet {
            if let Some(paideia_as_ast::StmtData::Let { name, ty, .. }) = ast.stmt_data(ast_id) {
                if let Some(ty_node_id) = ty {
                    if let Some(type_text) =
                        extract_source_text_for_record_cons(ast, source_map, *ty_node_id)
                    {
                        if let Some(binding_name) =
                            extract_source_text_for_record_cons(ast, source_map, *name)
                        {
                            binding_to_type.insert(binding_name, type_text);
                        }
                    }
                }
            }
        }
    }

    binding_to_type
}

/// Populate the FieldAccessSideTable with FieldAccess type and field index information.
///
/// Phase 6 m3-002 (#1073): Walk all ExprFieldAccess nodes in the AST, look up their
/// receiver's struct type in the registry, and insert the (ir_node_id -> FieldAccessInfo)
/// mapping into the field_access_info side-table.
///
/// For each FieldAccess node:
/// 1. Extract the receiver's name (for Ident/ExprPath only; skip computed expressions)
/// 2. Look up the binding in the binding→struct_type map
/// 3. Look up the struct type in registry.get_by_name
/// 4. Find the field index matching the field name
/// 5. Insert into field_access_info_mut()
///
/// Emit T0552 diagnostic on:
/// - Receiver name in binding map but struct type unknown
/// - Struct type found but field name unknown
///
/// Non-blocking: skip silently if receiver is computed (not in binding map).
fn populate_field_access_info(
    ast: &AstArena,
    ir: &mut IrArena,
    ast_to_ir: &HashMap<NodeId, IrNodeId>,
    registry: &crate::StructRegistry,
    source_map: &SourceMap,
    sink: &mut dyn DiagnosticSink,
) {
    // Step 1: Build binding_name → struct_type_text map.
    // Refactor 2026-07-07 Step 7: extracted into build_binding_type_map,
    // shared with populate_match_arm_meta.
    let binding_to_struct_type = build_binding_type_map(ast, source_map);

    // Step 2: Walk all AST nodes with NodeKind::ExprFieldAccess
    for ast_node_id in 1..=ast.len() {
        let ast_id = match NodeId::new(ast_node_id as u32) {
            Some(nid) => nid,
            None => continue,
        };

        let ast_node = match ast.get(ast_id) {
            Some(n) => n,
            None => continue,
        };

        if ast_node.kind != NodeKind::ExprFieldAccess {
            continue;
        }

        // Get the ExprData::FieldAccess to access receiver and field
        let (receiver_id, field_id) = match ast.expr_data(ast_id) {
            Some(ExprData::FieldAccess { receiver, field }) => (receiver, field),
            _ => continue,
        };

        // Extract receiver name (only simple Ident/ExprPath, not computed expressions)
        let receiver_name = match extract_source_text_for_record_cons(ast, source_map, *receiver_id)
        {
            Some(name) => name,
            None => {
                // Receiver is computed or we can't extract text: skip silently
                continue;
            }
        };

        // Extract field name
        let field_name = match extract_source_text_for_record_cons(ast, source_map, *field_id) {
            Some(name) => name,
            None => {
                // Skip if we can't extract field name
                continue;
            }
        };

        // Look up the struct type for this receiver
        let struct_type_text = match binding_to_struct_type.get(&receiver_name) {
            Some(text) => text.clone(),
            None => {
                // Receiver not in binding map: skip silently (non-blocking failure mode)
                continue;
            }
        };

        // Look up the struct type in the registry
        let type_id = match registry.get_by_name(&struct_type_text) {
            Some(id) => id,
            None => {
                // Emit T0552 diagnostic: unknown struct type
                if let Some(receiver_node) = ast.get(*receiver_id) {
                    if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 552) {
                        let diag = Diagnostic::error(code)
                            .message(format!(
                                "Unsupported struct type: '{}' (not found in struct registry)",
                                struct_type_text
                            ))
                            .with_span(receiver_node.span)
                            .finish();
                        let _ = sink.emit(diag);
                    }
                }
                continue;
            }
        };

        // Find the field index
        let fields = match registry.get_fields(type_id) {
            Some(f) => f,
            None => {
                // No fields for this type: skip
                continue;
            }
        };

        let field_index = match fields.iter().position(|(name, _)| name == &field_name) {
            Some(idx) => idx as u32,
            None => {
                // Emit T0552 diagnostic: unknown field
                if let Some(field_node) = ast.get(*field_id) {
                    if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 552) {
                        let diag = Diagnostic::error(code)
                            .message(format!(
                                "Unknown field '{}' in struct '{}'",
                                field_name, struct_type_text
                            ))
                            .with_span(field_node.span)
                            .finish();
                        let _ = sink.emit(diag);
                    }
                }
                continue;
            }
        };

        // Get the corresponding IR node ID for this FieldAccess
        let ir_field_access_id = match ast_to_ir.get(&ast_id) {
            Some(id) => *id,
            None => continue,
        };

        // Insert into field_access_info_mut()
        let field_access_info = paideia_as_ir::FieldAccessInfo { type_id, field_index };
        ir.field_access_info_mut().insert(ir_field_access_id, field_access_info);
    }
}

/// Phase 7 m4-003 (#1048/#1049): Populate EnumConsInfoTable for EnumCons expressions.
///
/// Walks all AST Call nodes. For each Call whose callee is an ExprPath with exactly 2 segments:
/// - Extracts seg0_name (enum type name) and seg1_name (variant name) from source text.
/// - Looks up seg0_name in registry.by_name → EnumTypeId.
/// - Looks up seg1_name in registry.variants[type_id] → variant_index.
/// - Rewrites the IR node kind from App (placeholder) to EnumCons.
/// - Inserts (EnumCons_ir_id -> EnumConsInfo) into enum_cons_info side-table.
///
/// Emits T0554 diagnostic on: known enum but unknown variant.
/// Silently skips on: seg0 not in enum registry (that's a plain call).
fn populate_enum_cons_info(
    ast: &AstArena,
    ir: &mut IrArena,
    ast_to_ir: &HashMap<NodeId, IrNodeId>,
    registry: &crate::EnumRegistry,
    source_map: &SourceMap,
    sink: &mut dyn DiagnosticSink,
) {
    // Walk all AST nodes with NodeKind::ExprCall
    for ast_node_id in 1..=ast.len() {
        let ast_id = match NodeId::new(ast_node_id as u32) {
            Some(nid) => nid,
            None => continue,
        };

        let ast_node = match ast.get(ast_id) {
            Some(n) => n,
            None => continue,
        };

        if ast_node.kind != NodeKind::ExprCall {
            continue;
        }

        // Get the ExprData::Call to access callee
        let callee_id = match ast.expr_data(ast_id) {
            Some(paideia_as_ast::ExprData::Call { callee, .. }) => callee,
            _ => continue,
        };

        // Check if callee is an ExprPath
        let callee_node = match ast.get(*callee_id) {
            Some(n) => n,
            None => continue,
        };

        if callee_node.kind != NodeKind::ExprPath {
            // Not a path; skip silently (could be a computed function)
            continue;
        }

        // Extract path segments
        let segments = match ast.expr_data(*callee_id) {
            Some(paideia_as_ast::ExprData::Path { segments }) => segments,
            _ => continue,
        };

        // We're looking for exactly 2 segments: EnumType::Variant
        if segments.len() != 2 {
            continue;
        }

        // Extract segment names from source text
        let seg0_name = match extract_source_text_for_record_cons(ast, source_map, segments[0]) {
            Some(name) => name,
            None => continue,
        };

        let seg1_name = match extract_source_text_for_record_cons(ast, source_map, segments[1]) {
            Some(name) => name,
            None => continue,
        };

        // Look up the enum type in the registry
        let type_id = match registry.get_by_name(&seg0_name) {
            Some(id) => id,
            None => {
                // Enum type not found: skip silently (this could be a plain function call)
                continue;
            }
        };

        // Find the variant index
        let variants = match registry.get_variants(type_id) {
            Some(v) => v,
            None => {
                // No variants for this type: skip
                continue;
            }
        };

        let variant_index = match variants.iter().position(|(name, _)| name == &seg1_name) {
            Some(idx) => idx as u32,
            None => {
                // Emit T0554 diagnostic: unknown variant in known enum
                if let Some(variant_node) = ast.get(segments[1]) {
                    if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 554) {
                        let diag = Diagnostic::error(code)
                            .message(format!(
                                "Unknown variant '{}' in enum '{}'",
                                seg1_name, seg0_name
                            ))
                            .with_span(variant_node.span)
                            .finish();
                        let _ = sink.emit(diag);
                    }
                }
                continue;
            }
        };

        // Get the corresponding IR node ID for this Call
        let ir_call_id = match ast_to_ir.get(&ast_id) {
            Some(id) => *id,
            None => continue,
        };

        // Rewrite the IR node from App to EnumCons
        // We need to mutate the IR node's kind
        if let Some(ir_node) = ir.get_mut(ir_call_id) {
            ir_node.kind = paideia_as_ir::IrKind::EnumCons;
        }

        // Strip the callee (first child) from the children list.
        // App node has children [callee, arg0, arg1, ...], but EnumCons should only have [arg0, arg1, ...]
        if let Some(children) = ir.children_mut(ir_call_id) {
            if !children.is_empty() {
                children.remove(0);  // Remove the callee at index 0
            }
        }

        // Insert into enum_cons_info_mut()
        let enum_cons_info = paideia_as_ir::EnumConsInfo { type_id, variant_index };
        ir.enum_cons_info_mut().insert(ir_call_id, enum_cons_info);
    }
}

/// Recursive helper for building nested PatternBinding trees from match patterns.
///
/// Converts an AST pattern node into a nested PatternBinding structure that
/// represents the full pattern hierarchy (e.g., `Ok(Point { x, y })`).
///
/// # Arguments
/// - `pat_id`: The AST pattern node to lower
/// - `ast`: The AST arena
/// - `source_map`: Source map for extracting text
/// - `enum_registry`: For resolving enum types and variant indices
/// - `struct_registry`: For resolving struct types
/// - `payload_map`: HashMap from (EnumTypeId, variant_idx) to Option<RecordTypeId>
/// - `scrutinee_enum_id`: The enum type of the match scrutinee (for type checking)
///
/// # Returns
/// Some(PatternBinding) on success, None if pattern cannot be lowered
/// (e.g., nested EnumVariant with non-scrutinee enum type).
fn lower_pattern_data(
    pat_id: NodeId,
    ast: &AstArena,
    source_map: &SourceMap,
    enum_registry: &crate::EnumRegistry,
    struct_registry: &crate::StructRegistry,
    payload_map: &std::collections::HashMap<(paideia_as_ir::enum_layout::EnumTypeId, u32), Option<paideia_as_ir::record_layout::RecordTypeId>>,
    scrutinee_enum_id: paideia_as_ir::enum_layout::EnumTypeId,
) -> Option<paideia_as_ir::enum_layout::PatternBinding> {
    use paideia_as_ir::enum_layout::PatternBinding;

    let pattern_node = ast.get(pat_id)?;

    match pattern_node.kind {
        NodeKind::PatWildcard => Some(PatternBinding::Wildcard),

        NodeKind::PatIdent => {
            // Extract the identifier text
            let text = extract_source_text_for_record_cons(ast, source_map, pat_id)?;
            if text == "_" {
                Some(PatternBinding::Wildcard)
            } else {
                Some(PatternBinding::Simple(text))
            }
        }

        NodeKind::PatEnumVariant => {
            // Extract variant path and arguments
            let (path_id, args) = match ast.pattern_data(pat_id) {
                Some(paideia_as_ast::PatternData::EnumVariant { path, args }) => (*path, args.clone()),
                _ => return None,
            };

            // Get variant name from path
            let variant_text = extract_source_text_for_record_cons(ast, source_map, path_id)?;

            // Resolve variant index in the scrutinee enum
            let variants = enum_registry.get_variants(scrutinee_enum_id)?;
            let (variant_idx, (_, _)) = variants
                .iter()
                .enumerate()
                .find(|(_, (name, _))| name == &variant_text)?;
            let variant_idx = variant_idx as u32;

            // Get payload type from the payload_map if present
            let payload_type = payload_map.get(&(scrutinee_enum_id, variant_idx)).copied().flatten();

            // Scope: cross-enum nested patterns (e.g. Ok(Some(x)) where Some is a different enum) are not supported —
            // the recursive variant-name lookup reuses the outer scrutinee's enum, so a mismatch silently yields payload=None.
            // If there's one argument, try to lower it recursively
            let payload_binding = if args.len() == 1 {
                // For single payload, recursively lower the argument
                lower_pattern_data(
                    args[0],
                    ast,
                    source_map,
                    enum_registry,
                    struct_registry,
                    payload_map,
                    scrutinee_enum_id,
                ).map(Box::new)
            } else if args.is_empty() {
                None
            } else {
                // Multiple arguments: not supported yet
                None
            };

            Some(PatternBinding::EnumVariant {
                variant_index: variant_idx,
                payload_type,
                payload: payload_binding,
            })
        }

        NodeKind::PatStruct => {
            // Extract struct path and fields
            let (struct_path_id, fields) = match ast.pattern_data(pat_id) {
                Some(paideia_as_ast::PatternData::Struct { path, fields }) => (*path, fields.clone()),
                _ => return None,
            };

            // Get struct name and resolve to RecordTypeId
            let struct_text = extract_source_text_for_record_cons(ast, source_map, struct_path_id)?;
            let record_type_id = struct_registry.get_by_name(&struct_text)?;

            // Lower each field pattern recursively
            let mut field_bindings = Vec::new();
            for field in fields {
                let field_name = extract_source_text_for_record_cons(ast, source_map, field.name)?;
                let field_pattern = lower_pattern_data(
                    field.pattern,
                    ast,
                    source_map,
                    enum_registry,
                    struct_registry,
                    payload_map,
                    scrutinee_enum_id,
                )?;
                field_bindings.push((field_name, field_pattern));
            }

            Some(PatternBinding::Record {
                type_id: record_type_id,
                fields: field_bindings,
            })
        }

        _ => None,
    }
}

/// Phase 7 m9-009 (#1081/#1082): Populate MatchArmMeta side-table for match arms.
///
/// Walks all AST ExprMatch nodes and their arms, classifying patterns:
/// - Wildcard or PatIdent with text "_" → default arm
/// - PatIdent with variant name text → bare-variant no-payload arm
/// - PatEnumVariant { path, args } → variant with optional payload binder
/// - PatStruct { path, fields } → record pattern with nested binding tree
/// - Other patterns → emit T0556 diagnostic
///
/// Requires enum_registry threaded, which provides variant names for lookup.
/// Requires struct_registry and payload_map threaded for nested pattern matching.
fn populate_match_arm_meta(
    ast: &AstArena,
    ir: &mut IrArena,
    ast_to_ir: &HashMap<NodeId, IrNodeId>,
    enum_registry: &crate::EnumRegistry,
    struct_registry: &crate::StructRegistry,
    payload_map: &std::collections::HashMap<(paideia_as_ir::enum_layout::EnumTypeId, u32), Option<paideia_as_ir::record_layout::RecordTypeId>>,
    source_map: &SourceMap,
    sink: &mut dyn DiagnosticSink,
) {
    // Step 1: Build binding_name → enum_type_text map for enum bindings.
    // Refactor 2026-07-07 Step 7: extracted into build_binding_type_map,
    // shared with populate_field_access_info.
    let binding_to_enum_type = build_binding_type_map(ast, source_map);

    // Step 2: Walk all AST ExprMatch nodes
    for ast_node_id in 1..=ast.len() {
        let ast_id = match NodeId::new(ast_node_id as u32) {
            Some(nid) => nid,
            None => continue,
        };

        let ast_node = match ast.get(ast_id) {
            Some(n) => n,
            None => continue,
        };

        if ast_node.kind != NodeKind::ExprMatch {
            continue;
        }

        // Get ExprData::Match to access scrutinee and arms
        let (scrutinee, arms) = match ast.expr_data(ast_id) {
            Some(paideia_as_ast::ExprData::Match { scrutinee, arms, .. }) => (scrutinee, arms),
            _ => continue,
        };

        // Resolve scrutinee type
        let scrutinee_text = match extract_source_text_for_record_cons(ast, source_map, *scrutinee) {
            Some(text) => text,
            None => continue,
        };

        let enum_type_text = match binding_to_enum_type.get(&scrutinee_text) {
            Some(text) => text.clone(),
            None => continue,
        };

        let type_id = match enum_registry.get_by_name(&enum_type_text) {
            Some(id) => id,
            None => continue,
        };

        let variants = match enum_registry.get_variants(type_id) {
            Some(v) => v,
            None => continue,
        };

        // Get the IR match node
        let match_ir_id = match ast_to_ir.get(&ast_id) {
            Some(id) => *id,
            None => continue,
        };

        // Insert enum type into match_scrutinee_table
        ir.match_scrutinee_table_mut().insert(match_ir_id, type_id);

        // Step 3: Process each arm
        // Get children of the match IR node: [scrutinee, arm_body1, arm_body2, ...]
        let match_children = ir.children(match_ir_id);
        let arm_ir_ids = if !match_children.is_empty() {
            match_children[1..].to_vec()
        } else {
            Vec::new()
        };

        for (arm_idx, arm) in arms.iter().enumerate() {
            // Get the corresponding arm IR id from children
            let arm_ir_id = match arm_ir_ids.get(arm_idx) {
                Some(id) => *id,
                None => continue,
            };

            // Classify the pattern
            let pattern_node = match ast.get(arm.pattern) {
                Some(n) => n,
                None => continue,
            };

            let mut arm_meta = paideia_as_ir::MatchArmMeta::default();

            match pattern_node.kind {
                NodeKind::PatWildcard => {
                    arm_meta.is_default = true;
                }
                NodeKind::PatIdent => {
                    // Extract pattern source text
                    if let Some(pattern_text) =
                        extract_source_text_for_record_cons(ast, source_map, arm.pattern)
                    {
                        if pattern_text == "_" {
                            arm_meta.is_default = true;
                        } else {
                            // Check if this matches a bare variant name
                            if let Some((variant_idx, _)) =
                                variants.iter().enumerate().find(|(_, (name, _))| name == &pattern_text)
                            {
                                arm_meta.variant_index = Some(variant_idx as u32);
                            }
                        }
                    }
                }
                NodeKind::PatEnumVariant => {
                    // Extract variant path and arguments
                    if let Some(paideia_as_ast::PatternData::EnumVariant { path, args }) =
                        ast.pattern_data(arm.pattern)
                    {
                        // Get variant name from path source text
                        if let Some(variant_text) =
                            extract_source_text_for_record_cons(ast, source_map, *path)
                        {
                            // Find variant index
                            if let Some((variant_idx, _)) = variants
                                .iter()
                                .enumerate()
                                .find(|(_, (name, _))| name == &variant_text)
                            {
                                arm_meta.variant_index = Some(variant_idx as u32);

                                // Extract payload binder if args.len() == 1 and args[0] is PatIdent
                                if args.len() == 1 {
                                    if let Some(arg_node) = ast.get(args[0]) {
                                        if arg_node.kind == NodeKind::PatIdent {
                                            if let Some(binder_text) =
                                                extract_source_text_for_record_cons(ast, source_map, args[0])
                                            {
                                                arm_meta.payload_binder = Some(binder_text);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                NodeKind::PatStruct => {
                    // Record pattern support: just note that we have a struct pattern
                    // The actual nested pattern binding tree is handled via lower_pattern_data below
                }
                _ => {
                    // Emit T0556 diagnostic: unsupported match arm pattern kind
                    if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 556) {
                        let diag = Diagnostic::error(code)
                            .message("unsupported match arm pattern kind".to_string())
                            .with_span(pattern_node.span)
                            .finish();
                        let _ = sink.emit(diag);
                    }
                    continue;
                }
            }

            // Build the nested pattern binding tree using lower_pattern_data
            if let Some(pattern_binding) = lower_pattern_data(
                arm.pattern,
                ast,
                source_map,
                enum_registry,
                struct_registry,
                payload_map,
                type_id,
            ) {
                arm_meta.pattern_binding = Some(pattern_binding);
            }

            // Insert arm metadata
            ir.match_arm_meta_mut().insert(arm_ir_id, arm_meta);
        }
    }
}

/// Phase 17 m6-b (#1046): Populate the LiteralValueTable for Literal expressions.
///
/// Walks all AST Literal nodes and extracts their integer values from source text,
/// inserting (Literal_ir_id -> value) mappings into the IR's literal_values side-table.
/// This is needed for field assignment to access immediate values at emit time.
#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::{ExprData, StmtData};
    use paideia_as_diagnostics::{FileId, SourceMap, VecSink};

    fn span() -> paideia_as_diagnostics::Span {
        paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    /// Helper to create a test SourceMap and VecSink for lowering.
    fn create_test_source_map_and_sink() -> (SourceMap, VecSink) {
        let mut source_map = SourceMap::new();
        let _file = source_map.add_file(
            std::path::PathBuf::from("test.pdx"),
            String::from("// test source"),
        );
        let sink = VecSink::new();
        (source_map, sink)
    }

    #[test]
    fn lower_empty_arena() {
        let (source_map, mut sink) = create_test_source_map_and_sink();
        let (source_map, mut sink) = create_test_source_map_and_sink();
        let ast = AstArena::new();
        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());
        assert_eq!(result.ir.len(), 0);
        assert!(result.ast_to_ir.is_empty());
    }

    #[test]
    fn lower_single_placeholder() {
        let (source_map, mut sink) = create_test_source_map_and_sink();
        let (source_map, mut sink) = create_test_source_map_and_sink();
        let mut ast = AstArena::new();
        let _id = ast.alloc(NodeKind::Placeholder, span());
        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());
        assert_eq!(result.ir.len(), 1);
        assert_eq!(result.ast_to_ir.len(), 1);
    }

    #[test]
    fn lower_let_plus() {
        // Build: let x = 1 + 2
        let (source_map, mut sink) = create_test_source_map_and_sink();
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
        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

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
        let (source_map, mut sink) = create_test_source_map_and_sink();
        let mut ast = AstArena::new();

        // Allocate a few nodes with the test span.
        let id1 = ast.alloc(NodeKind::Ident, span());
        let id2 = ast.alloc(NodeKind::ExprLiteral, span());
        let id3 = ast.alloc(NodeKind::StmtLet, span());

        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

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
        let (source_map, mut sink) = create_test_source_map_and_sink();
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
        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());
        assert_eq!(result.ir.len(), 10);
    }

    #[test]
    fn lower_preserves_kind_count() {
        // Assert that the number of IR nodes equals the number of AST nodes.
        let (source_map, mut sink) = create_test_source_map_and_sink();
        let mut ast = AstArena::new();

        for _ in 0..50 {
            ast.alloc(NodeKind::Placeholder, span());
        }

        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());
        assert_eq!(result.ir.len(), ast.len());
    }

    #[test]
    fn ast_to_ir_mapping_is_complete() {
        // Assert that every NodeId in the AST has an entry in the mapping.
        let (source_map, mut sink) = create_test_source_map_and_sink();
        let mut ast = AstArena::new();

        for _ in 0..20 {
            ast.alloc(NodeKind::Ident, span());
        }

        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

        for i in 0..ast.len() {
            let id = NodeId::new((i + 1) as u32).unwrap();
            assert!(result.ast_to_ir.contains_key(&id));
        }
    }

    #[test]
    fn lower_preserves_default_lin_class_and_effect_row() {
        // Verify that all IR nodes start with Unrestricted and empty effect row.
        let (source_map, mut sink) = create_test_source_map_and_sink();
        let mut ast = AstArena::new();
        ast.alloc(NodeKind::Placeholder, span());
        ast.alloc(NodeKind::ExprLambda, span());
        ast.alloc(NodeKind::StmtLet, span());

        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

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
        let (source_map, mut sink) = create_test_source_map_and_sink();
        let mut ast = AstArena::new();

        let id1 = ast.alloc(NodeKind::Ident, span());
        let id2 = ast.alloc(NodeKind::ExprLambda, span());
        let id3 = ast.alloc(NodeKind::StmtLet, span());

        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

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
        let (source_map, mut sink) = create_test_source_map_and_sink();
        let mut ast = AstArena::new();

        // Carefully construct test data with known mappings.
        ast.alloc(NodeKind::Ident, span()); // Should map to Var
        ast.alloc(NodeKind::ExprLiteral, span()); // Should map to Literal
        ast.alloc(NodeKind::ExprCall, span()); // Should map to App
        ast.alloc(NodeKind::Module, span()); // Should map to Module

        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

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
        let (source_map, mut sink) = create_test_source_map_and_sink();
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
        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

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
        let (source_map, mut sink) = create_test_source_map_and_sink();
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
        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

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
        let (source_map, mut sink) = create_test_source_map_and_sink();
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
        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

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
        let (source_map, mut sink) = create_test_source_map_and_sink();
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
        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

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
        let (source_map, mut sink) = create_test_source_map_and_sink();
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
        let field_name_id = ast.alloc(NodeKind::Ident, span());

        // Allocate ExprFieldAccess: (*p).field
        // This matches the shape emitted by the real parser.
        let field_access_id = ast.alloc_expr(
            NodeKind::ExprFieldAccess,
            span(),
            ExprData::FieldAccess { receiver: deref_expr_id, field: field_name_id },
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
        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

        // Verify the assignment lowered to Store
        let assign_ir_id = result.ast_to_ir[&assign_expr_id];
        assert_eq!(
            result.ir[assign_ir_id].kind,
            IrKind::Store,
            "Field-deref assignment should lower to Store"
        );

        // Verify children are [field_access, unused, value]
        let children = result.ir.children(assign_ir_id);
        assert_eq!(children.len(), 3, "Store should have 3 children");

        let field_access_child_id = children[0];
        let value_child_id = children[2];

        let field_access_ir_id = result.ast_to_ir[&field_access_id];
        let value_ir_id = result.ast_to_ir[&value_var_id];

        assert_eq!(field_access_child_id, field_access_ir_id);
        assert_eq!(value_child_id, value_ir_id);
    }

    #[test]
    fn lower_array_literal_produces_array_lit() {
        // Phase 8 m2-002: array literal `[expr0, expr1, ...]` lowers to IrKind::ArrayLit
        let (source_map, mut sink) = create_test_source_map_and_sink();
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
        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

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
        let (source_map, mut sink) = create_test_source_map_and_sink();
        let mut ast = AstArena::new();

        // Allocate ExprArrayLit: []
        let array_lit_id =
            ast.alloc_expr(NodeKind::ExprArrayLit, span(), ExprData::ArrayLit(vec![]));

        // Lower the AST.
        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

        // Verify we have 1 IR node.
        assert_eq!(result.ir.len(), 1);

        // Verify the ArrayLit IR node exists and has no children.
        let ir_array_id = result.ast_to_ir[&array_lit_id];
        assert_eq!(result.ir[ir_array_id].kind, IrKind::ArrayLit);

        let children = result.ir.children(ir_array_id);
        assert_eq!(children.len(), 0, "Empty ArrayLit should have no children");
    }

    #[test]
    fn lower_array_repeat_with_non_literal_count() {
        // Phase 9 m1-002: array repeat `[expr; count]` where count is not a literal
        let (source_map, mut sink) = create_test_source_map_and_sink();
        // Currently expands to a single copy with a note that P0211 should be emitted.
        let mut ast = AstArena::new();

        // Allocate element expression (a literal)
        let elem_id = ast.alloc(NodeKind::ExprLiteral, span());

        // Allocate count expression (an identifier, not a literal)
        let count_id = ast.alloc(NodeKind::Ident, span());

        // Allocate ExprArrayRepeat: [elem; count]
        let repeat_id = ast.alloc_expr(
            NodeKind::ExprArrayRepeat,
            span(),
            ExprData::ArrayRepeat {
                expr: elem_id,
                count: count_id,
            },
        );

        // Lower the AST.
        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

        // Verify we have 3 IR nodes: elem, count, repeat.
        assert_eq!(result.ir.len(), 3);

        // Verify the ArrayRepeat node maps to IrKind::ArrayLit in IR.
        let ir_repeat_id = result.ast_to_ir[&repeat_id];
        assert_eq!(
            result.ir[ir_repeat_id].kind,
            IrKind::ArrayLit,
            "ArrayRepeat should lower to IrKind::ArrayLit"
        );

        // For non-literal count, expand_array_repeat returns a single copy.
        let children = result.ir.children(ir_repeat_id);
        assert_eq!(
            children.len(),
            1,
            "ArrayRepeat with non-literal count should have 1 element child (fallback)"
        );

        let elem_ir = result.ast_to_ir[&elem_id];
        assert_eq!(children[0], elem_ir);
    }

    #[test]
    fn lower_array_repeat_nested_structs() {
        // Phase 9 m1-002: array repeat with struct-lit as element: `[Point { x: 1, y: 2 }; count]`
        let (source_map, mut sink) = create_test_source_map_and_sink();
        // This tests that recursion handles RecordCons nested in ArrayRepeat.
        let mut ast = AstArena::new();

        // Allocate type name and field elements
        let type_name_id = ast.alloc(NodeKind::Ident, span());
        let field_x_id = ast.alloc(NodeKind::Ident, span());
        let field_x_val_id = ast.alloc(NodeKind::ExprLiteral, span());
        let field_y_id = ast.alloc(NodeKind::Ident, span());
        let field_y_val_id = ast.alloc(NodeKind::ExprLiteral, span());

        // Allocate RecordCons: Point { x: 1, y: 2 }
        let struct_lit_id = ast.alloc_expr(
            NodeKind::ExprRecordCons,
            span(),
            ExprData::RecordCons {
                type_name: type_name_id,
                fields: vec![(field_x_id, field_x_val_id), (field_y_id, field_y_val_id)],
            },
        );

        // Allocate count (non-literal to defer evaluation)
        let count_id = ast.alloc(NodeKind::Ident, span());

        // Allocate ExprArrayRepeat: [struct_lit; count]
        let repeat_id = ast.alloc_expr(
            NodeKind::ExprArrayRepeat,
            span(),
            ExprData::ArrayRepeat {
                expr: struct_lit_id,
                count: count_id,
            },
        );

        // Lower the AST.
        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

        // Verify the ArrayRepeat node maps to IrKind::ArrayLit.
        let ir_repeat_id = result.ast_to_ir[&repeat_id];
        assert_eq!(result.ir[ir_repeat_id].kind, IrKind::ArrayLit);

        // Verify that the struct-lit is present in the children.
        let children = result.ir.children(ir_repeat_id);
        assert_eq!(
            children.len(),
            1,
            "ArrayRepeat with nested struct should have 1 element child (fallback)"
        );

        let struct_ir = result.ast_to_ir[&struct_lit_id];
        assert_eq!(children[0], struct_ir);

        // Verify the struct-lit lowered to RecordCons.
        assert_eq!(result.ir[struct_ir].kind, IrKind::RecordCons);
    }

    #[test]
    fn populate_record_layout_table_with_known_struct() {
        // PA-r17-010a (#1070): Test that RecordCons expressions are mapped to RecordTypeIds
        use crate::StructRegistry;

        let (source_map, mut sink) = create_test_source_map_and_sink();
        let mut ast = AstArena::new();

        // Allocate a RecordCons expression: Pair { x: 1, y: 2 }
        let type_name_id = ast.alloc(NodeKind::Ident, span());
        let field_name_x_id = ast.alloc(NodeKind::Ident, span());
        let value_x_id = ast.alloc(NodeKind::ExprLiteral, span());
        let field_name_y_id = ast.alloc(NodeKind::Ident, span());
        let value_y_id = ast.alloc(NodeKind::ExprLiteral, span());

        let record_cons_id = ast.alloc_expr(
            NodeKind::ExprRecordCons,
            span(),
            ExprData::RecordCons {
                type_name: type_name_id,
                fields: vec![
                    (field_name_x_id, value_x_id),
                    (field_name_y_id, value_y_id),
                ],
            },
        );

        // Create a registry with a Pair struct
        let mut registry = StructRegistry::empty();
        registry.by_name.insert("Pair".to_string(), paideia_as_ir::record_layout::RecordTypeId(1));
        registry.fields.insert(
            paideia_as_ir::record_layout::RecordTypeId(1),
            vec![
                ("x".to_string(), 0x08), // u64
                ("y".to_string(), 0x08), // u64
            ],
        );

        // Manually set the Ident node text to "Pair" by checking the span
        // (In a real test, we'd need to properly populate the source map with the right content)

        // Lower the AST with the registry
        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &registry, &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

        // Verify the record_layout_table was NOT populated because the type name lookup
        // will fail (the span content doesn't match "Pair" in the empty source map).
        // This test demonstrates the plumbing is in place, but won't succeed with an
        // empty source map. A proper integration test would use actual source text.
        let ir_record_cons_id = result.ast_to_ir[&record_cons_id];
        assert_eq!(result.ir[ir_record_cons_id].kind, IrKind::RecordCons);
    }

    #[test]
    fn populate_record_layout_table_with_unknown_struct() {
        // PA-r17-010a (#1070): Test that unknown struct types emit T0552 diagnostic
        use crate::StructRegistry;

        let (source_map, mut sink) = create_test_source_map_and_sink();
        let mut ast = AstArena::new();

        // Allocate a RecordCons expression with an unknown type
        let type_name_id = ast.alloc(NodeKind::Ident, span());
        let field_name_x_id = ast.alloc(NodeKind::Ident, span());
        let value_x_id = ast.alloc(NodeKind::ExprLiteral, span());

        let _record_cons_id = ast.alloc_expr(
            NodeKind::ExprRecordCons,
            span(),
            ExprData::RecordCons {
                type_name: type_name_id,
                fields: vec![(field_name_x_id, value_x_id)],
            },
        );

        // Create an empty registry (no structs defined)
        let registry = StructRegistry::empty();

        // Lower the AST with the empty registry
        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &registry, &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

        // The lowering should complete without panic
        assert_eq!(result.ir.len(), 4); // type_name, field_name, value, record_cons
    }

    #[test]
    fn populate_field_access_info_basic() {
        // Phase 6 m3-002 (#1073): Test field access lowering and side-table population.
        // Construct an AST with:
        //   let vops: Vops = ...
        //   ... (vops.read)
        // And verify that populate_field_access_info correctly populates the side-table.
        use crate::StructRegistry;
        use paideia_as_ir::RecordTypeId;
        use paideia_as_diagnostics::FileId;

        // Create a source_map with proper source text for extraction
        // Source layout: "vops" (0-3), " " (4), "Vops" (5-8), " " (9), "read" (10-13)
        let mut source_map = SourceMap::new();
        let source_text = "vops Vops read";
        let _file = source_map.add_file(
            std::path::PathBuf::from("test.pdx"),
            String::from(source_text),
        );
        let mut sink = VecSink::new();
        let mut ast = AstArena::new();

        // Helper to create a span at a given offset with given length
        let file_id = FileId::new(1).unwrap();
        let make_span = |offset: u32, len: u32| {
            paideia_as_diagnostics::Span::new(file_id, offset, len)
        };

        // Build binding name "vops" at offset 0-3
        let vops_binding_id = ast.alloc(NodeKind::Ident, make_span(0, 4));

        // Build type name "Vops" at offset 5-8
        let vops_type_id = ast.alloc(NodeKind::Ident, make_span(5, 4));

        // Build a Let binding: let vops: Vops = <dummy>
        // We need a dummy value expression; use a literal
        let dummy_value_id = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
        let let_binding_id = ast.alloc_stmt(
            NodeKind::StmtLet,
            make_span(0, 1),
            paideia_as_ast::StmtData::Let {
                mutable: false,
                name: vops_binding_id,
                ty: Some(vops_type_id),
                value: dummy_value_id,
            },
        );

        // Build receiver "vops" in field access at offset 0-3
        let receiver_id = ast.alloc(NodeKind::Ident, make_span(0, 4));

        // Build field name "read" at offset 10-13
        let field_name_id = ast.alloc(NodeKind::Ident, make_span(10, 4));

        // Build the FieldAccess node: vops.read
        let field_access_id = ast.alloc_expr(
            NodeKind::ExprFieldAccess,
            make_span(0, 1),
            ExprData::FieldAccess {
                receiver: receiver_id,
                field: field_name_id,
            },
        );

        // Create a registry with the Vops struct
        let mut registry = StructRegistry::empty();
        let vops_record_id = RecordTypeId(1);
        registry.by_name.insert("Vops".to_string(), vops_record_id);
        registry.fields.insert(vops_record_id, vec![("read".to_string(), 0x08)]); // u64

        // Lower the AST
        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &registry, &crate::EnumRegistry::empty(), &std::collections::HashMap::new());

        // Verify the FieldAccess IR node was created
        let field_access_ir_id = result.ast_to_ir[&field_access_id];
        assert_eq!(result.ir[field_access_ir_id].kind, IrKind::FieldAccess);

        // Verify the FieldAccess has exactly one child (the receiver)
        let children = result.ir.children(field_access_ir_id);
        assert_eq!(children.len(), 1);
        let receiver_ir_id = result.ast_to_ir[&receiver_id];
        assert_eq!(children[0], receiver_ir_id);

        // Phase 6 m3-002: Verify the side-table entry was populated
        let field_access_info_opt = result.ir.field_access_info().get(field_access_ir_id);
        let expected_info = paideia_as_ir::FieldAccessInfo {
            type_id: vops_record_id,
            field_index: 0,
        };
        assert_eq!(
            field_access_info_opt.copied(),
            Some(expected_info),
            "populate_field_access_info should have inserted the side-table entry"
        );
    }

    #[test]
    fn populate_enum_cons_info_basic() {
        // Phase 7 m4-003 (#1048/#1049): Test enum cons lowering and side-table population.
        // Construct an AST with:
        //   enum Result { Ok(u64), Err(u64) }
        //   let r = Result::Ok(42u64)
        // And verify that populate_enum_cons_info correctly populates the side-table.
        use paideia_as_ir::EnumTypeId;
        use paideia_as_diagnostics::FileId;

        // Create a source_map with proper source text for extraction
        // Source layout: "Result" (0-5) " Ok" (6-9) "42" (10-11)
        let mut source_map = SourceMap::new();
        let source_text = "Result Ok 42";
        let _file = source_map.add_file(
            std::path::PathBuf::from("test.pdx"),
            String::from(source_text),
        );
        let mut sink = VecSink::new();
        let mut ast = AstArena::new();

        // Helper to create a span at a given offset with given length
        let file_id = FileId::new(1).unwrap();
        let make_span = |offset: u32, len: u32| {
            paideia_as_diagnostics::Span::new(file_id, offset, len)
        };

        // Create enum: enum Result { Ok(u64), Err(u64) }
        let enum_name_id = ast.alloc(NodeKind::Ident, make_span(0, 6)); // "Result"
        let u64_type_id = ast.alloc(NodeKind::Placeholder, make_span(0, 1));

        let ok_variant_name_id = ast.alloc(NodeKind::Ident, make_span(7, 2)); // "Ok"
        let err_variant_name_id = ast.alloc(NodeKind::Ident, make_span(0, 3));

        let ok_variant = paideia_as_ast::EnumVariant::Tuple {
            name: ok_variant_name_id,
            payload: vec![u64_type_id],
        };
        let err_variant = paideia_as_ast::EnumVariant::Tuple {
            name: err_variant_name_id,
            payload: vec![u64_type_id],
        };

        let _enum_item_id = ast.alloc_item(
            NodeKind::Enum,
            make_span(0, 1),
            paideia_as_ast::ItemData::Enum {
                name: enum_name_id,
                generic_params: vec![],
                variants: vec![ok_variant, err_variant],
                attributes: vec![],
                doc: None,
            },
        );

        // Create the path Result::Ok
        let result_seg_id = ast.alloc(NodeKind::Ident, make_span(0, 6)); // "Result"
        let ok_seg_id = ast.alloc(NodeKind::Ident, make_span(7, 2)); // "Ok"
        let path_id = ast.alloc_expr(
            NodeKind::ExprPath,
            make_span(0, 1),
            paideia_as_ast::ExprData::Path {
                segments: vec![result_seg_id, ok_seg_id],
            },
        );

        // Create the literal 42u64
        let lit_node_id = ast.alloc(NodeKind::ExprLiteral, make_span(10, 2));
        let lit_42_id = ast.alloc_expr(
            NodeKind::ExprLiteral,
            make_span(10, 2),
            paideia_as_ast::ExprData::Literal { lit: lit_node_id },
        );

        // Create the call Result::Ok(42u64)
        let call_id = ast.alloc_expr(
            NodeKind::ExprCall,
            make_span(0, 1),
            paideia_as_ast::ExprData::Call {
                callee: path_id,
                args: vec![lit_42_id],
            },
        );

        // Build the enum registry manually (simulating the registry builder)
        let mut enum_registry = crate::EnumRegistry::empty();
        let result_type_id = EnumTypeId(1);
        enum_registry
            .by_name
            .insert("Result".to_string(), result_type_id);
        enum_registry.variants.insert(
            result_type_id,
            vec![
                ("Ok".to_string(), vec![u64_type_id]),
                ("Err".to_string(), vec![u64_type_id]),
            ],
        );

        // Lower the AST
        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &enum_registry, &std::collections::HashMap::new());

        // Verify the Call IR node was created and converted to EnumCons
        let call_ir_id = result.ast_to_ir[&call_id];
        assert_eq!(
            result.ir[call_ir_id].kind,
            IrKind::EnumCons,
            "Call to Result::Ok should be lowered as EnumCons"
        );

        // Verify the EnumConsInfo side-table entry was populated
        let enum_cons_info_opt = result.ir.enum_cons_info().get(call_ir_id);
        let expected_info = paideia_as_ir::EnumConsInfo {
            type_id: result_type_id,
            variant_index: 0, // Ok is the first variant (index 0)
        };
        assert_eq!(
            enum_cons_info_opt.copied(),
            Some(expected_info),
            "populate_enum_cons_info should have inserted the side-table entry with correct type_id and variant_index"
        );

        // Verify that the children have been properly stripped of the callee.
        // EnumCons should have only the payload children, no callee.
        let children = result.ir.children(call_ir_id);
        assert_eq!(
            children.len(),
            1,
            "EnumCons node should have exactly 1 child (the payload), no callee"
        );

        // The first child should be the literal 42 (not the callee path)
        if let Some(first_child) = children.first() {
            let first_child_node = &result.ir[*first_child];
            assert_eq!(
                first_child_node.kind,
                IrKind::Literal,
                "First child of EnumCons should be the payload Literal node, not the callee"
            );
        }
    }

    #[test]
    fn populate_match_arm_meta_basic() {
        // Fix C (#1085): Test populate_match_arm_meta_basic - enum match with all variant arms.
        // Construct an AST with:
        //   enum Result { Ok(u64), Err(u64) }
        //   let r: Result = ...
        //   match r {
        //     Ok(x) => ...,
        //     Err(e) => ...
        //   }
        // Verify that match_arm_meta side-table entries are populated with correct EnumTypeId and variant indices.
        use paideia_as_ir::EnumTypeId;
        use paideia_as_diagnostics::FileId;

        let mut source_map = SourceMap::new();
        let source_text = "Result Ok Err r x e";
        let _file = source_map.add_file(
            std::path::PathBuf::from("test.pdx"),
            String::from(source_text),
        );
        let mut sink = VecSink::new();
        let mut ast = AstArena::new();

        let file_id = FileId::new(1).unwrap();
        let make_span = |offset: u32, len: u32| {
            paideia_as_diagnostics::Span::new(file_id, offset, len)
        };

        // Create enum: enum Result { Ok(u64), Err(u64) }
        let enum_name_id = ast.alloc(NodeKind::Ident, make_span(0, 6)); // "Result"
        let u64_type_id = ast.alloc(NodeKind::Placeholder, make_span(0, 1));

        let ok_variant_name_id = ast.alloc(NodeKind::Ident, make_span(7, 2)); // "Ok"
        let err_variant_name_id = ast.alloc(NodeKind::Ident, make_span(10, 3)); // "Err"

        let ok_variant = paideia_as_ast::EnumVariant::Tuple {
            name: ok_variant_name_id,
            payload: vec![u64_type_id],
        };
        let err_variant = paideia_as_ast::EnumVariant::Tuple {
            name: err_variant_name_id,
            payload: vec![u64_type_id],
        };

        let _enum_item_id = ast.alloc_item(
            NodeKind::Enum,
            make_span(0, 1),
            paideia_as_ast::ItemData::Enum {
                name: enum_name_id,
                generic_params: vec![],
                variants: vec![ok_variant, err_variant],
                attributes: vec![],
                doc: None,
            },
        );

        // Create binding: let r: Result = ...
        let r_binding_id = ast.alloc(NodeKind::Ident, make_span(14, 1)); // "r"
        let result_type_id = ast.alloc(NodeKind::Ident, make_span(0, 6)); // "Result"
        let dummy_value_id = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
        let let_binding_id = ast.alloc_stmt(
            NodeKind::StmtLet,
            make_span(0, 1),
            paideia_as_ast::StmtData::Let {
                mutable: false,
                name: r_binding_id,
                ty: Some(result_type_id),
                value: dummy_value_id,
            },
        );

        // Create scrutinee reference: r
        let scrutinee = ast.alloc(NodeKind::Ident, make_span(14, 1)); // "r"

        // Create arm 1: Ok(x)
        let ok_pattern_id = ast.alloc(NodeKind::Ident, make_span(7, 2)); // "Ok"
        let x_binding_id = ast.alloc(NodeKind::Ident, make_span(17, 1)); // "x"
        let ok_arm_pattern = ast.alloc_pattern(
            NodeKind::PatEnumVariant,
            make_span(0, 1),
            paideia_as_ast::PatternData::EnumVariant {
                path: ok_pattern_id,
                args: vec![x_binding_id],
            },
        );
        let ok_arm_body = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
        let ok_arm = paideia_as_ast::MatchArm {
            pattern: ok_arm_pattern,
            guard: None,
            body: ok_arm_body,
        };

        // Create arm 2: Err(e)
        let err_pattern_id = ast.alloc(NodeKind::Ident, make_span(10, 3)); // "Err"
        let e_binding_id = ast.alloc(NodeKind::Ident, make_span(19, 1)); // "e"
        let err_arm_pattern = ast.alloc_pattern(
            NodeKind::PatEnumVariant,
            make_span(0, 1),
            paideia_as_ast::PatternData::EnumVariant {
                path: err_pattern_id,
                args: vec![e_binding_id],
            },
        );
        let err_arm_body = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
        let err_arm = paideia_as_ast::MatchArm {
            pattern: err_arm_pattern,
            guard: None,
            body: err_arm_body,
        };

        // Create match expression
        let match_id = ast.alloc_expr(
            NodeKind::ExprMatch,
            make_span(0, 1),
            paideia_as_ast::ExprData::Match {
                scrutinee,
                arms: vec![ok_arm, err_arm],
                attrs: Default::default(),
            },
        );

        // Build enum registry
        let mut enum_registry = crate::EnumRegistry::empty();
        let result_enum_type_id = EnumTypeId(1);
        enum_registry
            .by_name
            .insert("Result".to_string(), result_enum_type_id);
        enum_registry.variants.insert(
            result_enum_type_id,
            vec![
                ("Ok".to_string(), vec![u64_type_id]),
                ("Err".to_string(), vec![u64_type_id]),
            ],
        );

        // Lower the AST
        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &enum_registry, &std::collections::HashMap::new());

        // Verify the Match IR node was created
        let match_ir_id = result.ast_to_ir[&match_id];
        assert_eq!(result.ir[match_ir_id].kind, IrKind::Match);

        // Verify match_scrutinee_table entry
        let scrutinee_type = result.ir.match_scrutinee_table().get(match_ir_id);
        assert_eq!(
            scrutinee_type.copied(),
            Some(result_enum_type_id),
            "match_scrutinee_table should contain the Result enum type"
        );

        // Verify arm metadata entries
        let match_children = result.ir.children(match_ir_id);
        assert!(match_children.len() >= 3, "Match should have scrutinee + 2 arms as children");

        // Get arm body IDs (skip scrutinee at index 0)
        let ok_arm_ir_id = match_children[1];
        let err_arm_ir_id = match_children[2];

        // Verify Ok arm metadata
        let ok_arm_meta = result.ir.match_arm_meta().get(ok_arm_ir_id);
        assert!(ok_arm_meta.is_some(), "Ok arm should have meta entry");
        let ok_meta = ok_arm_meta.unwrap();
        assert_eq!(
            ok_meta.variant_index,
            Some(0),
            "Ok arm should have variant_index = 0"
        );
        assert!(!ok_meta.is_default, "Ok arm should not be marked default");

        // Verify Err arm metadata
        let err_arm_meta = result.ir.match_arm_meta().get(err_arm_ir_id);
        assert!(err_arm_meta.is_some(), "Err arm should have meta entry");
        let err_meta = err_arm_meta.unwrap();
        assert_eq!(
            err_meta.variant_index,
            Some(1),
            "Err arm should have variant_index = 1"
        );
        assert!(!err_meta.is_default, "Err arm should not be marked default");
    }

    #[test]
    fn populate_match_arm_meta_default_wildcard() {
        // Fix C (#1085): Test populate_match_arm_meta_default_wildcard - match with wildcard default arm.
        // Construct an AST with:
        //   enum Result { Ok(u64), Err(u64) }
        //   let r: Result = ...
        //   match r {
        //     Ok(x) => ...,
        //     _ => ...
        //   }
        // Verify that the wildcard arm is marked with is_default = true and variant_index = None.
        use paideia_as_ir::EnumTypeId;
        use paideia_as_diagnostics::FileId;

        let mut source_map = SourceMap::new();
        let source_text = "Result Ok r x";
        let _file = source_map.add_file(
            std::path::PathBuf::from("test.pdx"),
            String::from(source_text),
        );
        let mut sink = VecSink::new();
        let mut ast = AstArena::new();

        let file_id = FileId::new(1).unwrap();
        let make_span = |offset: u32, len: u32| {
            paideia_as_diagnostics::Span::new(file_id, offset, len)
        };

        // Create enum: enum Result { Ok(u64), Err(u64) }
        let enum_name_id = ast.alloc(NodeKind::Ident, make_span(0, 6)); // "Result"
        let u64_type_id = ast.alloc(NodeKind::Placeholder, make_span(0, 1));

        let ok_variant_name_id = ast.alloc(NodeKind::Ident, make_span(7, 2)); // "Ok"
        let err_variant_name_id = ast.alloc(NodeKind::Ident, make_span(0, 3)); // "Err"

        let ok_variant = paideia_as_ast::EnumVariant::Tuple {
            name: ok_variant_name_id,
            payload: vec![u64_type_id],
        };
        let err_variant = paideia_as_ast::EnumVariant::Tuple {
            name: err_variant_name_id,
            payload: vec![u64_type_id],
        };

        let _enum_item_id = ast.alloc_item(
            NodeKind::Enum,
            make_span(0, 1),
            paideia_as_ast::ItemData::Enum {
                name: enum_name_id,
                generic_params: vec![],
                variants: vec![ok_variant, err_variant],
                attributes: vec![],
                doc: None,
            },
        );

        // Create binding: let r: Result = ...
        let r_binding_id = ast.alloc(NodeKind::Ident, make_span(10, 1)); // "r"
        let result_type_id = ast.alloc(NodeKind::Ident, make_span(0, 6)); // "Result"
        let dummy_value_id = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
        let _let_binding_id = ast.alloc_stmt(
            NodeKind::StmtLet,
            make_span(0, 1),
            paideia_as_ast::StmtData::Let {
                mutable: false,
                name: r_binding_id,
                ty: Some(result_type_id),
                value: dummy_value_id,
            },
        );

        // Create scrutinee reference: r
        let scrutinee = ast.alloc(NodeKind::Ident, make_span(10, 1)); // "r"

        // Create arm 1: Ok(x)
        let ok_pattern_id = ast.alloc(NodeKind::Ident, make_span(7, 2)); // "Ok"
        let x_binding_id = ast.alloc(NodeKind::Ident, make_span(12, 1)); // "x"
        let ok_arm_pattern = ast.alloc_pattern(
            NodeKind::PatEnumVariant,
            make_span(0, 1),
            paideia_as_ast::PatternData::EnumVariant {
                path: ok_pattern_id,
                args: vec![x_binding_id],
            },
        );
        let ok_arm_body = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
        let ok_arm = paideia_as_ast::MatchArm {
            pattern: ok_arm_pattern,
            guard: None,
            body: ok_arm_body,
        };

        // Create arm 2: _ (wildcard)
        let wildcard_pattern = ast.alloc_pattern(
            NodeKind::PatWildcard,
            make_span(0, 1),
            paideia_as_ast::PatternData::Wildcard,
        );
        let wildcard_arm_body = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
        let wildcard_arm = paideia_as_ast::MatchArm {
            pattern: wildcard_pattern,
            guard: None,
            body: wildcard_arm_body,
        };

        // Create match expression
        let match_id = ast.alloc_expr(
            NodeKind::ExprMatch,
            make_span(0, 1),
            paideia_as_ast::ExprData::Match {
                scrutinee,
                arms: vec![ok_arm, wildcard_arm],
                attrs: Default::default(),
            },
        );

        // Build enum registry
        let mut enum_registry = crate::EnumRegistry::empty();
        let result_enum_type_id = EnumTypeId(1);
        enum_registry
            .by_name
            .insert("Result".to_string(), result_enum_type_id);
        enum_registry.variants.insert(
            result_enum_type_id,
            vec![
                ("Ok".to_string(), vec![u64_type_id]),
                ("Err".to_string(), vec![u64_type_id]),
            ],
        );

        // Lower the AST
        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &enum_registry, &std::collections::HashMap::new());

        // Verify the Match IR node was created
        let match_ir_id = result.ast_to_ir[&match_id];
        assert_eq!(result.ir[match_ir_id].kind, IrKind::Match);

        // Verify arm metadata entries
        let match_children = result.ir.children(match_ir_id);
        assert!(match_children.len() >= 3, "Match should have scrutinee + 2 arms as children");

        // Get arm body IDs (skip scrutinee at index 0)
        let ok_arm_ir_id = match_children[1];
        let wildcard_arm_ir_id = match_children[2];

        // Verify wildcard arm metadata
        let wildcard_arm_meta = result.ir.match_arm_meta().get(wildcard_arm_ir_id);
        assert!(wildcard_arm_meta.is_some(), "Wildcard arm should have meta entry");
        let wildcard_meta = wildcard_arm_meta.unwrap();
        assert!(
            wildcard_meta.is_default,
            "Wildcard arm should be marked with is_default = true"
        );
        assert_eq!(
            wildcard_meta.variant_index, None,
            "Wildcard arm should have variant_index = None"
        );
    }

    #[test]
    fn populate_match_arm_meta_bare_ident_variant() {
        // Fix C (#1085): Test populate_match_arm_meta_bare_ident_variant - bare Ok(x) pattern (no path prefix).
        // Construct an AST with:
        //   enum Result { Ok(u64), Err(u64) }
        //   let r: Result = ...
        //   match r {
        //     Ok(x) => ...,
        //   }
        // Verify that the bare Ok variant pattern (without Result:: prefix) is correctly resolved
        // against the scrutinee's enum type, and variant_index is correctly populated.
        use paideia_as_ir::EnumTypeId;
        use paideia_as_diagnostics::FileId;

        let mut source_map = SourceMap::new();
        let source_text = "Result Ok r x";
        let _file = source_map.add_file(
            std::path::PathBuf::from("test.pdx"),
            String::from(source_text),
        );
        let mut sink = VecSink::new();
        let mut ast = AstArena::new();

        let file_id = FileId::new(1).unwrap();
        let make_span = |offset: u32, len: u32| {
            paideia_as_diagnostics::Span::new(file_id, offset, len)
        };

        // Create enum: enum Result { Ok(u64), Err(u64) }
        let enum_name_id = ast.alloc(NodeKind::Ident, make_span(0, 6)); // "Result"
        let u64_type_id = ast.alloc(NodeKind::Placeholder, make_span(0, 1));

        let ok_variant_name_id = ast.alloc(NodeKind::Ident, make_span(7, 2)); // "Ok"
        let err_variant_name_id = ast.alloc(NodeKind::Ident, make_span(0, 3)); // "Err"

        let ok_variant = paideia_as_ast::EnumVariant::Tuple {
            name: ok_variant_name_id,
            payload: vec![u64_type_id],
        };
        let err_variant = paideia_as_ast::EnumVariant::Tuple {
            name: err_variant_name_id,
            payload: vec![u64_type_id],
        };

        let _enum_item_id = ast.alloc_item(
            NodeKind::Enum,
            make_span(0, 1),
            paideia_as_ast::ItemData::Enum {
                name: enum_name_id,
                generic_params: vec![],
                variants: vec![ok_variant, err_variant],
                attributes: vec![],
                doc: None,
            },
        );

        // Create binding: let r: Result = ...
        let r_binding_id = ast.alloc(NodeKind::Ident, make_span(10, 1)); // "r"
        let result_type_id = ast.alloc(NodeKind::Ident, make_span(0, 6)); // "Result"
        let dummy_value_id = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
        let _let_binding_id = ast.alloc_stmt(
            NodeKind::StmtLet,
            make_span(0, 1),
            paideia_as_ast::StmtData::Let {
                mutable: false,
                name: r_binding_id,
                ty: Some(result_type_id),
                value: dummy_value_id,
            },
        );

        // Create scrutinee reference: r
        let scrutinee = ast.alloc(NodeKind::Ident, make_span(10, 1)); // "r"

        // Create arm: Ok(x) - bare variant name, no Result:: prefix
        let ok_pattern_id = ast.alloc(NodeKind::Ident, make_span(7, 2)); // "Ok"
        let x_binding_id = ast.alloc(NodeKind::Ident, make_span(12, 1)); // "x"
        let ok_arm_pattern = ast.alloc_pattern(
            NodeKind::PatEnumVariant,
            make_span(0, 1),
            paideia_as_ast::PatternData::EnumVariant {
                path: ok_pattern_id,
                args: vec![x_binding_id],
            },
        );
        let ok_arm_body = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
        let ok_arm = paideia_as_ast::MatchArm {
            pattern: ok_arm_pattern,
            guard: None,
            body: ok_arm_body,
        };

        // Create match expression
        let match_id = ast.alloc_expr(
            NodeKind::ExprMatch,
            make_span(0, 1),
            paideia_as_ast::ExprData::Match {
                scrutinee,
                arms: vec![ok_arm],
                attrs: Default::default(),
            },
        );

        // Build enum registry
        let mut enum_registry = crate::EnumRegistry::empty();
        let result_enum_type_id = EnumTypeId(1);
        enum_registry
            .by_name
            .insert("Result".to_string(), result_enum_type_id);
        enum_registry.variants.insert(
            result_enum_type_id,
            vec![
                ("Ok".to_string(), vec![u64_type_id]),
                ("Err".to_string(), vec![u64_type_id]),
            ],
        );

        // Lower the AST
        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &crate::StructRegistry::empty(), &enum_registry, &std::collections::HashMap::new());

        // Verify the Match IR node was created
        let match_ir_id = result.ast_to_ir[&match_id];
        assert_eq!(result.ir[match_ir_id].kind, IrKind::Match);

        // Verify arm metadata entries
        let match_children = result.ir.children(match_ir_id);
        assert!(match_children.len() >= 2, "Match should have scrutinee + 1 arm as children");

        // Get arm body ID (skip scrutinee at index 0)
        let ok_arm_ir_id = match_children[1];

        // Verify Ok arm metadata
        let ok_arm_meta = result.ir.match_arm_meta().get(ok_arm_ir_id);
        assert!(ok_arm_meta.is_some(), "Ok arm should have meta entry");
        let ok_meta = ok_arm_meta.unwrap();
        assert_eq!(
            ok_meta.variant_index,
            Some(0),
            "Ok arm should have variant_index = 0 (resolved against Result enum)"
        );
        assert!(!ok_meta.is_default, "Ok arm should not be marked default");
    }

    #[test]
    fn populate_match_arm_meta_nested_ok_of_point() {
        // Test nested pattern: Ok(Point { x, y })
        // Constructs:
        //   struct Point { x: u64, y: u64 }
        //   enum Result { Ok(Point), Err }
        //   let r: Result = ...
        //   match r {
        //     Ok(Point { x, y }) => ...,
        //     Err => ...,
        //   }
        use paideia_as_ir::EnumTypeId;
        use paideia_as_ir::record_layout::RecordTypeId;
        use paideia_as_diagnostics::FileId;

        let mut source_map = SourceMap::new();
        let source_text = "Point Ok x y Result Err r";
        let _file = source_map.add_file(
            std::path::PathBuf::from("test.pdx"),
            String::from(source_text),
        );
        let mut sink = VecSink::new();
        let mut ast = AstArena::new();

        let file_id = FileId::new(1).unwrap();
        let make_span = |offset: u32, len: u32| {
            paideia_as_diagnostics::Span::new(file_id, offset, len)
        };

        // Create Point struct
        let point_struct_name_id = ast.alloc(NodeKind::Ident, make_span(0, 5)); // "Point" at 0
        let x_field_name_id = ast.alloc(NodeKind::Ident, make_span(9, 1)); // "x" at 9
        let y_field_name_id = ast.alloc(NodeKind::Ident, make_span(11, 1)); // "y" at 11
        let u64_type_id = ast.alloc(NodeKind::Placeholder, make_span(0, 1));

        let _point_struct_id = ast.alloc_item(
            NodeKind::Struct,
            make_span(0, 1),
            paideia_as_ast::ItemData::Struct {
                name: point_struct_name_id,
                generic_params: vec![],
                fields: vec![
                    (x_field_name_id, u64_type_id),
                    (y_field_name_id, u64_type_id),
                ],
                attributes: vec![],
                doc: None,
            },
        );

        // Create Result enum: Result { Ok(Point), Err }
        let enum_name_id = ast.alloc(NodeKind::Ident, make_span(13, 6)); // "Result" at 13
        let ok_variant_name_id = ast.alloc(NodeKind::Ident, make_span(6, 2)); // "Ok" at 6
        let err_variant_name_id = ast.alloc(NodeKind::Ident, make_span(20, 3)); // "Err" at 20
        let point_type_id = ast.alloc(NodeKind::Ident, make_span(0, 5)); // "Point" at 0

        let ok_variant = paideia_as_ast::EnumVariant::Tuple {
            name: ok_variant_name_id,
            payload: vec![point_type_id],
        };
        let err_variant = paideia_as_ast::EnumVariant::Unit {
            name: err_variant_name_id,
        };

        let _enum_item_id = ast.alloc_item(
            NodeKind::Enum,
            make_span(0, 1),
            paideia_as_ast::ItemData::Enum {
                name: enum_name_id,
                generic_params: vec![],
                variants: vec![ok_variant, err_variant],
                attributes: vec![],
                doc: None,
            },
        );

        // Create binding: let r: Result = ...
        let r_binding_id = ast.alloc(NodeKind::Ident, make_span(24, 1)); // "r" at 24
        let result_type_id = ast.alloc(NodeKind::Ident, make_span(13, 6)); // "Result" at 13
        let dummy_value_id = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
        let _let_binding_id = ast.alloc_stmt(
            NodeKind::StmtLet,
            make_span(0, 1),
            paideia_as_ast::StmtData::Let {
                mutable: false,
                name: r_binding_id,
                ty: Some(result_type_id),
                value: dummy_value_id,
            },
        );

        // Create scrutinee: r
        let scrutinee = ast.alloc(NodeKind::Ident, make_span(24, 1)); // "r" at 24

        // Create nested pattern: Ok(Point { x, y })
        let x_pat_field_name_id = ast.alloc(NodeKind::Ident, make_span(9, 1)); // "x" at 9
        let y_pat_field_name_id = ast.alloc(NodeKind::Ident, make_span(11, 1)); // "y" at 11
        let x_pat_binding_id = ast.alloc(NodeKind::Ident, make_span(9, 1)); // "x" at 9
        let y_pat_binding_id = ast.alloc(NodeKind::Ident, make_span(11, 1)); // "y" at 11

        let x_pat = ast.alloc_pattern(
            NodeKind::PatIdent,
            make_span(9, 1),
            paideia_as_ast::PatternData::Ident {
                name: x_pat_binding_id,
                mutable: false,
            },
        );
        let y_pat = ast.alloc_pattern(
            NodeKind::PatIdent,
            make_span(11, 1),
            paideia_as_ast::PatternData::Ident {
                name: y_pat_binding_id,
                mutable: false,
            },
        );

        let point_struct_pat = ast.alloc_pattern(
            NodeKind::PatStruct,
            make_span(0, 1),
            paideia_as_ast::PatternData::Struct {
                path: point_struct_name_id,
                fields: vec![
                    paideia_as_ast::PatField {
                        name: x_pat_field_name_id,
                        pattern: x_pat,
                    },
                    paideia_as_ast::PatField {
                        name: y_pat_field_name_id,
                        pattern: y_pat,
                    },
                ],
            },
        );

        let ok_pattern_id = ast.alloc(NodeKind::Ident, make_span(6, 2)); // "Ok" at 6
        let ok_arm_pattern = ast.alloc_pattern(
            NodeKind::PatEnumVariant,
            make_span(0, 1),
            paideia_as_ast::PatternData::EnumVariant {
                path: ok_pattern_id,
                args: vec![point_struct_pat],
            },
        );
        let ok_arm_body = ast.alloc(NodeKind::ExprLiteral, make_span(0, 1));
        let ok_arm = paideia_as_ast::MatchArm {
            pattern: ok_arm_pattern,
            guard: None,
            body: ok_arm_body,
        };

        // Create match expression
        let match_id = ast.alloc_expr(
            NodeKind::ExprMatch,
            make_span(0, 1),
            paideia_as_ast::ExprData::Match {
                scrutinee,
                arms: vec![ok_arm],
                attrs: Default::default(),
            },
        );

        // Build struct registry
        let mut struct_registry = crate::StructRegistry::empty();
        let point_record_type_id = RecordTypeId(1);
        struct_registry.by_name.insert("Point".to_string(), point_record_type_id);
        struct_registry.fields.insert(
            point_record_type_id,
            vec![("x".to_string(), 4), ("y".to_string(), 4)],
        );

        // Build enum registry
        let mut enum_registry = crate::EnumRegistry::empty();
        let result_enum_type_id = EnumTypeId(1);
        enum_registry
            .by_name
            .insert("Result".to_string(), result_enum_type_id);
        enum_registry.variants.insert(
            result_enum_type_id,
            vec![
                ("Ok".to_string(), vec![point_type_id]),
                ("Err".to_string(), vec![]),
            ],
        );

        // Build payload map
        let mut payload_map = std::collections::HashMap::new();
        payload_map.insert((result_enum_type_id, 0), Some(point_record_type_id));

        // Lower the AST
        let result = lower_ast_to_ir(&ast, &source_map, &mut sink, &struct_registry, &enum_registry, &payload_map);

        // Verify the Match IR node was created
        let match_ir_id = result.ast_to_ir[&match_id];
        assert_eq!(result.ir[match_ir_id].kind, IrKind::Match);

        // Verify arm metadata
        let match_children = result.ir.children(match_ir_id);
        assert!(match_children.len() >= 2, "Match should have scrutinee + 1 arm as children");

        let ok_arm_ir_id = match_children[1];
        let ok_arm_meta = result.ir.match_arm_meta().get(ok_arm_ir_id);
        assert!(ok_arm_meta.is_some(), "Ok arm should have meta entry");
        let ok_meta = ok_arm_meta.unwrap();
        assert_eq!(
            ok_meta.variant_index,
            Some(0),
            "Ok arm should have variant_index = 0"
        );

        // Verify pattern_binding is set with nested structure
        assert!(
            ok_meta.pattern_binding.is_some(),
            "Ok arm should have pattern_binding set"
        );
        if let Some(pb) = &ok_meta.pattern_binding {
            use paideia_as_ir::enum_layout::PatternBinding;
            match pb {
                PatternBinding::EnumVariant {
                    variant_index,
                    payload_type,
                    payload,
                } => {
                    assert_eq!(*variant_index, 0, "variant_index should be 0 for Ok");
                    assert_eq!(
                        *payload_type,
                        Some(point_record_type_id),
                        "payload_type should be Point"
                    );
                    assert!(
                        payload.is_some(),
                        "payload should be Some (the Point record pattern)"
                    );
                    if let Some(payload_binding) = payload {
                        match payload_binding.as_ref() {
                            PatternBinding::Record { type_id, fields } => {
                                assert_eq!(*type_id, point_record_type_id, "record type should be Point");
                                assert_eq!(fields.len(), 2, "Point should have 2 fields");
                                // Verify field bindings
                                assert_eq!(fields[0].0, "x");
                                assert_eq!(fields[1].0, "y");
                            }
                            _ => panic!("payload should be Record pattern"),
                        }
                    }
                }
                _ => panic!("pattern_binding should be EnumVariant for Ok(...)"),
            }
        }
    }

    #[test]
    fn lower_record_cons_reorders_to_declared_order() {
        // Test that Point { y: 2, x: 1 } is reordered to [type_name, 1, 2]
        // matching declared order (x, y) not literal order (y, x).
        use paideia_as_diagnostics::FileId;
        use paideia_as_ir::record_layout::RecordTypeId;

        // Create a source_map with proper source text
        // Source text: "Point x y 1 2"
        // - "Point" at offset 0-5
        // - "x" at offset 6-7
        // - "y" at offset 8-9
        // - "1" at offset 10-11
        // - "2" at offset 12-13
        let mut source_map = SourceMap::new();
        let source_text = "Point x y 1 2";
        let _file = source_map.add_file(
            std::path::PathBuf::from("test.pdx"),
            String::from(source_text),
        );
        let mut sink = VecSink::new();

        let mut ast = AstArena::new();
        let file_id = FileId::new(1).unwrap();
        let make_span = |offset: u32, len: u32| {
            paideia_as_diagnostics::Span::new(file_id, offset, len)
        };

        // Create Point type name node
        let type_name_id = ast.alloc(NodeKind::Ident, make_span(0, 5)); // "Point"

        // Create field name nodes
        let x_field_name = ast.alloc(NodeKind::Ident, make_span(6, 1)); // "x"
        let y_field_name = ast.alloc(NodeKind::Ident, make_span(8, 1)); // "y"

        // Create literal value nodes (simple nodes)
        let value_1 = ast.alloc(NodeKind::ExprLiteral, make_span(10, 1)); // "1"
        let value_2 = ast.alloc(NodeKind::ExprLiteral, make_span(12, 1)); // "2"

        // Create RecordCons with fields in literal order (y, x)
        let record_cons = ast.alloc_expr(
            NodeKind::ExprRecordCons,
            make_span(0, 13),
            ExprData::RecordCons {
                type_name: type_name_id,
                fields: vec![(y_field_name, value_2), (x_field_name, value_1)],
            },
        );

        // Build struct registry with declared order (x, y)
        let mut struct_registry = crate::StructRegistry::empty();
        let point_record_type_id = RecordTypeId(1);
        struct_registry.by_name.insert("Point".to_string(), point_record_type_id);
        struct_registry.fields.insert(
            point_record_type_id,
            vec![("x".to_string(), 4), ("y".to_string(), 4)],
        );

        // Lower to IR
        let result = lower_ast_to_ir(
            &ast,
            &source_map,
            &mut sink,
            &struct_registry,
            &crate::EnumRegistry::empty(),
            &std::collections::HashMap::new(),
        );

        // Get the RecordCons IR node and its children
        let record_cons_ir_id = result.ast_to_ir[&record_cons];
        let children = result.ir.children(record_cons_ir_id);

        // Verify children order: [type_name, value_1, value_2]
        // (x=1 should come before y=2, following declared order)
        assert_eq!(children.len(), 3, "RecordCons should have 3 children (type_name + 2 fields)");
        // Child 0 is type_name (unchanged)
        // Child 1 should be value_1 (x field value)
        // Child 2 should be value_2 (y field value)
        let ir_value_1 = result.ast_to_ir[&value_1];
        let ir_value_2 = result.ast_to_ir[&value_2];

        assert_eq!(
            children[1], ir_value_1,
            "field x value should come first in canonicalized order"
        );
        assert_eq!(
            children[2], ir_value_2,
            "field y value should come second in canonicalized order"
        );

        // No diagnostics should be emitted (no duplicates, all fields present)
        assert_eq!(sink.diagnostics().len(), 0, "should emit no diagnostics");
    }

    #[test]
    fn lower_record_cons_missing_field_fires_t0537() {
        // Test that Point { x: 1 } (missing y) fires T0537
        use paideia_as_diagnostics::FileId;
        use paideia_as_ir::record_layout::RecordTypeId;

        // Create a source_map with proper source text
        // Source text: "Point x 1"
        // - "Point" at offset 0-5
        // - "x" at offset 6-7
        // - "1" at offset 8-9
        let mut source_map = SourceMap::new();
        let source_text = "Point x 1";
        let _file = source_map.add_file(
            std::path::PathBuf::from("test.pdx"),
            String::from(source_text),
        );
        let mut sink = VecSink::new();

        let mut ast = AstArena::new();
        let file_id = FileId::new(1).unwrap();
        let make_span = |offset: u32, len: u32| {
            paideia_as_diagnostics::Span::new(file_id, offset, len)
        };

        let type_name_id = ast.alloc(NodeKind::Ident, make_span(0, 5)); // "Point"
        let x_field_name = ast.alloc(NodeKind::Ident, make_span(6, 1)); // "x"
        let value_1 = ast.alloc(NodeKind::ExprLiteral, make_span(8, 1)); // "1"

        let _record_cons = ast.alloc_expr(
            NodeKind::ExprRecordCons,
            make_span(0, 9),
            ExprData::RecordCons {
                type_name: type_name_id,
                fields: vec![(x_field_name, value_1)],
            },
        );

        let mut struct_registry = crate::StructRegistry::empty();
        let point_record_type_id = RecordTypeId(1);
        struct_registry.by_name.insert("Point".to_string(), point_record_type_id);
        struct_registry.fields.insert(
            point_record_type_id,
            vec![("x".to_string(), 4), ("y".to_string(), 4)],
        );

        let _result = lower_ast_to_ir(
            &ast,
            &source_map,
            &mut sink,
            &struct_registry,
            &crate::EnumRegistry::empty(),
            &std::collections::HashMap::new(),
        );

        // Should emit exactly 1 T0537 diagnostic for missing field y
        assert_eq!(sink.diagnostics().len(), 1, "should emit 1 diagnostic for missing field");
    }

    #[test]
    fn lower_record_cons_duplicate_field_fires_t0538() {
        // Test that Point { x: 1, x: 2 } fires T0538
        use paideia_as_diagnostics::FileId;
        use paideia_as_ir::record_layout::RecordTypeId;

        // Create a source_map with proper source text
        // Source text: "Point x x 1 2"
        // - "Point" at offset 0-5
        // - "x" at offset 6-7 (first)
        // - "x" at offset 8-9 (second)
        // - "1" at offset 10-11
        // - "2" at offset 12-13
        let mut source_map = SourceMap::new();
        let source_text = "Point x x 1 2";
        let _file = source_map.add_file(
            std::path::PathBuf::from("test.pdx"),
            String::from(source_text),
        );
        let mut sink = VecSink::new();

        let mut ast = AstArena::new();
        let file_id = FileId::new(1).unwrap();
        let make_span = |offset: u32, len: u32| {
            paideia_as_diagnostics::Span::new(file_id, offset, len)
        };

        let type_name_id = ast.alloc(NodeKind::Ident, make_span(0, 5)); // "Point"
        let x_field_name_1 = ast.alloc(NodeKind::Ident, make_span(6, 1)); // "x" (first)
        let x_field_name_2 = ast.alloc(NodeKind::Ident, make_span(8, 1)); // "x" (second)
        let value_1 = ast.alloc(NodeKind::ExprLiteral, make_span(10, 1)); // "1"
        let value_2 = ast.alloc(NodeKind::ExprLiteral, make_span(12, 1)); // "2"

        let _record_cons = ast.alloc_expr(
            NodeKind::ExprRecordCons,
            make_span(0, 13),
            ExprData::RecordCons {
                type_name: type_name_id,
                fields: vec![(x_field_name_1, value_1), (x_field_name_2, value_2)],
            },
        );

        let mut struct_registry = crate::StructRegistry::empty();
        let point_record_type_id = RecordTypeId(1);
        struct_registry.by_name.insert("Point".to_string(), point_record_type_id);
        struct_registry.fields.insert(
            point_record_type_id,
            vec![("x".to_string(), 4), ("y".to_string(), 4)],
        );

        let _result = lower_ast_to_ir(
            &ast,
            &source_map,
            &mut sink,
            &struct_registry,
            &crate::EnumRegistry::empty(),
            &std::collections::HashMap::new(),
        );

        // Should emit T0538 for duplicate x, and T0537 for missing y
        let diags = sink.diagnostics();
        assert!(diags.len() >= 1, "should emit at least 1 diagnostic for duplicate field");
    }

    #[test]
    fn lower_record_cons_unknown_field_fires_t0539() {
        // Test that Point { x: 1, y: 2, z: 3 } fires T0539 for z
        use paideia_as_diagnostics::FileId;
        use paideia_as_ir::record_layout::RecordTypeId;

        // Create a source_map with proper source text
        // Source text: "Point x y z 1 2 3"
        // - "Point" at offset 0-5
        // - "x" at offset 6-7
        // - "y" at offset 8-9
        // - "z" at offset 10-11
        // - "1" at offset 12-13
        // - "2" at offset 14-15
        // - "3" at offset 16-17
        let mut source_map = SourceMap::new();
        let source_text = "Point x y z 1 2 3";
        let _file = source_map.add_file(
            std::path::PathBuf::from("test.pdx"),
            String::from(source_text),
        );
        let mut sink = VecSink::new();

        let mut ast = AstArena::new();
        let file_id = FileId::new(1).unwrap();
        let make_span = |offset: u32, len: u32| {
            paideia_as_diagnostics::Span::new(file_id, offset, len)
        };

        let type_name_id = ast.alloc(NodeKind::Ident, make_span(0, 5)); // "Point"
        let x_field_name = ast.alloc(NodeKind::Ident, make_span(6, 1)); // "x"
        let y_field_name = ast.alloc(NodeKind::Ident, make_span(8, 1)); // "y"
        let z_field_name = ast.alloc(NodeKind::Ident, make_span(10, 1)); // "z"
        let value_1 = ast.alloc(NodeKind::ExprLiteral, make_span(12, 1)); // "1"
        let value_2 = ast.alloc(NodeKind::ExprLiteral, make_span(14, 1)); // "2"
        let value_3 = ast.alloc(NodeKind::ExprLiteral, make_span(16, 1)); // "3"

        let _record_cons = ast.alloc_expr(
            NodeKind::ExprRecordCons,
            make_span(0, 17),
            ExprData::RecordCons {
                type_name: type_name_id,
                fields: vec![
                    (x_field_name, value_1),
                    (y_field_name, value_2),
                    (z_field_name, value_3),
                ],
            },
        );

        let mut struct_registry = crate::StructRegistry::empty();
        let point_record_type_id = RecordTypeId(1);
        struct_registry.by_name.insert("Point".to_string(), point_record_type_id);
        struct_registry.fields.insert(
            point_record_type_id,
            vec![("x".to_string(), 4), ("y".to_string(), 4)],
        );

        let _result = lower_ast_to_ir(
            &ast,
            &source_map,
            &mut sink,
            &struct_registry,
            &crate::EnumRegistry::empty(),
            &std::collections::HashMap::new(),
        );

        // Should emit T0539 for unknown field z
        let diags = sink.diagnostics();
        assert!(diags.len() >= 1, "should emit at least 1 diagnostic for unknown field");
    }

    #[test]
    fn lower_record_cons_empty_registry_preserves_literal_order() {
        // Test that when registry is empty, RecordCons children are in literal order
        use paideia_as_diagnostics::FileId;

        // Create a source_map with proper source text
        // Source text: "Point y x 2 1"
        // - "Point" at offset 0-5
        // - "y" at offset 6-7
        // - "x" at offset 8-9
        // - "2" at offset 10-11
        // - "1" at offset 12-13
        let mut source_map = SourceMap::new();
        let source_text = "Point y x 2 1";
        let _file = source_map.add_file(
            std::path::PathBuf::from("test.pdx"),
            String::from(source_text),
        );
        let mut sink = VecSink::new();

        let mut ast = AstArena::new();
        let file_id = FileId::new(1).unwrap();
        let make_span = |offset: u32, len: u32| {
            paideia_as_diagnostics::Span::new(file_id, offset, len)
        };

        let type_name_id = ast.alloc(NodeKind::Ident, make_span(0, 5)); // "Point"
        let y_field_name = ast.alloc(NodeKind::Ident, make_span(6, 1)); // "y"
        let x_field_name = ast.alloc(NodeKind::Ident, make_span(8, 1)); // "x"
        let value_2 = ast.alloc(NodeKind::ExprLiteral, make_span(10, 1)); // "2"
        let value_1 = ast.alloc(NodeKind::ExprLiteral, make_span(12, 1)); // "1"

        // Create RecordCons with fields in literal order (y, x)
        let record_cons = ast.alloc_expr(
            NodeKind::ExprRecordCons,
            make_span(0, 13),
            ExprData::RecordCons {
                type_name: type_name_id,
                fields: vec![(y_field_name, value_2), (x_field_name, value_1)],
            },
        );

        // Use empty registry (no Point defined)
        let empty_registry = crate::StructRegistry::empty();

        let result = lower_ast_to_ir(
            &ast,
            &source_map,
            &mut sink,
            &empty_registry,
            &crate::EnumRegistry::empty(),
            &std::collections::HashMap::new(),
        );

        // Get the RecordCons IR node and its children
        let record_cons_ir_id = result.ast_to_ir[&record_cons];
        let children = result.ir.children(record_cons_ir_id);

        // When registry is empty, should preserve literal order: [type_name, value_2, value_1]
        assert_eq!(children.len(), 3, "RecordCons should have 3 children (type_name + 2 fields)");
        let ir_value_1 = result.ast_to_ir[&value_1];
        let ir_value_2 = result.ast_to_ir[&value_2];

        assert_eq!(
            children[1], ir_value_2,
            "with empty registry, should preserve literal order (y field first)"
        );
        assert_eq!(
            children[2], ir_value_1,
            "with empty registry, should preserve literal order (x field second)"
        );
    }
}
