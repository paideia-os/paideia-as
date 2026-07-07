//! Control-flow lowerers: `if`/`else`, `while`, and `loop`.
//!
//! Extracted from `emit_walker.rs` during the v0.17 refactor. Owns the
//! visit paths for `IrKind::Branch`, `IrKind::While`, and `IrKind::Loop`,
//! including label generation, condition tests, and jump emission.

use paideia_as_ir::instruction::{Cond, Instruction, Mnemonic, Operand};
use paideia_as_ir::{IrArena, IrNodeId, SmallVec, abi};

use crate::emit_pass_state::LoopContext;
use crate::emit_walker::EmitWalker;

impl EmitWalker {
    /// Phase 7 m1-001: Emit if-then-else expression lowering (IrKind::Branch).
    ///
    /// Handles three cases:
    /// 1. `if x { then_block }` (no else): emit test + jz end + then_block + end_label
    /// 2. `if x { then_block } else { else_block }`: emit test + jz else + then_block + jmp end + else_label + else_block + end_label
    /// 3. Nested if-else: each Branch node gets its own label triplet
    ///
    /// Branch node children: [condition, then_body, else_body (optional)]
    /// Labels are generated per node: if_then_{node_id}, if_else_{node_id}, if_end_{node_id}
    /// Label resolution is deferred to Phase 6 m4-004 (label patcher).
    pub(crate) fn visit_branch(&mut self, branch_node_id: IrNodeId, arena: &IrArena) {
        let children = arena.children(branch_node_id);
        if children.len() < 2 {
            // Malformed Branch node (needs at least condition + then_body).
            self.diagnostics.push(format!(
                "Branch node {} has {} children; expected at least 2",
                branch_node_id.get(),
                children.len()
            ));
            return;
        }

        let _cond_id = children[0];
        let _then_id = children[1];
        let else_id = if children.len() > 2 {
            Some(children[2])
        } else {
            None
        };

        // Generate label names unique per branch node.
        let then_label = format!("if_then_{}", branch_node_id.get());
        let else_label = format!("if_else_{}", branch_node_id.get());
        let end_label = format!("if_end_{}", branch_node_id.get());

        // Emit TEST instruction: test rdi, rdi (3 bytes: 48 85 FF)
        // Phase 7 m1-001 minimum: assume condition is in rdi (first argument).
        // Full type-directed encoding (cmp vs test) deferred to phase 8.
        let test_id = IrNodeId::new(branch_node_id.get() * 3).expect("test instr id");
        let mut test_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        test_operands.push(Operand::Reg(abi::RDI)); // rdi
        test_operands.push(Operand::Reg(abi::RDI)); // rdi

        let test_inst = Instruction {
            mnemonic: Mnemonic::Test,
            operands: test_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
        };

        self.emit_inst(test_id, test_inst);

        // Emit conditional jump (jz): Jump if zero to else-label (or end if no else).
        let target_label = if else_id.is_some() {
            &else_label
        } else {
            &end_label
        };
        let jz_id = IrNodeId::new(branch_node_id.get() * 3 + 1).expect("jz instr id");
        let mut jz_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        jz_operands.push(Operand::LabelRef {
            name: target_label.clone(),
            addend: 0,
        });

        let jz_inst = Instruction {
            mnemonic: Mnemonic::Jcc(Cond::Zero),
            operands: jz_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
        };

        self.emit_inst(jz_id, jz_inst);

        // Register then_label at current offset.
        self.state.register_label(then_label);

        // Placeholder: emit then_block instructions.
        // Phase 7: actual block emission deferred to full block lowering in m1-002+.
        // For now, we just track the label position.

        if let Some(_else_id) = else_id {
            // Else branch exists: emit jmp to end_label after then_block.
            let jmp_id = IrNodeId::new(branch_node_id.get() * 3 + 2).expect("jmp instr id");
            let mut jmp_operands: SmallVec<[Operand; 3]> = SmallVec::new();
            jmp_operands.push(Operand::LabelRef {
                name: end_label.clone(),
                addend: 0,
            });

            let jmp_inst = Instruction {
                mnemonic: Mnemonic::Jmp,
                operands: jmp_operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
            };

            self.emit_inst(jmp_id, jmp_inst);

            // Register else_label.
            self.state.register_label(else_label);

            // Placeholder: emit else_block instructions.
            // Phase 7: actual block emission deferred.
        }

        // Register end_label.
        self.state.register_label(end_label);
    }

