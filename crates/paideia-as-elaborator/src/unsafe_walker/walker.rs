//! `UnsafeWalker` driver: two-pass label collection + statement walk for each
//! pending unsafe body.
//! Split out of `unsafe_walker.rs` (paideia-as #1403).

use std::collections::{HashMap, HashSet};

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind, StmtData};
use paideia_as_diagnostics::{
    Category, Diagnostic, DiagnosticCode, DiagnosticSink, Severity,
};
use paideia_as_ir::instruction::{CpuFeature, InstrMode, Instruction, Mnemonic};
use paideia_as_ir::record_layout::{RecordLayout, RecordTypeId};
use paideia_as_ir::{IrArena, IrNodeId};

use crate::LocalBindingTable;
use super::diag::{u_code, U_DUPLICATE_LABEL, U_UNSUPPORTED_STMT_IN_UNSAFE};

/// #1270: number of `emission_order` ticks reserved for each `StmtExpr`
/// (call-expression) statement inside an unsafe block at the point it's
/// encountered by `UnsafeWalker::run`'s raw-instruction pass.
///
/// The statement's *real* instructions (arg marshalling + CALL, possibly
/// bridge push/pop, possibly a frame prologue) are synthesized later by
/// `emit_pending_unsafe_bodies` / `emit_call_args_and_call`; this reserved
/// gap guarantees room for all of them to land at their true source
/// position without colliding with the next reserved statement's range.
/// Any realistic call-lowering sequence is well under 100 instructions, so
/// this leaves a wide safety margin.
pub(super) const STMT_EXPR_ORDER_RESERVE: u32 = 4096;

/// UnsafeWalker — Phase 5 m3-004 elaborator for unsafe blocks.
///
/// Walks pending unsafe blocks (collected by EmitWalker m1-004) and emits
/// `Instruction` entries into the IR's InstructionSideTable. For each
/// `StmtInstruction` in the block, resolves the mnemonic and parses all operands,
/// then inserts an `Instruction` keyed by the statement's IrNodeId.
pub struct UnsafeWalker;

