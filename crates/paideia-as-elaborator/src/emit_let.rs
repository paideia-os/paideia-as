//! Let-binding lowerers.
//!
//! Extracted from `emit_walker.rs` during the v0.17 refactor. Owns the
//! narrow width-resolution helper (`resolve_let_width`) plus the two
//! `visit_let_*` paths (`Literal` RHS and `FieldAccess` RHS). Other Let
//! shapes are dispatched from `walk_inner` and lowered elsewhere
//! (String RHS routes through populate_data_table, e.g.).

use paideia_as_ir::instruction::{Instruction, IntWidth, Mnemonic, Operand};
use paideia_as_ir::{IrArena, IrNodeId, SmallVec, abi};

use crate::emit_walker::EmitWalker;

impl EmitWalker {
    /// Resolve the bound integer width for a Let node, if width-threadable.
    ///
    /// Phase 7 m4-003 (PA7C-m4-003): reads the binding's recorded
    /// [`LetInfo::ty`](paideia_as_ir::LetInfo) from the arena's let-meta table,
    /// bridges the IR-local `TypeId` to the type interner's `TypeId`, and maps
    /// the resulting bit-width to an [`IntWidth`]. Returns `None` when the
    /// binding has no recorded type, the type is non-integer, or the width is
    /// not one of 8/16/32/64 — in every such case the caller keeps the generic
    /// 64-bit `Mov` path.
    pub(crate) fn resolve_let_width(
        arena: &IrArena,
        let_node_id: IrNodeId,
        typer: &paideia_as_types::TypeInterner,
    ) -> Option<IntWidth> {
        let ir_ty = arena.let_meta().get(let_node_id).and_then(|info| info.ty)?;
        // The IR-local TypeId mirrors the interner's TypeId raw value (the
        // interner index + 1); bridge across the crate boundary by raw value.
        let types_ty = paideia_as_types::TypeId::new(ir_ty.0)?;
        let bits = paideia_as_types::bit_width(typer, types_ty)?;
        IntWidth::from_bits(bits)
    }

    /// Emit instruction for Let with Literal RHS.
    ///
    /// Lowers `let x : u64 = imm` to:
    /// - `mov rax, imm32` (7 bytes) if imm fits in i32
    /// - `mov rax, imm64` (10 bytes) if imm requires full 64 bits
    ///
    /// Phase 7 m4-003 (PA7C-m4-003): when `width` resolves to a sub-64-bit
    /// integer width (`W8`/`W16`/`W32`), emit the narrower `MovSized` form
    /// instead — e.g. `let x : u32 = 42` becomes the 5-byte `B8 imm32` move
    /// rather than the generic 10-byte 64-bit move. `width` is `None`, or
    /// `Some(W64)`, for untyped/64-bit bindings, which keep the generic path.
    ///
    /// PA8-m3-001: this width-routing is now shared with the in-block let-literal
    /// sites (`emit_block_body` / `emit_block_body_arm`), which resolve their Let
    /// node's width via the same [`resolve_let_width`] helper. The remaining
    /// immediate-`Mov` sites cannot be routed without further infrastructure:
    /// synthetic lambda-body moves carry no Let/binding width, function-call
    /// argument setup has no callee-signature table to read the parameter width
    /// from, and every other peer site is a reg-reg or memory move that the
    /// `(Reg, Imm64)`-only `MovSized` form cannot encode at all.
    pub(crate) fn visit_let_literal(&mut self, let_node_id: IrNodeId, value: i64, width: Option<IntWidth>) {
        let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();

        // Destination: rax (abi::RAX).
        operands.push(Operand::Reg(abi::RAX));

        // Source: immediate value.
        operands.push(Operand::Imm64(value));

        // Choose mnemonic. A sub-64-bit width emits MovSized; otherwise
        // (None or W64) we preserve the established generic 64-bit Mov path.
        match width {
            Some(w @ (IntWidth::W8 | IntWidth::W16 | IntWidth::W32)) => {
                // Single-encoding widths: use established w.estimated_size() path.
                let inst = Instruction {
                    mnemonic: Mnemonic::MovSized { width: w },
                    operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                };

                // PA8-m1-002: Lambda entry recording is now handled by record_lambda_entry() in visit_lambda.
                // This legacy path is no longer needed.

                // Emit the instruction.
                let inst_size = w.estimated_size();
                self.state.instructions.insert(let_node_id, inst);
                self.state.estimated_offset += inst_size;
            }
            _ => {
                // W64 or None: use generic Mov. The encoder now emits 7-byte C7 imm32 form
                // for i32-range values instead of 10-byte B8 movabs. Use emit_inst which
                // automatically calls paideia_as_encoder::estimated_bytes to get the correct size.
                let inst = Instruction {
                    mnemonic: Mnemonic::Mov,
                    operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                };

                self.emit_inst(let_node_id, inst);
            }
        }
    }

    /// Phase 6 m3-003: Emit instruction for Let with FieldAccess RHS.
    ///
    /// Handles in-block field bindings by assigning scratch registers in sequence:
    /// RAX(0), RCX(1), RDX(2), R8(8). After 4 in-flight bindings, fires T0517 via
    /// the typed diagnostic pipe.
    ///
    /// Delegates to visit_field_access_with_reg to emit the mov instruction
    /// to the assigned scratch register instead of RAX.
    pub(crate) fn visit_let_field_access(
        &mut self,
        _let_node_id: IrNodeId,
        field_access_id: IrNodeId,
        arena: &IrArena,
    ) {
        // Scratch register sequence (calling-convention scratch registers).
        let scratch_regs = [abi::RAX, abi::RCX, abi::RDX, abi::R8]; // RAX, RCX, RDX, R8

        // Check if we've exceeded register pressure.
        if self.state.scratch_count() >= scratch_regs.len() {
            // Fire T0517: register pressure exceeded (typed diagnostic).
            let code = "T0517".parse::<paideia_as_diagnostics::DiagnosticCode>()
                .expect("T0517 is a valid diagnostic code");
            let message = format!(
                "register pressure exceeded: more than {} in-flight bindings",
                scratch_regs.len()
            );
            self.push_typed_diag(code, message);
            return;
        }

        // Assign the next scratch register.
        let scratch_reg = scratch_regs[self.state.scratch_count()];
        self.state.assign_scratch(scratch_reg);

        // Emit the field access with the assigned scratch register.
        self.visit_field_access_with_reg(field_access_id, scratch_reg, arena);
    }
}
