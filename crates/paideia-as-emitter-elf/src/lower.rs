//! IR → bytes lowering per `custom-assembler.md` §6.4.
//!
//! Walks the **effect-rewritten** IR (PR-51 output) and emits the
//! corresponding x86_64 bytes:
//!
//! - `Let(Literal)` → `mov reg, imm` materialising the literal.
//! - `App` of a known mnemonic in an action block → that mnemonic's
//!   encoding.
//! - An `App` representing a handler indirect call (from PR-51) →
//!   `mov rax, [r15 + offset] ; call rax` per `calling-convention.md`
//!   §4.2.
//!
//! Phase-1 simplification: the IR doesn't yet carry rich
//! literal/operand data on each node. The public entry point here is
//! parameterised on a small `LowerCtx` describing the function's
//! shape — the IR walker fills it in. Tests exercise the per-shape
//! lowering helpers directly.

use paideia_as_ir::{IrArena, IrKind, IrNodeId};

use crate::encode::{
    CodeBuffer, Reg64, call_rel32, mov_reg64_imm32, mov_reg64_imm64, mov_reg64_mem_rbp_disp, ret,
};

/// Emit `lea rax, [rdi + disp32]` per `calling-convention.md` §12.1.
///
/// Used to lower a function body of the shape `fn x -> x + N` where
/// `N` is a small immediate. ABI passes `x` in `rdi`; the return
/// register is `rax`.
///
/// Encoding: `REX.W 8D /r [ModR/M] [disp]`. For `lea rax, [rdi + disp8]`
/// with disp != 0 we use mod=01, reg=rax(0), rm=rdi(7) → ModR/M = 0x47;
/// disp8 follows. For disp = 0 with rdi base, mod=00 form is valid
/// (no special case like rbp), saving a byte.
pub fn lea_rax_rdi_disp(buf: &mut CodeBuffer, disp: i32) {
    buf.bytes.push(0x48); // REX.W
    buf.bytes.push(0x8D); // LEA
    if disp == 0 {
        buf.bytes.push(0x07); // mod=00, reg=0, rm=7
    } else if (-128..=127).contains(&disp) {
        buf.bytes.push(0x47); // mod=01, reg=0, rm=7
        buf.bytes.push(disp as u8);
    } else {
        buf.bytes.push(0x87); // mod=10, reg=0, rm=7
        buf.bytes.extend(disp.to_le_bytes());
    }
}

/// Lower the "increment by 1" function `fn x -> x + 1` per
/// `calling-convention.md` §12.1.
///
/// Emits `lea rax, [rdi + 1] ; ret`.
pub fn lower_add_one(buf: &mut CodeBuffer) {
    lea_rax_rdi_disp(buf, 1);
    ret(buf);
}

/// Lower an effect-rewritten `IrPerform` (= `IrApp` with handler-table
/// offset) into the two-instruction indirect-call sequence per
/// `calling-convention.md` §4.2:
///
/// ```text
/// mov rax, [r15 + offset]
/// call rax
/// ```
///
/// Phase-1: the encoder doesn't yet have a generic `mov reg, [base+disp]`
/// for arbitrary base registers. We emit the byte sequence directly.
///
/// Encoding `mov rax, [r15 + offset]`:
/// - REX = 0x49 (W=1, B=1 to extend rm = r15)
/// - Opcode = 0x8B
/// - ModR/M with disp8 (offset fits in i8): mod=01, reg=0 (rax),
///   rm=7 (r15&7) → 0x47, followed by disp8.
/// - ModR/M with disp32: mod=10 → 0x87, followed by disp32.
/// - r15 (rm=7) does NOT require a SIB byte (unlike rbp / r13).
///
/// `call rax`:
/// - Opcode FF /2 with mod=11, reg=2, rm=0 → 0xD0; full encoding `FF D0`.
pub fn lower_handler_call(buf: &mut CodeBuffer, offset: i32) {
    // mov rax, [r15 + offset]
    buf.bytes.push(0x49); // REX.W + REX.B
    buf.bytes.push(0x8B);
    if (-128..=127).contains(&offset) {
        buf.bytes.push(0x47); // mod=01 reg=0 rm=7
        buf.bytes.push(offset as u8);
    } else {
        buf.bytes.push(0x87);
        buf.bytes.extend(offset.to_le_bytes());
    }
    // call rax (FF D0)
    buf.bytes.push(0xFF);
    buf.bytes.push(0xD0);
}

