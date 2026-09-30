//! paideia-as#1508 (PAS-DEBT-B2-015): elaborator-side byte-swap
//! insertion for struct fields annotated with `@endian(be|le)`.
//!
//! The parser stashes each `@endian(...)` occurrence on the AST's
//! `StructFieldAttrTable` keyed by the field-name NodeId
//! (paideia-as#1372, v0.28-M1-003). Wave-55 (v0.36.80) closes the
//! deferred elaborator half:
//!
//!   * `struct_registry.rs` records a parallel `field_endian` vector.
//!   * `walker_pipeline.rs` mirrors it into `EmitPassState::
//!     struct_field_endian` right after `finalise_record_layouts`.
//!   * `emit_field_access.rs` reads the annotation at emit time and
//!     inserts a width-appropriate byte-reversal on the load side
//!     (after the widening load) or on the store side (before the
//!     `mov [base + offset], src`).
//!
//! Byte-order recipes (little-endian native x86_64):
//!   - u16 → `rol r16, 8` (66h prefix; touches only low 2 bytes).
//!   - u32 → `bswap r32` (0F C8+rd; zero-extends to r64).
//!   - u64 → `bswap r64` (REX.W 0F C8+rd).
//!   - `@endian(le)` → no-op (native LE), just the plain load/store.
//!
//! These tests assert the emitted instruction sequence (mnemonic +
//! operand-shape) — mirroring `scratch_and_ops.rs`'s field-access
//! tests and `sret_slice_c.rs`'s multi-instruction assertions —
//! rather than round-tripping through the encoder for every
//! variant. The encoder primitives for `bswap`/`rol` are already
//! pinned by `paideia-as-encoder`'s own bitwise tests.

use super::super::*;
use paideia_as_ast::Endianness;
use paideia_as_diagnostics::{FileId, Span};
use paideia_as_ir::record_layout::{FieldAccessInfo, FieldLayout, RecordLayout, RecordTypeId};

fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

/// Build a bare `FieldAccess((*p).f)` IR arena, install `layout` and
/// (optionally) an `@endian(...)` annotation on `(RecordTypeId(1), 0)`,
/// walk it, and return every emitted instruction in emission order.
///
/// Registers the pointer receiver `p` in `local_bindings` at RDI so
/// the byte-level shape of the load matches the pre-Wave-55 field-
/// access tests in `scratch_and_ops.rs`.
fn build_endian_load(
    size: u8,
    signed: bool,
    offset: u64,
    endian: Option<Endianness>,
) -> Vec<Instruction> {
    let mut arena = IrArena::new();

    // Var(p) → Deref → FieldAccess
    let var_id = arena.alloc(IrKind::Var, span());
    arena.binding_names_mut().insert(var_id, "p".to_string());
    let deref_id = arena.alloc_with_children(IrKind::Deref, span(), [var_id]);
    let field_access_id =
        arena.alloc_with_children(IrKind::FieldAccess, span(), [deref_id]);

    arena.field_access_info_mut().insert(
        field_access_id,
        FieldAccessInfo {
            type_id: RecordTypeId(1),
            field_index: 0,
        },
    );

    let field_layout = FieldLayout {
        offset,
        size,
        signed,
        is_float: false,
    };
    let layout = RecordLayout::new(offset + size as u64, size.max(1), vec![field_layout]);

    let mut walker = EmitWalker::new();
    walker
        .state_mut()
        .insert_record_layout(RecordTypeId(1), layout);
    walker
        .state_mut()
        .local_bindings
        .insert("p".to_string(), abi::RDI);
    if let Some(e) = endian {
        walker.state_mut().insert_field_endian(RecordTypeId(1), 0, e);
    }
    walker.walk(&mut arena);

    // Collect every emitted instruction ordered by emission_order —
    // load then any byte-swap the endian helper appended.
    let mut owned: Vec<(u32, Instruction)> = walker
        .state()
        .instructions()
        .entries()
        .iter()
        .map(|(_, inst)| (inst.emission_order, inst.clone()))
        .collect();
    owned.sort_by_key(|(order, _)| *order);
    owned.into_iter().map(|(_, i)| i).collect()
}

