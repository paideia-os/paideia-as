//! PA-v0.21-003 (#1279) round-trip: RefcountOps + BitmapOps atomic-primitive
//! lowering recipes. Each method registered in stdlib_lowering.rs is exercised
//! for:
//!   - correct instruction count and mnemonic sequence
//!   - correct operand pattern (SysVRegs: RDI = pointer, RSI = bit index for
//!     BitmapOps; RAX = return)
//!   - SysVRegs arg convention (so the elaborator will marshal args into
//!     RDI/RSI/RDX before splicing the recipe)
//!
//! Recipe designs (SysVRegs convention):
//!   RefcountOps::refcount_incr(counter)          → mov eax, 1 ; lock xadd_d [rdi], eax
//!   RefcountOps::refcount_decr(counter)          → mov eax, -1; lock xadd_d [rdi], eax
//!   RefcountOps::refcount_decr_and_test(counter) → mov eax, -1; lock xadd_d [rdi], eax;
//!                                                  cmp eax, 1; sete al; movzx eax, al
//!   BitmapOps::bitmap_set(bmap, bit_index)       → lock bts_q [rdi], rsi; setc al; movzx eax, al
//!   BitmapOps::bitmap_clear(bmap, bit_index)     → lock btr_q [rdi], rsi; setc al; movzx eax, al
//!   BitmapOps::bitmap_toggle(bmap, bit_index)    → lock btc_q [rdi], rsi; setc al; movzx eax, al

use paideia_as_ir::{
    abi, InstrMode, IrArena, IrNodeId,
    instruction::{Cond, IntWidth, Mnemonic, Operand, Scale},
};
use paideia_as_elaborator::stdlib_lowering::{ArgConvention, LoweringRecipe};

fn expect_sysv_regs(recipe: &LoweringRecipe, n: usize, method: &str) {
    assert_eq!(
        recipe.arg_convention,
        ArgConvention::SysVRegs,
        "{}: arg convention must be SysVRegs (RDI/RSI marshalled by caller)",
        method
    );
    assert!(
        recipe.labels.is_empty(),
        "{}: recipe should not declare labels",
        method
    );
    assert_eq!(
        recipe.instructions.len(),
        n,
        "{}: recipe should have exactly {} instruction(s)",
        method,
        n
    );
}

fn expect_mem_rdi_disp0(op: &Operand, ctx: &str) {
    match op {
        Operand::MemSib { base, index, scale, disp } => {
            assert_eq!(*base, abi::RDI, "{}: base must be RDI", ctx);
            assert!(index.is_none(), "{}: no index", ctx);
            assert_eq!(*scale, Scale::X1, "{}: scale X1", ctx);
            assert_eq!(*disp, 0, "{}: disp 0", ctx);
        }
        other => panic!("{}: expected MemSib{{RDI, .., 0}}, got {:?}", ctx, other),
    }
}

fn expect_reg(op: &Operand, expected: paideia_as_ir::RegId, ctx: &str) {
    match op {
        Operand::Reg(r) => assert_eq!(*r, expected, "{}: wrong register", ctx),
        other => panic!("{}: expected Reg({:?}), got {:?}", ctx, expected, other),
    }
}

fn expect_imm(op: &Operand, expected: i64, ctx: &str) {
    match op {
        Operand::Imm64(v) => assert_eq!(*v, expected, "{}: wrong immediate", ctx),
        other => panic!("{}: expected Imm64({}), got {:?}", ctx, expected, other),
    }
}

// ============================================================================
// RefcountOps
// ============================================================================

