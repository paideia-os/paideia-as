//! PA8-m3-004 (#828): iced-x86 round-trip suite for the Phase-8 m3 width-aware
//! integer-lowering features.
//!
//! Mirrors the PA7C-m4-004 / PA8-m1-001a iced-x86 patterns: build small Paideia
//! sources through the real `build` CLI, parse the resulting object with the
//! `object` crate, and disassemble `.text` with the iced-x86 `IntelFormatter`.
//! For the milestone features whose width-aware lowering is *not yet reachable
//! through the `build` CLI surface* (see the architectural note below), the
//! suite additionally drives the elaborator's lowering tables directly and
//! disassembles the *encoded bytes* via iced-x86, so every milestone's
//! width-aware output is validated against a real x86_64 decoder.
//!
//! ## What the three m3 milestones changed, and where each is reachable
//!
//! - **m3-001** (#825) — width-routes *in-block* let-literal `Mov`s to the
//!   narrow `MovSized { width }` form. This routing fires only on the
//!   *typer-aware* `EmitWalker::walk_with_typer` path; the `build` CLI invokes
//!   the typer-free `EmitWalker::walk` (crates/paideia-as/src/cmd_build.rs), so
//!   a `build`-compiled block-body `let` emits no width-aware move (the unit
//!   body folds to a bare `ret`). Tier A pins that empirical `build` output;
//!   Tier B exercises the `MovSized { W8 | W16 | W32 }` encoder forms the
//!   width-router emits and confirms the decoded operand widths.
//!
//! - **m3-002** (#826) — replaces the always-`movsxd` cast lowering with the
//!   real `(src_width, dst_width, src_signed, dst_signed)` dispatch in
//!   `cast_plan` / `CastPlan::instruction`. The IR-pipeline callers still pass
//!   the canonical `i32 -> i64` shape pending TypeId width/signedness
//!   resolution, so a `build`-compiled `x as T` still disassembles to
//!   `movsxd rax,edi` for *every* target type. Tier A pins that canonical
//!   `build` output; Tier B drives `cast_plan` for widening-signed,
//!   widening-unsigned, and narrowing shapes and confirms the decoded mnemonic
//!   (`movsx` / `movzx` / 32-bit `mov`) and source/destination widths.
//!
//! - **m3-003** (#827) — retargets `mov r8, imm` -> `MovSized { W8 }` and
//!   `mov r32, imm` -> `MovSized { W32 }` (recovering the destination width
//!   from the register spelling before the sub-register `RegId` collapse). The
//!   `build` CLI cannot reach this path: the surface parser wraps an immediate
//!   operand in a `NodeKind::OperandImmediate` node, and the unsafe walker's
//!   `parse_operand_from_ast` has no arm for that kind, so an unsafe-block
//!   `mov reg, imm` fails operand parsing (diagnostic U1606) before lowering.
//!   Tier B therefore builds the exact `MovSized { W8 }` / `MovSized { W32 }`
//!   instructions the retarget produces and confirms the decoded width.
//!
//! ## Validation model
//!
//! - Tier A compares the full iced `IntelFormatter` line (mnemonic + operands)
//!   of the `build`-compiled `.text`, locking the instruction shape without
//!   asserting raw encoding bytes — exactly as PA7C-m4-004 does.
//! - Tier B encodes a hand-built `Instruction` through the production encoder
//!   (`paideia_as_encoder::encode_instruction`), decodes the bytes with the
//!   iced-x86 `Decoder`, and asserts both the decoded `Mnemonic` and the
//!   register/operand *widths* (via `iced_x86::Register` size), which is the
//!   property the m3 width-aware lowering is responsible for.
//!
//! PLATFORM: Linux-only (iced-x86 disassembly availability), matching the
//! m2/m3 round-trip tests.

#![cfg(target_os = "linux")]

use iced_x86::{
    Decoder, DecoderOptions, Formatter, Instruction as IcedInstruction, IntelFormatter,
    Mnemonic as IcedMnemonic, OpKind, Register,
};
use object::{Object, ObjectSection};
use std::path::PathBuf;
use std::process::Command;

