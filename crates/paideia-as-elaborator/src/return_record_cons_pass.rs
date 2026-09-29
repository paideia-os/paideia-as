//! Populator for `CallerSretSlotTable` and `CallerSretFrameBumpTable`
//! — the caller-side persistent-frame-slot side-tables that let a
//! record-returning call's sret buffer live past the CALL so the
//! caller can dereference fields out of it.
//!
//! PAS-DEBT-B4-002 Slice C (paideia-as#1554). Consumes Slice A's
//! `IrArena::return_record_layout_table` (indirectly via
//! `Symbol::return_record_layout`) and Slice B's caller-side sret
//! wiring in `emit_call.rs`.
//!
//! # Piece coverage
//!
//! * **Piece 3 (persistent caller slot)** — this pass. Walks every
//!   Lambda; for each descendant `App` whose callee has a
//!   record return type with `Memory` placement (or a register
//!   placement whose result we want to spill), allocates a
//!   non-overlapping slot on the caller Lambda's frame.
//!
//! * **Piece 1 (return-position record-cons materialisation)** —
//!   landed in Slice D (v0.36.71). `emit_visit_lambda.rs` grew an
//!   `IrKind::RecordCons` body arm that calls `emit_ret`, and
//!   `emit_walker/emit_core.rs::emit_callee_sret_splice` now
//!   populates the source buffer inline by walking the RecordCons
//!   children in step with the callee's `return_record_layout.fields`
//!   (Literal → `mov [rsp+off], imm`; Var → `mov [rsp+off], reg` via
//!   `local_bindings`). The Slice C `sub rsp, padded_size` scaffold
//!   is retained as the buffer allocation; Slice D writes into it
//!   before the aggregate-return helper reads it back.
//!
//! # Slot layout policy
//!
//! Slots are assigned per caller Lambda, packed downward from `RBP`
//! (each slot's `rbp_disp` is negative). Within a caller, slots for
//! distinct App nodes never overlap: the first App claims
//! `[RBP - padded_size_0]`, the second `[RBP - padded_size_0 -
//! padded_size_1]`, and so on. The per-Lambda total bump lands in
//! [`CallerSretFrameBumpTable`] so the prologue emitter can reserve
//! the whole area at once with a single `sub rsp, total`.
//!
//! # Absent-side-table policy
//!
//! Callers whose enclosing Lambda contains no record-returning App
//! get no entry in either table. `emit_call.rs`'s Slice B fallback
//! path (transient `sub/lea/add`) fires unchanged for such callers —
//! preserving byte-identical behaviour for the historical corpus,
//! including the Slice B fixture tests.
//!
//! # Slice E — recipe-callee participation (paideia-as#1554)
//!
//! PAS-DEBT-B4-002 Slice E extends the `callee_info` map to also
//! carry every entry from
//! `stdlib_lowering::enumerate_record_return_recipes()`, keyed by
//! the trait-qualified name `"<trait_name>::<method_name>"` — the
//! exact spelling that `walker_pipeline.rs`'s call-site scan stamps
//! into `CallMeta.callee_name` for a source-level call written
//! `CpuidOps::cpuid_leaf(...)` (see `is_valid_qualified_identifier`
//! in `cmd_build/identifier.rs`). Recipes have no user-code Let, so
//! `return_record_layout_table` never sees them; injecting them into
//! the same map at pass entry makes recipe-callee App sites
//! participate in caller-frame slot packing on the same footing as
//! user-Let callees.
//!
//! Consequence: an App that calls `CpuidOps::cpuid_leaf` now gets a
//! persistent 16 B slot in `caller_sret_slot_table`. `emit_call.rs`
//! reads the slot at the sret prelude and emits a single
//! `lea rdi, [rbp - slot_disp]` instead of the Slice B transient
//! `sub rsp, 16; lea rdi, [rsp+0]`. The SysVRegs recipe splice
//! branch — which returns without executing `add rsp, padded_slot`
//! — no longer leaves RSP 16 B low across the splice, because the
//! transient bump never fires. Recipe ABI is always SysV today
//! (the recipe registry produces only SysV callees), so the
//! classifier arm is hard-wired to `CallingConvention::Sysv`.
//!
//! Recipes with `arg_convention: ArgConvention::Literal` return
//! before reaching the sret setup in `emit_call.rs`; allocating a
//! slot for them here is harmless (the slot is never read) and
//! keeps the pass's registry-consumption loop uniform.
//!
//! **User-Let callee behaviour is byte-identical.** The
//! recipe-injection loop runs AFTER the user-Let loop, so a
//! HashMap conflict resolves recipe-wins — but the raw parser
//! rejects `::` inside plain identifiers (only qualified paths
//! carry the separator), so a genuine collision cannot arise from
//! well-formed pdx.