/// Build a bare `Store((*p).f = v)` IR arena for the store side, with
/// `v` bound to RDX and `p` bound to RDI. Returns every emitted
/// instruction in emission order (bswap prologue + MovSized store).
fn build_endian_store(
    size: u8,
    signed: bool,
    offset: u64,
    endian: Option<Endianness>,
) -> Vec<Instruction> {
    let mut arena = IrArena::new();

    let ptr_var_id = arena.alloc(IrKind::Var, span());
    arena.binding_names_mut().insert(ptr_var_id, "p".to_string());
    let deref_id = arena.alloc_with_children(IrKind::Deref, span(), [ptr_var_id]);
    let field_access_id =
        arena.alloc_with_children(IrKind::FieldAccess, span(), [deref_id]);
    let index_id = arena.alloc(IrKind::Var, span());
    let value_id = arena.alloc(IrKind::Var, span());
    arena.binding_names_mut().insert(value_id, "v".to_string());
    let _store_id = arena.alloc_with_children(
        IrKind::Store,
        span(),
        [field_access_id, index_id, value_id],
    );

    arena.field_access_info_mut().insert(
        field_access_id,
        FieldAccessInfo {
            type_id: RecordTypeId(1),
            field_index: 0,
        },
    );

    let field_layout = FieldLayout {
        offset,
        size,
        signed,
        is_float: false,
    };
    let layout = RecordLayout::new(offset + size as u64, size.max(1), vec![field_layout]);

    let mut walker = EmitWalker::new();
    walker
        .state_mut()
        .insert_record_layout(RecordTypeId(1), layout);
    walker
        .state_mut()
        .local_bindings
        .insert("p".to_string(), abi::RDI);
    walker
        .state_mut()
        .local_bindings
        .insert("v".to_string(), abi::RDX);
    if let Some(e) = endian {
        walker.state_mut().insert_field_endian(RecordTypeId(1), 0, e);
    }
    walker.walk(&mut arena);

    let mut owned: Vec<(u32, Instruction)> = walker
        .state()
        .instructions()
        .entries()
        .iter()
        .map(|(_, inst)| (inst.emission_order, inst.clone()))
        .collect();
    owned.sort_by_key(|(order, _)| *order);
    owned.into_iter().map(|(_, i)| i).collect()
}

// ── LOAD tests ─────────────────────────────────────────────────────────

/// `@endian(be)` on a u64 field: `mov rax, [rdi + 0]; bswap rax`.
///
/// The load byte-shape is byte-identical to the pre-Wave-55
/// `field_access_u64_offset_0` test in `scratch_and_ops.rs`;
/// Wave-55 appends the `bswap r64` (Bswap mnemonic, REX.W 0F C8+rd)
/// that flips the loaded value into native little-endian.
#[test]
fn endian_be_load_u64_emits_bswap_r64_after_mov() {
    let insts = build_endian_load(8, false, 0, Some(Endianness::Be));

    let mnems: Vec<Mnemonic> = insts.iter().map(|i| i.mnemonic).collect();
    assert_eq!(
        mnems,
        vec![Mnemonic::MovSized { width: IntWidth::W64 }, Mnemonic::Bswap],
        "u64 @endian(be) must lower to mov r64,[mem] then bswap r64"
    );

    // Second instruction is `bswap rax` — one register operand, RAX.
    match insts[1].operands.as_slice() {
        [Operand::Reg(r)] => assert_eq!(*r, abi::RAX, "bswap operand must be RAX (load dest)"),
        other => panic!("expected [Reg(RAX)], got {:?}", other),
    }
}

