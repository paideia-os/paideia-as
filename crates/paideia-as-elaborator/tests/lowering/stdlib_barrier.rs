//! PA-v0.21-004 (#1280) round-trip: calls to BarrierOps::barrier_full /
//! barrier_store / barrier_load / cpu_pause elaborate to the corresponding
//! one-instruction recipes (MFENCE / SFENCE / LFENCE / PAUSE) that the
//! encoder emits as the byte-exact sequences validated in
//! paideia-as-encoder/tests/memory_ops/mem_fences.rs and pause.rs.
//!
//! Coverage:
//!   barrier_full  → Mnemonic::Mfence, no operands  (encoder → 0F AE F0)
//!   barrier_store → Mnemonic::Sfence, no operands  (encoder → 0F AE F8)
//!   barrier_load  → Mnemonic::Lfence, no operands  (encoder → 0F AE E8)
//!   cpu_pause     → Mnemonic::Pause,  no operands  (encoder → F3 90)
//!
//! Unknown-method-under-BarrierOps returns None so the caller falls through
//! to normal call emission (guards against typos like "barrier_all" silently
//! being ignored — we want None, not Some(Err), so the normal call path
//! still tries).

use paideia_as_ir::{InstrMode, IrArena, IrNodeId};
use paideia_as_elaborator::stdlib_lowering::ArgConvention;

fn expect_nullary_recipe(
    method: &'static str,
    expected_mnemonic: paideia_as_ir::instruction::Mnemonic,
) {
    let arena = IrArena::new();
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "BarrierOps",
        method,
        InstrMode::Mode64,
        &[],
        &arena,
    );
    let recipe = result
        .unwrap_or_else(|| panic!("BarrierOps::{} must be registered", method))
        .unwrap_or_else(|e| panic!("BarrierOps::{} nullary lowering must succeed, got {:?}", method, e));

    assert_eq!(
        recipe.arg_convention,
        ArgConvention::Literal,
        "nullary barrier recipes take no args → Literal convention"
    );
    assert!(recipe.labels.is_empty(), "nullary barrier recipes declare no labels");
    assert_eq!(
        recipe.instructions.len(),
        1,
        "BarrierOps::{} should lower to exactly one instruction",
        method
    );

    let inst = &recipe.instructions[0];
    assert_eq!(inst.mnemonic, expected_mnemonic, "wrong mnemonic for {}", method);
    assert!(inst.operands.is_empty(), "barrier mnemonic takes no operands");
    assert_eq!(inst.mode, InstrMode::Mode64);
}

#[test]
fn barrier_full_lowers_to_mfence() {
    expect_nullary_recipe("barrier_full", paideia_as_ir::instruction::Mnemonic::Mfence);
}

#[test]
fn barrier_store_lowers_to_sfence() {
    expect_nullary_recipe("barrier_store", paideia_as_ir::instruction::Mnemonic::Sfence);
}

#[test]
fn barrier_load_lowers_to_lfence() {
    expect_nullary_recipe("barrier_load", paideia_as_ir::instruction::Mnemonic::Lfence);
}

#[test]
fn cpu_pause_lowers_to_pause() {
    expect_nullary_recipe("cpu_pause", paideia_as_ir::instruction::Mnemonic::Pause);
}

#[test]
fn barrier_recipes_ignore_extra_arg_ids_without_failing() {
    // Nullary recipes should tolerate a caller passing arg_ids they don't
    // consume — the elaborator's Literal convention only checks arity for
    // recipes that actually extract literals. This test guards against a
    // regression where we start rejecting args on nullary recipes.
    let arena = IrArena::new();
    let bogus_id = IrNodeId::new(1).expect("valid node id");
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "BarrierOps",
        "barrier_full",
        InstrMode::Mode64,
        &[bogus_id],
        &arena,
    );
    assert!(result.unwrap().is_ok(), "nullary recipe tolerates extra args");
}

#[test]
fn unknown_barrier_method_returns_none() {
    let arena = IrArena::new();
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "BarrierOps",
        "barrier_all", // typo — must NOT match any real method
        InstrMode::Mode64,
        &[],
        &arena,
    );
    assert!(
        result.is_none(),
        "unknown BarrierOps method returns None so the caller falls through to normal call emission"
    );
}