use paideia_as_ir::abi::{
    ms_return_placement_from_layout, sysv_return_placement_from_layout, MsReturnPlacement,
    SysvReturnPlacement,
};
use paideia_as_ir::let_meta::CallingConvention;
use paideia_as_ir::{CallerSretSlot, IrArena, IrKind, IrNodeId};

use crate::stdlib_lowering::enumerate_record_return_recipes;

/// Populate `IrArena::caller_sret_slot_table` and
/// `IrArena::caller_sret_frame_bump_table`.
///
/// For each top-level Lambda (function symbol whose kind is
/// `Function`), walk its descendant `App` nodes. For each App whose
/// callee has `Symbol::return_record_layout` present:
///
///   1. Resolve the SysV or MS placement based on the callee's ABI
///      (defaults to SysV when the callee is unannotated).
///   2. If the placement is `Memory` OR a register-return shape whose
///      caller wants a persistent slot (currently: every non-`None`
///      placement) — allocate a slot on the caller Lambda's frame.
///   3. Sum the padded slot sizes per caller Lambda; write the total
///      into `caller_sret_frame_bump_table`.
///
/// Slots are packed downward from `RBP` in App-encounter order (the
/// walk visits App nodes in a deterministic pre-order traversal of
/// the Lambda's IR subtree). This order is stable across builds: the
/// pass takes and returns nothing but the arena — no state carried
/// across calls.
///
/// Runs AFTER `populate_return_record_layouts` (Slice A pass) has
/// filled `IrArena::return_record_layout_table`, and BEFORE
/// `EmitWalker::walk` reads either of the new sret tables from
/// `emit_call.rs`. Scheduled inside `walker_pipeline::run_walker_pipeline`
/// so it sits between call-site pre-population and the walk itself.
pub fn populate_return_record_cons_slots(ir: &mut IrArena) {
    // name → CalleeInfo. Small (module-scoped), so HashMap is fine.
    let mut callee_info: std::collections::HashMap<String, CalleeInfo> =
        std::collections::HashMap::new();

    // The pass is scheduled BEFORE `EmitWalker::walk` stamps
    // `Symbol::return_record_layout` (see `cmd_build/walker_pipeline.rs`),
    // so it reads the raw side-tables — `return_record_layout_table`
    // keyed by Let IrNodeId, and `binding_names` / `let_meta` for the
    // Let's name and ABI. This keeps the caller-sret tables ready by
    // the time `emit_call.rs` consults them inside the walk, without
    // needing to split the walker pipeline into two phases.
    //
    // A snapshot Vec keeps the immutable borrow of the tables tight;
    // the mutation loop below then holds only the &mut IrArena.
    let layout_entries: Vec<(IrNodeId, paideia_as_ir::record_layout::RecordLayout)> = ir
        .return_record_layout_table()
        .entries()
        .iter()
        .map(|(k, v)| (*k, v.clone()))
        .collect();

    for (let_id, layout) in layout_entries {
        let name = match ir.binding_names().get(let_id) {
            Some(n) => n.to_string(),
            // No binding name → cannot cross-reference against CallMeta.
            None => continue,
        };
        let abi = ir
            .let_meta()
            .get(let_id)
            .and_then(|m| m.abi)
            .unwrap_or(CallingConvention::Sysv);
        let padded_size = padded_slot_bytes(layout.size);
        let inner = match abi {
            CallingConvention::Sysv => {
                classify_sysv(sysv_return_placement_from_layout(&layout))
            }
            CallingConvention::Ms => {
                classify_ms(ms_return_placement_from_layout(&layout))
            }
        };
        // Absent placement → no entry (matches Slice B's Absent
        // branch: no sret wiring, scalar-return path preserved).
        if matches!(inner, PlacementShapeInner::Absent) {
            continue;
        }
        callee_info.insert(name, CalleeInfo { shape: inner, padded_size });
    }

    // PAS-DEBT-B4-002 Slice E (paideia-as#1554): fold every
    // record-returning stdlib recipe into the same map, keyed by the
    // trait-qualified spelling `"<trait_name>::<method_name>"` that
    // `walker_pipeline.rs`'s call-site scan stamps into
    // `CallMeta.callee_name` for a source-level `Trait::method(...)`
    // call. Without this, recipe callees never receive a persistent
    // caller sret slot and `emit_call.rs`'s SysVRegs recipe splice
    // branch (which returns without executing `add rsp, padded_slot`)
    // leaves RSP 16 B low across every record-returning recipe call.
    //
    // ABI is hard-wired to SysV — every entry in
    // `enumerate_record_return_recipes()` currently uses the SysV
    // caller convention (recipes take args via RDI/RSI/RDX/... after
    // the sret shift). If a future MS-ABI recipe joins the registry,
    // this arm needs to widen; the classifier helpers below already
    // handle either convention.
    for entry in enumerate_record_return_recipes() {
        let key = format!("{}::{}", entry.trait_name, entry.method_name);
        let padded_size = padded_slot_bytes(entry.layout.size);
        // Recipes today all use SysV. When Ms recipes appear, this
        // hard-wire should read the recipe's own ABI tag (currently
        // `LoweringRecipe` does not carry one — SysV is implicit).
        let inner = classify_sysv(sysv_return_placement_from_layout(&entry.layout));
        if matches!(inner, PlacementShapeInner::Absent) {
            continue;
        }
        // insert(): recipe wins on any (impossible in well-formed
        // pdx) collision with a user-Let name — the parser rejects
        // `::` inside plain identifiers, so no genuine clash arises.
        callee_info.insert(key, CalleeInfo { shape: inner, padded_size });
    }

    if callee_info.is_empty() {
        return;
    }

    // Snapshot Lambda ids to walk. Every Let with a Lambda value
    // resolves via `binding_names().get(let_id)` (same lookup emit
    // uses); the Lambda id is the Let's `value` child in the IR
    // (mirrors `emit_walker/walk.rs::symbol_ir_node = rhs_id` for
    // function symbols). Collect via iterating all IR nodes and
    // filtering by IrKind::Lambda — the pre-order walk below then
    // gathers the App descendants of each.
    let caller_lambdas: Vec<IrNodeId> = (1..=ir.len())
        .filter_map(|i| IrNodeId::new(i as u32))
        .filter(|id| {
            ir.get(*id)
                .map(|n| matches!(n.kind, IrKind::Lambda))
                .unwrap_or(false)
        })
        .collect();

    for caller_id in caller_lambdas {
        // Collect App IrNodeIds inside this Lambda's subtree, in
        // deterministic pre-order.
        let mut apps: Vec<IrNodeId> = Vec::new();
        collect_app_descendants(ir, caller_id, &mut apps);
        if apps.is_empty() {
            continue;
        }

        let mut per_caller_total: u32 = 0;
        for app_id in apps {
            // Clone the callee name out so the immutable
            // `ir.call_sites()` borrow ends before the mutable
            // `caller_sret_slot_table_mut()` write below. NLL usually
            // shrinks the borrow past a single-field access, but
            // making the owning-string explicit removes any
            // borrow-checker ambiguity from a downstream refactor.
            let callee_name = match ir.call_sites().get(app_id) {
                Some(m) => m.callee_name.clone(),
                None => continue,
            };
            let Some(info) = callee_info.get(&callee_name) else {
                continue;
            };
            // PAS-DEBT-B4-002 Slice D (paideia-as#1554): lift the
            // Slice-C Memory-only gate. Register-return placements
            // (IntPair, SseSingle, IntSse, SseInt, SsePair,
            // XmmSingle, IntSingle) now also allocate a caller-side
            // persistent slot so the post-CALL caller-side
            // pair-unpack (`sysv_caller_read_return_pair` /
            // `ms_caller_read_return_reg`) has a durable
            // `[RBP + disp]` destination to spill each return
            // register into. Slice C's IntPair byte-identity
            // regression test carried a `@no_frame` state marker on
            // the caller — the emit-time guards in `emit_call.rs`
            // and `emit_visit_lambda.rs` fall back to Slice B on
            // that shape (no RBP anchor), so allocating a slot in
            // the arena side-table here is safe: the emit path
            // discards it. See `emit_call.rs`'s `is_caller_no_frame`
            // guard.
            let want_slot = !matches!(info.shape, PlacementShapeInner::Absent);
            if !want_slot {
                continue;
            }
            let padded_size = info.padded_size;
            per_caller_total = per_caller_total.saturating_add(padded_size);
            let rbp_disp = -(per_caller_total as i32);
            ir.caller_sret_slot_table_mut()
                .insert(app_id, CallerSretSlot::new(rbp_disp, padded_size));
        }
        if per_caller_total > 0 {
            // Round up the per-caller total to a 16-byte multiple so
            // the resulting `sub rsp, N` preserves the SysV
            // `rsp mod 16 == 0` invariant that every downstream call
            // in this body assumes. Individual slot sizes are
            // already 16-multiples (see `padded_slot_bytes`), so
            // this round-up is a defensive no-op today — but keeps
            // the invariant if slot sizes ever grow finer.
            let bump = (per_caller_total + 15) & !15;
            ir.caller_sret_frame_bump_table_mut().insert(caller_id, bump);
        }
    }
}