use paideia_as_elaborator::emit_walker::{CastShape, cast_plan};
use paideia_as_encoder::{CodeBuffer, EncodeStats, encode_instruction};
use paideia_as_ir::instruction::{IntWidth, Mnemonic};
use paideia_as_ir::{Instruction, Operand, RegId};

// ===========================================================================
// Tier A: `build` CLI round-trip (mirrors PA7C-m4-004 exactly).
// ===========================================================================

/// Canonical cast lowering through the `build` CLI: every `x as T` still
/// lowers to `movsxd rax,edi` (the IR pipeline passes the canonical i32->i64
/// shape; see the m3-002 note above).
const CAST_BUILD: &[&str] = &["movsxd rax,edi", "ret"];

/// A `build`-compiled block-body `let`/tail-binding function folds to a bare
/// `ret` on the typer-free `walk` path (m3-001 width routing is inert here).
const BLOCK_LET_BUILD: &[&str] = &["ret"];

/// Hand-rolled Tier-A fixture table: `(basename, source, expected_disasm)`.
///
/// `basename` is the snake_case tempfile stem; the top-level module name is its
/// PascalCase fold (e.g. `cast_i8_to_i64` -> module `CastI8ToI64`,
/// `block_let_u32` -> module `BlockLetU32`), matching the build CLI's
/// basename/module-name check.
type Fixture = (&'static str, &'static str, &'static [&'static str]);

const TIER_A_FIXTURES: &[Fixture] = &[
    // ---- m3-002: cast dispatch sources (all lower to canonical movsxd) -----
    // widening signed
    (
        "cast_i8_to_i64",
        "module CastI8ToI64 = structure {\n  let f : (i8) -> u64 = fn (x : i8) -> x as i64\n}\n",
        CAST_BUILD,
    ),
    (
        "cast_i16_to_i64",
        "module CastI16ToI64 = structure {\n  let f : (i16) -> u64 = fn (x : i16) -> x as i64\n}\n",
        CAST_BUILD,
    ),
    // widening unsigned
    (
        "cast_u8_to_u64",
        "module CastU8ToU64 = structure {\n  let f : (u8) -> u64 = fn (x : u8) -> x as u64\n}\n",
        CAST_BUILD,
    ),
    (
        "cast_u16_to_u64",
        "module CastU16ToU64 = structure {\n  let f : (u16) -> u64 = fn (x : u16) -> x as u64\n}\n",
        CAST_BUILD,
    ),
    // narrowing
    (
        "cast_u64_to_u32",
        "module CastU64ToU32 = structure {\n  let f : (u64) -> u64 = fn (x : u64) -> x as u32\n}\n",
        CAST_BUILD,
    ),
    (
        "cast_u64_to_u8",
        "module CastU64ToU8 = structure {\n  let f : (u64) -> u64 = fn (x : u64) -> x as u8\n}\n",
        CAST_BUILD,
    ),
    // ---- m3-001: block-body sized `let` sources (fold to `ret` via build) ---
    (
        "block_let_u8",
        "module BlockLetU8 = structure {\n  let f : () -> u8 !{} @{} = fn(_: ()) -> {\n    let a : u8 = 7\n    a\n  }\n}\n",
        BLOCK_LET_BUILD,
    ),
    (
        "block_let_u16",
        "module BlockLetU16 = structure {\n  let f : () -> u16 !{} @{} = fn(_: ()) -> {\n    let a : u16 = 7\n    a\n  }\n}\n",
        BLOCK_LET_BUILD,
    ),
    (
        "block_let_u32",
        "module BlockLetU32 = structure {\n  let f : () -> u32 !{} @{} = fn(_: ()) -> {\n    let a : u32 = 7\n    a\n  }\n}\n",
        BLOCK_LET_BUILD,
    ),
    // ---- regression: untyped/u64 block `let` still folds to `ret` -----------
    (
        "block_let_u64",
        "module BlockLetU64 = structure {\n  let f : () -> u64 !{} @{} = fn(_: ()) -> {\n    let a : u64 = 7\n    a\n  }\n}\n",
        BLOCK_LET_BUILD,
    ),
];

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