#[test]
fn refcount_incr_lowers_to_mov_1_then_lock_xadd_d() {
    let arena = IrArena::new();
    let counter_id = IrNodeId::new(1).expect("valid node id");
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "RefcountOps",
        "refcount_incr",
        InstrMode::Mode64,
        &[counter_id],
        &arena,
    );
    let recipe = result
        .expect("refcount_incr must be registered")
        .expect("refcount_incr lowering must succeed (SysVRegs takes any arg)");
    expect_sysv_regs(&recipe, 2, "refcount_incr");

    // Inst 0: mov eax, 1
    let mov = &recipe.instructions[0];
    assert_eq!(mov.mnemonic, Mnemonic::MovSized { width: IntWidth::W32 });
    assert_eq!(mov.operands.len(), 2);
    expect_reg(&mov.operands[0], abi::RAX, "refcount_incr/mov/dst");
    expect_imm(&mov.operands[1], 1, "refcount_incr/mov/imm");

    // Inst 1: lock xadd_d [rdi], eax
    let xadd = &recipe.instructions[1];
    assert_eq!(xadd.mnemonic, Mnemonic::LockXadd { width: IntWidth::W32 });
    assert_eq!(xadd.operands.len(), 2);
    expect_mem_rdi_disp0(&xadd.operands[0], "refcount_incr/xadd/mem");
    expect_reg(&xadd.operands[1], abi::RAX, "refcount_incr/xadd/src");
}

#[test]
fn refcount_decr_lowers_to_mov_neg1_then_lock_xadd_d() {
    let arena = IrArena::new();
    let counter_id = IrNodeId::new(1).expect("valid node id");
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "RefcountOps",
        "refcount_decr",
        InstrMode::Mode64,
        &[counter_id],
        &arena,
    );
    let recipe = result
        .expect("refcount_decr must be registered")
        .expect("refcount_decr lowering must succeed");
    expect_sysv_regs(&recipe, 2, "refcount_decr");

    let mov = &recipe.instructions[0];
    assert_eq!(mov.mnemonic, Mnemonic::MovSized { width: IntWidth::W32 });
    expect_imm(&mov.operands[1], -1, "refcount_decr/mov/imm");

    let xadd = &recipe.instructions[1];
    assert_eq!(xadd.mnemonic, Mnemonic::LockXadd { width: IntWidth::W32 });
    expect_mem_rdi_disp0(&xadd.operands[0], "refcount_decr/xadd/mem");
    expect_reg(&xadd.operands[1], abi::RAX, "refcount_decr/xadd/src");
}

#[test]
fn refcount_decr_and_test_lowers_to_xadd_cmp_sete_movzx() {
    let arena = IrArena::new();
    let counter_id = IrNodeId::new(1).expect("valid node id");
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "RefcountOps",
        "refcount_decr_and_test",
        InstrMode::Mode64,
        &[counter_id],
        &arena,
    );
    let recipe = result
        .expect("refcount_decr_and_test must be registered")
        .expect("refcount_decr_and_test lowering must succeed");
    expect_sysv_regs(&recipe, 5, "refcount_decr_and_test");

    // 0: mov eax, -1
    assert_eq!(
        recipe.instructions[0].mnemonic,
        Mnemonic::MovSized { width: IntWidth::W32 }
    );
    expect_imm(&recipe.instructions[0].operands[1], -1, "step0/imm");

    // 1: lock xadd_d [rdi], eax
    assert_eq!(
        recipe.instructions[1].mnemonic,
        Mnemonic::LockXadd { width: IntWidth::W32 }
    );

    // 2: cmp eax, 1 — ZF = 1 iff previous == 1 (i.e., new == 0)
    assert_eq!(
        recipe.instructions[2].mnemonic,
        Mnemonic::CmpSized { width: IntWidth::W32 }
    );
    expect_reg(&recipe.instructions[2].operands[0], abi::RAX, "cmp/dst");
    expect_imm(&recipe.instructions[2].operands[1], 1, "cmp/imm");

    // 3: sete al
    assert_eq!(recipe.instructions[3].mnemonic, Mnemonic::Setcc(Cond::Eq));
    expect_reg(&recipe.instructions[3].operands[0], abi::RAX, "sete/dst");

    // 4: movzx eax, al — encoding_hint operand_size=1 so movzx r/m8 form fires
    let movzx = &recipe.instructions[4];
    assert_eq!(movzx.mnemonic, Mnemonic::Movzx);
    let hint = movzx.encoding_hint.expect("movzx must carry EncodingHint");
    assert_eq!(hint.operand_size, 1, "movzx source is 1 byte (AL)");
}

// ============================================================================
// BitmapOps atomic RMW primitives
// ============================================================================

