//! Byte-exact validation of the atomic load / store code-recipes that
//! `@atomic(Ordering)` bindings compile down to (paideia-as#1296, v0.21-003b).
//!
//! **Scope**: this file exercises the **encoder building blocks** the
//! elaborator will call from a phase-2 emit change — plain `mov` variants
//! and the `mfence` fence — arranged into the exact byte sequences that the
//! four x86_64 orderings prescribe. The elaborator emit path itself is
//! deliberately inert in phase-1 (parser + AST/IR plumbing only, see
//! `crates/paideia-as-parser/src/parse_stmt.rs::parse_optional_atomic_prefix`),
//! so this file's role is to freeze the *recipe* against a decoder round-trip
//! so a later elaborator wiring cannot silently drift the encoding.
//!
//! # x86_64 TSO lowering table (per `AtomicOrdering` × integer width)
//!
//! | Ordering | Load                          | Store                         |
//! |----------|-------------------------------|-------------------------------|
//! | Relaxed  | `mov r,  [rdi]`               | `mov [rdi], r`                |
//! | Acquire  | `mov r,  [rdi]` (TSO gives it)| — (paired-store side)         |
//! | Release  | — (paired-load side)          | `mov [rdi], r` (TSO gives it) |
//! | SeqCst   | `mfence ; mov r, [rdi]`       | `mov [rdi], r ; mfence`       |
//!
//! Rationale: on x86_64 an aligned scalar `mov` **is** acquire on loads and
//! release on stores — TSO forbids the reordering that any weaker model
//! would allow. Only SeqCst needs an explicit `mfence` to enforce a single
//! global total order across cores. This mirrors LLVM's x86 lowering and
//! the Intel SDM Vol. 3A §8.2 memory-ordering guarantees.
//!
//! # Widths
//!
//! The user-visible integer types the paideia-os kernel refcount / spinlock
//! sites care about — `i8`, `i16`, `u32`, `u64` — map onto encoder widths
//! `W8`, `W16`, `W32`, `W64`. `paideia-as` does not distinguish sign at the
//! encoder boundary (bit-width only), so the sign is exercised at the
//! elaborator's type-check layer in a separate suite; the byte-exact
//! encoding is identical for signed / unsigned of the same width.

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, IntWidth, Mnemonic, Operand, RegId, Scale};

// ---------- shared helpers ----------

/// Encode one instruction in 64-bit mode and return the emitted bytes.
fn encode_one(mnemonic: Mnemonic, operands: Vec<Operand>) -> Vec<u8> {
    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let inst = Instruction {
        mnemonic,
        operands: smallvec::SmallVec::from_vec(operands),
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
        emission_order: 0,
    };
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encode_instruction: unexpected failure");
    buf.as_slice().to_vec()
}

/// Encode a plain `mov r{width}, [rdi]` load and return the bytes.
fn encode_load(width: IntWidth) -> Vec<u8> {
    encode_one(
        Mnemonic::MovSized { width },
        vec![
            Operand::Reg(RegId(0)), // rax / eax / ax / al
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
        ],
    )
}

/// Encode a plain `mov [rdi], r{width}` store and return the bytes.
fn encode_store(width: IntWidth) -> Vec<u8> {
    encode_one(
        Mnemonic::MovSized { width },
        vec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(0)),
        ],
    )
}

/// Encode a bare `mfence` and return the three canonical bytes (0F AE F0).
fn encode_mfence() -> Vec<u8> {
    encode_one(Mnemonic::Mfence, vec![])
}

// The three canonical mfence bytes (Intel SDM Vol. 2A: MFENCE = 0F AE F0).
const MFENCE_BYTES: [u8; 3] = [0x0F, 0xAE, 0xF0];

/// Fold a load's bytes with the ordering-appropriate fences prepended.
///
/// Phase-2 elaborator emit contract: SeqCst load emits `mfence` **before**
/// the plain `mov`. Every other ordering emits just the `mov`.
fn atomic_load_recipe(width: IntWidth, ordering: RecipeOrd) -> Vec<u8> {
    let mut out = Vec::new();
    if matches!(ordering, RecipeOrd::SeqCst) {
        out.extend_from_slice(&MFENCE_BYTES);
    }
    out.extend_from_slice(&encode_load(width));
    out
}

/// Fold a store's bytes with the ordering-appropriate fences appended.
///
/// Phase-2 elaborator emit contract: SeqCst store emits `mfence` **after**
/// the plain `mov`. Every other ordering emits just the `mov`.
fn atomic_store_recipe(width: IntWidth, ordering: RecipeOrd) -> Vec<u8> {
    let mut out = encode_store(width);
    if matches!(ordering, RecipeOrd::SeqCst) {
        out.extend_from_slice(&MFENCE_BYTES);
    }
    out
}

