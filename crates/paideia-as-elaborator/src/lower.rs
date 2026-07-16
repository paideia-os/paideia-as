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
//!
//! # Internal structure
//!
//! Refactored 2026-07-08 from a single 4031-line file into a `lower/`
//! submodule directory. Public API is unchanged (`LoweringResult`,
//! `lower_ast_to_ir`); every internal helper is `pub(super)`-scoped so it
//! cannot leak out of the `lower` module.

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind};
use paideia_as_diagnostics::{DiagnosticSink, SourceMap};
use paideia_as_ir::{IrArena, IrKind};
use std::collections::HashMap;

use paideia_as_ir::IrNodeId;

// --- Internal submodules ---
mod array_repeat;
mod children;
mod enum_cons;
mod enum_lit_var;
mod field_access;
mod kind_map;
mod match_arm;
mod match_auto_dispatch;
mod match_dispatch;
mod pattern_data;
mod record_cons;
mod record_layout;
mod store_lvalue;
mod text_extract;
mod unsafe_scan;

#[cfg(test)]
mod tests;

use children::collect_ast_children;
use enum_cons::populate_enum_cons_info;
use enum_lit_var::populate_enum_lit_var_rewrites;
use field_access::populate_field_access_info;
use kind_map::map_node_kind;
use match_arm::populate_match_arm_meta;
use match_auto_dispatch::populate_auto_jump_table_meta;
use match_dispatch::populate_match_dispatch_meta;
use record_layout::populate_record_layout_table;
use store_lvalue::is_lvalue_infix_assignment;
use unsafe_scan::collect_nodes_in_unsafe_blocks;

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

    // Pre-pass: collect Loop/While nodes that live inside unsafe blocks.
    let nodes_in_unsafe_blocks = collect_nodes_in_unsafe_blocks(ast);

    // First pass: allocate all IR nodes (without children).
    for i in 0..ast.len() {
        // NodeId and IrNodeId both index from 1.
        let ast_id = NodeId::new((i + 1) as u32).expect("non-zero node id");
        let node = &ast[ast_id];
        let ir_kind = refine_ir_kind(node, ast, ast_id, source_map);
        let ir_id = ir.alloc(ir_kind, node.span);
        ast_to_ir.insert(ast_id, ir_id);

        // Issue #1212: Track statement-scope Let nodes for gating data-section emission.
        // Only `NodeKind::StmtLet` (function-local) are added; `NodeKind::Let` (module-scope) are not.
        if node.kind == NodeKind::StmtLet {
            ir.stmt_lets_mut().insert(ir_id);
        }
    }

    // Second pass: transfer structure (children) from AST to IR.
    for i in 0..ast.len() {
        let ast_id = NodeId::new((i + 1) as u32).expect("non-zero node id");
        let ir_id = ast_to_ir[&ast_id];

        let ast_children = collect_ast_children(
            ast,
            &ir,
            ast_id,
            ir_id,
            source_map,
            sink,
            registry,
            &nodes_in_unsafe_blocks,
        );

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
    populate_match_dispatch_meta(ast, &mut ir, &ast_to_ir, source_map, sink);

    // PA-r17-010a (#1070): Populate RecordLayoutTable for RecordCons expressions.
    populate_record_layout_table(ast, &mut ir, &ast_to_ir, registry, source_map, sink);

    // Phase 6 m3-002 (#1073): Populate FieldAccessSideTable for FieldAccess expressions.
    populate_field_access_info(ast, &mut ir, &ast_to_ir, registry, source_map, sink);

    // Phase 7 m4-003 (#1048/#1049): Populate EnumConsInfoTable for EnumCons expressions.
    populate_enum_cons_info(ast, &mut ir, &ast_to_ir, enum_registry, source_map, sink);

    // Issue #1198: Rewrite bare enum-variant Vars (unit-payload only) to EnumCons.
    populate_enum_lit_var_rewrites(&mut ir, ast, &ast_to_ir, enum_registry, source_map, sink);

    // Phase 7 m9-009 (#1081/#1082): Populate MatchArmMeta for match arms and
    // match_scrutinee_table for match expressions.
    populate_match_arm_meta(
        ast,
        &mut ir,
        &ast_to_ir,
        enum_registry,
        registry,
        payload_map,
        source_map,
        sink,
    );

    // Issue #1052: Auto-detect dense enum-variant matches and inject MatchDispatchMeta.
    // Must run AFTER populate_match_arm_meta (needs variant_index) and BEFORE emit walker.
    populate_auto_jump_table_meta(&mut ir);

    // PA19-r19-006: Populate let_meta with ABI and other metadata from AST Let nodes.
    // This enables the elaborator to look up calling conventions for lambda bindings.
    populate_let_meta(ast, &mut ir, &ast_to_ir);

    // Phase-5-m1-001: Literal values are populated by cmd_build.rs Phase-5-m1-001 walk
    // before emit_walker::walk() runs. No need to duplicate that work here.

    LoweringResult { ir, ast_to_ir }
}

