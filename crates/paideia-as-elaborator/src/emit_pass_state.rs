//! `EmitPassState` — the accumulator carried by `EmitWalker` during the walk.
//!
//! Extracted from `emit_walker.rs` during the v0.17 refactor. Owns every
//! per-function scratchpad the walker mutates: emitted instructions, byte
//! offsets, record/enum layouts, local bindings, labels, and the mode stack.
//!
//! The `impl EmitPassState` block hosts the label-registration and record-
//! layout helpers that operate purely on the state without touching walker
//! bookkeeping.

use std::collections::{HashMap, HashSet};

use paideia_as_ir::instruction::{CpuFeature, InstrMode, InstructionSideTable, RegId};
use paideia_as_ir::let_meta::CallingConvention;
use paideia_as_ir::record_layout::{FieldLayout, RecordLayout, RecordTypeId};
use paideia_as_ir::{EnumLayout, EnumTypeId, IrNodeId};

use crate::LocalBindingTable;

/// LoopContext: tracks the nesting level of loop vs while for break validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopContext {
    /// Infinite `loop { ... }` — can accept break values.
    Loop,
    /// `while cond { ... }` — cannot accept break values.
    While,
}

/// Tracks emission state during IR traversal.
///
/// Accumulates instructions keyed by IrNodeId and tracks byte offsets
/// for function-level metadata used by downstream m5-m6 phases.
/// Phase 6 m3-001: Also tracks finalised record layouts.
#[derive(Debug)]
pub struct EmitPassState {
    /// The emitted instructions, keyed by IrNodeId, per the existing
    /// Phase-3 m2-001 InstructionSideTable convention.
    pub(crate) instructions: InstructionSideTable,

    /// IrNodeId of the function currently being lowered (or 0 if none).
    pub(crate) current_function: u32,

    /// Pending lambda IR node id whose first instruction has not yet been emitted.
    /// Set by Action and Match sentinels; consumed by emit_inst on the first instruction.
    pub(crate) pending_first_instr_lambda: Option<u32>,

    /// Estimated byte offset within the current function. Reset to 0 on each
    /// new function entry. This is an advisory estimate based on instruction
    /// mnemonics and is verified to match the actual encoded byte count at
    /// the end of the build (phase-7-m1-003). m5 (symbols + relocs) will
    /// consume the actual offsets from Instruction.byte_offset_in_text.
    pub(crate) estimated_offset: u32,

    /// Lambda IR node id -> IrNodeId of its first emitted instruction.
    /// Populated by record_lambda_entry. Resolved to byte offsets
    /// post-encoding via EmitResult.offset_map (future use).
    pub(crate) lambda_first_instr: HashMap<u32, IrNodeId>,

    /// IrNodeIds of Lambdas that actually emitted bytecode.
    /// Used to filter out symbols for non-emitting lambdas.
    pub(crate) emitted_lambdas: HashSet<u32>,

    /// Lambda IR node id -> calling convention (ABI).
    /// Populated by emit_walker when processing Let bindings with @abi directives.
    /// Used by lambda emitters to determine which register pool to use for parameters.
    pub(crate) lambda_abis: HashMap<u32, CallingConvention>,

    /// IrNodeIds of IrKind::Unsafe nodes encountered during the walk.
    /// m3 UnsafeWalker drains this via take_pending_unsafe() and lowers
    /// the block contents.
    pub(crate) pending_unsafe_blocks: Vec<u32>,

    /// Phase 6 m3-001: C-ABI natural-alignment record layouts,
    /// keyed by RecordTypeId. Populated by finalise_record_layouts().
    pub(crate) record_layouts: HashMap<RecordTypeId, RecordLayout>,

    /// PA-r17-007: Enum layouts keyed by EnumTypeId.
    /// Populated during emission pass; consumed by visit_enum_cons.
    pub(crate) enum_layouts: HashMap<EnumTypeId, EnumLayout>,

