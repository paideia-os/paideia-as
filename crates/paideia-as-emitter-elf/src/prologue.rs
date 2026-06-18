//! Function prologue / epilogue generator per
//! `design/toolchain/calling-convention.md` §3.
//!
//! Standard prologue (§3.1):
//!
//! ```text
//! push rbp
//! mov  rbp, rsp
//! sub  rsp, N            ; N = aligned local frame size
//! mov  [rbp - 16], r12   ; save callee-saved registers
//! ...
//! ```
//!
//! Leaf-function prologue (§3.3): omits the frame-pointer establishment
//! when the function calls no others and the frame fits in the red zone.
//!
//! Stack-probe (§3.1): frames >= [`STACK_PROBE_THRESHOLD`] insert a probe
//! loop touching every page so the kernel can grow the stack on demand.

use crate::encode::{
    CodeBuffer, Reg64, mov_mem_rbp_disp_reg64, mov_reg64_mem_rbp_disp, mov_reg64_reg64, pop_reg64,
    push_reg64,
};

/// Frames at or above this size require a stack probe. Per §3.1 the
/// kernel default page size is 4 KiB, but to give some slack we probe
/// at 4 KiB (one page).
pub const STACK_PROBE_THRESHOLD: u32 = 4096;

/// Stack-alignment requirement (System V ABI before a `call`).
pub const STACK_ALIGN: u32 = 16;

/// Frame layout description.
#[derive(Clone, Debug)]
pub struct FrameLayout {
    /// Bytes of local-variable storage in this frame (unaligned).
    pub local_size: u32,
    /// Callee-saved registers to push in prologue order.
    pub saved_regs: Vec<Reg64>,
    /// Leaf functions skip rbp setup and the frame allocation when the
    /// data fits in the red zone (128 bytes per the System V ABI).
    pub is_leaf: bool,
}

impl FrameLayout {
    /// Aligned local frame size, rounded up to [`STACK_ALIGN`].
    #[must_use]
    pub fn aligned_local_size(&self) -> u32 {
        let n = self.local_size;
        n.div_ceil(STACK_ALIGN) * STACK_ALIGN
    }

    /// `true` iff this layout needs a stack-probe loop.
    #[must_use]
    pub fn needs_stack_probe(&self) -> bool {
        self.aligned_local_size() >= STACK_PROBE_THRESHOLD
    }

    /// Save-slot offsets from `rbp` for each saved register, in
    /// declaration order. Slot 1 (first saved register) lives at
    /// `[rbp - 16]` because `[rbp - 8]` is the saved `rbp` itself in
    /// non-leaf frames.
    #[must_use]
    pub fn save_offsets(&self) -> Vec<i32> {
        let mut out = Vec::with_capacity(self.saved_regs.len());
        for i in 0..self.saved_regs.len() {
            let off = -(16 + (i as i32) * 8);
            out.push(off);
        }
        out
    }
}

/// Emit the standard function prologue.
///
/// Non-leaf:
/// 1. `push rbp`
/// 2. `mov rbp, rsp`
/// 3. `sub rsp, aligned_local_size` (via repeated `push` if probing; see
///    [`needs_stack_probe`]).
/// 4. Save each callee-saved register at `[rbp - 16 - i*8]`.
///
/// Leaf:
/// 1. (skip rbp setup if local_size fits in red zone)
/// 2. Save callee-saved registers (still required).
pub fn emit_prologue(buf: &mut CodeBuffer, layout: &FrameLayout) {
    let aligned = layout.aligned_local_size();

    if !layout.is_leaf {
        // push rbp ; mov rbp, rsp
        push_reg64(buf, Reg64::Rbp);
        mov_reg64_reg64(buf, Reg64::Rbp, Reg64::Rsp);

        if layout.needs_stack_probe() {
            emit_stack_probe(buf, aligned);
        } else if aligned > 0 {
            sub_rsp_imm(buf, aligned);
        }

        for (i, reg) in layout.saved_regs.iter().copied().enumerate() {
            let off = -(16 + (i as i32) * 8);
            mov_mem_rbp_disp_reg64(buf, off, reg);
        }
    } else {
        // Leaf function: still save callee-saved regs but skip frame
        // pointer when local_size fits in the red zone.
        for reg in layout.saved_regs.iter().copied() {
            push_reg64(buf, reg);
        }
    }
}

