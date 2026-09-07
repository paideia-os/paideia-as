//! Walker pipeline: LinearityWalker → EffectRowWalker → CapWalker → EmitWalker
//! → UnsafeWalker → resolve_var_operands.
//!
//! Extracted from `cmd_build.rs` (2026-09-07 refactor, issue #1401).
//! Runs against a non-empty IR arena; drains all walker diagnostics into the
//! caller-provided sink at the end. Mutates `lowering.ir` and `emit_walker`
//! in place; produces no return value.
//!
//! Also hosts `find_outermost_root` (issue #994), which is called only from
//! this phase to locate the true outermost IR node (the enclosing Module)
//! for the walker driver.

use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{DiagnosticSink, FileId, SourceMap, VecSink};
use paideia_as_elaborator::{
    CapWalker, EffectRowWalker, EmitWalker, LinearityWalker, LoweringResult, UnsafeWalker,
};
use paideia_as_ir::{IrKind, IrNodeId, walk};

use crate::resolve_var_operands;

use super::identifier::{is_known_operator, is_valid_identifier, is_valid_qualified_identifier};
use super::root_attrs::{extract_root_module_bits, extract_root_module_features};

/// Issue #994: find the arena's true outermost node — the one node that is
/// not listed as a child of any other node.
///
/// `lower_ast_to_ir` mirrors the AST's own node-allocation order 1-1, and
/// this parser allocates a construct's children before the construct itself
/// (bottom-up), so the enclosing Module ends up as one of the *last*
/// IDs, not `IrNodeId::new(1)`. Returns `None` only for a degenerate arena
/// where no such node exists (e.g. empty, or every node is referenced —
/// which would indicate a cycle and should not occur for a real program).
pub(super) fn find_outermost_root(ir: &paideia_as_ir::IrArena) -> Option<IrNodeId> {
    let len = ir.len() as u32;
    if len == 0 {
        return None;
    }
    let mut is_child = vec![false; (len + 1) as usize];
    for i in 1..=len {
        if let Some(id) = IrNodeId::new(i) {
            for &child in ir.children(id) {
                let idx = child.get() as usize;
                if idx < is_child.len() {
                    is_child[idx] = true;
                }
            }
        }
    }
    // Prefer the highest-numbered unreferenced node: the outermost wrapper
    // is constructed last under this parser's bottom-up allocation order.
    (1..=len).rev().find(|&i| !is_child[i as usize]).and_then(IrNodeId::new)
}