    /// Phase 6 m3-003: Scratch register assignment for in-block field bindings.
    /// Tracks which scratch registers have been assigned in the current
    /// function. Reset to empty at function entry. Sequence: RAX(0), RCX(1),
    /// RDX(2), R8(8).
    pub(crate) scratch_assignment: Vec<RegId>,

    /// Phase 6 m4-003: Label name → byte offset mapping.
    /// Populated during unsafe block lowering when labels are encountered.
    /// Used to resolve backward label references at encoding time.
    /// Scoped to the current function; reset at function entry.
    pub(crate) labels: HashMap<String, u32>,

    /// Phase 6 m4-004: Label name → instruction IR node ID mapping.
    /// Populated from unsafe_walker output, used to compute actual label
    /// offsets based on instruction offsets from the encoder's offset_map.
    pub(crate) label_to_instr: HashMap<String, IrNodeId>,

    /// PA8-m1-002b: Unsafe lambda IR node id → index in pending_unsafe_blocks.
    /// Maps each unsafe-bodied lambda to its position in the pending list,
    /// allowing us to look up its first instruction from UnsafeWalker's
    /// first_instrs vec.
    pub(crate) unsafe_lambda_to_pending_idx: HashMap<u32, usize>,

    /// PA8-m1-002b: Unsafe body IR node id → lambda IR node id.
    /// Used to track which lambda has which unsafe body during the walk.
    pub(crate) unsafe_body_to_lambda: HashMap<u32, u32>,

    /// Phase 7 m1-001: Local binding table for multi-statement function bodies.
    /// Maps binding names (from let-statements) to their assigned scratch
    /// registers. Scoped to the current function; reset at function entry.
    pub(crate) local_bindings: LocalBindingTable,

    /// Stack of instruction modes during nested scope walk.
    /// Used to propagate #![bits=32] or #![bits=64] from module inner_attrs.
    pub(crate) mode_stack: Vec<InstrMode>,

    /// PA-r16-004-backtrack-a (#1033): declared CPU feature set from
    /// `#![target_features = "..."]`. Populated at build entry; consumed
    /// during unsafe-block lowering to gate feature-tagged mnemonics.
    pub(crate) enabled_features: HashSet<CpuFeature>,

    /// PA-r17-013 (#991): tracks Match IR nodes already emitted in
    /// trailing position so walk_inner's top-level loop can skip them
    /// (avoiding double-dispatch that produces post-ret dead code).
    pub(crate) emitted_match_ids: HashSet<u32>,

    /// PA-r17-013 (#991): tracks the max IrNodeId assigned via emit_inst.
    /// Used so lambda's ret_id sorts AFTER all match instructions.
    #[allow(dead_code)]
    pub(crate) max_emitted_node_id: u32,

    /// #1086: RecordCons IR nodes already handled by other lowering paths
    /// (e.g., data_encoder::encode_record_cons for Let → RecordCons).
    /// Populated by walk_inner pre-pass. Signals scope-limited visitor skips it
    /// to prevent T0518/T0516 false positives.
    pub(crate) handled_record_cons_ids: HashSet<u32>,

    /// #1084: EnumCons IR nodes already handled by other lowering paths
    /// (e.g., data_encoder::encode_enum_cons for Let → EnumCons).
    /// Populated by walk_inner pre-pass. Signals scope-limited visitor skips it
    /// to prevent double-emission in .text section.
    pub(crate) handled_enum_cons_ids: HashSet<u32>,

    /// #1086: FieldAccess IR nodes already handled by other lowering paths
    /// (e.g., emit_field_access_lambda for Lambda → FieldAccess).
    /// Populated by walk_inner pre-pass. Signals scope-limited visitor skips it
    /// to prevent T0518/T0516 false positives.
    pub(crate) handled_field_access_ids: HashSet<u32>,

    /// #1131: Let IR nodes already handled by populate_data_table (data section lowering).
    /// Populated by walk_inner pre-pass. Signals visit_let_literal to skip emitting Mov
    /// for module-level Let nodes that already have a DataEntry, preventing spurious
    /// .text emission before function bodies.
    pub(crate) handled_data_let_ids: HashSet<u32>,