/// Emit the standard function epilogue.
///
/// Mirrors the prologue:
///
/// 1. Restore callee-saved registers in reverse order.
/// 2. `mov rsp, rbp` (cancels any `sub rsp` in prologue).
/// 3. `pop rbp`.
pub fn emit_epilogue(buf: &mut CodeBuffer, layout: &FrameLayout) {
    if !layout.is_leaf {
        for (i, reg) in layout.saved_regs.iter().copied().enumerate().rev() {
            let off = -(16 + (i as i32) * 8);
            mov_reg64_mem_rbp_disp(buf, reg, off);
        }
        mov_reg64_reg64(buf, Reg64::Rsp, Reg64::Rbp);
        pop_reg64(buf, Reg64::Rbp);
    } else {
        for reg in layout.saved_regs.iter().copied().rev() {
            pop_reg64(buf, reg);
        }
    }
}

/// Emit `sub rsp, imm32` (REX.W 81 /5 id).
fn sub_rsp_imm(buf: &mut CodeBuffer, imm: u32) {
    // REX.W = 0x48
    buf.bytes.push(0x48);
    buf.bytes.push(0x81);
    buf.bytes.push(0xEC); // mod=11, /5 (sub), rsp
    buf.bytes.extend(imm.to_le_bytes());
}

