//! tests/cross-build/tests/uefi-loader.rs
//!
//! Phase-2-m6-009 cross-build smoke test for the UEFI loader fixture.
//! Invokes tools/cross-build/tools/cross-build.sh and verifies exit code.

use std::process::Command;

fn workspace_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
#[ignore = "requires nasm + objdump (m1-013's tooling stack); enable locally with --ignored"]
fn uefi_loader_cross_build_succeeds() {
    let root = workspace_root();
    let script = root.join("tools/cross-build/tools/cross-build.sh");
    let fixture = root.join("tools/cross-build/fixtures/uefi_loader");

    let output = Command::new("bash")
        .arg(&script)
        .arg(&fixture)
        .current_dir(&root)
        .output()
        .expect("spawn cross-build.sh");

    if !output.status.success() {
        panic!(
            "cross-build.sh failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}

#[test]
fn cross_build_script_exists() {
    let script = workspace_root().join("tools/cross-build/tools/cross-build.sh");
    assert!(
        script.exists(),
        "cross-build.sh must exist: {}",
        script.display()
    );
}

#[test]
fn uefi_loader_fixture_files_present() {
    let fixture = workspace_root().join("tools/cross-build/fixtures/uefi_loader");
    for name in &["module.asm", "module.pdx", "module.expect-mnemonics.txt"] {
        let path = fixture.join(name);
        assert!(path.exists(), "fixture file missing: {}", path.display());
    }
}