    /// #1116: Store IR nodes already handled by visit_lambda's Store arm.
    /// Populated by walk_inner pre-pass when a Lambda has a Store body.
    /// Signals walk_inner's top-level Store gate to skip double-emission.
    pub(crate) emitted_store_ids: HashSet<u32>,

    /// #1140: Monotonic emission order counter for instructions.
    /// Initialized to 1 (0 is reserved sentinel for test-only instructions).
    /// Incremented before each instruction insertion to break virtual-IrNodeId collisions.
    pub(crate) next_emission_order: u32,

    /// #1141: Monotonic synthetic-IrNodeId allocator for instructions that
    /// have no natural AST-derived id (bridge saves, CALL sites, indirect-call
    /// scaffolds, etc.). Post-#1140 the sort key is `(emission_order, node_id)`,
    /// so these ids are identity-only — but they must not collide with real
    /// arena ids or with each other. Initialized well above any realistic
    /// arena id (2^31) so a monotonic bump can't wrap into the arena range.
    pub(crate) next_synthetic_id: u32,

    /// #1139: Per-lambda local_bindings snapshot. Maps lambda_id to the
    /// local_bindings table captured after register_nested_lambda_params.
    /// Used by resolve_var_operands to resolve Var operands against the
    /// correct scope when an instruction belongs to an unsafe lambda.
    pub(crate) per_lambda_bindings: HashMap<u32, LocalBindingTable>,

    /// #1139: Instruction→lambda mapping. Maps each emitted instruction's
    /// IrNodeId to the lambda_id that owns it. Populated at emit_inst and
    /// during UnsafeWalker instruction insertion. Used by resolve_var_operands
    /// to look up the correct per_lambda_bindings snapshot for each instruction.
    pub(crate) instr_to_lambda: HashMap<IrNodeId, u32>,
}

impl Default for EmitPassState {
    fn default() -> Self {
        Self {
            instructions: Default::default(),
            current_function: Default::default(),
            pending_first_instr_lambda: None,
            estimated_offset: Default::default(),
            lambda_first_instr: Default::default(),
            emitted_lambdas: Default::default(),
            lambda_abis: Default::default(),
            pending_unsafe_blocks: Default::default(),
            record_layouts: Default::default(),
            enum_layouts: Default::default(),
            scratch_assignment: Default::default(),
            labels: Default::default(),
            label_to_instr: Default::default(),
            unsafe_lambda_to_pending_idx: Default::default(),
            unsafe_body_to_lambda: Default::default(),
            local_bindings: Default::default(),
            mode_stack: Default::default(),
            enabled_features: Default::default(),
            emitted_match_ids: Default::default(),
            max_emitted_node_id: Default::default(),
            handled_record_cons_ids: Default::default(),
            handled_enum_cons_ids: Default::default(),
            handled_field_access_ids: Default::default(),
            handled_data_let_ids: Default::default(),
            emitted_store_ids: Default::default(),
            next_emission_order: 1,
            next_synthetic_id: 2_000_000_000,
            per_lambda_bindings: Default::default(),
            instr_to_lambda: Default::default(),
        }
    }
}

impl EmitPassState {
    // ── Record layouts ───────────────────────────────────────────────────

    /// Look up the finalised layout for a record type. Returns `None` if
    /// the layout has not yet been computed.
    #[must_use]
    pub fn record_layout(&self, type_id: RecordTypeId) -> Option<&RecordLayout> {
        self.record_layouts.get(&type_id)
    }

    /// Install (or overwrite) the layout for a record type.
    pub fn insert_record_layout(&mut self, type_id: RecordTypeId, layout: RecordLayout) {
        self.record_layouts.insert(type_id, layout);
    }

    /// Number of finalised record layouts.
    #[must_use]
    pub fn record_layout_count(&self) -> usize {
        self.record_layouts.len()
    }