/// `@endian(be)` on a u32 field: `mov eax, [rdi]; bswap eax` (Bswap32
/// mnemonic, 0F C8+rd — no REX.W). The 32-bit load already zeroes
/// the upper 32 bits of RAX, so `bswap32` yielding a zero-extended
/// swapped low-4-byte value is the correct u32 semantic.
#[test]
fn endian_be_load_u32_emits_bswap32_after_mov() {
    let insts = build_endian_load(4, false, 0, Some(Endianness::Be));

    let mnems: Vec<Mnemonic> = insts.iter().map(|i| i.mnemonic).collect();
    assert_eq!(
        mnems,
        vec![Mnemonic::MovSized { width: IntWidth::W32 }, Mnemonic::Bswap32],
        "u32 @endian(be) must lower to mov r32,[mem] then bswap r32"
    );

    match insts[1].operands.as_slice() {
        [Operand::Reg(r)] => assert_eq!(*r, abi::RAX, "bswap32 operand must be RAX (load dest)"),
        other => panic!("expected [Reg(RAX)], got {:?}", other),
    }
}

/// `@endian(be)` on a u16 field: `movzx eax, word [rdi]; rol ax, 8`.
/// `rol r16, 8` (Rol{W16}) is the canonical 2-byte swap on x86 (no
/// dedicated `bswap r16` exists — Intel SDM Vol 2A: the 16-bit form
/// is undefined). MOVZX left the upper 48 bits zero, so the ROL on
/// only the low 16 bits preserves u16 semantics.
#[test]
fn endian_be_load_u16_emits_rol_r16_after_movzx() {
    let insts = build_endian_load(2, false, 0, Some(Endianness::Be));

    let mnems: Vec<Mnemonic> = insts.iter().map(|i| i.mnemonic).collect();
    assert_eq!(
        mnems,
        vec![Mnemonic::Movzx, Mnemonic::Rol { width: IntWidth::W16 }],
        "u16 @endian(be) must lower to movzx r64,word[mem] then rol r16, 8"
    );

    // rol reg, imm8 — verify RAX and imm 8.
    match insts[1].operands.as_slice() {
        [Operand::Reg(r), Operand::Imm64(imm)] => {
            assert_eq!(*r, abi::RAX, "rol dest must be RAX (load dest)");
            assert_eq!(*imm, 8, "rol amount must be 8 for a byte-pair swap");
        }
        other => panic!("expected [Reg(RAX), Imm64(8)], got {:?}", other),
    }
}

/// `@endian(le)` on a u32 field is a no-op on the native little-
/// endian x86_64 target — only the plain widening load is emitted,
/// byte-identical to the unannotated form.
#[test]
fn endian_le_load_u32_is_noop_on_native_le() {
    let insts = build_endian_load(4, false, 0, Some(Endianness::Le));

    let mnems: Vec<Mnemonic> = insts.iter().map(|i| i.mnemonic).collect();
    assert_eq!(
        mnems,
        vec![Mnemonic::MovSized { width: IntWidth::W32 }],
        "u32 @endian(le) on x86_64 must emit only the plain load — no bswap"
    );
}

/// Regression: an unannotated u32 field must emit byte-identical
/// bytes to pre-Wave-55. The endian helper's HashMap-miss returns
/// early without touching the instruction stream.
#[test]
fn unannotated_u32_load_is_byte_identical_to_pre_wave() {
    let insts = build_endian_load(4, false, 0, None);

    let mnems: Vec<Mnemonic> = insts.iter().map(|i| i.mnemonic).collect();
    assert_eq!(
        mnems,
        vec![Mnemonic::MovSized { width: IntWidth::W32 }],
        "unannotated u32 must emit exactly one MovSized{W32} — no bswap"
    );
}

// ── STORE tests ────────────────────────────────────────────────────────