/// Lower a single `IrLet(Literal)` node materialising the literal into
/// `dst`. Phase-1: the literal value is supplied by the caller as a
/// `u64`. The IR will carry literal data once node-level payloads land.
pub fn lower_let_literal(buf: &mut CodeBuffer, dst: Reg64, value: u64) {
    if value <= u64::from(u32::MAX) {
        // i32-fitting fast path keeps the binary smaller.
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        mov_reg64_imm32(buf, dst, value as i32);
    } else {
        mov_reg64_imm64(buf, dst, value);
    }
}

/// Description of a function body to lower. Phase-1 supports the two
/// shapes the milestone-7 deliverable needs.
pub enum BodyShape {
    /// `fn x -> x + N` shape: `lea rax, [rdi + N] ; ret`.
    AddImmediate {
        /// Immediate value to add.
        imm: i32,
    },
    /// `perform E.op(args) ; ret` after PR-51's effect rewrite.
    /// `offset` is the handler-table slot.
    HandlerCall {
        /// Byte offset into the handler table.
        offset: i32,
    },
}

/// Lower a function body to bytes.
///
/// Phase-1: the IR walker hasn't yet learned to dispatch on node
/// children; the caller selects the shape via [`BodyShape`]. Once the
/// IR carries enough structure, this will become an arena walker.
pub fn lower_function_body(buf: &mut CodeBuffer, shape: &BodyShape) {
    match shape {
        BodyShape::AddImmediate { imm } => {
            lea_rax_rdi_disp(buf, *imm);
            ret(buf);
        }
        BodyShape::HandlerCall { offset } => {
            lower_handler_call(buf, *offset);
            ret(buf);
        }
    }
}

/// Lower an entire IR arena. Phase-1 walks the arena counting nodes
/// of interest and emits zero or more function bodies; the per-shape
/// dispatch happens externally via [`BodyShape`]. The walker is
/// included so downstream emitter integration has a single entry
/// point.
pub fn lower_ir_to_bytes(ir: &IrArena, shapes: &[BodyShape]) -> Vec<u8> {
    let _ = ir; // walker is structural in phase-1; real dispatch lives in the
    // tree-walker integration that lands once IR child wiring exists.
    let mut buf = CodeBuffer::new();
    for shape in shapes {
        lower_function_body(&mut buf, shape);
    }
    buf.bytes
}

/// Lower a stub `IrApp` of a known mnemonic. Phase-1 supports a tiny
/// dispatch table for action-block lowering; the full table lands when
/// the encoder + IR are wired end-to-end.
pub fn lower_app_mnemonic(buf: &mut CodeBuffer, ir: &IrArena, id: IrNodeId, mnemonic: &str) {
    debug_assert_eq!(ir[id].kind, IrKind::App);
    match mnemonic {
        "ret" => ret(buf),
        "call_local" => call_rel32(buf, 0), // patched by linker later
        other => {
            // Unknown mnemonics emit a single 0x90 NOP placeholder so
            // the buffer length tracks the intended instruction count;
            // the diagnostic emitter is the IR walker's job.
            let _ = other;
            buf.bytes.push(0x90);
        }
    }
}

