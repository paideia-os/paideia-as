//! REX/EVEX prefix tightening pass regression tests.
//!
//! Phase-3-m3-004 shipped `EncodeTightPass` in `paideia-as-emitter-elf::opt`
//! with an `OptPass::apply` implementation that emits an O1506 would-fire
//! diagnostic through `OptDiagSink`. Real byte-level tightening happens
//! caller-side in the encoder (see `EncodeStats` + `can_shorten_add_to_32bit`
//! / `can_use_rel8` — `crates/paideia-as-encoder`); the pass entry point
//! keeps a would-fire marker so the catalog dispatcher composes uniformly
//! with the IR-side passes.
//!
//! Historical note: this file shipped with a single `#[ignore]`d placeholder
//! (m3-008 doc `19 active + 1 ignored`) because the harness `Cargo.toml` did
//! not depend on `paideia-as-emitter-elf`, so the emitter-side pass symbol
//! was not reachable. B7-005 (PAS-DEBT #1533) adds that dev-dependency and
//! reactivates real assertions modelled on `align_regression.rs` +
//! `unroll_regression.rs`. Once encode-tight flips from would-fire to
//! rewrote-N-sites (i.e. the pass reads `InstructionSideTable` and mutates
//! `EncodingHint` flags itself, mirroring `MacroFusionPass`), the message
//! assertion here must be updated in lock-step.

mod common;

use common::create_test_arena;
use paideia_as_emitter_elf::opt::EncodeTightPass;
use paideia_as_ir::opt::{OptDiagSink, OptPass};

/// Encode-tight advertises its canonical catalog name.
#[test]
fn encode_tight_pass_registered() {
    let pass = EncodeTightPass;
    assert_eq!(
        pass.name(),
        "encode-tight",
        "EncodeTightPass should have canonical name matching the m9-006 catalog entry"
    );
}

/// Encode-tight always emits its O1506 would-fire marker when dispatched,
/// even against an empty arena — the pass entry point is a would-fire stub;
/// real tightening is caller-side in the encoder (EncodeStats).
///
/// When the pass flips to a real IR-side rewrite (reading InstructionSideTable
/// and mutating EncodingHint flags), this test breaks intentionally and the
/// assertion must be updated to `"O1506 rewrote N sites"` shape.
#[test]
fn encode_tight_apply_emits_o1506_would_fire() {
    let (mut arena, root) = create_test_arena();

    let mut sink = OptDiagSink::new();
    let pass = EncodeTightPass;

    let changed = pass.apply(&mut arena, root, &mut sink);

    assert!(
        !changed,
        "EncodeTightPass::apply is a would-fire stub and must return false \
         (real byte tightening happens caller-side in the encoder, not here)"
    );
    assert_eq!(
        sink.diagnostics.len(),
        1,
        "EncodeTightPass should emit exactly one O1506 would-fire diagnostic per dispatch"
    );
    assert_eq!(sink.diagnostics[0].pass, "encode-tight");
    assert!(
        sink.diagnostics[0]
            .message
            .contains("O1506 (would-fire): REX/EVEX prefix tightening dispatched"),
        "diagnostic message drifted from the m3-004 pinned shape: {:?}",
        sink.diagnostics[0].message
    );
}