/// `@endian(be)` on a u32 field store: `mov r11, rdx; bswap r11d;
/// mov [rdi], r11d`. The value in RDX (bound to `v`) is copied into
/// R11 first — R11 is caller-saved and outside the SysV
/// argument-passing sequence, so the value binding survives the
/// byte-swap. The store then narrows to 4 bytes via MovSized{W32}.
#[test]
fn endian_be_store_u32_emits_mov_r11_bswap32_store() {
    let insts = build_endian_store(4, false, 0, Some(Endianness::Be));

    let mnems: Vec<Mnemonic> = insts.iter().map(|i| i.mnemonic).collect();
    assert_eq!(
        mnems,
        vec![
            Mnemonic::Mov,
            Mnemonic::Bswap32,
            Mnemonic::MovSized { width: IntWidth::W32 },
        ],
        "u32 @endian(be) store must lower to mov r11,rdx; bswap r11d; mov [rdi], r11d"
    );

    // First instruction: mov r11, rdx.
    match (&insts[0].operands[0], &insts[0].operands[1]) {
        (Operand::Reg(dst), Operand::Reg(src)) => {
            assert_eq!(*dst, abi::R11, "bswap scratch must be R11");
            assert_eq!(*src, abi::RDX, "value source must be RDX (bound `v`)");
        }
        other => panic!("expected (Reg(R11), Reg(RDX)), got {:?}", other),
    }
    // Second instruction: bswap32 r11.
    match insts[1].operands.as_slice() {
        [Operand::Reg(r)] => assert_eq!(*r, abi::R11),
        other => panic!("expected [Reg(R11)], got {:?}", other),
    }
    // Third instruction: mov [rdi], r11 (32-bit).
    match (&insts[2].operands[0], &insts[2].operands[1]) {
        (Operand::MemSib { base, disp, .. }, Operand::Reg(src)) => {
            assert_eq!(*base, abi::RDI, "store base must be RDI (bound `p`)");
            assert_eq!(*disp, 0);
            assert_eq!(*src, abi::R11, "store source must be R11 (post-bswap)");
        }
        other => panic!("expected (MemSib base=RDI, Reg(R11)), got {:?}", other),
    }
}

/// `@endian(be)` on a u64 field store: `mov r11, rdx; bswap r11;
/// mov [rdi], r11` (full 8-byte MovSized). Same sequence as the u32
/// case but with Bswap (r64) and MovSized{W64}.
#[test]
fn endian_be_store_u64_emits_mov_r11_bswap_store() {
    let insts = build_endian_store(8, false, 0, Some(Endianness::Be));

    let mnems: Vec<Mnemonic> = insts.iter().map(|i| i.mnemonic).collect();
    assert_eq!(
        mnems,
        vec![
            Mnemonic::Mov,
            Mnemonic::Bswap,
            Mnemonic::MovSized { width: IntWidth::W64 },
        ],
        "u64 @endian(be) store must lower to mov r11,rdx; bswap r11; mov [rdi], r11"
    );

    match (&insts[2].operands[0], &insts[2].operands[1]) {
        (Operand::MemSib { base, disp, .. }, Operand::Reg(src)) => {
            assert_eq!(*base, abi::RDI);
            assert_eq!(*disp, 0);
            assert_eq!(
                *src,
                abi::R11,
                "store source must be R11 (post-bswap scratch)"
            );
        }
        other => panic!("expected (MemSib base=RDI, Reg(R11)), got {:?}", other),
    }
}

/// Regression: unannotated u32 store must be byte-identical to
/// pre-Wave-55 (single `mov [rdi], edx`, no scratch copy, no
/// bswap). This is the load-side regression's write-side twin —
/// both prove the endian hot path is truly no-cost on the
/// unannotated hot path.
#[test]
fn unannotated_u32_store_is_byte_identical_to_pre_wave() {
    let insts = build_endian_store(4, false, 0, None);

    let mnems: Vec<Mnemonic> = insts.iter().map(|i| i.mnemonic).collect();
    assert_eq!(
        mnems,
        vec![Mnemonic::MovSized { width: IntWidth::W32 }],
        "unannotated u32 store must emit exactly one MovSized{W32} — no bswap prologue"
    );

    // Confirm the source register is RDX (the pre-Wave path), not R11.
    match (&insts[0].operands[0], &insts[0].operands[1]) {
        (Operand::MemSib { base, disp, .. }, Operand::Reg(src)) => {
            assert_eq!(*base, abi::RDI);
            assert_eq!(*disp, 0);
            assert_eq!(
                *src,
                abi::RDX,
                "unannotated store source must be RDX (pre-Wave-55 shape)"
            );
        }
        other => panic!("expected (MemSib base=RDI, Reg(RDX)), got {:?}", other),
    }
}

