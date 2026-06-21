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
    /// m1-003: processes Lambda bodies, emitting Mov/Lea/Ret for simple cases.
    pub fn walk(&mut self, arena: &IrArena) {
        // Iterate over all nodes, looking for Let and Lambda nodes.
        for i in 1..=arena.len() as u32 {
            if let Some(node_id) = IrNodeId::new(i) {
                if let Some(node) = arena.get(node_id) {
                    match node.kind {
                        IrKind::Let => {
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
                        IrKind::Lambda => {
                            // Lambda lowering: emit Mov/Lea/Ret for simple cases.
                            self.visit_lambda(node_id, arena);
                        }
                        _ => {}
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

    /// Emit instructions for Lambda body lowering (m1-003).
    ///
    /// Handles three cases:
    /// 1. Identity: `fn (x) -> x` → `mov rax, rdi; ret` (5 bytes: `48 89 f8 c3`)
    /// 2. Double: `fn (x) -> x + x` → `lea rax, [rdi + rdi]; ret` (5 bytes: `48 8d 04 3f c3`)
    /// 3. Add-immediate: `fn (x) -> x + N` → `lea rax, [rdi + N]; ret` (5 bytes: `48 8d 47 NN c3`)
    /// Other lambda shapes are deferred to m1-004+.
    fn visit_lambda(&mut self, lambda_node_id: IrNodeId, arena: &IrArena) {
        // Record the lambda's starting offset (current position in the emitted code).
        self.state
            .function_offsets
            .insert(lambda_node_id.get(), self.state.current_offset);

        // Get the body (Lambda has exactly one child).
        let children = arena.children(lambda_node_id);
        if let Some(&body_id) = children.first() {
            if let Some(body_node) = arena.get(body_id) {
                match body_node.kind {
                    // Case 1: Identity function `fn (x) -> x`
                    IrKind::Var => {
                        self.emit_identity_lambda(lambda_node_id);
                    }
                    // Case 2 & 3: Application `fn (x) -> x + ...` or `fn (x) -> ... + x`
                    IrKind::App => {
                        let app_children = arena.children(body_id);
                        // App has structure: [callee, arg0, arg1, ...]
                        if app_children.len() >= 3 {
                            let callee_id = app_children[0];
                            let arg0_id = app_children[1];
                            let arg1_id = app_children[2];

                            // Check if callee is the + builtin.
                            if let Some(callee_node) = arena.get(callee_id) {
                                if callee_node.kind == IrKind::Var {
                                    // We assume this is +; ideally we'd check a builtin registry.
                                    // For now, we inspect the arguments.
                                    if let (Some(arg0_node), Some(arg1_node)) =
                                        (arena.get(arg0_id), arena.get(arg1_id))
                                    {
                                        match (arg0_node.kind, arg1_node.kind) {
                                            // Case 2: x + x (double)
                                            (IrKind::Var, IrKind::Var) => {
                                                self.emit_double_lambda(lambda_node_id);
                                            }
                                            // Case 3: x + literal
                                            (IrKind::Var, IrKind::Literal) => {
                                                if let Some(value) = arena.literal_values().get(arg1_id)
                                                {
                                                    self.emit_add_imm_lambda(lambda_node_id, value);
                                                }
                                            }
                                            // Case 3 (reversed): literal + x
                                            (IrKind::Literal, IrKind::Var) => {
                                                if let Some(value) = arena.literal_values().get(arg0_id)
                                                {
                                                    self.emit_add_imm_lambda(lambda_node_id, value);
                                                }
                                            }
                                            _ => {
                                                // Other shapes deferred to m1-004+
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {
                        // Other lambda shapes deferred to m1-004+
                    }
                }
            }
        }
    }

    /// Emit identity lambda: `mov rax, rdi; ret` (5 bytes).
    fn emit_identity_lambda(&mut self, lambda_node_id: IrNodeId) {
        // Mov rax, rdi: 48 89 f8 (3 bytes)
        let mut mov_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        mov_operands.push(Operand::Reg(RegId(0))); // rax
        mov_operands.push(Operand::Reg(RegId(7))); // rdi (arg0)

        let mov_inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: mov_operands,
            encoding_hint: None,
        };

        self.state.instructions.insert(lambda_node_id, mov_inst);
        self.state.current_offset += 3;

        // Ret: c3 (1 byte)
        // Ret: c3 (1 byte)
        // We record the Ret as a separate "virtual" node (use a derived id or skip).
        // For now, we'll skip recording Ret separately and just bump the offset.
        self.state.current_offset += 1;
    }

    /// Emit double lambda: `lea rax, [rdi + rdi]; ret` (5 bytes).
    fn emit_double_lambda(&mut self, lambda_node_id: IrNodeId) {
        // Lea rax, [rdi + rdi]: 48 8d 04 3f (4 bytes)
        let mut lea_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        lea_operands.push(Operand::Reg(RegId(0))); // rax (destination)
        lea_operands.push(Operand::MemSib {
            base: RegId(7),        // rdi
            index: Some(RegId(7)), // rdi
            scale: paideia_as_ir::instruction::Scale::X1,
            disp: 0,
        });

        let lea_inst = Instruction {
            mnemonic: Mnemonic::Lea,
            operands: lea_operands,
            encoding_hint: None,
        };

        self.state.instructions.insert(lambda_node_id, lea_inst);
        self.state.current_offset += 4;

        // Ret: c3 (1 byte)
        self.state.current_offset += 1;
    }

    /// Emit add-immediate lambda: `lea rax, [rdi + imm]; ret`.
    /// For small immediates (disp8, -128..127), this is 4 bytes (48 8d 47 NN).
    /// Larger immediates require disp32 (7 bytes).
    fn emit_add_imm_lambda(&mut self, lambda_node_id: IrNodeId, imm: i64) {
        // Clamp to disp8 range if applicable.
        let disp = if imm >= -128 && imm <= 127 {
            imm as i32
        } else {
            // For now, only handle disp8; larger immediates can be deferred.
            return;
        };

        // Lea rax, [rdi + disp8]: 48 8d 47 NN (4 bytes)
        let mut lea_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        lea_operands.push(Operand::Reg(RegId(0))); // rax
        lea_operands.push(Operand::MemSib {
            base: RegId(7), // rdi
            index: None,
            scale: paideia_as_ir::instruction::Scale::X1,
            disp,
        });

        let lea_inst = Instruction {
            mnemonic: Mnemonic::Lea,
            operands: lea_operands,
            encoding_hint: None,
        };

        self.state.instructions.insert(lambda_node_id, lea_inst);
        self.state.current_offset += 4;

        // Ret: c3 (1 byte)
        self.state.current_offset += 1;
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

    // ── Lambda lowering tests (m1-003) ──────────────────────────────────

    #[test]
    fn emit_walker_lambda_identity_emits_mov_rax_rdi_ret() {
        let mut arena = IrArena::new();

        // Allocate: Var node (the body), then Lambda with Var as child.
        let var_id = arena.alloc(IrKind::Var, span());
        let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [var_id]);

        // Walk the arena.
        let mut walker = EmitWalker::new();
        walker.walk(&arena);

        // Verify instruction was emitted for the lambda.
        let inst = walker
            .state()
            .instructions
            .get(lambda_id)
            .expect("instruction should be emitted");
        assert_eq!(inst.mnemonic, Mnemonic::Mov);
        assert_eq!(inst.operands.len(), 2);
        assert_eq!(inst.operands[0], Operand::Reg(RegId(0))); // rax
        assert_eq!(inst.operands[1], Operand::Reg(RegId(7))); // rdi

        // Verify offset: 3 bytes for mov + 1 byte for ret = 4 bytes.
        // (We track offset before recording ret separately)
        assert_eq!(walker.state().current_offset, 4);

        // Verify lambda offset recorded.
        assert!(
            walker
                .state()
                .function_offsets
                .contains_key(&lambda_id.get())
        );
    }

    #[test]
    fn emit_walker_lambda_double_emits_lea_rdi_rdi_ret() {
        let mut arena = IrArena::new();

        // Allocate: Var nodes for both operands, then App with [callee, arg0, arg1].
        // Assume callee is +.
        let callee_id = arena.alloc(IrKind::Var, span());
        let arg0_id = arena.alloc(IrKind::Var, span());
        let arg1_id = arena.alloc(IrKind::Var, span());
        let app_id = arena.alloc_with_children(IrKind::App, span(), [callee_id, arg0_id, arg1_id]);

        // Allocate Lambda with App as body.
        let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [app_id]);

        // Walk the arena.
        let mut walker = EmitWalker::new();
        walker.walk(&arena);

        // Verify instruction was emitted for the lambda.
        let inst = walker
            .state()
            .instructions
            .get(lambda_id)
            .expect("instruction should be emitted");
        assert_eq!(inst.mnemonic, Mnemonic::Lea);
        assert_eq!(inst.operands.len(), 2);
        assert_eq!(inst.operands[0], Operand::Reg(RegId(0))); // rax

        // Check MemSib: [rdi + rdi]
        match inst.operands[1] {
            Operand::MemSib {
                base,
                index,
                scale,
                disp,
            } => {
                assert_eq!(base, RegId(7)); // rdi
                assert_eq!(index, Some(RegId(7))); // rdi
                assert_eq!(scale, paideia_as_ir::instruction::Scale::X1);
                assert_eq!(disp, 0);
            }
            _ => panic!("Expected MemSib operand"),
        }

        // Verify offset: 4 bytes for lea + 1 byte for ret = 5 bytes.
        assert_eq!(walker.state().current_offset, 5);

        // Verify lambda offset recorded.
        assert!(
            walker
                .state()
                .function_offsets
                .contains_key(&lambda_id.get())
        );
    }

    #[test]
    fn emit_walker_lambda_add_one_emits_lea_rdi_1_ret() {
        let mut arena = IrArena::new();

        // Allocate: Var (arg0), Literal (1), and App with [callee, arg0, lit].
        let callee_id = arena.alloc(IrKind::Var, span());
        let arg0_id = arena.alloc(IrKind::Var, span());
        let lit_id = arena.alloc(IrKind::Literal, span());
        let app_id = arena.alloc_with_children(IrKind::App, span(), [callee_id, arg0_id, lit_id]);

        // Register the literal value 1.
        arena.literal_values_mut().insert(lit_id, 1);

        // Allocate Lambda with App as body.
        let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [app_id]);

        // Walk the arena.
        let mut walker = EmitWalker::new();
        walker.walk(&arena);

        // Verify instruction was emitted for the lambda.
        let inst = walker
            .state()
            .instructions
            .get(lambda_id)
            .expect("instruction should be emitted");
        assert_eq!(inst.mnemonic, Mnemonic::Lea);
        assert_eq!(inst.operands.len(), 2);
        assert_eq!(inst.operands[0], Operand::Reg(RegId(0))); // rax

        // Check MemSib: [rdi + 1]
        match inst.operands[1] {
            Operand::MemSib {
                base,
                index,
                scale,
                disp,
            } => {
                assert_eq!(base, RegId(7)); // rdi
                assert_eq!(index, None);
                assert_eq!(scale, paideia_as_ir::instruction::Scale::X1);
                assert_eq!(disp, 1);
            }
            _ => panic!("Expected MemSib operand"),
        }

        // Verify offset: 4 bytes for lea + 1 byte for ret = 5 bytes.
        assert_eq!(walker.state().current_offset, 5);

        // Verify lambda offset recorded.
        assert!(
            walker
                .state()
                .function_offsets
                .contains_key(&lambda_id.get())
        );
    }
}