/// Build a fixture to ELF64 and return the disassembled `.text` lines.
fn build_and_disasm(basename: &str, source: &str) -> Vec<String> {
    let dir = std::env::temp_dir().join("paideia_as_pa8_m3_004");
    std::fs::create_dir_all(&dir).expect("create temp dir");

    let src_path: PathBuf = dir.join(format!("{basename}.pdx"));
    let obj_path: PathBuf = dir.join(format!("{basename}.o"));
    let _ = std::fs::remove_file(&obj_path);
    std::fs::write(&src_path, source).expect("write fixture source");

    let out = cargo_run(&[
        "build",
        src_path.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        obj_path.to_str().unwrap(),
    ]);

    assert!(
        out.status.success(),
        "build failed for fixture `{basename}`:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let bytes = std::fs::read(&obj_path)
        .unwrap_or_else(|e| panic!("output ELF for `{basename}` should exist: {e}"));
    assert_eq!(
        &bytes[0..4],
        b"\x7FELF",
        "ELF magic missing for `{basename}`"
    );

    let file = object::File::parse(&*bytes)
        .unwrap_or_else(|e| panic!("should parse ELF for `{basename}`: {e}"));

    let mut text_bytes = Vec::new();
    for section in file.sections() {
        if section.name().unwrap_or("") == ".text" {
            text_bytes = section.data().unwrap_or(b"").to_vec();
            break;
        }
    }
    assert!(
        !text_bytes.is_empty(),
        ".text section must exist and be non-empty for `{basename}`"
    );

    let mut decoder = Decoder::new(64, &text_bytes, DecoderOptions::NONE);
    let mut formatter = IntelFormatter::new();
    let mut lines = Vec::new();
    let mut rendered = String::new();
    for inst in decoder.iter() {
        rendered.clear();
        formatter.format(&inst, &mut rendered);
        lines.push(rendered.clone());
    }

    let _ = std::fs::remove_file(&obj_path);
    let _ = std::fs::remove_file(&src_path);

    lines
}

#[test]
fn tier_a_build_round_trip_all_fixtures() {
    let mut failures: Vec<String> = Vec::new();

    for (basename, source, expected) in TIER_A_FIXTURES {
        let actual = build_and_disasm(basename, source);

        // The emitted `.text` must begin with the expected instruction lines.
        let matches = actual.len() >= expected.len()
            && actual
                .iter()
                .zip(expected.iter())
                .all(|(got, want)| got == want);

        if !matches {
            failures.push(format!(
                "fixture `{basename}`:\n    expected: {expected:?}\n    actual:   {actual:?}"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} Tier-A build round-trip fixtures failed:\n\n{}",
        failures.len(),
        TIER_A_FIXTURES.len(),
        failures.join("\n\n"),
    );
}

/// Tier-A regression: the existing reg-reg unsafe-block `mov` fixture (which the
/// `build` CLI *can* lower) still disassembles to three 64-bit `mov`s, proving
/// the m3 immediate-width work left the reg-reg path untouched.
#[test]
fn tier_a_reg_reg_mov_regression() {
    let mut input = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    input.push("../../tests/build-emit/pa7c_unsafe_body/unsafe_body_mov_reg_reg.pdx");

    let tmp = std::env::temp_dir().join("paideia_as_pa8_m3_004_regreg.o");
    let _ = std::fs::remove_file(&tmp);
    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        tmp.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "build failed for unsafe_body_mov_reg_reg.pdx: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("should parse ELF");
    let mut text_bytes = Vec::new();
    for section in file.sections() {
        if section.name().unwrap_or("") == ".text" {
            text_bytes = section.data().unwrap_or(b"").to_vec();
            break;
        }
    }
    assert!(!text_bytes.is_empty(), ".text must exist");

    let mut decoder = Decoder::new(64, &text_bytes, DecoderOptions::NONE);
    let movs = decoder
        .iter()
        .filter(|i| i.mnemonic() == IcedMnemonic::Mov)
        .count();
    assert!(
        movs >= 3,
        "expected >= 3 reg-reg `mov` instructions, got {movs}"
    );
    let _ = std::fs::remove_file(&tmp);
}

// ===========================================================================
// Tier B: encode -> iced decode of the width-aware lowering tables.
// ===========================================================================

/// Encode a single production `Instruction` and decode the first resulting
/// instruction with the iced-x86 `Decoder`.
fn encode_and_decode(inst: &Instruction) -> IcedInstruction {
    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    encode_instruction(inst, &mut buf, &mut stats).expect("encode_instruction failed");
    let bytes = buf.as_slice().to_vec();
    assert!(!bytes.is_empty(), "encoder produced no bytes");
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    decoder.decode(),
    mode: InstrMode::default(),
}

/// Decode and also return the iced `IntelFormatter` rendering of an
/// instruction (for diagnostics).
fn rendered(inst: &IcedInstruction) -> String {
    let mut f = IntelFormatter::new();
    let mut s = String::new();
    f.format(inst, &mut s);
    s
}

/// Build the `MovSized { width }` instruction the m3-001 width router / m3-003
/// retarget emits: `mov <reg-of-width>, imm`.
fn mov_sized(width: IntWidth, reg: u8, imm: i64) -> Instruction {
    Instruction {
        mnemonic: Mnemonic::MovSized { width },
        operands: smallvec::smallvec![Operand::Reg(RegId(reg)), Operand::Imm64(imm)],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
    }
}

/// Build the cast `Instruction` that m3-002's `cast_plan` selects for `shape`,
/// targeting `mov/movsx/movzx rax, <src-sub-reg-of-rdi>`. Returns `None` for a
/// `Nop` (same-width) plan.
fn cast_instruction(shape: CastShape) -> Option<Instruction> {
    let (mnemonic, encoding_hint, _size) = cast_plan(shape).instruction()?;
    Some(Instruction {
        mnemonic,
        // rax (dst) <- rdi (src); the encoder narrows rdi per operand_size.
        operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(7))],
        encoding_hint,
        byte_offset_in_text: None,
            mode: InstrMode::default(),
        })
}

fn shape(src_width: u8, dst_width: u8, src_signed: bool, dst_signed: bool) -> CastShape {
    CastShape {
        src_width,
        dst_width,
        src_signed,
        dst_signed,
    }
}

/// Size in bits of the destination register of a decoded instruction.
fn dst_reg_bits(inst: &IcedInstruction) -> u32 {
    assert!(inst.op_count() >= 1, "expected a destination operand");
    assert_eq!(inst.op0_kind(), OpKind::Register, "op0 must be a register");
    inst.op0_register().size() as u32 * 8
}

/// Size in bits of a register *source* operand (op1) of a decoded instruction.
fn src_reg_bits(inst: &IcedInstruction) -> u32 {
    assert!(inst.op_count() >= 2, "expected a source operand");
    assert_eq!(inst.op1_kind(), OpKind::Register, "op1 must be a register");
    inst.op1_register().size() as u32 * 8
}

// ---- m3-001 / m3-003: MovSized width-aware immediate moves -----------------

/// m3-001 (W8) / m3-003 (r8 retarget): `MovSized { W8 }` decodes to an 8-bit
/// immediate move (`mov r8, imm8`).
#[test]
fn tier_b_mov_sized_w8_decodes_to_8bit_imm_move() {
    let inst = mov_sized(IntWidth::W8, /* rcx -> cl */ 1, 0x2A);
    let dec = encode_and_decode(&inst);
    assert_eq!(dec.mnemonic(), IcedMnemonic::Mov, "{}", rendered(&dec));
    assert_eq!(
        dst_reg_bits(&dec),
        8,
        "dst should be an 8-bit reg: {}",
        rendered(&dec)
    );
    assert!(
        matches!(dec.op1_kind(), OpKind::Immediate8),
        "src should be imm8: {}",
        rendered(&dec)
    );
    assert_eq!(dec.op0_register(), Register::CL);
    assert_eq!(dec.immediate8(), 0x2A);
}

/// m3-001 (W16): `MovSized { W16 }` decodes to a 16-bit immediate move
/// (`mov r16, imm16`, operand-size prefix 66h).
#[test]
fn tier_b_mov_sized_w16_decodes_to_16bit_imm_move() {
    let inst = mov_sized(IntWidth::W16, /* rcx -> cx */ 1, 0x2A);
    let dec = encode_and_decode(&inst);
    assert_eq!(dec.mnemonic(), IcedMnemonic::Mov, "{}", rendered(&dec));
    assert_eq!(
        dst_reg_bits(&dec),
        16,
        "dst should be a 16-bit reg: {}",
        rendered(&dec)
    );
    assert_eq!(dec.op0_register(), Register::CX);
}

/// m3-001 (W32) / m3-003 (r32 retarget): `MovSized { W32 }` decodes to a 32-bit
/// immediate move (`mov r32, imm32`, implicit zero-extend, no REX.W).
#[test]
fn tier_b_mov_sized_w32_decodes_to_32bit_imm_move() {
    let inst = mov_sized(IntWidth::W32, /* rcx -> ecx */ 1, 0x2A);
    let dec = encode_and_decode(&inst);
    assert_eq!(dec.mnemonic(), IcedMnemonic::Mov, "{}", rendered(&dec));
    assert_eq!(
        dst_reg_bits(&dec),
        32,
        "dst should be a 32-bit reg: {}",
        rendered(&dec)
    );
    assert_eq!(dec.op0_register(), Register::ECX);
}

/// m3-003 ship-minimum boundary: `mov al, imm` retargets to `MovSized { W8 }`.
/// (`rax` -> `al`.)
#[test]
fn tier_b_mov_r8_retarget_decodes_to_al_imm8() {
    let inst = mov_sized(IntWidth::W8, /* rax -> al */ 0, 0x2A);
    let dec = encode_and_decode(&inst);
    assert_eq!(dec.mnemonic(), IcedMnemonic::Mov, "{}", rendered(&dec));
    assert_eq!(dec.op0_register(), Register::AL, "{}", rendered(&dec));
    assert_eq!(dst_reg_bits(&dec), 8);
}

/// m3-003 ship-minimum boundary: `mov eax, imm` retargets to `MovSized { W32 }`.
#[test]
fn tier_b_mov_r32_retarget_decodes_to_eax_imm32() {
    let inst = mov_sized(IntWidth::W32, /* rax -> eax */ 0, 0x2A);
    let dec = encode_and_decode(&inst);
    assert_eq!(dec.mnemonic(), IcedMnemonic::Mov, "{}", rendered(&dec));
    assert_eq!(dec.op0_register(), Register::EAX, "{}", rendered(&dec));
    assert_eq!(dst_reg_bits(&dec), 32);
}

// ---- m3-002: cast dispatch table ------------------------------------------

// The m3-002 cast lowering has two distinct layers, and Tier B validates each
// against the layer that is actually responsible for it:
//
//   1. DISPATCH TABLE (`cast_plan` -> `CastPlan::instruction`): selects the
//      mnemonic and `operand_size` hint per `(src,dst,signed)` shape. This is
//      the milestone's real deliverable, so every shape is asserted at this
//      layer (mnemonic + hinted width).
//
//   2. ENCODER (`encode_instruction`): turns the selected `(mnemonic, hint,
//      operands)` into bytes. The encoder honours the width hint only for the
//      sign-extending forms (`movsx` / `movsxd`), which are therefore *also*
//      validated by an end-to-end encode -> iced-decode round-trip. The
//      zero-extending and width-narrowing forms are NOT yet width-honoured by
//      the encoder (empirically verified, see the `*_encoder_gap` pins below),
//      so asserting a decoded sub-64-bit width there would be asserting a bug.
//      Those pins lock the *current* encoder behaviour so the suite flags the
//      day the encoder catches up to the dispatch table.

/// Assert the mnemonic + hinted source/destination width the m3-002 dispatch
/// table selects for `shape` (layer 1).
fn assert_cast_plan(shape: CastShape, want_mnemonic: Mnemonic, want_operand_size: u8) {
    let (mnemonic, hint, _size) = cast_plan(shape)
        .instruction()
        .expect("non-nop cast should produce an instruction");
    assert_eq!(mnemonic, want_mnemonic, "dispatch mnemonic for {shape:?}");
    let got_size = hint
        .expect("widening/narrowing plans carry a width hint")
        .operand_size;
    assert_eq!(
        got_size, want_operand_size,
        "dispatch operand_size for {shape:?}"
    );
}

/// m3-002 dispatch (layer 1): widening signed selects `movsx` (1/2-byte src) or
/// `movsxd` (4-byte src, `Mnemonic::Movsx` + hint 4); widening unsigned selects
/// `movzx` (1/2-byte src) or 32-bit `mov` (4-byte src); narrowing selects a
/// destination-sized `mov`.
#[test]
fn tier_b_cast_dispatch_table_selects_expected_mnemonic_and_width() {
    // widening signed
    assert_cast_plan(shape(1, 8, true, true), Mnemonic::Movsx, 1);
    assert_cast_plan(shape(2, 8, true, true), Mnemonic::Movsx, 2);
    assert_cast_plan(shape(4, 8, true, true), Mnemonic::Movsx, 4);
    // widening unsigned
    assert_cast_plan(shape(1, 8, false, false), Mnemonic::Movzx, 1);
    assert_cast_plan(shape(2, 8, false, false), Mnemonic::Movzx, 2);
    assert_cast_plan(shape(4, 8, false, false), Mnemonic::Mov, 4);
    // narrowing (destination-sized mov)
    assert_cast_plan(shape(8, 4, false, false), Mnemonic::Mov, 4);
    assert_cast_plan(shape(8, 2, false, false), Mnemonic::Mov, 2);
    assert_cast_plan(shape(8, 1, false, false), Mnemonic::Mov, 1);
}

/// m3-002 widening signed, 1-byte source: `i8 -> i64` encodes and decodes to
/// `movsx rax, <r8>` — destination 64-bit, source 8-bit (layer 2 round-trip).
#[test]
fn tier_b_cast_widen_signed_i8_to_i64_is_movsx_8bit_src() {
    let inst = cast_instruction(shape(1, 8, true, true)).expect("non-nop cast");
    let dec = encode_and_decode(&inst);
    assert_eq!(dec.mnemonic(), IcedMnemonic::Movsx, "{}", rendered(&dec));
    assert_eq!(dst_reg_bits(&dec), 64, "{}", rendered(&dec));
    assert_eq!(src_reg_bits(&dec), 8, "{}", rendered(&dec));
}

/// m3-002 widening signed, 2-byte source: `i16 -> i64` encodes and decodes to
/// `movsx rax, <r16>` — source 16-bit (layer 2 round-trip).
#[test]
fn tier_b_cast_widen_signed_i16_to_i64_is_movsx_16bit_src() {
    let inst = cast_instruction(shape(2, 8, true, true)).expect("non-nop cast");
    let dec = encode_and_decode(&inst);
    assert_eq!(dec.mnemonic(), IcedMnemonic::Movsx, "{}", rendered(&dec));
    assert_eq!(dst_reg_bits(&dec), 64, "{}", rendered(&dec));
    assert_eq!(src_reg_bits(&dec), 16, "{}", rendered(&dec));
}

/// m3-002 widening signed, 4-byte source: `i32 -> i64` encodes and decodes to
/// `movsxd rax, <r32>` (iced spells the 0x63 form as `Movsxd`), source 32-bit
/// (layer 2 round-trip).
#[test]
fn tier_b_cast_widen_signed_i32_to_i64_is_movsxd_32bit_src() {
    let inst = cast_instruction(shape(4, 8, true, true)).expect("non-nop cast");
    let dec = encode_and_decode(&inst);
    assert_eq!(dec.mnemonic(), IcedMnemonic::Movsxd, "{}", rendered(&dec));
    assert_eq!(dst_reg_bits(&dec), 64, "{}", rendered(&dec));
    assert_eq!(src_reg_bits(&dec), 32, "{}", rendered(&dec));
}

/// m3-002 same-width reinterpret: `u32 -> i32` is a `Nop` — no instruction is
/// emitted by the dispatch table (layer 1).
#[test]
fn tier_b_cast_same_width_reinterpret_is_nop() {
    assert!(
        cast_instruction(shape(4, 4, false, true)).is_none(),
        "same-width reinterpret must emit no conversion instruction"
    );
}

// ---- m3-002 encoder gaps: pinned current behaviour (layer 2) ----------------
//
// These tests document — and lock — the fact that the production encoder does
// not yet realise the dispatch table's narrowing/zero-extending widths in
// bytes. They are *intentionally* asserting today's behaviour; when the encoder
// is extended (e.g. a width-honouring `mov` and a register-source `movzx`),
// these pins fail and should be promoted to positive width assertions.

/// Encoder gap: the zero-extending plan (`movzx rax, <reg>`) selected for
/// unsigned 1/2-byte widening cannot be encoded — `encode_movzx` accepts only a
/// memory source, so the register-source cast errors `OperandShape`. (The
/// dispatch table is correct; the encoder lacks the reg-reg `movzx` form.)
#[test]
fn tier_b_cast_widen_unsigned_movzx_regreg_is_encoder_gap() {
    for s in [shape(1, 8, false, false), shape(2, 8, false, false)] {
        let inst = cast_instruction(s).expect("non-nop cast");
        let mut buf = CodeBuffer::new();
        let mut stats = EncodeStats::new();
        let res = encode_instruction(&inst, &mut buf, &mut stats);
        assert!(
            res.is_err(),
            "register-source movzx is not yet encodable; if this now succeeds, \
             promote to a positive width assertion (shape {s:?})"
        );
    }
}

/// Encoder gap: the `mov` plans for unsigned 32->64 widening (`Mov32`) and for
/// narrowing (`Narrow`) carry a sub-64-bit `operand_size` hint, but the reg-reg
/// `mov` encoder ignores the hint and always emits the 64-bit `mov rax,rdi`.
/// Pin that current behaviour: decoded destination is 64-bit, not the hinted
/// width.
#[test]
fn tier_b_cast_mov_width_hint_not_yet_honoured_by_encoder() {
    for s in [
        shape(4, 8, false, false), // Mov32 (u32 -> u64)
        shape(8, 4, false, false), // Narrow (u64 -> u32)
        shape(8, 2, false, false), // Narrow (u64 -> u16)
    ] {
        let inst = cast_instruction(s).expect("non-nop cast");
        let dec = encode_and_decode(&inst);
        assert_eq!(dec.mnemonic(), IcedMnemonic::Mov, "{}", rendered(&dec));
        assert_eq!(
            dst_reg_bits(&dec),
            64,
            "reg-reg mov currently ignores the width hint (shape {s:?}); if this \
             is now sub-64-bit, promote to a positive width assertion: {}",
            rendered(&dec)
        );
    }
}

// ===========================================================================
// Coverage guard.
// ===========================================================================

/// Guard against accidental shrinkage of the gating corpus: >= 15 source/disasm
/// pairs across the three milestones.
///
/// Tier A: 10 `build`-CLI fixtures (6 cast sources + 4 block-let sources) + the
/// reg-reg `mov` regression = 11 build/disasm pairs.
///
/// Tier B: 12 encode/decode `#[test]` functions covering, in aggregate, the 5
/// `MovSized` width forms (m3-001 / m3-003), all 9 non-nop cast dispatch shapes
/// plus the same-width nop (m3-002, layer 1), the 3 sign-extending cast
/// round-trips (m3-002, layer 2), and the 2 pinned encoder-gap shapes.
#[test]
fn corpus_covers_at_least_15_pairs() {
    let tier_a = TIER_A_FIXTURES.len();
    // Tier-B pairs are the `#[test]` functions prefixed `tier_b_`. Derive the
    // count from this very source file (via `include_str!`) so deleting a
    // Tier-B test actually shrinks the number the guard sees, rather than
    // trusting a hand-maintained literal.
    let tier_b = include_str!("build_emit_pa8_m3_004.rs")
        .matches("fn tier_b_")
        .count();
    let reg_reg_regression = 1;
    let total = tier_a + tier_b + reg_reg_regression;
    assert!(
        total >= 15,
        "expected >= 15 source/disasm pairs, found {total} (tier_a={tier_a}, tier_b={tier_b})"
    );

    // Cast-source coverage breakdown (Tier A, all canonical movsxd).
    let cast_sources = TIER_A_FIXTURES
        .iter()
        .filter(|(b, _, _)| b.starts_with("cast_"))
        .count();
    assert_eq!(cast_sources, 6, "expected 6 cast-source fixtures");

    let block_let_sources = TIER_A_FIXTURES
        .iter()
        .filter(|(b, _, _)| b.starts_with("block_let_"))
        .count();
    assert_eq!(block_let_sources, 4, "expected 4 block-let-source fixtures");
}
