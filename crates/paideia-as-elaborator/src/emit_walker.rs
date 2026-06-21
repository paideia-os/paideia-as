//! EmitWalker — Phase 5 m1-001 entry to the build-emit pipeline.
//!
//! Walks the IR; per-construct lowering (m1-002 Let-literal, m1-003 Lambda,
//! m1-004 Unsafe) lands as siblings inside this module. The walker
//! populates an InstructionSideTable + tracks per-function offsets.

use paideia_as_ir::instruction::{Instruction, InstructionSideTable, Mnemonic, Operand, RegId};
use paideia_as_ir::{IrArena, IrKind, IrNodeId, SmallVec};
use std::collections::HashMap;

/// Tracks emission state during IR traversal.
///
/// Accumulates instructions keyed by IrNodeId and tracks byte offsets
/// for function-level metadata used by downstream m5-m6 phases.
#[derive(Default, Debug)]
pub struct EmitPassState {
    /// The emitted instructions, keyed by IrNodeId, per the existing
    /// Phase-3 m2-001 InstructionSideTable convention.
    pub instructions: InstructionSideTable,

    /// IrNodeId of the function currently being lowered (or 0 if none).
    pub current_function: u32,

    /// Byte offset within the current function. Reset to 0 on each
    /// new function entry. m5 (symbols + relocs) will consume this
    /// to populate function-symbol size metadata.
    pub current_offset: u32,

    /// IrNodeId of the lambda/function -> first instruction's IrNodeId.
    /// Allows m6 end-to-end smoke to verify byte offsets.
    pub function_offsets: HashMap<u32, u32>,
}

/// EmitWalker — drives IR traversal and instruction emission.
///
/// Skeleton implementation for Phase 5 m1-001. Per-construct lowering
/// hooks (visit_let, visit_lambda, visit_unsafe) land in m1-002..004
/// as siblings of this walker.
pub struct EmitWalker {
    state: EmitPassState,
    diagnostics: Vec<String>,
}

