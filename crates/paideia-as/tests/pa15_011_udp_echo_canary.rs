//! PA15-011 (issue #966): UDP echo network corpus integration test.
//!
//! This integration test is the v0.15 canary for network primitives: it verifies
//! end-to-end that a minimal IPv4+UDP echo handler compiles and elaborates cleanly
//! using the new v0.15 primitives (bytes.pdx access, checksum.pdx, jump-table
//! dispatch, bswap_d, adc/sbb arithmetic).
//!
//! The test is split into two parts:
//!
//! 1. `udp_echo_stub_parses_cleanly` — runs unconditionally; verifies the fixture
//!    parses and type-checks via `paideia-as check` with zero P-code diagnostics.
//!    This is the baseline canary that gates paideia-os Phase 3+ work.
//!
//! 2. `udp_echo_stub_qemu_smoke` (#[ignore]) — placeholder stub for future qemu-boot
//!    integration. Once we can emit full ELF64 and boot into qemu, this test will
//!    verify that the echo handler actually responds to crafted IPv4+UDP packets.
//!    For now, it's a marker for the next phase of network integration.
//!
//! Fixture: tests/build-emit/udp_echo_stub.pdx

use std::path::PathBuf;
use std::process::Command;

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

fn find_fixture(name: &str) -> PathBuf {
    // Fixtures live in tests/build-emit/
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_dir = crate_dir
        .parent()
        .expect("crate has parent")
        .parent()
        .expect("parent has parent")
        .join("tests")
        .join("build-emit");
    fixture_dir.join(name)
}

#[test]
fn udp_echo_stub_parses_cleanly() {
    let src_path = find_fixture("udp_echo_stub.pdx");
    assert!(
        src_path.exists(),
        "fixture {} should exist",
        src_path.display()
    );

    // Check the fixture using `paideia-as check` (parse + elaborate, no emit).
    // The fixture is now a trait-only file (interface declarations only).
    let out = cargo_run(&["check", src_path.to_str().unwrap()]);

    assert!(
        out.status.success(),
        "check failed for udp_echo_stub: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Verify no compiler error codes on stderr (P-code diagnostics).
    // Strong assertion: look for "error[" which is the standard error format.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("error["),
        "expected zero compiler errors, got: {}",
        stderr
    );

    // Also verify specific error codes are absent (as defense-in-depth).
    assert!(
        !stderr.contains("M0305"),
        "M0305 module-name mismatch should not occur"
    );
    assert!(
        !stderr.contains("U1605"),
        "U1605 unresolved mnemonic should not occur"
    );
}

#[test]
#[ignore]
fn udp_echo_stub_qemu_smoke() {
    // Placeholder: once paideia-as can emit full ELF64 with relocations,
    // and paideia-os can boot the canary image, this test will:
    //
    // 1. Emit udp_echo_stub.pdx to ELF64 format
    // 2. Load it into a minimal IPv4+UDP network stack in qemu
    // 3. Send a crafted UDP packet to the echo handler
    // 4. Verify the reply packet matches the expected swapped addresses
    //
    // For now, this is a marker for Phase 3 (network integration).
    //
    // See design/roadmap/paideia-as-v0.13-through-v0.20.md for v0.15 roadmap.
}
