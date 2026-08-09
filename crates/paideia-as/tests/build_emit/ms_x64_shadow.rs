//! v0.21-001 (#1277, closes #1011): MS x64 ABI regression fixture.
//!
//! Named per the milestone AC. Verifies that a UEFI-shaped extern function
//! accepting 5 integer arguments compiles cleanly under `@abi("ms")` and
//! that the callee-side body reads arg[0] out of RCX (not RDI).
//!
//! - Callee-side (this test): arg[0] returned via `mov rax, rcx; ret`.
//! - Caller-side 32-byte shadow-space bump: covered by
//!   `two_let_with_ms_call_saves_rcx.rs` and `bridge_thunk.rs`.

use crate::common::elf;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn ms_x64_shadow_five_arg_uefi_shape_compiles() {
    let out = run_build(build_emit("ms_x64_shadow.pdx"));
    out.assert_ok();
    assert!(
        !out.stderr_contains("U1620"),
        "5-arg MS x64 UEFI-shaped fn must NOT emit U1620; stderr:\n{}",
        out.stderr
    );
    assert!(
        !out.stderr_contains("T0521"),
        "unused stack-passed arg[4] must NOT emit T0521; stderr:\n{}",
        out.stderr
    );

    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);
}

#[test]
fn ms_x64_shadow_five_arg_body_reads_rcx_not_rdi() {
    let out = run_build(build_emit("ms_x64_shadow.pdx"));
    out.assert_ok();
    let bytes = out.artifact_bytes();

    let func_bytes = elf::symbol_bytes(&bytes, "uefi_5arg")
        .expect("uefi_5arg symbol not found in ELF");

    // MS x64 identity from arg[0]: mov rax, rcx == 48 89 C8
    let has_ms_identity = func_bytes
        .windows(3)
        .any(|w| w[0] == 0x48 && w[1] == 0x89 && w[2] == 0xC8);
    assert!(
        has_ms_identity,
        "Expected MS x64 identity `mov rax, rcx` (48 89 C8) in uefi_5arg: {}",
        func_bytes
            .iter()
            .take(48)
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ")
    );

    // SysV identity `mov rax, rdi` (48 89 F8) must be absent — proves the
    // callee-side arg-reg mapping is genuinely ABI-aware, not accidental.
    let has_sysv_identity = func_bytes
        .windows(3)
        .any(|w| w[0] == 0x48 && w[1] == 0x89 && w[2] == 0xF8);
    assert!(
        !has_sysv_identity,
        "MS x64 lambda must NOT emit SysV `mov rax, rdi` (48 89 F8): {}",
        func_bytes
            .iter()
            .take(48)
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ")
    );
}