fn assert_bitmap_atomic_recipe(method: &str, expected_op: Mnemonic) {
    let arena = IrArena::new();
    let bmap_id = IrNodeId::new(1).expect("valid node id");
    let bit_id = IrNodeId::new(2).expect("valid node id");
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "BitmapOps",
        method,
        InstrMode::Mode64,
        &[bmap_id, bit_id],
        &arena,
    );
    let recipe = result
        .unwrap_or_else(|| panic!("BitmapOps::{} must be registered", method))
        .unwrap_or_else(|e| panic!("BitmapOps::{} lowering must succeed, got {:?}", method, e));
    expect_sysv_regs(&recipe, 3, method);

    // 0: lock b(ts|tr|tc)_q [rdi], rsi
    let bt = &recipe.instructions[0];
    assert_eq!(bt.mnemonic, expected_op, "{}: first instruction mnemonic", method);
    assert_eq!(bt.operands.len(), 2);
    expect_mem_rdi_disp0(&bt.operands[0], &format!("{}/bt/mem", method));
    expect_reg(&bt.operands[1], abi::RSI, &format!("{}/bt/bit_index", method));

    // 1: setc al (Cond::Below → SETC opcode 0F 92)
    let setcc = &recipe.instructions[1];
    assert_eq!(
        setcc.mnemonic,
        Mnemonic::Setcc(Cond::Below),
        "{}: setcc must be Below/setc so CF flows into AL",
        method
    );
    expect_reg(&setcc.operands[0], abi::RAX, &format!("{}/setc/dst", method));

    // 2: movzx eax, al
    let movzx = &recipe.instructions[2];
    assert_eq!(movzx.mnemonic, Mnemonic::Movzx);
    let hint = movzx.encoding_hint.expect("movzx must carry EncodingHint");
    assert_eq!(hint.operand_size, 1, "movzx source is AL (1 byte)");
}

#[test]
fn bitmap_set_lowers_to_lock_bts_setc_movzx() {
    assert_bitmap_atomic_recipe("bitmap_set", Mnemonic::LockBts { width: IntWidth::W64 });
}

#[test]
fn bitmap_clear_lowers_to_lock_btr_setc_movzx() {
    assert_bitmap_atomic_recipe("bitmap_clear", Mnemonic::LockBtr { width: IntWidth::W64 });
}

#[test]
fn bitmap_toggle_lowers_to_lock_btc_setc_movzx() {
    assert_bitmap_atomic_recipe("bitmap_toggle", Mnemonic::LockBtc { width: IntWidth::W64 });
}

// ============================================================================
// End-to-end byte-exact verification through the encoder
// ============================================================================
//
// Verify that a real encoding pass over each recipe reproduces the byte
// sequence documented in the recipe's inline comments — the ultimate
// witness that stdlib_lowering + encoder + SysV convention agree.

fn encode_recipe(recipe: &LoweringRecipe) -> Vec<u8> {
    use paideia_as_encoder::{CodeBuffer, EncodeStats};
    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    for inst in &recipe.instructions {
        paideia_as_encoder::encode_instruction(inst, &mut buf, &mut stats)
            .expect("encode_instruction must succeed for a stdlib recipe");
    }
    buf.as_slice().to_vec()
}

#[test]
fn refcount_incr_encodes_to_expected_bytes() {
    let arena = IrArena::new();
    let counter_id = IrNodeId::new(1).expect("valid node id");
    let recipe = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "RefcountOps", "refcount_incr", InstrMode::Mode64, &[counter_id], &arena,
    ).unwrap().unwrap();

    // mov eax, 1              → B8 01 00 00 00                       (5 bytes)
    // lock xadd_d [rdi], eax  → F0 0F C1 07                          (4 bytes)
    let bytes = encode_recipe(&recipe);
    assert_eq!(
        bytes,
        vec![
            0xB8, 0x01, 0x00, 0x00, 0x00,
            0xF0, 0x0F, 0xC1, 0x07,
        ],
        "refcount_incr byte sequence"
    );
}