/// Local enum for the recipe tables — mirrors [`paideia_as_ast::AtomicOrdering`]
/// / [`paideia_as_ir::AtomicOrdering`] one-to-one but avoids either dependency
/// so the encoder test crate stays free of the AST / IR ordering surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RecipeOrd {
    Relaxed,
    Acquire,
    Release,
    SeqCst,
}

// ---------- fence-primitive sanity ----------

/// Every recipe below composes onto this ground truth: `mfence` must be
/// `0F AE F0`. If this test fails, every recipe test below is meaningless.
#[test]
fn mfence_primitive_is_0f_ae_f0() {
    assert_eq!(encode_mfence(), MFENCE_BYTES);
}

// ---------- Relaxed load (mov reg, [rdi]) ----------
//
// Baseline case: no fence, plain `mov`. All four widths must round-trip
// to the standard x86 load opcodes.

#[test]
fn atomic_load_relaxed_w8_al_rdi() {
    // mov al, [rdi] → 8A 07
    assert_eq!(atomic_load_recipe(IntWidth::W8, RecipeOrd::Relaxed), &[0x8A, 0x07]);
}

#[test]
fn atomic_load_relaxed_w16_ax_rdi() {
    // mov ax, [rdi] → 66 8B 07
    assert_eq!(
        atomic_load_recipe(IntWidth::W16, RecipeOrd::Relaxed),
        &[0x66, 0x8B, 0x07]
    );
}

#[test]
fn atomic_load_relaxed_w32_eax_rdi() {
    // mov eax, [rdi] → 8B 07
    assert_eq!(atomic_load_recipe(IntWidth::W32, RecipeOrd::Relaxed), &[0x8B, 0x07]);
}

#[test]
fn atomic_load_relaxed_w64_rax_rdi() {
    // mov rax, [rdi] → 48 8B 07
    assert_eq!(
        atomic_load_recipe(IntWidth::W64, RecipeOrd::Relaxed),
        &[0x48, 0x8B, 0x07]
    );
}

// ---------- Acquire load (mov reg, [rdi]) ----------
//
// On x86_64 TSO an aligned load is already acquire — no lfence needed.
// The bytes therefore match Relaxed load exactly; the tests still assert
// distinct constants so a future ordering-model shift (e.g. adding an
// lfence prefix) can only slip in through a visible test-file change.

#[test]
fn atomic_load_acquire_w8_al_rdi() {
    assert_eq!(atomic_load_recipe(IntWidth::W8, RecipeOrd::Acquire), &[0x8A, 0x07]);
}

#[test]
fn atomic_load_acquire_w16_ax_rdi() {
    assert_eq!(
        atomic_load_recipe(IntWidth::W16, RecipeOrd::Acquire),
        &[0x66, 0x8B, 0x07]
    );
}

#[test]
fn atomic_load_acquire_w32_eax_rdi() {
    assert_eq!(atomic_load_recipe(IntWidth::W32, RecipeOrd::Acquire), &[0x8B, 0x07]);
}

#[test]
fn atomic_load_acquire_w64_rax_rdi() {
    assert_eq!(
        atomic_load_recipe(IntWidth::W64, RecipeOrd::Acquire),
        &[0x48, 0x8B, 0x07]
    );
}

// ---------- SeqCst load (mfence ; mov reg, [rdi]) ----------
//
// The `mfence` prefix is the only ordering-observable difference on the
// load side. It buys total ordering with SeqCst stores from other cores.

#[test]
fn atomic_load_seqcst_w8_mfence_then_al_rdi() {
    // 0F AE F0 ; 8A 07
    assert_eq!(
        atomic_load_recipe(IntWidth::W8, RecipeOrd::SeqCst),
        &[0x0F, 0xAE, 0xF0, 0x8A, 0x07]
    );
}

#[test]
fn atomic_load_seqcst_w16_mfence_then_ax_rdi() {
    // 0F AE F0 ; 66 8B 07
    assert_eq!(
        atomic_load_recipe(IntWidth::W16, RecipeOrd::SeqCst),
        &[0x0F, 0xAE, 0xF0, 0x66, 0x8B, 0x07]
    );
}

#[test]
fn atomic_load_seqcst_w32_mfence_then_eax_rdi() {
    // 0F AE F0 ; 8B 07
    assert_eq!(
        atomic_load_recipe(IntWidth::W32, RecipeOrd::SeqCst),
        &[0x0F, 0xAE, 0xF0, 0x8B, 0x07]
    );
}

#[test]
fn atomic_load_seqcst_w64_mfence_then_rax_rdi() {
    // 0F AE F0 ; 48 8B 07
    assert_eq!(
        atomic_load_recipe(IntWidth::W64, RecipeOrd::SeqCst),
        &[0x0F, 0xAE, 0xF0, 0x48, 0x8B, 0x07]
    );
}

// ---------- Relaxed store (mov [rdi], reg) ----------

#[test]
fn atomic_store_relaxed_w8_rdi_al() {
    // mov [rdi], al → 88 07
    assert_eq!(atomic_store_recipe(IntWidth::W8, RecipeOrd::Relaxed), &[0x88, 0x07]);
}