    /// Phase 7 m1-002: Emit while-loop lowering.
    ///
    /// Lowers `while x < 10 { x = x + 1 }` to:
    /// - top_label: (at offset O)
    /// - test rdi, rdi (3 bytes, offset O -> O+3)
    /// - jnz exit_label (6 bytes, offset O+3 -> O+9)
    /// - [body emitted elsewhere]
    /// - jmp top_label (5 bytes)
    /// - exit_label: (at final offset)
    ///
    /// break jumps to exit_label; continue jumps to top_label.
    pub(crate) fn visit_while(&mut self, while_node_id: IrNodeId, arena: &IrArena) {
        let children = arena.children(while_node_id);
        // PA-R17-012c: While nodes inside unsafe blocks have zero children (child transfer skipped).
        // While nodes in normal function bodies have children [condition, body].
        if children.is_empty() {
            // Zero children: this is a While inside an unsafe block.
            // Do not emit any control flow instructions; the unsafe block will handle
            // the statements independently. This prevents label conflicts and ensures
            // the unsafe_walker processes the While's contents directly.
            return;
        }

        if children.len() < 2 {
            // Malformed While node (expected 2+ children for normal bodies).
            self.diagnostics.push(format!(
                "While node {} has {} children; expected 2 (condition + body)",
                while_node_id.get(),
                children.len()
            ));
            return;
        }

        let _cond_id = if children.len() >= 1 { Some(children[0]) } else { None };
        let _body_id = if children.len() >= 2 { Some(children[1]) } else { None };

        // Generate label names unique per while node.
        let top_label = format!("while_top_{}", while_node_id.get());
        let exit_label = format!("while_exit_{}", while_node_id.get());

        // Register top_label at current offset.
        self.state.register_label(top_label.clone());

        // Emit TEST instruction: test rdi, rdi (3 bytes: 48 85 FF)
        // Phase 7 m1-002 minimum: assume condition is in rdi (first argument).
        let test_id = IrNodeId::new(while_node_id.get() * 4).expect("test instr id");
        let mut test_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        test_operands.push(Operand::Reg(abi::RDI)); // rdi
        test_operands.push(Operand::Reg(abi::RDI)); // rdi

        let test_inst = Instruction {
            mnemonic: Mnemonic::Test,
            operands: test_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
        };

        self.emit_inst(test_id, test_inst);

        // Emit conditional jump (jnz): Jump if NOT zero to exit_label.
        let jnz_id = IrNodeId::new(while_node_id.get() * 4 + 1).expect("jnz instr id");
        let mut jnz_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        jnz_operands.push(Operand::LabelRef {
            name: exit_label.clone(),
            addend: 0,
        });

        let jnz_inst = Instruction {
            mnemonic: Mnemonic::Jcc(Cond::NonZero),
            operands: jnz_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
        };

        self.emit_inst(jnz_id, jnz_inst);

        // Placeholder: emit body instructions.
        // Phase 7: actual body emission deferred.
        // After body, emit unconditional jump back to top_label.

        // Emit unconditional jump (jmp) to top_label (5 bytes: E9 XX XX XX XX)
        let jmp_id = IrNodeId::new(while_node_id.get() * 4 + 2).expect("jmp instr id");
        let mut jmp_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        jmp_operands.push(Operand::LabelRef {
            name: top_label,
            addend: 0,
        });

        let jmp_inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: jmp_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
        };

        self.emit_inst(jmp_id, jmp_inst);

        // Register exit_label at final offset.
        self.state.register_label(exit_label.clone());

        // Push While context for break validation.
        self.loop_contexts.push((LoopContext::While, exit_label));
        // (Pop happens after body processing, deferred in full elaboration)
    }

    /// Phase 7 m1-008 (PA7-008): Emit infinite loop lowering for loop { ... } expressions.
    ///
    /// Infinite loops produce values via break. Lowers `loop { body; break value }` to:
    /// - top_label: [body]
    /// - jmp top (5 bytes: E9 fixup top)
    /// - exit_label: (break value returns via RAX)
    ///
    /// Structure: Loop has single child [body]. Tracks loop context for break validation.
    /// - loop { hlt } emits top: F4 ; E9 fixup top
    /// - loop { if cond { break 42 } } emits top_label, body, break-via-jmp, exit_label
    ///
    /// Validation:
    /// - break outside loop → T0524 ("break outside loop body")
    /// - break value in while context → T0525 ("break value in unit-typed loop")
    pub(crate) fn visit_loop(&mut self, loop_node_id: IrNodeId, arena: &IrArena) {
        let children = arena.children(loop_node_id);
        // PA-R17-012c: Loop nodes inside unsafe blocks have zero children (child transfer skipped).
        // Loop nodes in normal function bodies have children [body].
        if children.is_empty() {
            // This could be a Loop inside an unsafe block (child transfer skipped).
            // Do not emit any control flow instructions; the unsafe block will handle
            // the statements independently.
            return;
        }

        let _body_id = children[0];

        // Generate label names unique per loop node.
        let top_label = format!("loop_top_{}", loop_node_id.get());
        let exit_label = format!("loop_exit_{}", loop_node_id.get());

        // Register top_label at current offset.
        self.state.register_label(top_label.clone());

        // Placeholder: emit body instructions.
        // Phase 7: actual body emission deferred.
        // After body, emit unconditional jump back to top_label.

        // Emit unconditional jump (jmp) to top_label (5 bytes: E9 XX XX XX XX)
        let jmp_id = IrNodeId::new(loop_node_id.get() * 4).expect("jmp instr id");
        let mut jmp_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        jmp_operands.push(Operand::LabelRef {
            name: top_label,
            addend: 0,
        });

        let jmp_inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: jmp_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
        };

        self.emit_inst(jmp_id, jmp_inst);

        // Register exit_label at final offset.
        self.state.register_label(exit_label.clone());

        // Push Loop context for break validation.
        self.loop_contexts.push((LoopContext::Loop, exit_label));
        // (Pop happens after body processing, deferred in full elaboration)
    }
}