// ── SIGNED-NARROW tests (Wave 57, paideia-as#1560) ────────────────────
//
// Wave 56 (v0.36.80, #1508) landed the byte-swap for u8/u16/u32/u64/i64
// and diagnosed i16/i32 with T0567 to prevent silent miscompile. Wave 57
// (v0.36.81, #1560) lands the three-instruction `swap-low; movsx-widen`
// recipe on the load side and the matching `mov r11; swap-low r11;
// narrow-store` recipe on the store side, retiring T0567 for the
// signed-narrow case (the diagnostic remains as a defensive arm for
// truly unsupported widths).
//
// The load recipes must re-sign-extend AFTER the swap so upper bits
// track the true sign of the intended value — the initial movsx/movsxd
// baked stale sign-extension from the raw big-endian byte sequence,
// which the swap alone does not repair. The store recipes can skip the
// re-widen because the downstream MovSized{width} narrows the write.

/// `@endian(be)` on an i16 field: `movsx r64, word[rdi];
/// rol r16, 8; movsx r64, r16`. The first movsx (0F BF) loads
/// the raw big-endian bytes as a mis-signed 64-bit value. The ROL
/// swaps the low 16 bits in place. The second movsx (0F BF, reg-
/// reg) re-derives sign from the correctly-swapped low half,
/// discarding the stale upper 48. Byte-exact: sequence, register
/// (RAX), immediate (8), and encoding_hint (opcode 0x0F, operand_size 2)
/// are all pinned.
#[test]
fn i16_be_load_swap_and_sign_extend() {
    let insts = build_endian_load(2, true, 0, Some(Endianness::Be));

    let mnems: Vec<Mnemonic> = insts.iter().map(|i| i.mnemonic).collect();
    assert_eq!(
        mnems,
        vec![
            Mnemonic::Movsx,
            Mnemonic::Rol { width: IntWidth::W16 },
            Mnemonic::Movsx,
        ],
        "i16 @endian(be) must lower to movsx r64,word[mem]; rol r16, 8; movsx r64, r16"
    );

    // inst[0] — movsx r64, word[rdi + 0], encoding_hint 0x0F/2.
    match (&insts[0].operands[0], &insts[0].operands[1]) {
        (Operand::Reg(dst), Operand::MemSib { base, disp, .. }) => {
            assert_eq!(*dst, abi::RAX, "movsx dst must be RAX (load dest)");
            assert_eq!(*base, abi::RDI, "movsx base must be RDI (bound `p`)");
            assert_eq!(*disp, 0);
        }
        other => panic!("expected movsx (Reg(RAX), MemSib base=RDI disp=0), got {:?}", other),
    }
    let hint0 = insts[0].encoding_hint.expect("movsx load must carry hint");
    assert_eq!(hint0.opcode, 0x0F, "i16 load opcode must be 0x0F (0F BF /r)");
    assert_eq!(hint0.operand_size, 2, "i16 load operand_size must be 2");

    // inst[1] — rol RAX, 8 (Rol{W16}).
    match insts[1].operands.as_slice() {
        [Operand::Reg(r), Operand::Imm64(imm)] => {
            assert_eq!(*r, abi::RAX, "rol dest must be RAX");
            assert_eq!(*imm, 8, "rol amount must be 8 for a byte-pair swap");
        }
        other => panic!("expected [Reg(RAX), Imm64(8)], got {:?}", other),
    }

    // inst[2] — movsx r64, r16 (reg-reg), encoding_hint 0x0F/2.
    match (&insts[2].operands[0], &insts[2].operands[1]) {
        (Operand::Reg(dst), Operand::Reg(src)) => {
            assert_eq!(*dst, abi::RAX, "re-widen dst must be RAX");
            assert_eq!(*src, abi::RAX, "re-widen src must be RAX (same reg, low 16)");
        }
        other => panic!("expected movsx (Reg(RAX), Reg(RAX)), got {:?}", other),
    }
    let hint2 = insts[2].encoding_hint.expect("post-swap movsx must carry hint");
    assert_eq!(hint2.opcode, 0x0F, "post-swap re-widen opcode must be 0x0F (0F BF /r)");
    assert_eq!(hint2.operand_size, 2, "post-swap re-widen operand_size must be 2");
}