impl UnsafeWalker {
    /// Run the unsafe walker on a set of pending unsafe blocks.
    ///
    /// # Arguments
    ///
    /// * `arena` - The IR arena containing the unsafe block nodes.
    /// * `ast` - The AST arena containing the block's statement data.
    /// * `pending_ids` - IrNodeIds of IrKind::Unsafe nodes to elaborate.
    /// * `source_map` - The source map for resolving file content from spans.
    /// * `sink` - Diagnostic sink for emitting errors.
    /// * `record_layouts` - Record layout table for field offset resolution (Phase 6 m3-005).
    /// * `local_bindings` - LocalBindingTable from EmitWalker (Phase 7 m2-003): maps let-binding names to scratch registers.
    /// * `instr_mode` - The current instruction mode (Mode64 or Mode32).
    ///
    /// # Returns
    ///
    /// A vector of diagnostics emitted during elaboration.
    ///
    /// # Side effects
    ///
    /// Mutates `arena.instructions_mut()` to insert Instruction entries.
    ///
    /// # Returns
    ///
    /// A tuple of (labels_map, diagnostics) where labels_map contains all collected
    /// local labels from the unsafe blocks (populated during label collection pass).
    pub fn run(
        arena: &mut IrArena,
        ast: &AstArena,
        pending_ids: Vec<u32>,
        source_map: &paideia_as_diagnostics::SourceMap,
        sink: &mut dyn DiagnosticSink,
        record_layouts: &HashMap<RecordTypeId, RecordLayout>,
        local_bindings: &LocalBindingTable,
        instr_mode: InstrMode,
        enabled_features: &HashSet<CpuFeature>,
        unsafe_body_to_lambda: &HashMap<u32, u32>,
        instr_to_lambda: &mut HashMap<IrNodeId, u32>,
        next_emission_order: &mut u32,
        stmt_expr_order_base: &mut HashMap<(u32, usize), u32>,
    ) -> (
        HashMap<String, u32>,
        HashMap<String, paideia_as_ir::IrNodeId>,
        Vec<Option<IrNodeId>>,
        Vec<Diagnostic>,
    ) {
        let mut diags = Vec::new();
        let mut all_labels: HashMap<String, u32> = HashMap::new();
        let mut label_to_instr: HashMap<String, paideia_as_ir::IrNodeId> = HashMap::new();
        let mut first_instrs: Vec<Option<IrNodeId>> = Vec::new();

        // Track which ExprUnsafe we've processed to avoid N×M cross-product.
        // Each pending IR node ID corresponds to exactly one ExprUnsafe in source order.
        let mut unsafe_block_idx = 0;
        for ir_node_id_u32 in pending_ids {
            let _ir_node_id = match IrNodeId::new(ir_node_id_u32) {
                Some(id) => id,
                None => continue,
            };
            // #1139: Look up the lambda_id for this unsafe body.
            let owning_lambda_id = unsafe_body_to_lambda.get(&ir_node_id_u32).copied();

            // Get the IR node to find the AST node it references.
            // The IR node for Unsafe should have been constructed during lowering.
            // We need to find the corresponding AST node via the elaborator's
            // lowering tables (typically stored in a context struct).
            // For this phase, we assume that the unsafe block's AST node ID
            // can be derived or is passed via context. Placeholder: search in AST.

            // Scan the AST for ExprUnsafe nodes in source order.
            // Match the Nth ExprUnsafe to the Nth pending IR node ID (one-to-one correspondence).
            // This ensures each unsafe block is processed exactly once.
            let mut current_unsafe_idx = 0;
            for ast_idx in 1..=ast.len() {
                if let Some(ast_node_id) = NodeId::new(ast_idx as u32) {
                    if let Some(ast_node) = ast.get(ast_node_id) {
                        if ast_node.kind == NodeKind::ExprUnsafe {
                            // Check if this ExprUnsafe matches our target index
                            if current_unsafe_idx == unsafe_block_idx {
                                // Found our target ExprUnsafe; process it.
                                if let Some(ExprData::Unsafe { block, .. }) =
                                    ast.expr_data(ast_node_id)
                                {
                                    // Phase 6 m4-002: Two-pass processing for labels.
                                    // Pass 1: Collect all label declarations into a HashMap.
                                    let mut labels: HashMap<String, u32> = HashMap::new();
                                    for &stmt_id in block {
                                        if let Some(ast_stmt_node) = ast.get(stmt_id) {
                                            if ast_stmt_node.kind == NodeKind::StmtLabel {
                                                // Collect label: extract label name from StmtData::Label
                                                if let Some(StmtData::Label { name }) =
                                                    ast.stmt_data(stmt_id)
                                                {
                                                    if let Some(name_node) = ast.get(*name) {
                                                        if name_node.kind == NodeKind::Ident {
                                                            // Extract the label name from source
                                                            let span = name_node.span;
                                                            let file_id = span.file();
                                                            let source =
                                                                source_map.content(file_id);
                                                            let label_text =
                                                                &source[span.byte_start() as usize
                                                                    ..(span.byte_start()
                                                                        + span.byte_len())
                                                                        as usize];
                                                            // Check for duplicate label (U1609)
                                                            if labels.contains_key(label_text) {
                                                                let diag = Diagnostic::error(u_code(
                                                                    U_DUPLICATE_LABEL,
                                                                ))
                                                                .message(format!(
                                                                    "duplicate label declaration: {}",
                                                                    label_text
                                                                ))
                                                                .with_span(span)
                                                                .finish();
                                                                let _ = sink.emit(diag.clone());
                                                                diags.push(diag);
                                                            } else {
                                                                // Store label with a placeholder byte offset (0 for now)
                                                                labels.insert(
                                                                    label_text.to_string(),
                                                                    0,
                                                                );
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // Collect labels from this unsafe block into all_labels
                                    for (label_name, offset) in labels.iter() {
                                        all_labels.insert(label_name.clone(), *offset);
                                    }

                                    // Pass 2: Process instructions and check label references.
                                    // PA-R13-011 (#924): pending_labels holds every label that has been
                                    // declared since the last instruction; when the next instruction lands
                                    // they ALL alias to it (same byte offset). This lets back-to-back labels
                                    // (`label1: label2: mov ...;`) both resolve.
                                    let mut pending_labels: Vec<String> = Vec::new();
                                    let mut block_first_instr: Option<IrNodeId> = None;
                                    for (stmt_idx, &stmt_id) in block.iter().enumerate() {
                                        if let Some(ast_stmt_node) = ast.get(stmt_id) {
                                            if ast_stmt_node.kind == NodeKind::StmtLabel {
                                                if let Some(StmtData::Label { name }) =
                                                    ast.stmt_data(stmt_id)
                                                {
                                                    if let Some(name_node) = ast.get(*name) {
                                                        if name_node.kind == NodeKind::Ident {
                                                            let span = name_node.span;
                                                            let source =
                                                                source_map.content(span.file());
                                                            let label_text =
                                                                &source[span.byte_start() as usize
                                                                    ..(span.byte_start()
                                                                        + span.byte_len())
                                                                        as usize];
                                                            pending_labels.push(label_text.to_string());
                                                        }
                                                    }
                                                }
                                            } else if ast_stmt_node.kind
                                                == NodeKind::StmtInstruction
                                            {
                                                // Process this instruction statement.
                                                // Issue #1244: Pass next_emission_order to thread counter
                                                let instr_ir_node = Self::process_instruction_stmt(
                                                    arena,
                                                    ast,
                                                    stmt_id,
                                                    &mut diags,
                                                    sink,
                                                    source_map,
                                                    record_layouts,
                                                    &labels,
                                                    local_bindings,
                                                    instr_mode,
                                                    enabled_features,
                                                    owning_lambda_id,
                                                    instr_to_lambda,
                                                    next_emission_order,
                                                );

                                                // Track first instruction of this unsafe block
                                                if block_first_instr.is_none() {
                                                    block_first_instr = instr_ir_node;
                                                }

                                                // Alias every pending label to this instruction.
                                                match instr_ir_node {
                                                    Some(instr_id) => {
                                                        for label_name in pending_labels.drain(..) {
                                                            label_to_instr.insert(label_name, instr_id);
                                                        }
                                                    }
                                                    None => {
                                                        // Encoding failed for this instruction — mirror the
                                                        // pre-existing behaviour of losing the preceding label
                                                        // rather than mis-attaching it to the *next* instruction.
                                                        pending_labels.clear();
                                                    }
                                                }
                                            } else if ast_stmt_node.kind == NodeKind::StmtExpr {
                                                // Issue #1088: StmtExpr (call expressions, field access, etc.)
                                                // are routable via the emit pipeline; their real instructions
                                                // are synthesized later by emit_pending_unsafe_bodies.
                                                //
                                                // Issue #1270: previously this arm did nothing else, which
                                                // left the statement with NO reserved emission_order and no
                                                // resolvable position for a preceding label. Its instructions
                                                // would only get an emission_order once ALL unsafe blocks in
                                                // the whole file had already been lowered by UnsafeWalker::run
                                                // — sorting it to the very end of `.text` (even after its own
                                                // function's `ret`) regardless of its true source position,
                                                // and leaving any label immediately before it either dropped
                                                // or mis-aliased to a much later raw instruction (corrupting
                                                // jcc/jmp fixup targets). Reserve a real position for it now,
                                                // at its true program point, and insert a 1-byte NOP marker
                                                // so labels resolve correctly; emit_pending_unsafe_bodies
                                                // resumes emission_order from this reserved base when it
                                                // later lowers the statement's actual instructions.
                                                let reserved_base = *next_emission_order;
                                                *next_emission_order += STMT_EXPR_ORDER_RESERVE;
                                                stmt_expr_order_base
                                                    .insert((ir_node_id_u32, stmt_idx), reserved_base);

                                                // Only materialize a physical 1-byte NOP marker when
                                                // there's a label that actually needs a resolvable
                                                // target here. The common case — a call/field-write
                                                // with no preceding label — needs the emission_order
                                                // reservation above (so its real instructions land in
                                                // the right position) but does NOT need an extra byte
                                                // in `.text` (see #1146's redundant-load-elimination
                                                // tests, which assert exact byte counts).
                                                if !pending_labels.is_empty() {
                                                    let marker_span = ast
                                                        .get(stmt_id)
                                                        .map(|n| n.span)
                                                        .unwrap_or_else(|| {
                                                            paideia_as_diagnostics::Span::new(
                                                                paideia_as_diagnostics::FileId::new(1)
                                                                    .unwrap(),
                                                                0,
                                                                1,
                                                            )
                                                        });
                                                    let marker_id = arena.alloc(
                                                        paideia_as_ir::IrKind::Placeholder,
                                                        marker_span,
                                                    );
                                                    let marker_inst = Instruction {
                                                        mnemonic: Mnemonic::Nop,
                                                        operands: Default::default(),
                                                        encoding_hint: None,
                                                        byte_offset_in_text: None,
                                                        mode: instr_mode,
                                                        emission_order: reserved_base,
                                                    };
                                                    arena.instructions_mut().insert(marker_id, marker_inst);
                                                    if let Some(lambda_id) = owning_lambda_id {
                                                        instr_to_lambda.insert(marker_id, lambda_id);
                                                    }
                                                    if block_first_instr.is_none() {
                                                        block_first_instr = Some(marker_id);
                                                    }
                                                    for label_name in pending_labels.drain(..) {
                                                        label_to_instr.insert(label_name, marker_id);
                                                    }
                                                }

                                                if cfg!(debug_assertions) {
                                                    eprintln!(
                                                        "[unsafe_walker] StmtExpr in unsafe block reserved emission_order base {reserved_base} (real instructions deferred to emit pipeline)"
                                                    );
                                                }
                                            } else {
                                                // U1614: Unsupported statement kind in unsafe block
                                                // (see #1088 for follow-up on broadening unsafe-block coverage)
                                                let stmt_kind = ast
                                                    .get(stmt_id)
                                                    .map(|n| n.kind)
                                                    .unwrap_or(NodeKind::Placeholder);
                                                let stmt_span = ast
                                                    .get(stmt_id)
                                                    .map(|n| n.span)
                                                    .unwrap_or_else(|| {
                                                        paideia_as_diagnostics::Span::new(
                                                            paideia_as_diagnostics::FileId::new(1)
                                                                .unwrap(),
                                                            0,
                                                            1,
                                                        )
                                                    });
                                                let diag = Diagnostic::error(
                                                    DiagnosticCode::new(
                                                        Category::U,
                                                        Severity::Error,
                                                        U_UNSUPPORTED_STMT_IN_UNSAFE,
                                                    )
                                                    .expect("valid U1614 code"),
                                                )
                                                .message(format!(
                                                    "unsupported statement in unsafe block: {:?} — only asm mnemonics and labels are emitted today",
                                                    stmt_kind
                                                ))
                                                .with_span(stmt_span)
                                                .finish();
                                                let _ = sink.emit(diag.clone());
                                                diags.push(diag);
                                            }
                                        }
                                    }
                                    // Record the first instruction for this unsafe block
                                    first_instrs.push(block_first_instr);
                                }
                                // After processing this unsafe block, break and move to the next pending ID.
                                break;
                            }
                            current_unsafe_idx += 1;
                        }
                    }
                }
            }
            unsafe_block_idx += 1;
        }

        (all_labels, label_to_instr, first_instrs, diags)
    }
}