#[test]
fn atomic_store_relaxed_w16_rdi_ax() {
    // mov [rdi], ax → 66 89 07
    assert_eq!(
        atomic_store_recipe(IntWidth::W16, RecipeOrd::Relaxed),
        &[0x66, 0x89, 0x07]
    );
}

#[test]
fn atomic_store_relaxed_w32_rdi_eax() {
    // mov [rdi], eax → 89 07
    assert_eq!(atomic_store_recipe(IntWidth::W32, RecipeOrd::Relaxed), &[0x89, 0x07]);
}

#[test]
fn atomic_store_relaxed_w64_rdi_rax() {
    // mov [rdi], rax → 48 89 07
    assert_eq!(
        atomic_store_recipe(IntWidth::W64, RecipeOrd::Relaxed),
        &[0x48, 0x89, 0x07]
    );
}

// ---------- Release store (mov [rdi], reg) ----------
//
// x86_64 TSO gives release ordering on every aligned store — plain `mov`
// suffices. Same bytes as Relaxed store; explicit constants pin the shape.

#[test]
fn atomic_store_release_w8_rdi_al() {
    assert_eq!(atomic_store_recipe(IntWidth::W8, RecipeOrd::Release), &[0x88, 0x07]);
}

#[test]
fn atomic_store_release_w16_rdi_ax() {
    assert_eq!(
        atomic_store_recipe(IntWidth::W16, RecipeOrd::Release),
        &[0x66, 0x89, 0x07]
    );
}

#[test]
fn atomic_store_release_w32_rdi_eax() {
    assert_eq!(atomic_store_recipe(IntWidth::W32, RecipeOrd::Release), &[0x89, 0x07]);
}

#[test]
fn atomic_store_release_w64_rdi_rax() {
    assert_eq!(
        atomic_store_recipe(IntWidth::W64, RecipeOrd::Release),
        &[0x48, 0x89, 0x07]
    );
}

// ---------- SeqCst store (mov [rdi], reg ; mfence) ----------
//
// The trailing `mfence` is the SeqCst store's globally-visible marker.
// Without it, the store could be reordered past a subsequent SeqCst load
// on the same core, breaking the single-total-order guarantee.

#[test]
fn atomic_store_seqcst_w8_rdi_al_then_mfence() {
    // 88 07 ; 0F AE F0
    assert_eq!(
        atomic_store_recipe(IntWidth::W8, RecipeOrd::SeqCst),
        &[0x88, 0x07, 0x0F, 0xAE, 0xF0]
    );
}

#[test]
fn atomic_store_seqcst_w16_rdi_ax_then_mfence() {
    // 66 89 07 ; 0F AE F0
    assert_eq!(
        atomic_store_recipe(IntWidth::W16, RecipeOrd::SeqCst),
        &[0x66, 0x89, 0x07, 0x0F, 0xAE, 0xF0]
    );
}

#[test]
fn atomic_store_seqcst_w32_rdi_eax_then_mfence() {
    // 89 07 ; 0F AE F0
    assert_eq!(
        atomic_store_recipe(IntWidth::W32, RecipeOrd::SeqCst),
        &[0x89, 0x07, 0x0F, 0xAE, 0xF0]
    );
}

#[test]
fn atomic_store_seqcst_w64_rdi_rax_then_mfence() {
    // 48 89 07 ; 0F AE F0
    assert_eq!(
        atomic_store_recipe(IntWidth::W64, RecipeOrd::SeqCst),
        &[0x48, 0x89, 0x07, 0x0F, 0xAE, 0xF0]
    );
}

// ---------- iced-x86 round-trip: SeqCst load and store ----------
//
// Independent decoder witness that the composite byte streams above
// decode back to exactly the two-instruction sequences we claimed. If
// iced-x86 disagrees, one of the recipe constants above is wrong.

#[test]
fn atomic_load_seqcst_w64_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};
    let bytes = atomic_load_recipe(IntWidth::W64, RecipeOrd::SeqCst);
    let mut decoder = Decoder::with_ip(64, &bytes, 0, DecoderOptions::NONE);
    let first = decoder.decode();
    let second = decoder.decode();
    assert_eq!(first.mnemonic(), IcedMnem::Mfence, "first opcode should be mfence");
    assert_eq!(second.mnemonic(), IcedMnem::Mov, "second opcode should be mov");
}

#[test]
fn atomic_store_seqcst_w64_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};
    let bytes = atomic_store_recipe(IntWidth::W64, RecipeOrd::SeqCst);
    let mut decoder = Decoder::with_ip(64, &bytes, 0, DecoderOptions::NONE);
    let first = decoder.decode();
    let second = decoder.decode();
    assert_eq!(first.mnemonic(), IcedMnem::Mov, "first opcode should be mov");
    assert_eq!(second.mnemonic(), IcedMnem::Mfence, "second opcode should be mfence");
}