// ── helpers ────────────────────────────────────────────────────────────

fn padded_slot_bytes(size: u64) -> u32 {
    // Match `emit_call.rs::sret_padded_slot_bytes`: round to
    // `max(align, 16)` — here we assume 16 dominates, mirroring the
    // Slice B wire-up. If a future aggregate needs > 16 B alignment
    // this helper widens naturally with the RecordLayout's real
    // align field.
    let padded = (size + 15) & !15;
    padded as u32
}

/// Per-callee return-shape snapshot the pass consults for each
/// enclosing App. Slice C uses only `Memory` today (the other two are
/// scaffolded so Slice D can lift the register-return path onto the
/// same table without another schema change).
struct CalleeInfo {
    shape: PlacementShapeInner,
    padded_size: u32,
}

#[derive(Copy, Clone)]
enum PlacementShapeInner {
    Absent,
    Memory,
    // Slice D (paideia-as#1554): register-return placements now
    // allocate a caller slot so the post-CALL pair-unpack has a
    // durable destination. The pass no longer distinguishes Memory
    // vs Register at slot-allocation time — both go through
    // `caller_sret_slot_table`. Kept as a separate variant so
    // future callers of this classifier (e.g. a byte-shape probe)
    // can still discriminate without re-computing the placement.
    Register,
}