/// Run walkers over the IR to surface S/F/C diagnostics.
///
/// Walkers now traverse the real IR subtree; diagnostic firing remains gated
/// by payload injection (m3/m5). See issue #1237 for root-walker fix and
/// design/paideia-as/non-milestone-issue-1237-cmd-build-root-walker.md.
///
/// Phase-5-m1-005: EmitWalker chains into the walker pipeline and populates
/// InstructionSideTable for downstream emit stages.
///
/// This function is a no-op when `lowering.ir.is_empty()`.
pub(super) fn run_walker_pipeline(
    arena: &AstArena,
    root_id: Option<paideia_as_ast::NodeId>,
    source_map: &SourceMap,
    file: FileId,
    registry: &paideia_as_elaborator::StructRegistry,
    lowering: &mut LoweringResult,
    emit_walker: &mut EmitWalker,
    sink: &mut VecSink,
) {
    if lowering.ir.is_empty() {
        return;
    }

    // Create a walker sink to accumulate diagnostics from all walkers.
    let mut walker_sink = VecSink::new();

    // PA-R12-004 (#913): control-flow support added in PA-R17-012 (#990)
    // Pure fn bodies can now contain if/match/while/loop; T0532 stub retired.

    // Determine the real root node ID for walking. The parser allocates
    // constructs' children before the construct itself (bottom-up), so
    // the enclosing Module ends up as one of the *last*-allocated nodes,
    // not IrNodeId::new(1). See issue #1237 and
    // design/paideia-as/non-milestone-issue-1237-cmd-build-root-walker.md.
    if let Some(walker_root) = find_outermost_root(&lowering.ir) {
        // Run each walker with a fresh WalkerCtx to avoid borrow conflicts.
        // Each walker emits diagnostics into walker_sink.

        // 1. LinearityWalker: drain analyzed captures from the same instance.
        {
            let mut ctx = paideia_as_ir::WalkerCtx::new(source_map, &mut walker_sink);
            let mut linearity_walker = LinearityWalker::new();
            walk(&mut linearity_walker, &lowering.ir, walker_root, &mut ctx);

            // Drain the walker's per-lambda capture analysis into the IR
            // arena's CapturesTable side-table. The IrWalker trait only
            // exposes an immutable &IrArena to post_visit, so
            // LinearityWalker cannot populate arena.captures_mut()
            // in-place during the walk itself; instead it accumulates
            // results internally and we drain them here.
            for (lambda_raw_id, captured_bindings) in linearity_walker.into_analyzed_captures() {
                if let Some(lambda_id) = IrNodeId::new(lambda_raw_id) {
                    let analyzed: Vec<paideia_as_ir::AnalyzedCapture> = captured_bindings
                        .iter()
                        .map(|c| paideia_as_ir::AnalyzedCapture {
                            symbol: c.symbol,
                            kind: match c.kind {
                                paideia_as_elaborator::CaptureKind::Reference => 0,
                                paideia_as_elaborator::CaptureKind::Value => 1,
                                paideia_as_elaborator::CaptureKind::Consume => 2,
                            },
                        })
                        .collect();
                    lowering.ir.captures_mut().insert(lambda_id, analyzed);
                }
            }
        }

        // Issue #994 piece 2: convert function-local closure-typed lets
        // (`let f: |T| -> R = |x| ...;`) from a bare Lambda RHS into a
        // ClosureCons-wrapped Lambda, and fire T0538 for fn-ptr-typed
        // lambdas that actually capture free variables. Must run after
        // the capture-table drain above (needs arena.captures()) and
        // before EmitWalker::walk (register_closure_body_symbols /
        // precompute_caller_frame assume ClosureCons nodes already exist).
        paideia_as_elaborator::convert_closure_lets(
            arena,
            &mut lowering.ir,
            &lowering.ast_to_ir,
            &mut walker_sink,
        );

        // 2. EffectRowWalker
        {
            let mut ctx = paideia_as_ir::WalkerCtx::new(source_map, &mut walker_sink);
            let mut effect_walker = EffectRowWalker::new();
            walk(&mut effect_walker, &lowering.ir, walker_root, &mut ctx);
        }

        // 3. CapWalker
        {
            let mut ctx = paideia_as_ir::WalkerCtx::new(source_map, &mut walker_sink);
            let mut cap_walker = CapWalker::new();
            walk(&mut cap_walker, &lowering.ir, walker_root, &mut ctx);
        }

        // Phase-5-m1-005: Run EmitWalker to populate InstructionSideTable.
        // EmitWalker does not use the walker framework (it uses direct arena iteration),
        // so we call its walk method directly rather than through the walk() driver.

        // Phase 15 m2-002a: Extract root module's #![bits = N] and set initial mode.
        // root_id is the AST root from parsing; it's in scope here.
        let root_mode = extract_root_module_bits(root_id, arena)
            .map(|bits| {
                if bits == 32 {
                    paideia_as_ir::instruction::InstrMode::Mode32
                } else {
                    paideia_as_ir::instruction::InstrMode::Mode64
                }
            })
            .unwrap_or(paideia_as_ir::instruction::InstrMode::Mode64);
        emit_walker.set_root_mode(root_mode);

        // PA-r16-004-backtrack-a (#1033): Extract root module's #![target_features = "..."]
        // and set enabled CPU features for feature gating.
        let features = extract_root_module_features(root_id, arena, source_map, file);
        emit_walker.state_mut().set_enabled_features(features);

        // PA-r17-010c (#1072): populate finalised record layouts from the
        // StructRegistry so visit_record_cons + emit_store_record can consume them.
        emit_walker.state_mut().finalise_record_layouts(&registry.fields);

        // Issue #1157: Mirror finalised record layouts into arena for tight-pack encoding.
        // After state.finalise_record_layouts populates registry state, copy into arena
        // so data_encoder::encode_record_cons can access layouts during static init encoding.
        for (record_type_id, record_layout) in emit_walker.state().record_layouts() {
            lowering.ir.finalised_record_layouts_mut().insert(*record_type_id, record_layout.clone());
        }

        // PA-r17-007 (#1050): Mirror enum layouts from IR into walker state.
        // This enables visit_enum_cons + emit_enum_discriminant to consume layouts during emission.
        for (type_id, layout) in lowering.ir.enum_layout_table().iter() {
            emit_walker.state_mut().insert_enum_layout(*type_id, layout.clone());
        }

        // PA-r17-004: Pre-emit pass to populate call_sites metadata for App nodes.
        // Walk IR to find all App nodes and extract callee names, storing metadata
        // in the CallSideTable for later dispatch in emit_walker.
        {
            let content_ref = source_map.content(file);

            // Collect all App node metadata in a separate pass to avoid borrow conflicts
            let mut app_metadata = Vec::new();

            {
                let ir_arena = &lowering.ir;

                // Walk all IR nodes to find App nodes
                for ir_idx in 0..ir_arena.len() {
                    if let Some(ir_id) = IrNodeId::new((ir_idx + 1) as u32) {
                        if let Some(ir_node) = ir_arena.get(ir_id) {
                            if ir_node.kind == IrKind::App {
                                let app_children = ir_arena.children(ir_id);

                                // App structure: [callee, arg0, arg1, ...]
                                if !app_children.is_empty() {
                                    let callee_id = app_children[0];
                                    let arg_count = (app_children.len() - 1) as u32;

                                    // Extract callee name from the callee node's span
                                    if let Some(callee_node) = ir_arena.get(callee_id) {
                                        let span = callee_node.span;
                                        let start = span.byte_start() as usize;
                                        let len = span.byte_len() as usize;
                                        if start + len <= content_ref.len() {
                                            let callee_text = content_ref[start..start + len].to_string();

                                            // Only record if it's a valid identifier, a qualified path
                                            // (issue #1290: `TraitName::method`), or a known operator.
                                            if is_valid_identifier(&callee_text)
                                                || is_valid_qualified_identifier(&callee_text)
                                                || is_known_operator(&callee_text)
                                            {
                                                app_metadata.push((ir_id, callee_text, arg_count));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Now insert all collected metadata into the IR's call_sites table
            for (ir_id, callee_name, arg_count) in app_metadata {
                let call_meta = paideia_as_ir::call_meta::CallMeta {
                    callee_name,
                    arg_count,
                    is_intrinsic: false,
                };
                lowering.ir.call_sites_mut().insert(ir_id, call_meta);
            }
        }

        // PA19-r19-006: Pre-populate lambda_abi in emit_walker state before walk.
        // This ensures that when Lambda nodes are visited during walk (which may occur
        // before their Let binding is processed, if the Lambda has a lower node ID),
        // the ABI is already available for lookup.
        for i in 0..arena.len() {
            if let Some(ast_id) = paideia_as_ast::NodeId::new((i + 1) as u32) {
                if let Some(node) = arena.get(ast_id) {
                    if node.kind == paideia_as_ast::NodeKind::Let {
                        if let Some(paideia_as_ast::ItemData::Let {
                            abi: Some(cc),
                            value: value_id,
                            ..
                        }) = arena.item_data(ast_id)
                        {
                            // Convert AST CallingConvention to IR CallingConvention
                            let ir_cc = match cc {
                                paideia_as_ast::CallingConvention::Ms => paideia_as_ir::CallingConvention::Ms,
                                paideia_as_ast::CallingConvention::Sysv => paideia_as_ir::CallingConvention::Sysv,
                            };
                            // The value (RHS of Let, usually a Lambda) has the same numeric ID in IR
                            let ir_lambda_id = value_id.get();
                            emit_walker.state_mut().insert_lambda_abi(ir_lambda_id, ir_cc);
                        }
                    }
                }
            }
        }

        // #1131: Populate the IR's data table with module-level Let bindings.
        // This enables emit_walker to skip Mov emission for Let nodes that are
        // already handled by data section lowering, preventing spurious .text
        // emission before function bodies.
        //
        // We temporarily detach the data_table from the arena to avoid borrow checker issues.
        let mut temp_data_table = std::mem::take(lowering.ir.data_mut());
        paideia_as_elaborator::EmitWalker::populate_data_table(&lowering.ir, &mut temp_data_table);
        *lowering.ir.data_mut() = temp_data_table;

        // Issue #1219 populated let_meta.ty; consumption at walk() awaits a follow-up
        // that flips to walk_with_typer to activate resolve_let_width in production.
        emit_walker.walk(&mut lowering.ir);

        // Phase 15 m2-002: Verify mode_stack is properly cleaned up after walk.
        debug_assert!(
            emit_walker.state().mode_stack_is_empty()
                || emit_walker.state().mode_stack_len() == 1,
            "EmitWalker mode_stack should be empty or have 1 entry at end of walk; got {}",
            emit_walker.state().mode_stack_len()
        );

        // Refactor 2026-07-07 Step 3 (FLOOR): drain the canonical typed
        // diagnostic pipe from EmitWalker into walker_sink so no new
        // T-code can be silently discarded. The legacy `Vec<String>`
        // buffer accessed via `emit_walker.diagnostics()` still exists
        // but is not drained here — retirement is a follow-up.
        for diag in emit_walker.take_typed_diagnostics() {
            let _ = walker_sink.emit(diag);
        }

        // Issue #1082: drain the legacy Vec<String> channel through U1616.
        // Post-#1086 migration this holds only non-T-coded internal errors
        // (invariant violations, missing side-tables, unpopulated layouts).
        // Any fire == silent-broken-.o class bug; emit as error.
        for msg in emit_walker.take_legacy_diagnostics() {
            let code = paideia_as_diagnostics::DiagnosticCode::new(
                paideia_as_diagnostics::Category::U,
                paideia_as_diagnostics::Severity::Error,
                1616,
            ).expect("valid U1616 code");
            let diag = paideia_as_diagnostics::Diagnostic::error(code)
                .message(msg)
                .finish();
            let _ = walker_sink.emit(diag);
        }

        // Phase-5-m3-005: Run UnsafeWalker to elaborate pending unsafe blocks.
        // Take pending unsafe blocks from EmitWalker state and process them.
        let pending = emit_walker.state_mut().take_pending_unsafe();
        // Issue #1088: Clone pending for emit_pending_unsafe_bodies (after UnsafeWalker).
        let pending_for_ir_emit = pending.clone();
        // #1139: Extract and clone the data we need before calling UnsafeWalker to avoid borrow conflicts.
        let record_layouts = emit_walker.state().record_layouts().clone();
        let local_bindings = emit_walker.state().local_bindings().clone();
        let enabled_features = emit_walker.state().enabled_features().clone();
        let unsafe_body_to_lambda = emit_walker.state().unsafe_body_to_lambda().clone();
        // Issue #1244: Extract mutable references before call to avoid borrow conflicts
        // Issue #1270: also thread the StmtExpr order-base reservation map.
        let (instr_to_lambda_ref, emission_order_ref, stmt_expr_order_base_ref) =
            emit_walker.state_mut().unsafe_walker_refs();
        let (unsafe_labels, label_to_instr, first_instrs, unsafe_diags) = UnsafeWalker::run(
            &mut lowering.ir,
            arena,
            pending,
            source_map,
            &mut walker_sink,
            &record_layouts,
            &local_bindings,
            root_mode,
            &enabled_features,
            &unsafe_body_to_lambda,
            instr_to_lambda_ref,
            emission_order_ref,
            stmt_expr_order_base_ref,
        );

        // Register collected unsafe block labels with emit_walker state
        for (label_name, _label_offset) in unsafe_labels {
            emit_walker.state_mut().register_label(label_name);
        }

        // Store label_to_instr mapping for use in label offset computation after encoding
        // (We'll use this to resolve label offsets based on instruction offsets from offset_map)
        emit_walker.state_mut().set_label_to_instr(label_to_instr);

        // PA8-m1-002b: Wire first_instrs back to lambda_first_instr for unsafe lambdas.
        // first_instrs[i] is the first instruction of the i-th pending unsafe block.
        // We look up which lambda corresponds to that pending index via unsafe_lambda_to_pending_idx.
        //
        // paideia-as#1278 phase 2: for lambdas marked `@interrupt(...)`,
        // the ISR entry point is the very first `push rax` of the 13-push
        // spill (already recorded at `emit_interrupt_prologue` time via
        // `record_lambda_entry`), NOT the first raw instruction of the
        // unsafe body. Overwriting `lambda_first_instr` with the body's
        // first instruction here would land the ELF symbol INSIDE the
        // spill chain — the CPU's IDT dispatch would skip the spill and
        // iretq would pop random values off the stack. Skip the overwrite
        // for interrupt lambdas so the prologue's record stands.
        {
            let pending_idx_map: Vec<_> = emit_walker
                .state()
                .unsafe_lambda_to_pending_idx()
                .iter()
                .map(|(&lambda_id, &idx)| (lambda_id, idx))
                .collect();
            for (lambda_id, idx) in pending_idx_map {
                if emit_walker.state().lambda_interrupt(lambda_id).is_some() {
                    continue;
                }
                if let Some(Some(first_instr)) = first_instrs.get(idx) {
                    emit_walker
                        .state_mut()
                        .insert_lambda_first_instr(lambda_id, *first_instr);
                }
            }
        }

        // Phase 7 m4-003: Emit pending unsafe-block statement bodies.
        // Issue #1088: After UnsafeWalker processes raw instructions and labels,
        // emit any pending action statements (call expressions, etc.) through the
        // standard IR emit pipeline.
        emit_walker.emit_pending_unsafe_bodies(
            pending_for_ir_emit,
            &mut lowering.ir,
            None,
        );
        for diag in emit_walker.take_typed_diagnostics() {
            let _ = walker_sink.emit(diag);
        }

        // paideia-as#1278 phase 2: emit the ISR entry-stub tail for every
        // lambda marked `@interrupt(...)` / `@interrupt_error(...)` —
        // 13-pop GPR restore + optional `add rsp, 8` errcode-skip +
        // `iretq`. Must run AFTER the two body-emission passes above so
        // the tail sorts strictly last in each ISR function's .text range
        // (the text emitter sorts by `(emission_order, node_id)`; the tail
        // instructions here allocate emission_order values above every
        // body instruction). The matching 13-push + `cld` prologue was
        // already emitted during `walk()` at Lambda entry.
        emit_walker.emit_interrupt_epilogues(&mut lowering.ir);
        for diag in emit_walker.take_typed_diagnostics() {
            let _ = walker_sink.emit(diag);
        }

        // Phase-7-m2-003: Resolve Operand::Var references to Operand::Reg.
        // Call resolve_var_operands on the arena's owned instruction table,
        // then re-clone for the encoder pipeline.
        // PA10-005 §3.5: Thread SymbolTable through for T0531 diagnostic.
        //
        // Refactor 2026-07-07 Step 2: resolve_var_operands now returns
        // typed `Diagnostic` values (was `Vec<String>` requiring a
        // string re-parse to recover the T-code). This retires the
        // fragile `msg_str.find(':')` decoder and the fabricated `700`
        // catch-all that hid diagnostic-catalog mismatches.
        {
            let symbol_table_clone = lowering.ir.symbols().clone();
            let bindings = emit_walker.state().local_bindings();
            let per_lambda_bindings = emit_walker.state().per_lambda_bindings();
            let instr_to_lambda = emit_walker.state().instr_to_lambda();
            let mut resolve_diags: Vec<paideia_as_diagnostics::Diagnostic> = Vec::new();
            resolve_var_operands::resolve_var_operands(
                lowering.ir.instructions_mut(),
                bindings,
                Some(symbol_table_clone),
                &mut resolve_diags,
                per_lambda_bindings,
                instr_to_lambda,
            );
            for diag in resolve_diags {
                let _ = walker_sink.emit(diag);
            }
        }

        for d in unsafe_diags {
            let _ = walker_sink.emit(d);
        }
    }

    // Drain walker diagnostics into the main sink for rendering.
    for d in walker_sink.into_diagnostics() {
        let _ = sink.emit(d);
    }
}
