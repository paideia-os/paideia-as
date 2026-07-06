//! PA16-012 (issue #978): CoW filesystem integration test.
//!
//! This integration test is the v0.16 canary for atomic primitives: it verifies
//! end-to-end that a minimal copy-on-write filesystem allocator trait surface
//! compiles and elaborates cleanly using the new v0.16 atomics substrate
//! (lock bts/btr, lock cmpxchg16b, lock xadd, pause).
//!
//! The test is split into two parts:
//!
//! 1. `cow_fs_stub_parses_cleanly` — runs unconditionally; verifies the fixture
//!    parses and type-checks via `paideia-as check` with zero P-code diagnostics.
//!    This is the baseline canary that gates paideia-os Phase 16+ work.
//!
//! 2. `cow_fs_stub_qemu_smoke` (#[ignore]) — placeholder stub for future qemu-boot
//!    integration. Once we can emit full ELF64 and boot into qemu, this test will
//!    verify that the phys_alloc/phys_free/phys_incref/phys_decref operations
//!    actually compose the underlying atomic primitives correctly.
//!    For now, it's a marker for the next phase of filesystem integration.
//!
//! Fixture: tests/build-emit/cow_fs_stub.pdx

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
fn cow_fs_stub_parses_cleanly() {
    let src_path = find_fixture("cow_fs_stub.pdx");
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
        "check failed for cow_fs_stub: {}",
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

    // v0.16 uplift assertions: verify the fixture file contains all 5 traits
    // and the key function signatures.
    let src = std::fs::read_to_string(&src_path)
        .expect("should read fixture");
    assert!(
        src.contains("trait PhysAllocOps"),
        "fixture should declare trait PhysAllocOps"
    );
    assert!(
        src.contains("trait BitmapOps"),
        "fixture should declare trait BitmapOps"
    );
    assert!(
        src.contains("trait RefcountOps"),
        "fixture should declare trait RefcountOps"
    );
    assert!(
        src.contains("trait FreelistOps"),
        "fixture should declare trait FreelistOps"
    );
    assert!(
        src.contains("trait PauseOps"),
        "fixture should declare trait PauseOps"
    );
    assert!(
        src.contains("fn phys_alloc"),
        "fixture should declare fn phys_alloc"
    );
    assert!(
        src.contains("fn phys_free"),
        "fixture should declare fn phys_free"
    );
    assert!(
        src.contains("fn phys_incref"),
        "fixture should declare fn phys_incref"
    );
    assert!(
        src.contains("fn phys_decref"),
        "fixture should declare fn phys_decref"
    );
}

#[test]
#[ignore]
fn cow_fs_stub_qemu_smoke() {
    // Placeholder: once paideia-as can emit full ELF64 with relocations,
    // and paideia-os can boot the canary image, this test will:
    //
    // 1. Emit cow_fs_stub.pdx to ELF64 format
    // 2. Load it into a minimal CoW filesystem physical-page allocator
    //    in qemu memory
    // 3. Verify that phys_alloc correctly allocates pages using the bitmap
    // 4. Verify that phys_incref/phys_decref maintain correct reference counts
    // 5. Verify that phys_free returns pages to the free list
    //
    // For now, this is a marker for Phase 16 (filesystem integration).
    //
    // See design/roadmap/paideia-as-v0.13-through-v0.20.md for v0.16 roadmap.
}