/// `@endian(be)` on an i32 field: `movsxd r64, dword[rdi];
/// bswap r32; movsxd r64, r32`. bswap r32 zero-extends to r64,
/// which is *wrong* for negative i32 values — the second movsxd
/// (0x63) re-derives the true sign from the swapped low 32.
/// Byte-exact: sequence, register (RAX), and encoding_hint
/// (opcode 0x63, operand_size 4) are all pinned.
#[test]
fn i32_be_load_swap_and_sign_extend() {
    let insts = build_endian_load(4, true, 0, Some(Endianness::Be));

    let mnems: Vec<Mnemonic> = insts.iter().map(|i| i.mnemonic).collect();
    assert_eq!(
        mnems,
        vec![Mnemonic::Movsx, Mnemonic::Bswap32, Mnemonic::Movsx],
        "i32 @endian(be) must lower to movsxd r64,dword[mem]; bswap r32; movsxd r64, r32"
    );

    // inst[0] — movsxd r64, dword[rdi + 0], encoding_hint 0x63/4.
    match (&insts[0].operands[0], &insts[0].operands[1]) {
        (Operand::Reg(dst), Operand::MemSib { base, disp, .. }) => {
            assert_eq!(*dst, abi::RAX, "movsxd dst must be RAX (load dest)");
            assert_eq!(*base, abi::RDI, "movsxd base must be RDI (bound `p`)");
            assert_eq!(*disp, 0);
        }
        other => panic!("expected movsxd (Reg(RAX), MemSib base=RDI disp=0), got {:?}", other),
    }
    let hint0 = insts[0].encoding_hint.expect("movsxd load must carry hint");
    assert_eq!(hint0.opcode, 0x63, "i32 load opcode must be 0x63 (single-byte MOVSXD)");
    assert_eq!(hint0.operand_size, 4, "i32 load operand_size must be 4");

    // inst[1] — bswap RAX (32-bit).
    match insts[1].operands.as_slice() {
        [Operand::Reg(r)] => assert_eq!(*r, abi::RAX, "bswap32 operand must be RAX"),
        other => panic!("expected [Reg(RAX)], got {:?}", other),
    }

    // inst[2] — movsxd r64, r32 (reg-reg), encoding_hint 0x63/4.
    match (&insts[2].operands[0], &insts[2].operands[1]) {
        (Operand::Reg(dst), Operand::Reg(src)) => {
            assert_eq!(*dst, abi::RAX, "re-widen dst must be RAX");
            assert_eq!(*src, abi::RAX, "re-widen src must be RAX (same reg, low 32)");
        }
        other => panic!("expected movsxd (Reg(RAX), Reg(RAX)), got {:?}", other),
    }
    let hint2 = insts[2].encoding_hint.expect("post-swap movsxd must carry hint");
    assert_eq!(hint2.opcode, 0x63, "post-swap re-widen opcode must be 0x63");
    assert_eq!(hint2.operand_size, 4, "post-swap re-widen operand_size must be 4");
}