    /// True if no record layouts have been finalised yet.
    #[must_use]
    pub fn record_layouts_is_empty(&self) -> bool {
        self.record_layouts.is_empty()
    }

    // ── Enum layouts ─────────────────────────────────────────────────────

    /// Look up the layout for an enum type.
    #[must_use]
    pub fn enum_layout(&self, type_id: EnumTypeId) -> Option<&EnumLayout> {
        self.enum_layouts.get(&type_id)
    }

    /// Install (or overwrite) the layout for an enum type.
    pub fn insert_enum_layout(&mut self, type_id: EnumTypeId, layout: EnumLayout) {
        self.enum_layouts.insert(type_id, layout);
    }

    // ── Scratch-register assignment (per-function) ──────────────────────

    /// Number of scratch registers assigned in the current function.
    #[must_use]
    pub fn scratch_count(&self) -> usize {
        self.scratch_assignment.len()
    }

    /// Append `reg` to the scratch assignment sequence.
    pub fn assign_scratch(&mut self, reg: RegId) {
        self.scratch_assignment.push(reg);
    }

    /// Reset the scratch assignment sequence (called at function entry).
    pub fn clear_scratch(&mut self) {
        self.scratch_assignment.clear();
    }

    // ── Pending unsafe blocks ────────────────────────────────────────────

    /// Queue `node_id` as an unsafe block for later lowering by
    /// `UnsafeWalker`.
    pub fn push_pending_unsafe(&mut self, node_id: u32) {
        self.pending_unsafe_blocks.push(node_id);
    }

    /// Number of pending unsafe blocks queued.
    #[must_use]
    pub fn pending_unsafe_count(&self) -> usize {
        self.pending_unsafe_blocks.len()
    }

    /// True if no unsafe blocks are queued.
    #[must_use]
    pub fn pending_unsafe_is_empty(&self) -> bool {
        self.pending_unsafe_blocks.is_empty()
    }