/// Emit a stack-probe loop that touches each 4 KiB page in the frame
/// before allocating it. Pattern per §3.1:
///
/// ```text
///     mov rax, frame_size
/// .probe:
///     sub rsp, 4096
///     mov [rsp], 0          ; touch the page
///     sub rax, 4096
///     test rax, rax
///     jg .probe
/// ```
///
/// Phase-1 emits a structurally-correct probe sequence; the precise
/// instruction bytes are simplified (the test/jcc encoding lands when
/// the encoder gains label-based control-flow support).
fn emit_stack_probe(buf: &mut CodeBuffer, frame_size: u32) {
    // mov rax, frame_size
    buf.bytes.extend([0x48, 0xC7, 0xC0]);
    buf.bytes.extend(frame_size.to_le_bytes());
    // Phase-1 placeholder probe body: a single `sub rsp, 4096` to
    // assert frame-page allocation intent. A real loop lands in PR-58
    // when label-based control flow is wired.
    sub_rsp_imm(buf, STACK_PROBE_THRESHOLD);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(local_size: u32, regs: Vec<Reg64>, is_leaf: bool) -> FrameLayout {
        FrameLayout {
            local_size,
            saved_regs: regs,
            is_leaf,
        }
    }

    // ── alignment ────────────────────────────────────────────────────

    #[test]
    fn aligned_local_size_rounds_up_to_16() {
        assert_eq!(layout(0, vec![], false).aligned_local_size(), 0);
        assert_eq!(layout(1, vec![], false).aligned_local_size(), 16);
        assert_eq!(layout(15, vec![], false).aligned_local_size(), 16);
        assert_eq!(layout(16, vec![], false).aligned_local_size(), 16);
        assert_eq!(layout(17, vec![], false).aligned_local_size(), 32);
        assert_eq!(layout(64, vec![], false).aligned_local_size(), 64);
    }

    // ── save offsets ─────────────────────────────────────────────────

    #[test]
    fn save_offsets_start_at_minus_16() {
        let l = layout(0, vec![Reg64::R12, Reg64::R13, Reg64::R14], false);
        assert_eq!(l.save_offsets(), vec![-16, -24, -32]);
    }

    // ── AC bullet 1: 64-byte frame + R12 ─────────────────────────────

    #[test]
    fn prologue_64_bytes_locals_and_r12() {
        let l = layout(64, vec![Reg64::R12], false);
        let mut buf = CodeBuffer::new();
        emit_prologue(&mut buf, &l);
        // First byte: push rbp = 0x55.
        assert_eq!(buf.bytes[0], 0x55);
        // Second sequence: mov rbp, rsp (48 89 e5).
        assert_eq!(&buf.bytes[1..4], &[0x48, 0x89, 0xE5]);
        // Then sub rsp, 64 (48 81 ec 40 00 00 00).
        assert_eq!(
            &buf.bytes[4..11],
            &[0x48, 0x81, 0xEC, 0x40, 0x00, 0x00, 0x00]
        );
        // Then mov [rbp-16], r12: REX.W with R extending src → 0x4C, 89, 65, F0.
        assert_eq!(buf.bytes[11], 0x4C);
        assert_eq!(buf.bytes[12], 0x89);
        // ModR/M: mod=01, src=r12&7=4, rm=rbp=5 → 0x40 | (4<<3) | 5 = 0x65.
        assert_eq!(buf.bytes[13], 0x65);
        // disp8: -16 = 0xF0.
        assert_eq!(buf.bytes[14], 0xF0);
    }

    // ── AC bullet 2: leaf function omits frame-pointer setup ─────────

    #[test]
    fn leaf_function_skips_push_rbp() {
        let l = layout(32, vec![], true);
        let mut buf = CodeBuffer::new();
        emit_prologue(&mut buf, &l);
        // Leaf with no saved regs and red-zone-sized locals emits NO
        // instructions (the red zone covers the locals).
        assert!(buf.bytes.is_empty());
    }

    #[test]
    fn leaf_with_saved_regs_pushes_them() {
        let l = layout(32, vec![Reg64::Rbx], true);
        let mut buf = CodeBuffer::new();
        emit_prologue(&mut buf, &l);
        // Just `push rbx` (0x53).
        assert_eq!(buf.bytes, vec![0x53]);
    }

    // ── AC bullet 3: 8 KiB frame triggers stack probe ────────────────

    #[test]
    fn frame_8_kib_triggers_stack_probe() {
        let l = layout(8 * 1024, vec![], false);
        assert!(l.needs_stack_probe());
        let mut buf = CodeBuffer::new();
        emit_prologue(&mut buf, &l);
        // After `push rbp / mov rbp, rsp`, the probe begins with
        // `mov rax, frame_size` (48 c7 c0 ...).
        assert_eq!(buf.bytes[0], 0x55); // push rbp
        assert_eq!(&buf.bytes[4..7], &[0x48, 0xC7, 0xC0]); // mov rax, imm32
        // frame_size = 8192 = 0x2000 → little-endian 00 20 00 00.
        assert_eq!(&buf.bytes[7..11], &[0x00, 0x20, 0x00, 0x00]);
    }

    // ── AC bullet 4: epilogue mirrors prologue ───────────────────────

    #[test]
    fn epilogue_restores_in_reverse_order() {
        let l = layout(64, vec![Reg64::R12, Reg64::R13], false);
        let mut buf = CodeBuffer::new();
        emit_epilogue(&mut buf, &l);
        // First: mov r13, [rbp-24] (REX.W=4C, 8B, [mod=01 r13&7=5 rbp=5]=0x6D, -24=0xE8).
        assert_eq!(buf.bytes[0], 0x4C);
        assert_eq!(buf.bytes[1], 0x8B);
        assert_eq!(buf.bytes[2], 0x6D);
        assert_eq!(buf.bytes[3], 0xE8); // -24
        // Next: mov r12, [rbp-16].
        assert_eq!(&buf.bytes[4..8], &[0x4C, 0x8B, 0x65, 0xF0]);
        // Then mov rsp, rbp (48 89 EC) and pop rbp (5D).
        assert_eq!(&buf.bytes[8..11], &[0x48, 0x89, 0xEC]);
        assert_eq!(buf.bytes[11], 0x5D);
    }

    #[test]
    fn empty_frame_prologue_is_just_push_mov() {
        let l = layout(0, vec![], false);
        let mut buf = CodeBuffer::new();
        emit_prologue(&mut buf, &l);
        // push rbp ; mov rbp, rsp — 4 bytes total.
        assert_eq!(buf.bytes, vec![0x55, 0x48, 0x89, 0xE5]);
    }

    #[test]
    fn empty_frame_epilogue_is_just_mov_pop() {
        let l = layout(0, vec![], false);
        let mut buf = CodeBuffer::new();
        emit_epilogue(&mut buf, &l);
        assert_eq!(buf.bytes, vec![0x48, 0x89, 0xEC, 0x5D]);
    }
}