/// `@endian(be)` on an i16 field store: `mov r11, rdx;
/// rol r11w, 8; mov word[rdi], r11w`. Same three-instruction
/// shape as the u16 store — no re-widen because MovSized{W16}
/// narrows the write to 2 bytes and drops any stale upper bits
/// left by `rol r16` (the upper 48 of R11 still hold the
/// sign-extended value from RDX, but the store only writes the
/// low 2). Byte-exact: mnemonic sequence, all three register
/// operands, and the immediate 8 are pinned.
#[test]
fn i16_be_store_truncate_swap_narrow() {
    let insts = build_endian_store(2, true, 0, Some(Endianness::Be));

    let mnems: Vec<Mnemonic> = insts.iter().map(|i| i.mnemonic).collect();
    assert_eq!(
        mnems,
        vec![
            Mnemonic::Mov,
            Mnemonic::Rol { width: IntWidth::W16 },
            Mnemonic::MovSized { width: IntWidth::W16 },
        ],
        "i16 @endian(be) store must lower to mov r11,rdx; rol r11w, 8; mov word[rdi], r11w"
    );

    // inst[0] — mov r11, rdx.
    match (&insts[0].operands[0], &insts[0].operands[1]) {
        (Operand::Reg(dst), Operand::Reg(src)) => {
            assert_eq!(*dst, abi::R11, "bswap scratch must be R11");
            assert_eq!(*src, abi::RDX, "value source must be RDX (bound `v`)");
        }
        other => panic!("expected (Reg(R11), Reg(RDX)), got {:?}", other),
    }

    // inst[1] — rol R11, 8 (Rol{W16}).
    match insts[1].operands.as_slice() {
        [Operand::Reg(r), Operand::Imm64(imm)] => {
            assert_eq!(*r, abi::R11, "rol dest must be R11 (post-scratch-copy)");
            assert_eq!(*imm, 8, "rol amount must be 8 for a byte-pair swap");
        }
        other => panic!("expected [Reg(R11), Imm64(8)], got {:?}", other),
    }

    // inst[2] — mov word[rdi], r11 (MovSized{W16}).
    match (&insts[2].operands[0], &insts[2].operands[1]) {
        (Operand::MemSib { base, disp, .. }, Operand::Reg(src)) => {
            assert_eq!(*base, abi::RDI, "store base must be RDI (bound `p`)");
            assert_eq!(*disp, 0);
            assert_eq!(*src, abi::R11, "store source must be R11 (post-swap)");
        }
        other => panic!("expected (MemSib base=RDI, Reg(R11)), got {:?}", other),
    }
}

/// `@endian(be)` on an i32 field store: `mov r11, rdx;
/// bswap r11d; mov dword[rdi], r11d`. bswap32 zero-extends the
/// upper 32 of R11 (Intel-defined behaviour) but that's harmless
/// — MovSized{W32} writes exactly 4 bytes. Byte-exact: mnemonic
/// sequence and all register operands are pinned.
#[test]
fn i32_be_store_truncate_swap_narrow() {
    let insts = build_endian_store(4, true, 0, Some(Endianness::Be));

    let mnems: Vec<Mnemonic> = insts.iter().map(|i| i.mnemonic).collect();
    assert_eq!(
        mnems,
        vec![
            Mnemonic::Mov,
            Mnemonic::Bswap32,
            Mnemonic::MovSized { width: IntWidth::W32 },
        ],
        "i32 @endian(be) store must lower to mov r11,rdx; bswap r11d; mov dword[rdi], r11d"
    );

    // inst[0] — mov r11, rdx.
    match (&insts[0].operands[0], &insts[0].operands[1]) {
        (Operand::Reg(dst), Operand::Reg(src)) => {
            assert_eq!(*dst, abi::R11, "bswap scratch must be R11");
            assert_eq!(*src, abi::RDX, "value source must be RDX (bound `v`)");
        }
        other => panic!("expected (Reg(R11), Reg(RDX)), got {:?}", other),
    }

    // inst[1] — bswap32 R11.
    match insts[1].operands.as_slice() {
        [Operand::Reg(r)] => assert_eq!(*r, abi::R11, "bswap32 operand must be R11"),
        other => panic!("expected [Reg(R11)], got {:?}", other),
    }

    // inst[2] — mov dword[rdi], r11 (MovSized{W32}).
    match (&insts[2].operands[0], &insts[2].operands[1]) {
        (Operand::MemSib { base, disp, .. }, Operand::Reg(src)) => {
            assert_eq!(*base, abi::RDI, "store base must be RDI (bound `p`)");
            assert_eq!(*disp, 0);
            assert_eq!(*src, abi::R11, "store source must be R11 (post-bswap)");
        }
        other => panic!("expected (MemSib base=RDI, Reg(R11)), got {:?}", other),
    }
}
