//! PA7C-m6-001: PaideiaOS boot orchestration v2 integration smoke test.
//!
//! This test verifies that checkpoint-1 unquarantined files compose with the
//! current paideia-as elaborator/encoder to produce a complete PaideiaOS kernel.elf.
//! It drives the full build pipeline: paideia-as parsing/elaboration/emission + ld linking.
//!
//! Acceptance criteria:
//! - tools/build.sh exits 0 (Linux-only gate)
//! - build/kernel.elf exists and is > 1024 bytes
//! - Build output contains at least 5 successfully compiled .pdx files
//! - Reported fixture count matches assertion

use std::path::PathBuf;
use std::process::Command;

#[test]
fn boot_orchestration_v2_smoke() {
    // Linux-only gate: skip on non-Linux platforms
    if !cfg!(target_os = "linux") {
        eprintln!("boot_orchestration_v2_smoke: skipped (Linux-only)");
        return;
    }

    // Discover PaideiaOS directory.
    // This test only runs when PaideiaOS is the parent repo (paideia-as is a submodule at tools/paideia-as).
    let paideia_as_manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Walk: crates/paideia-as → tools/paideia-as → PaideiaOS
    let mut paideia_os_path = paideia_as_manifest.clone();

    // Pop 2 levels to go from crates/paideia-as up to paideia-as (submodule root)
    if !paideia_os_path.pop() || !paideia_os_path.pop() {
        eprintln!("boot_orchestration_v2_smoke: skipped (cannot navigate directory structure)");
        return;
    }

    // Now we're at tools/paideia-as; pop twice more to get to PaideiaOS root
    if !paideia_os_path.pop() || !paideia_os_path.pop() {
        eprintln!("boot_orchestration_v2_smoke: skipped (cannot navigate to PaideiaOS root)");
        return;
    }

    let build_sh = paideia_os_path.join("tools").join("build.sh");
    if !build_sh.exists() {
        eprintln!(
            "boot_orchestration_v2_smoke: skipped (tools/build.sh not found at {})",
            build_sh.display()
        );
        return;
    }

    eprintln!(
        "boot_orchestration_v2_smoke: running PaideiaOS build from {}",
        paideia_os_path.display()
    );

    // Run tools/build.sh
    let mut build_cmd = Command::new("bash");
    build_cmd
        .arg("tools/build.sh")
        .current_dir(&paideia_os_path)
        .env("NO_COLOR", "1");

    eprintln!("boot_orchestration_v2_smoke: running tools/build.sh");
    let output = build_cmd.output().expect("failed to run tools/build.sh");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    eprintln!(
        "boot_orchestration_v2_smoke: build exit code = {}",
        output.status.code().unwrap_or(-1)
    );

    // Assert build succeeded
    assert!(
        output.status.success(),
        "tools/build.sh failed:\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    // Verify kernel.elf exists and has size > 1024 bytes
    let kernel_elf = paideia_os_path.join("build").join("kernel.elf");
    assert!(
        kernel_elf.exists(),
        "build/kernel.elf does not exist at {}",
        kernel_elf.display()
    );

    let elf_size = std::fs::metadata(&kernel_elf)
        .expect("could not stat kernel.elf")
        .len();
    assert!(
        elf_size > 1024,
        "kernel.elf is too small: {} bytes (expected > 1024)",
        elf_size
    );

    eprintln!(
        "boot_orchestration_v2_smoke: kernel.elf = {} bytes",
        elf_size
    );

    // Count .pdx files processed from build output
    let mut pdx_count = 0;
    for line in stdout.lines().chain(stderr.lines()) {
        if line.contains("paideia-as") && line.contains(".pdx") && line.contains("->") {
            pdx_count += 1;
            eprintln!("  processed: {}", line.trim());
        }
    }

    assert!(
        pdx_count >= 5,
        "expected at least 5 .pdx files compiled, found {}",
        pdx_count
    );

    eprintln!(
        "boot_orchestration_v2_smoke: OK (kernel.elf={} bytes, {} pdx files)",
        elf_size, pdx_count
    );
}