/// Convenience: lower a `mov dst, [rbp + disp]` for a local-variable
/// load.
pub fn lower_local_load(buf: &mut CodeBuffer, dst: Reg64, disp: i32) {
    mov_reg64_mem_rbp_disp(buf, dst, disp);
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── AC bullet 1: fn x -> x + 1 → lea rax, [rdi+1] ; ret ──────────

    #[test]
    fn add_one_lowers_to_lea_plus_ret() {
        let mut buf = CodeBuffer::new();
        lower_add_one(&mut buf);
        // lea rax, [rdi+1] → 48 8d 47 01 ; ret → c3
        assert_eq!(buf.bytes, vec![0x48, 0x8D, 0x47, 0x01, 0xC3]);
    }

    #[test]
    fn lea_rax_rdi_disp_0_uses_mod00() {
        let mut buf = CodeBuffer::new();
        lea_rax_rdi_disp(&mut buf, 0);
        assert_eq!(buf.bytes, vec![0x48, 0x8D, 0x07]);
    }

    #[test]
    fn lea_rax_rdi_disp_large_uses_disp32() {
        let mut buf = CodeBuffer::new();
        lea_rax_rdi_disp(&mut buf, 0x1000);
        assert_eq!(buf.bytes, vec![0x48, 0x8D, 0x87, 0x00, 0x10, 0x00, 0x00]);
    }

    // ── AC bullet 2: handler call sequence ───────────────────────────

    #[test]
    fn handler_call_offset_0_uses_disp8() {
        let mut buf = CodeBuffer::new();
        lower_handler_call(&mut buf, 0);
        // 49 8b 47 00 ff d0
        assert_eq!(buf.bytes, vec![0x49, 0x8B, 0x47, 0x00, 0xFF, 0xD0]);
    }

    #[test]
    fn handler_call_offset_16_uses_disp8() {
        let mut buf = CodeBuffer::new();
        lower_handler_call(&mut buf, 16);
        assert_eq!(buf.bytes, vec![0x49, 0x8B, 0x47, 0x10, 0xFF, 0xD0]);
    }

    #[test]
    fn handler_call_offset_300_uses_disp32() {
        let mut buf = CodeBuffer::new();
        lower_handler_call(&mut buf, 300);
        assert_eq!(buf.bytes[0..3], [0x49, 0x8B, 0x87]);
        assert_eq!(&buf.bytes[3..7], &[0x2C, 0x01, 0x00, 0x00]); // 300 LE
        assert_eq!(&buf.bytes[7..9], &[0xFF, 0xD0]);
    }

    // ── lower_let_literal ────────────────────────────────────────────

    #[test]
    fn small_literal_uses_imm32_form() {
        let mut buf = CodeBuffer::new();
        lower_let_literal(&mut buf, Reg64::Rax, 1);
        // mov rax, 1 → 48 c7 c0 01 00 00 00 (7 bytes)
        assert_eq!(buf.bytes.len(), 7);
        assert_eq!(buf.bytes, vec![0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn large_literal_uses_imm64_form() {
        let mut buf = CodeBuffer::new();
        lower_let_literal(&mut buf, Reg64::Rax, 0x1_0000_0000);
        // mov rax, imm64 → 48 b8 <8 bytes> = 10 bytes total.
        assert_eq!(buf.bytes.len(), 10);
        assert_eq!(buf.bytes[0..2], [0x48, 0xB8]);
    }

    // ── lower_function_body dispatch ─────────────────────────────────

    #[test]
    fn lower_body_add_immediate_matches_lower_add_one() {
        let mut a = CodeBuffer::new();
        lower_function_body(&mut a, &BodyShape::AddImmediate { imm: 1 });
        let mut b = CodeBuffer::new();
        lower_add_one(&mut b);
        assert_eq!(a.bytes, b.bytes);
    }

    #[test]
    fn lower_body_handler_call_matches_lower_handler_call() {
        let mut a = CodeBuffer::new();
        lower_function_body(&mut a, &BodyShape::HandlerCall { offset: 16 });
        // Should be `mov rax, [r15+16] ; call rax ; ret`.
        assert_eq!(&a.bytes[0..6], &[0x49, 0x8B, 0x47, 0x10, 0xFF, 0xD0]);
        assert_eq!(a.bytes[6], 0xC3);
    }

    // ── lower_ir_to_bytes: multiple shapes ───────────────────────────

    #[test]
    fn lower_ir_to_bytes_emits_multiple_bodies() {
        let ir = IrArena::new();
        let shapes = vec![
            BodyShape::AddImmediate { imm: 1 },
            BodyShape::HandlerCall { offset: 0 },
        ];
        let bytes = lower_ir_to_bytes(&ir, &shapes);
        // 5 bytes for add_one + 7 bytes for handler call + ret.
        assert_eq!(bytes.len(), 5 + 6 + 1);
    }

    // ── lower_app_mnemonic ───────────────────────────────────────────

    #[test]
    fn lower_app_ret_emits_c3() {
        let mut ir = IrArena::new();
        let span = paideia_as_diagnostics::Span::new(
            paideia_as_diagnostics::FileId::new(1).unwrap(),
            0,
            1,
        );
        let id = ir.alloc(IrKind::App, span);
        let mut buf = CodeBuffer::new();
        lower_app_mnemonic(&mut buf, &ir, id, "ret");
        assert_eq!(buf.bytes, vec![0xC3]);
    }

    #[test]
    fn lower_app_unknown_emits_nop() {
        let mut ir = IrArena::new();
        let span = paideia_as_diagnostics::Span::new(
            paideia_as_diagnostics::FileId::new(1).unwrap(),
            0,
            1,
        );
        let id = ir.alloc(IrKind::App, span);
        let mut buf = CodeBuffer::new();
        lower_app_mnemonic(&mut buf, &ir, id, "definitely_not_real");
        assert_eq!(buf.bytes, vec![0x90]);
    }
}