/// First-pass `IrKind` classifier: `map_node_kind` plus a few refinements
/// that depend on `ExprData` payload (BitNot prefix, Store lvalue, Loop/While).
fn refine_ir_kind(node: &paideia_as_ast::NodeData, ast: &AstArena, ast_id: NodeId, source_map: &SourceMap) -> IrKind {
    let mut ir_kind = map_node_kind(node.kind);

    // Phase 7 m4-001: prefix `~` lowers to a dedicated `IrKind::BitNot`
    // rather than the generic `App`. `map_node_kind` only sees the AST
    // node kind, so refine the bucket here where the `PrefixOp` payload is
    // available. `!`/`-`/other keep the generic `App` mapping.
    if node.kind == NodeKind::ExprPrefix {
        if let Some(ExprData::Prefix {
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
    if node.kind == NodeKind::ExprInfix {
        if let Some(ExprData::Infix { op, lhs, .. }) = ast.expr_data(ast_id) {
            let op_node = &ast[*op];
            // Guard: check that the operator text is exactly "=" and LHS is a valid l-value
            let is_eq = {
                let content = source_map.content(op_node.span.file());
                let start = op_node.span.byte_start() as usize;
                let len = op_node.span.byte_len() as usize;
                if start + len <= content.len() {
                    &content[start..start + len] == "="
                } else {
                    false
                }
            };
            if is_eq && is_lvalue_infix_assignment(ast, *lhs) {
                ir_kind = IrKind::Store;
            }
        }
    }

    // PA-R17-012: ExprLoop distinction (infinite loop vs while).
    // Loop (infinite) → IrKind::Loop
    // While (conditional) → IrKind::While
    if node.kind == NodeKind::ExprLoop {
        if let Some(ExprData::Loop { kind, .. }) = ast.expr_data(ast_id) {
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

    ir_kind
}

/// PA19-r19-006: Populate let_meta with ABI and other metadata from AST Let nodes.
///
/// This pass scans the AST for Let nodes with ABI annotations and populates the
/// corresponding let_meta entries in the IR arena. This enables the elaborator's
/// emit_walker to look up calling conventions for lambda bindings during type lowering.
fn populate_let_meta(
    ast: &paideia_as_ast::AstArena,
    ir: &mut paideia_as_ir::IrArena,
    ast_to_ir: &std::collections::HashMap<paideia_as_ast::NodeId, paideia_as_ir::IrNodeId>,
) {
    use paideia_as_ast::{NodeId, ItemData};
    use paideia_as_ir::let_meta::{LetInfo, CallingConvention};

    for i in 0..ast.len() {
        if let Some(ast_id) = NodeId::new((i + 1) as u32) {
            if let Some(node) = ast.get(ast_id) {
                if node.kind == paideia_as_ast::NodeKind::Let {
                    if let Some(ItemData::Let { abi, value: value_id, .. }) = ast.item_data(ast_id) {
                        // Convert AST CallingConvention to IR CallingConvention
                        let ir_abi = abi.map(|cc| {
                            use paideia_as_ast::CallingConvention as AstCC;
                            match cc {
                                AstCC::Ms => CallingConvention::Ms,
                                AstCC::Sysv => CallingConvention::Sysv,
                            }
                        });

                        // If there's an ABI annotation, look up the RHS (value) in AST
                        if let Some(ir_abi_val) = ir_abi {
                            if let Some(value_node) = ast.get(*value_id) {
                                // If the RHS is a Lambda, record the ABI for the Lambda's IR node
                                if value_node.kind == paideia_as_ast::NodeKind::ExprLambda {
                                    if let Some(_lambda_ir_id) = ast_to_ir.get(value_id) {
                                        // Look up the Let's IR node to potentially update its metadata
                                        if let Some(let_ir_id) = ast_to_ir.get(&ast_id) {
                                            // Create or update LetInfo with ABI on the Let node
                                            let mut let_info = ir.let_meta_mut()
                                                .get(*let_ir_id)
                                                .cloned()
                                                .unwrap_or_else(|| LetInfo {
                                                    mutable: false,
                                                    ty: None,
                                                    align: None,
                                                    ring: None,
                                                    link_section: None,
                                                    abi: None,
                                                });
                                            let_info.abi = Some(ir_abi_val);
                                            ir.let_meta_mut().insert(*let_ir_id, let_info);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