fn classify_sysv(p: SysvReturnPlacement) -> PlacementShapeInner {
    match p {
        SysvReturnPlacement::Memory => PlacementShapeInner::Memory,
        SysvReturnPlacement::None => PlacementShapeInner::Absent,
        // Every non-None non-Memory placement is a register-return.
        // Wildcard arm required: SysvReturnPlacement is
        // `#[non_exhaustive]` (see debt-catalog reminder).
        _ => PlacementShapeInner::Register,
    }
}

fn classify_ms(p: MsReturnPlacement) -> PlacementShapeInner {
    match p {
        MsReturnPlacement::Memory => PlacementShapeInner::Memory,
        MsReturnPlacement::None => PlacementShapeInner::Absent,
        // Wildcard arm required: MsReturnPlacement is
        // `#[non_exhaustive]`.
        _ => PlacementShapeInner::Register,
    }
}

/// Depth-first pre-order collection of App IrNodeIds within a
/// Lambda's subtree. Does not descend into nested Lambdas: those
/// are separate function bodies with their own caller-sret area.
fn collect_app_descendants(ir: &IrArena, root: IrNodeId, out: &mut Vec<IrNodeId>) {
    // Pre-order walk with an explicit stack to keep the code
    // borrow-free of any &mut self on the recursion path.
    let mut stack: Vec<IrNodeId> = vec![root];
    while let Some(id) = stack.pop() {
        let Some(node) = ir.get(id) else { continue };
        match node.kind {
            IrKind::App => {
                out.push(id);
            }
            IrKind::Lambda if id != root => {
                // Nested Lambda: its own caller-sret area is handled
                // by its own outer pass iteration (function symbols
                // for nested Lambdas are stamped via
                // closure_dispatch / lower's ClosureBody path). Do
                // not descend so its App nodes don't get double-
                // counted in the outer caller's bump.
                continue;
            }
            _ => {}
        }
        // Push children in reverse so pre-order visits leftmost
        // first when popped.
        let children = ir.children(id);
        for &c in children.iter().rev() {
            stack.push(c);
        }
    }
}

// ── tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{FileId, Span};
    use paideia_as_ir::record_layout::{FieldLayout, RecordLayout};
    use paideia_as_ir::CallMeta;

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    fn memory_24b_layout() -> RecordLayout {
        RecordLayout::new(
            24,
            8,
            vec![
                FieldLayout { offset: 0, size: 8, signed: false, is_float: false },
                FieldLayout { offset: 8, size: 8, signed: false, is_float: false },
                FieldLayout { offset: 16, size: 8, signed: false, is_float: false },
            ],
        )
    }

    fn int_pair_16b_layout() -> RecordLayout {
        RecordLayout::new(
            16,
            8,
            vec![
                FieldLayout { offset: 0, size: 8, signed: false, is_float: false },
                FieldLayout { offset: 8, size: 8, signed: false, is_float: false },
            ],
        )
    }

    /// Attach the callee's return layout via the two pre-walk raw
    /// side-tables the pass reads: `return_record_layout_table`
    /// (Slice A's population) and `binding_names` (the Let's binding
    /// name that `CallMeta.callee_name` resolves against).
    fn stamp_callee(
        ir: &mut IrArena,
        let_id: IrNodeId,
        name: &str,
        layout: RecordLayout,
    ) {
        ir.return_record_layout_table_mut().insert(let_id, layout);
        ir.binding_names_mut().insert(let_id, name.to_string());
    }

    /// Callee returns a 24-byte Memory-classified record; caller
    /// contains one App to that callee → one slot at RBP-32.
    #[test]
    fn memory_callee_single_call_allocates_one_slot() {
        let mut ir = IrArena::new();

        // Simulate a Let-Lambda binding for the callee. In real
        // compilation the outer Let carries the layout entry; here
        // we allocate a Let node id (via alloc, kind is irrelevant
        // to the pass because it reads by IrNodeId key, not kind).
        let callee_let = ir.alloc(IrKind::Let, span());
        stamp_callee(&mut ir, callee_let, "callee", memory_24b_layout());

        // caller lambda: body is App(callee_var).
        let callee_var = ir.alloc(IrKind::Var, span());
        let app = ir.alloc_with_children(IrKind::App, span(), [callee_var]);
        ir.call_sites_mut().insert(
            app,
            CallMeta {
                callee_name: "callee".to_string(),
                arg_count: 0,
                is_intrinsic: false,
            },
        );
        let caller_lambda = ir.alloc_with_children(IrKind::Lambda, span(), [app]);

        populate_return_record_cons_slots(&mut ir);

        let slot = ir
            .caller_sret_slot_table()
            .get(app)
            .expect("caller sret slot populated");
        // 24 padded to 16 multiple = 32; single slot lives at [RBP-32].
        assert_eq!(slot.padded_size, 32);
        assert_eq!(slot.rbp_disp, -32);

        let bump = ir
            .caller_sret_frame_bump_table()
            .get(caller_lambda)
            .expect("caller frame bump populated");
        assert_eq!(*bump, 32);
    }

    /// PAS-DEBT-B4-002 Slice D (paideia-as#1554): register-return
    /// callee (IntPair) now allocates a caller slot so the post-CALL
    /// caller-side pair-unpack has a durable `[RBP + disp]`
    /// destination to spill RAX/RDX into. Slot size mirrors the
    /// layout's padded byte size (16 → 16). Emit-time guards in
    /// `emit_call.rs` / `emit_visit_lambda.rs` fall back to Slice B
    /// byte-identity when the caller carries `@no_frame` (no RBP
    /// anchor) — that fall-back is orthogonal to slot allocation
    /// here.
    #[test]
    fn intpair_callee_allocates_persistent_slot_in_slice_d() {
        let mut ir = IrArena::new();

        let callee_let = ir.alloc(IrKind::Let, span());
        stamp_callee(&mut ir, callee_let, "callee", int_pair_16b_layout());

        let callee_var = ir.alloc(IrKind::Var, span());
        let app = ir.alloc_with_children(IrKind::App, span(), [callee_var]);
        ir.call_sites_mut().insert(
            app,
            CallMeta {
                callee_name: "callee".to_string(),
                arg_count: 0,
                is_intrinsic: false,
            },
        );
        let caller_lambda = ir.alloc_with_children(IrKind::Lambda, span(), [app]);

        populate_return_record_cons_slots(&mut ir);

        let slot = ir
            .caller_sret_slot_table()
            .get(app)
            .expect("IntPair register-return must allocate a caller sret slot in Slice D");
        // 16 padded to 16-multiple = 16; slot lives at [RBP-16].
        assert_eq!(slot.padded_size, 16);
        assert_eq!(slot.rbp_disp, -16);

        let bump = ir
            .caller_sret_frame_bump_table()
            .get(caller_lambda)
            .expect("IntPair register-return must bump the caller frame in Slice D");
        assert_eq!(*bump, 16);
    }

    /// Non-record-returning callee → no side-table entries anywhere.
    /// Pins the scalar-return regression at the pass boundary.
    #[test]
    fn scalar_returning_callee_produces_no_entries() {
        let mut ir = IrArena::new();

        // No return_record_layout entry for "callee".
        let callee_var = ir.alloc(IrKind::Var, span());
        let app = ir.alloc_with_children(IrKind::App, span(), [callee_var]);
        ir.call_sites_mut().insert(
            app,
            CallMeta {
                callee_name: "callee".to_string(),
                arg_count: 0,
                is_intrinsic: false,
            },
        );
        let caller_lambda = ir.alloc_with_children(IrKind::Lambda, span(), [app]);

        populate_return_record_cons_slots(&mut ir);

        assert!(ir.caller_sret_slot_table().is_empty());
        assert!(ir.caller_sret_frame_bump_table().is_empty());
        // Also assert the caller_lambda binding didn't get a bump —
        // silences an "unused" warning while pinning the negative case.
        assert!(ir.caller_sret_frame_bump_table().get(caller_lambda).is_none());
    }

    /// Two Memory-callee calls in the same caller pack into two
    /// non-overlapping slots (RBP-32 and RBP-64), and the caller's
    /// bump = 64.
    #[test]
    fn two_memory_calls_pack_non_overlapping_slots() {
        let mut ir = IrArena::new();

        let callee_let = ir.alloc(IrKind::Let, span());
        stamp_callee(&mut ir, callee_let, "callee", memory_24b_layout());

        // Two App nodes to `callee` inside a single caller Lambda.
        let callee_var_1 = ir.alloc(IrKind::Var, span());
        let app_1 = ir.alloc_with_children(IrKind::App, span(), [callee_var_1]);
        ir.call_sites_mut().insert(
            app_1,
            CallMeta {
                callee_name: "callee".to_string(),
                arg_count: 0,
                is_intrinsic: false,
            },
        );
        let callee_var_2 = ir.alloc(IrKind::Var, span());
        let app_2 = ir.alloc_with_children(IrKind::App, span(), [callee_var_2]);
        ir.call_sites_mut().insert(
            app_2,
            CallMeta {
                callee_name: "callee".to_string(),
                arg_count: 0,
                is_intrinsic: false,
            },
        );
        let caller_lambda =
            ir.alloc_with_children(IrKind::Lambda, span(), [app_1, app_2]);

        populate_return_record_cons_slots(&mut ir);

        let s1 = ir.caller_sret_slot_table().get(app_1).unwrap();
        let s2 = ir.caller_sret_slot_table().get(app_2).unwrap();
        assert_eq!(s1.padded_size, 32);
        assert_eq!(s2.padded_size, 32);
        // Two slots at RBP-32 and RBP-64 (order matches pre-order
        // traversal of the caller Lambda's children).
        let disps = [s1.rbp_disp, s2.rbp_disp];
        assert!(disps.contains(&-32));
        assert!(disps.contains(&-64));
        assert_ne!(s1.rbp_disp, s2.rbp_disp, "slots must not overlap");

        let bump = ir
            .caller_sret_frame_bump_table()
            .get(caller_lambda)
            .expect("bump present for caller with slots");
        assert_eq!(*bump, 64);
    }

    // ── Slice E: recipe-callee participation (paideia-as#1554) ─────

    /// PAS-DEBT-B4-002 Slice E (paideia-as#1554): an App calling
    /// `CpuidOps::cpuid_leaf` — a stdlib recipe with a record-return
    /// layout registered in `enumerate_record_return_recipes()` — now
    /// receives a persistent caller-frame slot without any user-Let
    /// entry in `return_record_layout_table`. Pre-Slice E, this call
    /// site fell through to `emit_call.rs`'s Slice B transient path
    /// (`sub rsp, 16; lea rdi, [rsp+0]`), which the SysVRegs recipe
    /// splice branch left unreleased (no `add rsp, 16`), leaving the
    /// caller's RSP 16 B low across the splice.
    ///
    /// Slot size: CpuidRegs is 16 B natural-aligned → `padded_slot_bytes`
    /// rounds to 16-multiple = 16; single slot lives at `[RBP - 16]`.
    #[test]
    fn recipe_callee_cpuid_leaf_allocates_persistent_slot() {
        let mut ir = IrArena::new();

        // No stamp_callee here: `CpuidOps::cpuid_leaf` has no
        // user-code Let. The pass must synthesise the callee_info
        // entry directly from `enumerate_record_return_recipes()`.
        let callee_var = ir.alloc(IrKind::Var, span());
        let app = ir.alloc_with_children(IrKind::App, span(), [callee_var]);
        ir.call_sites_mut().insert(
            app,
            CallMeta {
                // walker_pipeline.rs stamps the trait-qualified
                // spelling verbatim from the source (see
                // `is_valid_qualified_identifier`).
                callee_name: "CpuidOps::cpuid_leaf".to_string(),
                arg_count: 2,
                is_intrinsic: false,
            },
        );
        let caller_lambda = ir.alloc_with_children(IrKind::Lambda, span(), [app]);

        populate_return_record_cons_slots(&mut ir);

        let slot = ir
            .caller_sret_slot_table()
            .get(app)
            .expect("recipe callee must allocate a persistent caller sret slot in Slice E");
        assert_eq!(slot.padded_size, 16, "CpuidRegs is 16 B, already 16-aligned");
        assert_eq!(slot.rbp_disp, -16, "single slot lives at [RBP - 16]");

        let bump = ir
            .caller_sret_frame_bump_table()
            .get(caller_lambda)
            .expect("recipe callee must bump the caller frame in Slice E");
        assert_eq!(*bump, 16);
    }

    /// PAS-DEBT-B4-002 Slice E: two calls to the same recipe callee
    /// in the same caller pack into two non-overlapping slots (each
    /// 16 B), summing to a 32 B caller-frame bump. Mirrors the
    /// user-Let `two_memory_calls_pack_non_overlapping_slots`
    /// invariant on the recipe-callee axis.
    #[test]
    fn recipe_callee_two_calls_pack_non_overlapping_slots() {
        let mut ir = IrArena::new();

        let callee_var_1 = ir.alloc(IrKind::Var, span());
        let app_1 = ir.alloc_with_children(IrKind::App, span(), [callee_var_1]);
        ir.call_sites_mut().insert(
            app_1,
            CallMeta {
                callee_name: "CpuidOps::cpuid_leaf".to_string(),
                arg_count: 2,
                is_intrinsic: false,
            },
        );
        let callee_var_2 = ir.alloc(IrKind::Var, span());
        let app_2 = ir.alloc_with_children(IrKind::App, span(), [callee_var_2]);
        ir.call_sites_mut().insert(
            app_2,
            CallMeta {
                callee_name: "CpuidOps::cpuid_leaf".to_string(),
                arg_count: 2,
                is_intrinsic: false,
            },
        );
        let caller_lambda =
            ir.alloc_with_children(IrKind::Lambda, span(), [app_1, app_2]);

        populate_return_record_cons_slots(&mut ir);

        let s1 = ir.caller_sret_slot_table().get(app_1).unwrap();
        let s2 = ir.caller_sret_slot_table().get(app_2).unwrap();
        assert_eq!(s1.padded_size, 16);
        assert_eq!(s2.padded_size, 16);
        let disps = [s1.rbp_disp, s2.rbp_disp];
        assert!(disps.contains(&-16));
        assert!(disps.contains(&-32));
        assert_ne!(s1.rbp_disp, s2.rbp_disp, "slots must not overlap");

        let bump = ir
            .caller_sret_frame_bump_table()
            .get(caller_lambda)
            .expect("caller frame bump present for two recipe calls");
        assert_eq!(*bump, 32);
    }

    /// PAS-DEBT-B4-002 Slice E: mixing a user-Let callee and a
    /// recipe callee inside the same caller Lambda produces
    /// independent slots for each App, both packed downward from
    /// RBP. The user-Let entry must retain byte-identical shape
    /// with the pre-Slice-E `memory_callee_single_call_allocates_
    /// one_slot` fixture — Slice E's registry-fold must not perturb
    /// the historical user-Let path.
    #[test]
    fn mixed_user_let_and_recipe_callees_both_get_slots() {
        let mut ir = IrArena::new();

        // User-Let callee: 24 B Memory-classified record → 32 B slot.
        let user_let = ir.alloc(IrKind::Let, span());
        stamp_callee(&mut ir, user_let, "user_callee", memory_24b_layout());

        let user_callee_var = ir.alloc(IrKind::Var, span());
        let user_app =
            ir.alloc_with_children(IrKind::App, span(), [user_callee_var]);
        ir.call_sites_mut().insert(
            user_app,
            CallMeta {
                callee_name: "user_callee".to_string(),
                arg_count: 0,
                is_intrinsic: false,
            },
        );

        // Recipe callee: CpuidRegs 16 B → 16 B slot.
        let recipe_callee_var = ir.alloc(IrKind::Var, span());
        let recipe_app =
            ir.alloc_with_children(IrKind::App, span(), [recipe_callee_var]);
        ir.call_sites_mut().insert(
            recipe_app,
            CallMeta {
                callee_name: "CpuidOps::cpuid_leaf".to_string(),
                arg_count: 2,
                is_intrinsic: false,
            },
        );

        let caller_lambda = ir.alloc_with_children(
            IrKind::Lambda,
            span(),
            [user_app, recipe_app],
        );

        populate_return_record_cons_slots(&mut ir);

        let user_slot = ir
            .caller_sret_slot_table()
            .get(user_app)
            .expect("user-Let callee must still allocate a slot");
        let recipe_slot = ir
            .caller_sret_slot_table()
            .get(recipe_app)
            .expect("recipe callee must allocate a slot in Slice E");

        // Two distinct sizes, two distinct RBP disps. Total bump = 48.
        assert_eq!(user_slot.padded_size, 32);
        assert_eq!(recipe_slot.padded_size, 16);
        assert_ne!(
            user_slot.rbp_disp, recipe_slot.rbp_disp,
            "user-Let and recipe slots must not overlap"
        );

        let bump = ir
            .caller_sret_frame_bump_table()
            .get(caller_lambda)
            .expect("mixed-callee caller must have a frame bump");
        assert_eq!(*bump, 48, "32 + 16 = 48, no additional 16-multiple round-up needed");
    }
}