impl EmitWalker {
    /// Create a new, empty EmitWalker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: EmitPassState::default(),
            diagnostics: Vec::new(),
        }
    }

    /// Access the emission state (read-only).
    #[must_use]
    pub fn state(&self) -> &EmitPassState {
        &self.state
    }

    /// Access the emission state (mutable).
    #[must_use]
    pub fn state_mut(&mut self) -> &mut EmitPassState {
        &mut self.state
    }

    /// Access the accumulated diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    /// Drive the walker over an IR arena.
    ///
    /// m1-002: processes Let → Literal bindings, emitting Mov instructions.
    pub fn walk(&mut self, arena: &IrArena) {
        // Iterate over all nodes, looking for Let nodes.
        for i in 1..=arena.len() as u32 {
            if let Some(node_id) = IrNodeId::new(i) {
                if let Some(node) = arena.get(node_id) {
                    if node.kind == IrKind::Let {
                        // Get the single child (the RHS expression).
                        let children = arena.children(node_id);
                        if let Some(&rhs_id) = children.first() {
                            if let Some(rhs_node) = arena.get(rhs_id) {
                                if rhs_node.kind == IrKind::Literal {
                                    // Check if we have a literal value for this node.
                                    if let Some(value) = arena.literal_values().get(rhs_id) {
                                        self.visit_let_literal(node_id, value);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Emit instruction for Let with Literal RHS.
    ///
    /// Lowers `let x : u64 = imm` to:
    /// - `mov rax, imm32` (7 bytes) if imm fits in i32
    /// - `mov rax, imm64` (10 bytes) if imm requires full 64 bits
    fn visit_let_literal(&mut self, let_node_id: IrNodeId, value: i64) {
        let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();

        // Destination: rax (RegId(0)).
        operands.push(Operand::Reg(RegId(0)));

        // Source: immediate value.
        operands.push(Operand::Imm64(value));

        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands,
            encoding_hint: None,
        };

        // Calculate instruction size:
        // - i32 encoding: 7 bytes (48 c7 c0 <imm32 LE>)
        // - i64 encoding: 10 bytes (48 b8 <imm64 LE>)
        let inst_size = if value >= i32::MIN as i64 && value <= i32::MAX as i64 {
            7
        } else {
            10
        };

        // Record function entry on first emission if needed.
        if self.state.current_function > 0 && self.state.current_offset == 0 {
            self.state
                .function_offsets
                .insert(self.state.current_function, let_node_id.get());
        }

        // Emit the instruction.
        self.state.instructions.insert(let_node_id, inst);

        // Bump offset.
        self.state.current_offset += inst_size;
    }
}

impl Default for EmitWalker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{FileId, Span};

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn emit_walker_new_starts_empty() {
        let walker = EmitWalker::new();
        assert!(walker.state().instructions.is_empty());
        assert_eq!(walker.state().current_function, 0);
        assert_eq!(walker.state().current_offset, 0);
        assert!(walker.state().function_offsets.is_empty());
    }

    #[test]
    fn emit_walker_walk_on_empty_arena_emits_zero_diagnostics() {
        let mut walker = EmitWalker::new();
        let arena = IrArena::new();
        walker.walk(&arena);
        assert!(walker.diagnostics().is_empty());
    }

    #[test]
    fn emit_pass_state_default_is_clean() {
        let state = EmitPassState::default();
        assert!(state.instructions.is_empty());
        assert_eq!(state.current_function, 0);
        assert_eq!(state.current_offset, 0);
        assert!(state.function_offsets.is_empty());
    }

    #[test]
    fn emit_walker_lets_literal_42_emits_7_byte_mov() {
        let mut arena = IrArena::new();

        // Allocate: Literal node, then Let with Literal as child.
        let lit_id = arena.alloc(IrKind::Literal, span());
        let let_id = arena.alloc_with_children(IrKind::Let, span(), [lit_id]);

        // Register the literal value 42.
        arena.literal_values_mut().insert(lit_id, 42);

        // Walk the arena.
        let mut walker = EmitWalker::new();
        walker.walk(&arena);

        // Verify instruction was emitted.
        let inst = walker
            .state()
            .instructions
            .get(let_id)
            .expect("instruction should be emitted");
        assert_eq!(inst.mnemonic, Mnemonic::Mov);
        assert_eq!(inst.operands.len(), 2);
        assert_eq!(inst.operands[0], Operand::Reg(RegId(0))); // rax
        assert_eq!(inst.operands[1], Operand::Imm64(42));

        // Verify offset advanced by 7 bytes (32-bit immediate encoding).
        assert_eq!(walker.state().current_offset, 7);
    }

    #[test]
    fn emit_walker_lets_literal_64bit_emits_10_byte_mov() {
        let mut arena = IrArena::new();

        // Allocate: Literal node, then Let with Literal as child.
        let lit_id = arena.alloc(IrKind::Literal, span());
        let let_id = arena.alloc_with_children(IrKind::Let, span(), [lit_id]);

        // Register the literal value 0xCAFE_F00D_DEAD_BEEF (as signed i64).
        let value = 0xCAFE_F00D_DEAD_BEEFu64 as i64;
        arena.literal_values_mut().insert(lit_id, value);

        // Walk the arena.
        let mut walker = EmitWalker::new();
        walker.walk(&arena);

        // Verify instruction was emitted.
        let inst = walker
            .state()
            .instructions
            .get(let_id)
            .expect("instruction should be emitted");
        assert_eq!(inst.mnemonic, Mnemonic::Mov);
        assert_eq!(inst.operands.len(), 2);
        assert_eq!(inst.operands[0], Operand::Reg(RegId(0))); // rax
        assert_eq!(inst.operands[1], Operand::Imm64(value));

        // Verify offset advanced by 10 bytes (64-bit immediate encoding).
        assert_eq!(walker.state().current_offset, 10);
    }
}