#[test]
fn refcount_decr_encodes_to_expected_bytes() {
    let arena = IrArena::new();
    let counter_id = IrNodeId::new(1).expect("valid node id");
    let recipe = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "RefcountOps", "refcount_decr", InstrMode::Mode64, &[counter_id], &arena,
    ).unwrap().unwrap();

    // mov eax, -1              → B8 FF FF FF FF
    // lock xadd_d [rdi], eax   → F0 0F C1 07
    let bytes = encode_recipe(&recipe);
    assert_eq!(
        bytes,
        vec![
            0xB8, 0xFF, 0xFF, 0xFF, 0xFF,
            0xF0, 0x0F, 0xC1, 0x07,
        ],
        "refcount_decr byte sequence"
    );
}

#[test]
fn bitmap_set_encodes_to_expected_bytes() {
    let arena = IrArena::new();
    let bmap_id = IrNodeId::new(1).expect("valid node id");
    let bit_id = IrNodeId::new(2).expect("valid node id");
    let recipe = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "BitmapOps", "bitmap_set", InstrMode::Mode64, &[bmap_id, bit_id], &arena,
    ).unwrap().unwrap();

    // lock bts_q [rdi], rsi    → F0 48 0F AB 37   (5 bytes)
    // setc al                  → 0F 92 C0         (3 bytes)
    // movzx eax, al            → 48 0F B6 C0      (4 bytes)
    let bytes = encode_recipe(&recipe);
    assert_eq!(
        bytes,
        vec![
            0xF0, 0x48, 0x0F, 0xAB, 0x37,
            0x0F, 0x92, 0xC0,
            0x48, 0x0F, 0xB6, 0xC0,
        ],
        "bitmap_set byte sequence"
    );
}

#[test]
fn bitmap_clear_encodes_to_expected_bytes() {
    let arena = IrArena::new();
    let bmap_id = IrNodeId::new(1).expect("valid node id");
    let bit_id = IrNodeId::new(2).expect("valid node id");
    let recipe = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "BitmapOps", "bitmap_clear", InstrMode::Mode64, &[bmap_id, bit_id], &arena,
    ).unwrap().unwrap();

    // lock btr_q [rdi], rsi    → F0 48 0F B3 37
    // setc al                  → 0F 92 C0
    // movzx eax, al            → 48 0F B6 C0
    let bytes = encode_recipe(&recipe);
    assert_eq!(
        bytes,
        vec![
            0xF0, 0x48, 0x0F, 0xB3, 0x37,
            0x0F, 0x92, 0xC0,
            0x48, 0x0F, 0xB6, 0xC0,
        ],
        "bitmap_clear byte sequence"
    );
}

#[test]
fn bitmap_toggle_encodes_to_expected_bytes() {
    let arena = IrArena::new();
    let bmap_id = IrNodeId::new(1).expect("valid node id");
    let bit_id = IrNodeId::new(2).expect("valid node id");
    let recipe = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "BitmapOps", "bitmap_toggle", InstrMode::Mode64, &[bmap_id, bit_id], &arena,
    ).unwrap().unwrap();

    // lock btc_q [rdi], rsi    → F0 48 0F BB 37
    // setc al                  → 0F 92 C0
    // movzx eax, al            → 48 0F B6 C0
    let bytes = encode_recipe(&recipe);
    assert_eq!(
        bytes,
        vec![
            0xF0, 0x48, 0x0F, 0xBB, 0x37,
            0x0F, 0x92, 0xC0,
            0x48, 0x0F, 0xB6, 0xC0,
        ],
        "bitmap_toggle byte sequence"
    );
}

// ============================================================================
// Negative tests: unknown methods fall through
// ============================================================================

#[test]
fn unknown_refcount_method_returns_none() {
    let arena = IrArena::new();
    let counter_id = IrNodeId::new(1).expect("valid node id");
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "RefcountOps",
        "refcount_read", // not registered
        InstrMode::Mode64,
        &[counter_id],
        &arena,
    );
    assert!(result.is_none(), "unknown RefcountOps method must fall through to normal call emission");
}

#[test]
fn unknown_bitmap_method_returns_none() {
    let arena = IrArena::new();
    let bmap_id = IrNodeId::new(1).expect("valid node id");
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "BitmapOps",
        "bitmap_pop_count", // not registered
        InstrMode::Mode64,
        &[bmap_id],
        &arena,
    );
    assert!(result.is_none(), "unknown BitmapOps method must fall through");
}
