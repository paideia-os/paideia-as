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

use paideia_as_ir::instruction::{InstrMode, InstructionSideTable, RegId};
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
#[derive(Default, Debug)]
pub struct EmitPassState {
    /// The emitted instructions, keyed by IrNodeId, per the existing
    /// Phase-3 m2-001 InstructionSideTable convention.
    pub instructions: InstructionSideTable,

    /// IrNodeId of the function currently being lowered (or 0 if none).
    pub current_function: u32,

    /// Estimated byte offset within the current function. Reset to 0 on each
    /// new function entry. This is an advisory estimate based on instruction
    /// mnemonics and is verified to match the actual encoded byte count at
    /// the end of the build (phase-7-m1-003). m5 (symbols + relocs) will
    /// consume the actual offsets from Instruction.byte_offset_in_text.
    pub estimated_offset: u32,

    /// Lambda IR node id -> estimated byte offset within function.
    /// Populated by record_lambda_entry_with_offset during lambda emission.
    /// Used to compute function symbols' st_value in cmd_build.
    pub function_offsets: HashMap<u32, u32>,

    /// Lambda IR node id -> IrNodeId of its first emitted instruction.
    /// Populated by record_lambda_entry. Resolved to byte offsets
    /// post-encoding via EmitResult.offset_map (future use).
    pub lambda_first_instr: HashMap<u32, IrNodeId>,

    /// IrNodeIds of Lambdas that actually emitted bytecode.
    /// Used to filter out symbols for non-emitting lambdas.
    pub emitted_lambdas: HashSet<u32>,

    /// IrNodeIds of IrKind::Unsafe nodes encountered during the walk.
    /// m3 UnsafeWalker drains this via take_pending_unsafe() and lowers
    /// the block contents.
    pub pending_unsafe_blocks: Vec<u32>,

    /// Phase 6 m3-001: C-ABI natural-alignment record layouts,
    /// keyed by RecordTypeId. Populated by finalise_record_layouts().
    pub record_layouts: HashMap<RecordTypeId, RecordLayout>,

    /// PA-r17-007: Enum layouts keyed by EnumTypeId.
    /// Populated during emission pass; consumed by visit_enum_cons.
    pub enum_layouts: HashMap<EnumTypeId, EnumLayout>,

    /// Phase 6 m3-003: Scratch register assignment for in-block field bindings.
    /// Tracks which scratch registers have been assigned in the current
    /// function. Reset to empty at function entry. Sequence: RAX(0), RCX(1),
    /// RDX(2), R8(8).
    pub scratch_assignment: Vec<RegId>,

    /// Phase 6 m4-003: Label name → byte offset mapping.
    /// Populated during unsafe block lowering when labels are encountered.
    /// Used to resolve backward label references at encoding time.
    /// Scoped to the current function; reset at function entry.
    pub labels: HashMap<String, u32>,

    /// Phase 6 m4-004: Label name → instruction IR node ID mapping.
    /// Populated from unsafe_walker output, used to compute actual label
    /// offsets based on instruction offsets from the encoder's offset_map.
    pub label_to_instr: HashMap<String, IrNodeId>,

    /// PA8-m1-002b: Unsafe lambda IR node id → index in pending_unsafe_blocks.
    /// Maps each unsafe-bodied lambda to its position in the pending list,
    /// allowing us to look up its first instruction from UnsafeWalker's
    /// first_instrs vec.
    pub unsafe_lambda_to_pending_idx: HashMap<u32, usize>,

    /// PA8-m1-002b: Unsafe body IR node id → lambda IR node id.
    /// Used to track which lambda has which unsafe body during the walk.
    pub unsafe_body_to_lambda: HashMap<u32, u32>,

    /// Phase 7 m1-001: Local binding table for multi-statement function bodies.
    /// Maps binding names (from let-statements) to their assigned scratch
    /// registers. Scoped to the current function; reset at function entry.
    pub local_bindings: LocalBindingTable,

    /// Stack of instruction modes during nested scope walk.
    /// Used to propagate #![bits=32] or #![bits=64] from module inner_attrs.
    pub mode_stack: Vec<InstrMode>,
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
            let mut valid = true;

            for (_field_name, field_size_byte_code) in fields {
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

                current_offset += field_size as u64;
            }

            if valid {
                let struct_size = ((current_offset + (struct_align as u64) - 1)
                    / (struct_align as u64))
                    * (struct_align as u64);

                self.record_layouts.insert(
                    type_id,
                    RecordLayout::new(struct_size, struct_align, finalised_fields),
                );
            }
        }
    }
}