    /// Iterate the queued unsafe-block ids in insertion order.
    pub fn iter_pending_unsafe(&self) -> std::slice::Iter<'_, u32> {
        self.pending_unsafe_blocks.iter()
    }

    /// Drain and return the pending unsafe blocks.
    pub fn take_pending_unsafe(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.pending_unsafe_blocks)
    }

    // ── Lambda emission tracking ─────────────────────────────────────────

    /// Record that `lambda_id` produced bytecode. Symbol emission filters
    /// out lambdas absent from this set.
    pub fn mark_lambda_emitted(&mut self, lambda_id: u32) {
        self.emitted_lambdas.insert(lambda_id);
    }

    /// Look up the lambda associated with an unsafe body.
    #[must_use]
    pub fn unsafe_body_lambda(&self, body_id: u32) -> Option<u32> {
        self.unsafe_body_to_lambda.get(&body_id).copied()
    }

    // ── Lambda ABI tracking (PA19-r19-006) ────────────────────────────────

    /// Record the calling convention (ABI) for a lambda.
    pub fn insert_lambda_abi(&mut self, lambda_id: u32, cc: CallingConvention) {
        self.lambda_abis.insert(lambda_id, cc);
    }

    /// Look up the calling convention for a lambda. Returns Sysv if not found.
    #[must_use]
    pub fn lambda_abi(&self, lambda_id: u32) -> CallingConvention {
        self.lambda_abis.get(&lambda_id).copied().unwrap_or(CallingConvention::Sysv)
    }

    /// Look up the calling convention for a lambda, returning Option to distinguish
    /// unannotated (None) from explicitly annotated (Some). Used for bridge-save
    /// decisions where paideia (unannotated) differs from explicitly SysV.
    #[must_use]
    pub fn lambda_abi_option(&self, lambda_id: u32) -> Option<CallingConvention> {
        self.lambda_abis.get(&lambda_id).copied()
    }

    // ── Match emission tracking (PA-r17-013) ─────────────────────────────

    /// Mark a Match node as emitted in trailing position.
    pub fn mark_match_emitted(&mut self, match_id: u32) {
        self.emitted_match_ids.insert(match_id);
    }

    /// Check if a Match node was already emitted in trailing position.
    #[must_use]
    pub fn was_match_emitted(&self, match_id: u32) -> bool {
        self.emitted_match_ids.contains(&match_id)
    }

    /// Return the max IrNodeId assigned so far via emit_inst.
    /// Used so lambda's ret_id sorts AFTER all match instructions.
    #[must_use]
    pub fn max_emitted_id(&self) -> u32 {
        self.instructions.iter().map(|(id, _)| id.get()).max().unwrap_or(0)
    }

    // ── Scope-limited visitor guards (PA-r15-m4-xxx #1086) ────────────────

    /// Mark a RecordCons node as handled by another lowering path.
    pub(crate) fn mark_record_cons_handled(&mut self, id: u32) {
        self.handled_record_cons_ids.insert(id);
    }

    /// Check if a RecordCons node was already handled by another lowering path.
    #[must_use]
    pub(crate) fn was_record_cons_handled(&self, id: u32) -> bool {
        self.handled_record_cons_ids.contains(&id)
    }

    /// Mark an EnumCons node as handled by another lowering path.
    pub(crate) fn mark_enum_cons_handled(&mut self, id: u32) {
        self.handled_enum_cons_ids.insert(id);
    }

    /// Check if an EnumCons node was already handled by another lowering path.
    #[must_use]
    pub(crate) fn was_enum_cons_handled(&self, id: u32) -> bool {
        self.handled_enum_cons_ids.contains(&id)
    }

    /// Mark a FieldAccess node as handled by another lowering path.
    pub(crate) fn mark_field_access_handled(&mut self, id: u32) {
        self.handled_field_access_ids.insert(id);
    }

    /// Check if a FieldAccess node was already handled by another lowering path.
    #[must_use]
    pub(crate) fn was_field_access_handled(&self, id: u32) -> bool {
        self.handled_field_access_ids.contains(&id)
    }

    /// Mark a Let node as handled by populate_data_table (data section lowering).
    pub(crate) fn mark_data_let_handled(&mut self, id: u32) {
        self.handled_data_let_ids.insert(id);
    }

    /// Check if a Let node was already handled by populate_data_table.
    #[must_use]
    pub(crate) fn was_data_let_handled(&self, id: u32) -> bool {
        self.handled_data_let_ids.contains(&id)
    }

    /// Mark a Store node as emitted by visit_lambda's Store arm.
    pub(crate) fn mark_store_emitted(&mut self, id: u32) {
        self.emitted_store_ids.insert(id);
    }

    /// Check if a Store node was already emitted by visit_lambda's Store arm.
    #[must_use]
    pub(crate) fn was_store_emitted(&self, id: u32) -> bool {
        self.emitted_store_ids.contains(&id)
    }

    // ── Whole-map accessors for cross-crate consumers ────────────────────
    //
    // cmd_build.rs (paideia-as) needs read access to entire maps for
    // symbol-table population and offset resolution. These accessors
    // return borrows so callers can iterate/clone without letting them
    // mutate the underlying storage through the map's own API. Once the
    // fields flip to `pub(crate)`, these are the only public path.

    /// Read-only view of the finalised record layouts, keyed by
    /// `RecordTypeId`.
    #[must_use]
    pub fn record_layouts(&self) -> &HashMap<RecordTypeId, RecordLayout> {
        &self.record_layouts
    }

    /// Read-only view of the label → IR-node-id map populated by the
    /// unsafe-block lowering pass.
    #[must_use]
    /// Read-only view of the label name → estimated byte offset map.
    /// Populated by `register_label()` during instruction emission (e.g., match arm labels).
    pub fn labels(&self) -> &HashMap<String, u32> {
        &self.labels
    }

    /// Read-only view of the label name → instruction IR node ID map.
    /// Used to compute actual label offsets based on instruction offsets from the encoder.
    pub fn label_to_instr(&self) -> &HashMap<String, IrNodeId> {
        &self.label_to_instr
    }

    /// Install the label → IR-node-id map wholesale. Called by
    /// `cmd_build` after collecting label references from the unsafe
    /// walker.
    pub fn set_label_to_instr(&mut self, map: HashMap<String, IrNodeId>) {
        // Merge the new labels with existing ones, preserving any labels
        // that were registered during normal emit (e.g., match arm labels)
        for (name, instr_id) in map {
            self.label_to_instr.insert(name, instr_id);
        }
    }

    /// Register a label name → IrNodeId mapping in label_to_instr.
    /// Used by stdlib recipe splicing (#1066) to install per-recipe
    /// synthetic labels for intra-recipe jumps.
    pub fn insert_label(&mut self, name: String, instr_id: IrNodeId) {
        self.label_to_instr.insert(name, instr_id);
    }

    /// Read-only view of the lambda-first-instruction map.
    #[must_use]
    pub fn lambda_first_instr(&self) -> &HashMap<u32, IrNodeId> {
        &self.lambda_first_instr
    }

    /// Install the first-instruction id for `lambda_id`.
    pub fn insert_lambda_first_instr(&mut self, lambda_id: u32, first_instr: IrNodeId) {
        self.lambda_first_instr.insert(lambda_id, first_instr);
    }

    /// Read-only view of the unsafe-lambda → pending-block-index map.
    /// Populated during the walk, consumed by `cmd_build` when it
    /// projects post-encoding byte offsets back onto unsafe lambdas.
    #[must_use]
    pub fn unsafe_lambda_to_pending_idx(&self) -> &HashMap<u32, usize> {
        &self.unsafe_lambda_to_pending_idx
    }

    /// Record that `lambda_id` corresponds to pending unsafe-block
    /// index `idx`.
    pub fn insert_unsafe_lambda_pending_idx(&mut self, lambda_id: u32, idx: usize) {
        self.unsafe_lambda_to_pending_idx.insert(lambda_id, idx);
    }

    /// Read-only view of the emitted instructions, keyed by IR node id.
    #[must_use]
    pub fn instructions(&self) -> &InstructionSideTable {
        &self.instructions
    }

    /// Read-only view of the current function's local binding table.
    #[must_use]
    pub fn local_bindings(&self) -> &LocalBindingTable {
        &self.local_bindings
    }

    /// Current byte offset within the function being lowered.
    #[must_use]
    pub fn estimated_offset(&self) -> u32 {
        self.estimated_offset
    }

    /// Number of instruction-mode frames currently on the stack (used by
    /// cmd_build's #![bits] pre-flight check).
    #[must_use]
    pub fn mode_stack_len(&self) -> usize {
        self.mode_stack.len()
    }

    /// True if no instruction-mode frames are on the stack (root scope).
    #[must_use]
    pub fn mode_stack_is_empty(&self) -> bool {
        self.mode_stack.is_empty()
    }

    // ── CPU features (PA-r16-004-backtrack-a #1033) ──────────────────────

    /// Enable a specific CPU feature.
    pub fn enable_feature(&mut self, feat: CpuFeature) {
        self.enabled_features.insert(feat);
    }

    /// Read-only view of enabled CPU features.
    #[must_use]
    pub fn enabled_features(&self) -> &HashSet<CpuFeature> {
        &self.enabled_features
    }

    /// Read-only view of the unsafe-body → lambda mapping.
    /// #1139: Used to identify which lambda owns each unsafe body.
    #[must_use]
    pub fn unsafe_body_to_lambda(&self) -> &HashMap<u32, u32> {
        &self.unsafe_body_to_lambda
    }

    /// Mutable access to the instruction → lambda mapping.
    /// #1139: Used by UnsafeWalker to record which lambda owns each instruction.
    pub fn instr_to_lambda_mut(&mut self) -> &mut HashMap<IrNodeId, u32> {
        &mut self.instr_to_lambda
    }

    /// Mutable access to the per-lambda bindings map.
    /// #1139: Used by resolve_var_operands to look up scope snapshots for instructions.
    pub fn per_lambda_bindings_mut(&mut self) -> &mut HashMap<u32, LocalBindingTable> {
        &mut self.per_lambda_bindings
    }

    /// Read-only view of the per-lambda bindings map.
    /// #1139: Used by resolve_var_operands to look up scope snapshots for instructions.
    #[must_use]
    pub fn per_lambda_bindings(&self) -> &HashMap<u32, LocalBindingTable> {
        &self.per_lambda_bindings
    }

    /// Read-only view of the instruction → lambda mapping.
    /// #1139: Used by resolve_var_operands to look up which lambda owns each instruction.
    #[must_use]
    pub fn instr_to_lambda(&self) -> &HashMap<IrNodeId, u32> {
        &self.instr_to_lambda
    }

    /// Check if a specific CPU feature is enabled.
    #[must_use]
    pub fn has_feature(&self, feat: CpuFeature) -> bool {
        self.enabled_features.contains(&feat)
    }

    /// Replace the entire set of enabled CPU features.
    pub fn set_enabled_features(&mut self, feats: HashSet<CpuFeature>) {
        self.enabled_features = feats;
    }

    /// Phase 6 m4-003: Register a label at the current byte offset.
    ///
    /// Called during unsafe block lowering when a label definition is
    /// encountered. The label can then be referenced by forward or backward
    /// Jcc/Jmp instructions.
    pub fn register_label(&mut self, name: String) {
        self.labels.insert(name, self.estimated_offset);
    }

    /// Phase 6 m4-003: Compute rel32 displacement for a label reference.
    ///
    /// Used during encoding to resolve backward (already-defined) labels.
    /// Returns `Some(rel32)` if label is found, `None` otherwise.
    /// `rel32 = label_offset - (current_offset + instruction_size)`
    pub fn compute_label_rel32(
        &self,
        label_name: &str,
        current_offset: u32,
        instruction_size: u32,
    ) -> Option<i32> {
        self.labels.get(label_name).map(|&label_offset| {
            let rel = (label_offset as i64) - ((current_offset as i64) + (instruction_size as i64));
            rel as i32
        })
    }

    /// Phase 6 m3-001: Compute C-ABI natural-alignment layouts for all record
    /// types referenced in the IR, storing finalised layouts in
    /// `self.record_layouts`.
    ///
    /// Layout computation follows C ABI rules:
    /// - u8/i8: size 1, align 1
    /// - u16/i16: size 2, align 2 (Phase 13 m6-001)
    /// - u32/i32: size 4, align 4
    /// - u64/i64: size 8, align 8
    /// - *T (any pointer): size 8, align 8
    /// - Other types: rejected with diagnostic T0515
    ///
    /// Signedness is encoded in bit 4 of `field_size_byte_code`:
    /// - Low 4 bits: size code (1, 2, 4, 8)
    /// - Bit 4 (0x10): 1 if signed, 0 if unsigned
    ///
    /// Fields are placed at offsets that respect natural alignment (no
    /// explicit padding beyond alignment requirements). Struct alignment is
    /// the max of all field alignments.
    pub fn finalise_record_layouts(
        &mut self,
        record_types: &HashMap<RecordTypeId, Vec<(String, u8)>>,
    ) {
        for (&type_id, fields) in record_types {
            if fields.is_empty() {
                self.record_layouts
                    .insert(type_id, RecordLayout::new(0, 1, Vec::new()));
                continue;
            }

            let mut struct_align: u8 = 1;
            let mut current_offset: u64 = 0;
            let mut finalised_fields = Vec::new();
            let mut field_names_vec: Vec<String> = Vec::new();
            let mut valid = true;

            for (field_name, field_size_byte_code) in fields {
                let size_code = field_size_byte_code & 0x0F;
                let is_signed = (field_size_byte_code & 0x10) != 0;

                let (field_align, field_size) = match size_code {
                    1 => (1u8, 1u8),
                    2 => (2u8, 2u8),
                    4 => (4u8, 4u8),
                    8 => (8u8, 8u8),
                    _ => {
                        valid = false;
                        break;
                    }
                };

                struct_align = struct_align.max(field_align);

                current_offset = ((current_offset + (field_align as u64) - 1)
                    / (field_align as u64))
                    * (field_align as u64);

                finalised_fields.push(FieldLayout {
                    offset: current_offset,
                    size: field_size,
                    signed: is_signed,
                });

                field_names_vec.push(field_name.clone());
                current_offset += field_size as u64;
            }

            if valid {
                let struct_size = ((current_offset + (struct_align as u64) - 1)
                    / (struct_align as u64))
                    * (struct_align as u64);

                self.record_layouts.insert(
                    type_id,
                    RecordLayout::with_field_names(struct_size, struct_align, finalised_fields, field_names_vec),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finalise_record_layouts_mixed_widths() {
        // Phase 6 m3-001 (#1045 AC3): Test offset calculation for mixed-width fields.
        // Verifies C-ABI natural alignment for a struct with fields at different widths:
        // - Field a: u32 (size_code=4, size=4 bytes, align=4)
        // - Field b: u16 (size_code=2, size=2 bytes, align=2)
        // - Field c: ptr (size_code=8, size=8 bytes, align=8)
        //
        // Expected layout:
        // - a: offset 0 (no padding needed, starts at 0)
        // - b: offset 4 (u32 is 4 bytes, so b starts after, naturally aligned to 2)
        // - c: offset 8 (b is 2 bytes at offset 4, so b+2=6, but c needs align 8, so pad to 8)
        // - Struct size: 16 (c at 8, c is 8 bytes, so 8+8=16, already aligned to 8)
        // - Struct align: 8 (max of 4, 2, 8)

        let mut state = EmitPassState::default();

        let type_id = RecordTypeId(1);
        let mut record_types = HashMap::new();

        // Field a: u32 (size_code = 4, unsigned)
        // Field b: u16 (size_code = 2, unsigned)
        // Field c: ptr (size_code = 8, unsigned)
        let fields = vec![
            ("a".to_string(), 0x04u8), // size_code=4 (u32)
            ("b".to_string(), 0x02u8), // size_code=2 (u16)
            ("c".to_string(), 0x08u8), // size_code=8 (ptr)
        ];
        record_types.insert(type_id, fields);

        state.finalise_record_layouts(&record_types);

        // Verify layout was computed
        let layout = state.record_layout(type_id).expect("layout should exist");

        // Assert field count
        assert_eq!(
            layout.fields.len(),
            3,
            "struct should have 3 fields, got: {}",
            layout.fields.len()
        );

        // Assert field a: offset 0, size 4
        assert_eq!(
            layout.fields[0].offset, 0,
            "field a should be at offset 0, got: {}",
            layout.fields[0].offset
        );
        assert_eq!(
            layout.fields[0].size, 4,
            "field a should be size 4, got: {}",
            layout.fields[0].size
        );

        // Assert field b: offset 4, size 2
        assert_eq!(
            layout.fields[1].offset, 4,
            "field b should be at offset 4, got: {}",
            layout.fields[1].offset
        );
        assert_eq!(
            layout.fields[1].size, 2,
            "field b should be size 2, got: {}",
            layout.fields[1].size
        );

        // Assert field c: offset 8, size 8
        // (b ends at 4+2=6, but c needs alignment 8, so pad to 8)
        assert_eq!(
            layout.fields[2].offset, 8,
            "field c should be at offset 8, got: {}",
            layout.fields[2].offset
        );
        assert_eq!(
            layout.fields[2].size, 8,
            "field c should be size 8, got: {}",
            layout.fields[2].size
        );

        // Assert struct size and alignment
        assert_eq!(
            layout.size, 16,
            "struct size should be 16 (c at 8, size 8), got: {}",
            layout.size
        );
        assert_eq!(
            layout.align, 8,
            "struct alignment should be 8 (max of field aligns), got: {}",
            layout.align
        );
    }
}
