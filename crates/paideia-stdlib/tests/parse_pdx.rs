//! Verify each .pdx file in pdx/ parses cleanly via paideia-as.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn paideia_as_bin() -> Option<PathBuf> {
    let release = workspace_root().join("target/release/paideia-as");
    let debug = workspace_root().join("target/debug/paideia-as");
    if release.exists() {
        Some(release)
    } else if debug.exists() {
        Some(debug)
    } else {
        None
    }
}

#[test]
fn paideia_stdlib_pdx_dir_exists() {
    assert!(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("pdx")
            .is_dir()
    );
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn alloc_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/alloc.pdx");
    let result = Command::new(bin)
        .args(["check", &pdx.to_string_lossy()])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore]
fn bump_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bump.pdx");
    let result = Command::new(bin)
        .args(["check", &pdx.to_string_lossy()])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn bump_new_creates_zero_offset_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bump_new_creates_zero_offset.pdx");
    let result = Command::new(bin)
        .args(["check", &pdx.to_string_lossy()])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn bump_alloc_advances_offset_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bump_alloc_advances_offset.pdx");
    let result = Command::new(bin)
        .args(["check", &pdx.to_string_lossy()])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn bump_alloc_respects_alignment_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bump_alloc_respects_alignment.pdx");
    let result = Command::new(bin)
        .args(["check", &pdx.to_string_lossy()])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn bump_reset_returns_offset_zero_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bump_reset_returns_offset_zero.pdx");
    let result = Command::new(bin)
        .args(["check", &pdx.to_string_lossy()])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn arena_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/arena.pdx");
    let result = Command::new(bin)
        .args(["check", &pdx.to_string_lossy()])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn arena_new_creates_arena_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/arena_new_creates_arena.pdx");
    let result = Command::new(bin)
        .args(["check", &pdx.to_string_lossy()])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn arena_alloc_returns_pointer_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/arena_alloc_returns_pointer.pdx");
    let result = Command::new(bin)
        .args(["check", &pdx.to_string_lossy()])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn arena_multi_region_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/arena_multi_region.pdx");
    let result = Command::new(bin)
        .args(["check", &pdx.to_string_lossy()])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn arena_reset_releases_all_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/arena_reset_releases_all.pdx");
    let result = Command::new(bin)
        .args(["check", &pdx.to_string_lossy()])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
