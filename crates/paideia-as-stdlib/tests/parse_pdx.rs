//! Verify each .pdx file in pdx/ parses cleanly via paideia-as.

use std::path::PathBuf;
use std::process::Command;
use regex::Regex;
use std::collections::HashMap;

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
fn paideia_as_stdlib_pdx_dir_exists() {
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

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn box_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/box.pdx");
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
fn box_new_returns_box_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/box_new_returns_box.pdx");
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
fn box_deref_returns_inner_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/box_deref_returns_inner.pdx");
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
fn box_linear_discipline_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/box_linear_discipline.pdx");
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
fn box_drop_releases_pointer_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/box_drop_releases_pointer.pdx");
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
fn string_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/string.pdx");
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
fn string_new_creates_empty_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/string_new_creates_empty.pdx");
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
fn string_with_capacity_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/string_with_capacity.pdx");
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
fn string_push_appends_byte_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/string_push_appends_byte.pdx");
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
fn string_from_str_coerces_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/string_from_str_coerces.pdx");
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
fn option_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/option.pdx");
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
fn result_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/result.pdx");
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
fn option_some_construct_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/option_some_construct.pdx");
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
fn option_none_construct_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/option_none_construct.pdx");
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
fn option_unwrap_some_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/option_unwrap_some.pdx");
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
fn option_unwrap_or_none_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/option_unwrap_or_none.pdx");
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
fn option_map_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/option_map.pdx");
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
fn option_and_then_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/option_and_then.pdx");
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
fn option_ok_or_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/option_ok_or.pdx");
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
fn result_ok_construct_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/result_ok_construct.pdx");
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
fn result_err_construct_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/result_err_construct.pdx");
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
fn result_unwrap_ok_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/result_unwrap_ok.pdx");
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
fn result_map_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/result_map.pdx");
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
fn result_map_err_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/result_map_err.pdx");
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
fn option_u64_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/option_u64.pdx");
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
fn option_match_some_extract_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/option_match_some_extract.pdx");
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
fn option_match_none_default_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/option_match_none_default.pdx");
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
fn option_match_wildcard_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/option_match_wildcard.pdx");
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
fn option_match_guard_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/option_match_guard.pdx");
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
fn option_match_or_pattern_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/option_match_or_pattern.pdx");
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
fn option_fn_returns_option_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/option_fn_returns_option.pdx");
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
fn result_u64_u64_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/result_u64_u64.pdx");
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
fn result_match_ok_extract_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/result_match_ok_extract.pdx");
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
fn result_match_err_extract_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/result_match_err_extract.pdx");
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
fn string_ops_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/string_ops.pdx");
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
fn string_new_op_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/string_new_op.pdx");
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
fn string_from_str_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/string_from_str.pdx");
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
fn string_push_str_appends_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/string_push_str_appends.pdx");
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
fn string_push_char_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/string_push_char.pdx");
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
fn string_len_op_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/string_len_op.pdx");
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
fn string_as_str_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/string_as_str.pdx");
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
fn str_chars_yields_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/str_chars_yields.pdx");
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
fn str_bytes_yields_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/str_bytes_yields.pdx");
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
fn str_split_yields_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/str_split_yields.pdx");
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
fn str_starts_with_true_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/str_starts_with_true.pdx");
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
fn str_starts_with_false_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/str_starts_with_false.pdx");
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
fn string_str_round_trip_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/string_str_round_trip.pdx");
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

// v0.18 #998: Str type and field accessors (downgraded scope, 7 fixtures)
#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn str_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/str.pdx");
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
fn str_len_ptr_form_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/str_len_ptr_form.pdx");
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
fn str_ptr_ptr_form_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/str_ptr_ptr_form.pdx");
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
fn str_eq_shape_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/str_eq_shape.pdx");
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
fn str_hash_shape_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/str_hash_shape.pdx");
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
fn str_substring_shape_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/str_substring_shape.pdx");
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
fn str_split_at_shape_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/str_split_at_shape.pdx");
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
fn io_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/io.pdx");
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
fn io_stdin_open_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/io_stdin_open.pdx");
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
fn io_stdout_write_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/io_stdout_write.pdx");
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
fn io_stderr_write_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/io_stderr_write.pdx");
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
fn io_println_macro_stub_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/io_println_macro_stub.pdx");
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
fn mmio_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/mmio.pdx");
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
fn mmio_read_u32_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/mmio_read_u32.pdx");
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
fn mmio_write_u32_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/mmio_write_u32.pdx");
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
fn bytes_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bytes.pdx");
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
fn bytes_get_u8_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bytes_get_u8.pdx");
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
fn bytes_put_u8_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bytes_put_u8.pdx");
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
fn bytes_get_u16_be_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bytes_get_u16_be.pdx");
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
fn bytes_get_u16_le_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bytes_get_u16_le.pdx");
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
fn bytes_put_u16_be_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bytes_put_u16_be.pdx");
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
fn bytes_put_u16_le_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bytes_put_u16_le.pdx");
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
fn bytes_get_u32_be_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bytes_get_u32_be.pdx");
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
fn bytes_get_u32_le_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bytes_get_u32_le.pdx");
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
fn bytes_put_u32_be_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bytes_put_u32_be.pdx");
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
fn bytes_put_u32_le_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bytes_put_u32_le.pdx");
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
fn bytes_get_u64_be_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bytes_get_u64_be.pdx");
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
fn bytes_get_u64_le_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bytes_get_u64_le.pdx");
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
fn bytes_put_u64_be_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bytes_put_u64_be.pdx");
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
fn bytes_put_u64_le_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bytes_put_u64_le.pdx");
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
fn bytes_ipv4_version_ihl_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bytes_ipv4_version_ihl.pdx");
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
fn bytes_ipv4_total_length_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bytes_ipv4_total_length.pdx");
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
fn bytes_ipv4_identification_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bytes_ipv4_identification.pdx");
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
fn bytes_ipv4_ttl_proto_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bytes_ipv4_ttl_proto.pdx");
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
fn bytes_ipv4_src_addr_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bytes_ipv4_src_addr.pdx");
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
fn bytes_ipv4_dst_addr_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bytes_ipv4_dst_addr.pdx");
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
fn checksum_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/checksum.pdx");
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
fn checksum_zero_length_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/checksum_zero_length.pdx");
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
fn checksum_single_16bit_word_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/checksum_single_16bit_word.pdx");
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
fn checksum_two_words_no_carry_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/checksum_two_words_no_carry.pdx");
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
fn checksum_two_words_with_carry_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/checksum_two_words_with_carry.pdx");
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
fn checksum_rfc1071_example_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/checksum_rfc1071_example.pdx");
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
fn checksum_odd_length_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/checksum_odd_length.pdx");
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
fn checksum_all_ones_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/checksum_all_ones.pdx");
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
fn checksum_20_byte_header_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/checksum_20_byte_header.pdx");
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
fn percpu_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/percpu.pdx");
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
fn pause_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/pause.pdx");
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
fn percpu_inc_single_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/percpu_inc_single.pdx");
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
fn percpu_add_single_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/percpu_add_single.pdx");
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
fn percpu_inc_zero_offset_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/percpu_inc_zero_offset.pdx");
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
fn percpu_add_various_offsets_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/percpu_add_various_offsets.pdx");
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
fn refcount_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/refcount.pdx");
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
fn refcount_incr_single_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/refcount_incr_single.pdx");
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
fn refcount_decr_single_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/refcount_decr_single.pdx");
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
fn refcount_decr_and_test_single_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/refcount_decr_and_test_single.pdx");
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
fn refcount_incr_decr_pair_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/refcount_incr_decr_pair.pdx");
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
fn refcount_atomic_lifecycle_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/refcount_atomic_lifecycle.pdx");
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
fn bitmap_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bitmap.pdx");
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
fn bitmap_get_single_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bitmap_get_single.pdx");
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
fn bitmap_word_count_single_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bitmap_word_count_single.pdx");
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
fn bitmap_set_single_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bitmap_set_single.pdx");
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
fn bitmap_first_free_single_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bitmap_first_free_single.pdx");
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
fn bitmap_claim_first_free_single_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bitmap_claim_first_free_single.pdx");
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
fn bitmap_atomic_ops_trio_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bitmap_atomic_ops_trio.pdx");
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
fn bitmap_lifecycle_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bitmap_lifecycle.pdx");
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
fn freelist_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/freelist.pdx");
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
fn freelist_push_pop_pair_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/freelist_push_pop_pair.pdx");
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
fn freelist_empty_single_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/freelist_empty_single.pdx");
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
fn freelist_stress_shape_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/freelist_stress_shape.pdx");
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

// PA-v0.21-012 (#1288): BitfieldOps trait + fixture parse-cleanliness.
#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn bitfield_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bitfield.pdx");
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
fn bitfield_get_bits_single_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bitfield_get_bits_single.pdx");
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
fn bitfield_set_bits_single_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bitfield_set_bits_single.pdx");
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
fn bitfield_acpi_sdt_length_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bitfield_acpi_sdt_length.pdx");
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
fn bulkmem_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/bulkmem.pdx");
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
fn uefi_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/uefi.pdx");
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
fn uefi_header_template_bytes_valid_pe32plus() {
    let template_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("pdx/uefi_header_template.bin");
    let bytes = std::fs::read(&template_path)
        .expect("Failed to read uefi_header_template.bin");

    // Check file size
    assert_eq!(bytes.len(), 512, "Template must be exactly 512 bytes");

    // DOS magic at bytes 0-1 should be "MZ"
    assert_eq!(&bytes[0..2], b"MZ", "DOS magic should be 'MZ'");

    // e_lfanew at bytes 60-63 should point to 64 (0x40)
    let e_lfanew = u32::from_le_bytes([bytes[60], bytes[61], bytes[62], bytes[63]]);
    assert_eq!(e_lfanew, 64, "e_lfanew should point to offset 64 for PE signature");

    // PE signature at offset 64 should be "PE\0\0"
    assert_eq!(&bytes[64..68], b"PE\0\0", "PE signature should be present at offset 64");

    // COFF machine at offset 68-69 should be 0x8664 (AMD64)
    let machine = u16::from_le_bytes([bytes[68], bytes[69]]);
    assert_eq!(machine, 0x8664, "COFF machine should be 0x8664 (AMD64)");

    // PE32+ magic at offset 88 should be 0x020b
    let pe_magic = u16::from_le_bytes([bytes[88], bytes[89]]);
    assert_eq!(pe_magic, 0x020b, "PE32+ magic should be 0x020b");
}

#[test]
fn uefi_header_template_defaults_reproducible() {
    let template_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("pdx/uefi_header_template.bin");
    let bytes = std::fs::read(&template_path)
        .expect("Failed to read uefi_header_template.bin");

    // TimeDateStamp at bytes 72-75 should be 0
    let timestamp = u32::from_le_bytes([bytes[72], bytes[73], bytes[74], bytes[75]]);
    assert_eq!(timestamp, 0, "TimeDateStamp should be 0");

    // CheckSum at offset 152 (0x98) should be 0
    let checksum = u32::from_le_bytes([bytes[152], bytes[153], bytes[154], bytes[155]]);
    assert_eq!(checksum, 0, "CheckSum should be 0");

    // Subsystem at offset 156 (0x9C) should be 10 (EFI_APPLICATION)
    let subsystem = u16::from_le_bytes([bytes[156], bytes[157]]);
    assert_eq!(subsystem, 10, "Subsystem should be 10 (EFI_APPLICATION)");

    // ImageBase at offset 112-119 (0x70-0x77) should be 0x140000000
    let image_base = u64::from_le_bytes([
        bytes[0x70], bytes[0x71], bytes[0x72], bytes[0x73],
        bytes[0x74], bytes[0x75], bytes[0x76], bytes[0x77],
    ]);
    assert_eq!(image_base, 0x140000000, "ImageBase should be 0x140000000");

    // SectionAlignment at offset 120 (0x78) should be 0x1000
    let section_align = u32::from_le_bytes([bytes[0x78], bytes[0x79], bytes[0x7A], bytes[0x7B]]);
    assert_eq!(section_align, 0x1000, "SectionAlignment should be 0x1000");

    // FileAlignment at offset 124 (0x7C) should be 0x200
    let file_align = u32::from_le_bytes([bytes[0x7C], bytes[0x7D], bytes[0x7E], bytes[0x7F]]);
    assert_eq!(file_align, 0x200, "FileAlignment should be 0x200");
}

fn parse_off_constants(src: &str) -> HashMap<String, u64> {
    let mut constants = HashMap::new();
    let re = Regex::new(r"(?m)^\s*pub let\s+(OFF_\w+)\s*:\s*u64\s*=\s*0x([0-9A-Fa-f_]+)\b")
        .expect("regex should compile");

    for cap in re.captures_iter(src) {
        if let (Some(name_match), Some(value_match)) = (cap.get(1), cap.get(2)) {
            let name = name_match.as_str().to_string();
            let hex_str = value_match.as_str().replace('_', "");
            if let Ok(value) = u64::from_str_radix(&hex_str, 16) {
                constants.insert(name, value);
            }
        }
    }
    constants
}

#[test]
fn uefi_pdx_constants_match_template_bytes() {
    let pdx_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/uefi.pdx");
    let template_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("pdx/uefi_header_template.bin");

    // Read sources of truth
    let pdx_src = std::fs::read_to_string(&pdx_path)
        .expect("Failed to read uefi.pdx");
    let bytes = std::fs::read(&template_path)
        .expect("Failed to read uefi_header_template.bin");

    // Verify template is exactly 512 bytes
    assert_eq!(
        bytes.len(),
        512,
        "template blob is {} bytes, expected 512",
        bytes.len()
    );

    // Extract all OFF_* constants from uefi.pdx
    let extracted = parse_off_constants(&pdx_src);

    // Expected table: (name, width in bytes, expected_value)
    let expected_table: Vec<(&str, usize, u64)> = vec![
        ("OFF_COFF_NUM_SECTIONS", 2, 0),
        ("OFF_COFF_TIMESTAMP", 4, 0),
        ("OFF_OPT_SIZE_OF_CODE", 4, 0),
        ("OFF_OPT_ADDRESS_OF_ENTRY", 4, 0),
        ("OFF_OPT_SIZE_OF_IMAGE", 4, 0),
        ("OFF_OPT_SIZE_OF_HEADERS", 4, 0),
        ("OFF_OPT_CHECKSUM", 4, 0),
        ("OFF_OPT_SUBSYSTEM", 2, 10),
        ("OFF_SECTION_TABLE_START", 0, 0), // 0 width means in-bounds only, no value check
    ];

    // Forward consistency: each expected constant must exist and pass checks
    for (name, width, expected_value) in &expected_table {
        let offset = extracted.get(*name)
            .unwrap_or_else(|| panic!("uefi.pdx missing constant {}", name));

        // Check bounds
        let end = offset + *width as u64;
        assert!(
            end <= 512,
            "{} at 0x{:x} + {} exceeds 512-byte template",
            name, offset, width
        );

        // Check value (skip if width is 0, which means in-bounds only)
        if *width > 0 {
            let actual = match width {
                2 => u16::from_le_bytes([
                    bytes[*offset as usize],
                    bytes[(*offset + 1) as usize],
                ]) as u64,
                4 => u32::from_le_bytes([
                    bytes[*offset as usize],
                    bytes[(*offset + 1) as usize],
                    bytes[(*offset + 2) as usize],
                    bytes[(*offset + 3) as usize],
                ]) as u64,
                _ => panic!("unsupported width {}", width),
            };

            assert_eq!(
                actual, *expected_value,
                "{} at 0x{:x} reads {} in template, expected {}",
                name, offset, actual, expected_value
            );
        }
    }

    // Reverse consistency: every OFF_* in the extracted map must be in the expected table
    for (name, _) in &extracted {
        let is_expected = expected_table.iter().any(|(expected_name, _, _)| {
            *expected_name == name
        });
        assert!(
            is_expected,
            "uefi.pdx defines {} but it is not in the expected test table",
            name
        );
    }
}

// v0.31 M1-003 (#1385): CICP-tagged image-encoding helpers
#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn cicp_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/cicp.pdx");
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
fn cicp_pdx_source_declares_five_named_tuples() {
    // Source-of-truth check: the .pdx surface must document the same five
    // named accessors the Rust hook implements (BT.709 / sRGB / Display-P3
    // / BT.2020 PQ / BT.2020 HLG). If either file drifts, we want to know
    // without waiting on the paideia-as binary to be built.
    let pdx_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/cicp.pdx");
    let src = std::fs::read_to_string(&pdx_path)
        .expect("pdx/cicp.pdx missing — v0.31 M1-003 (#1385) surface not present");

    for tuple in [
        "cicp_bt709",
        "cicp_srgb",
        "cicp_display_p3",
        "cicp_bt2020_pq",
        "cicp_bt2020_hlg",
    ] {
        assert!(
            src.contains(tuple),
            "pdx/cicp.pdx does not declare `{}` — surface out of sync with src/cicp.rs",
            tuple
        );
    }

    // Struct name + full four-field surface must all be present.
    assert!(src.contains("struct CicpTag"), "pdx/cicp.pdx missing `struct CicpTag`");
    for field in ["colour_primaries", "transfer", "matrix", "full_range"] {
        assert!(
            src.contains(field),
            "pdx/cicp.pdx does not name CicpTag field `{}`",
            field
        );
    }
}

// =========================================================================
// R220.M4 (paideia-as #1418): Str::eq — byte-equal predicate.
// Closes #998b. Blocks semantic-shell R222 command dispatch and R228 tab
// completion. Parse-only fixtures at this landing; end-to-end lowering is
// a follow-on integration when the debugger cross-verifies the emitted
// binary against each fixture's fingerprint tag.
// =========================================================================

fn check_pdx_parses(rel_path: &str) {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel_path);
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

// Real module implementations (str_eq.pdx, str_hash.pdx) — the surface
// that shell R222/R228 links against.

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m4_str_eq_module_parses_cleanly() {
    check_pdx_parses("pdx/str_eq.pdx");
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m5_str_hash_module_parses_cleanly() {
    check_pdx_parses("pdx/str_hash.pdx");
}

// R220.M4 — 15 Str::eq unit-test fixtures.

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m4_str_eq_identical_bytes_parses_cleanly() {
    check_pdx_parses("pdx/str_eq_identical_bytes.pdx");
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m4_str_eq_distinct_bytes_parses_cleanly() {
    check_pdx_parses("pdx/str_eq_distinct_bytes.pdx");
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m4_str_eq_empty_both_parses_cleanly() {
    check_pdx_parses("pdx/str_eq_empty_both.pdx");
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m4_str_eq_empty_vs_nonempty_parses_cleanly() {
    check_pdx_parses("pdx/str_eq_empty_vs_nonempty.pdx");
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m4_str_eq_ascii_case_diff_parses_cleanly() {
    check_pdx_parses("pdx/str_eq_ascii_case_diff.pdx");
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m4_str_eq_4byte_match_parses_cleanly() {
    check_pdx_parses("pdx/str_eq_4byte_match.pdx");
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m4_str_eq_32byte_match_parses_cleanly() {
    check_pdx_parses("pdx/str_eq_32byte_match.pdx");
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m4_str_eq_1024byte_match_parses_cleanly() {
    check_pdx_parses("pdx/str_eq_1024byte_match.pdx");
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m4_str_eq_len_mismatch_parses_cleanly() {
    check_pdx_parses("pdx/str_eq_len_mismatch.pdx");
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m4_str_eq_first_byte_diff_parses_cleanly() {
    check_pdx_parses("pdx/str_eq_first_byte_diff.pdx");
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m4_str_eq_last_byte_diff_parses_cleanly() {
    check_pdx_parses("pdx/str_eq_last_byte_diff.pdx");
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m4_str_eq_middle_byte_diff_parses_cleanly() {
    check_pdx_parses("pdx/str_eq_middle_byte_diff.pdx");
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m4_str_eq_high_bit_bytes_parses_cleanly() {
    check_pdx_parses("pdx/str_eq_high_bit_bytes.pdx");
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m4_str_eq_embedded_nul_parses_cleanly() {
    check_pdx_parses("pdx/str_eq_embedded_nul.pdx");
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m4_str_eq_prefix_match_shorter_parses_cleanly() {
    check_pdx_parses("pdx/str_eq_prefix_match_shorter.pdx");
}

// R220.M5 — 5 Str::hash vector fixtures.

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m5_str_hash_empty_vector_parses_cleanly() {
    check_pdx_parses("pdx/str_hash_empty_vector.pdx");
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m5_str_hash_a_vector_parses_cleanly() {
    check_pdx_parses("pdx/str_hash_a_vector.pdx");
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m5_str_hash_ab_vector_parses_cleanly() {
    check_pdx_parses("pdx/str_hash_ab_vector.pdx");
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m5_str_hash_abc_vector_parses_cleanly() {
    check_pdx_parses("pdx/str_hash_abc_vector.pdx");
}

#[test]
#[ignore = "needs paideia-as built; run with --ignored after cargo build --release -p paideia-as"]
fn r220m5_str_hash_foobar_vector_parses_cleanly() {
    check_pdx_parses("pdx/str_hash_foobar_vector.pdx");
}

// -----------------------------------------------------------------------
// R220.M5 — algorithm cross-check tests (Rust-side authoritative).
//
// These tests run in the default `cargo test` invocation (no `--ignored`);
// they verify the FNV-1a-64 algorithm and constants documented in
// design/terminal/schema-registry.md §3 independently of paideia-as
// codegen. Purpose: (a) catch drift in the schema-registry constants,
// (b) provide authoritative expected values for the paideia-as .pdx
// fixtures once str_hash.pdx lowers end-to-end.
//
// If any of the `expected` constants baked into str_hash_*_vector.pdx
// disagrees with the value these tests compute, the corresponding
// `*_matches_spec` assertion prints both values; update the .pdx to
// match the printed hex.
// -----------------------------------------------------------------------

const FNV1A_64_OFFSET_BASIS: u64 = 0xCBF29CE484222325;
const FNV1A_64_PRIME: u64 = 0x100000001B3;

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut h = FNV1A_64_OFFSET_BASIS;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(FNV1A_64_PRIME);
    }
    h
}

#[test]
fn r220m5_fnv1a_64_constants_match_schema_registry_doc() {
    // schema-registry.md §3:
    //   offset basis: 0xCBF29CE484222325
    //   prime:        0x00000100000001B3
    assert_eq!(FNV1A_64_OFFSET_BASIS, 0xCBF29CE484222325_u64);
    assert_eq!(FNV1A_64_PRIME, 0x0000_0100_0000_01B3_u64);
}

#[test]
fn r220m5_hash_empty_matches_offset_basis() {
    // hash("") is the offset basis, by construction.
    assert_eq!(fnv1a_64(b""), FNV1A_64_OFFSET_BASIS);
}

/// Parse the `pub let expected : u64 = 0x<hex>` line out of a vector
/// fixture and return the u64 value. Used by the "matches_spec" tests
/// below so a drifted constant in a fixture is caught at test time.
fn read_expected_u64(fixture_rel: &str) -> u64 {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fixture_rel);
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("fixture {} not present", fixture_rel));
    let re = Regex::new(r"pub let expected\s*:\s*u64\s*=\s*0x([0-9A-Fa-f_]+)").unwrap();
    let caps = re
        .captures(&src)
        .unwrap_or_else(|| panic!("fixture {} lacks `pub let expected : u64 = 0x…`", fixture_rel));
    let hex = caps.get(1).unwrap().as_str().replace('_', "");
    u64::from_str_radix(&hex, 16)
        .unwrap_or_else(|_| panic!("fixture {} expected value not parseable as u64", fixture_rel))
}

fn assert_fixture_matches_spec(fixture_rel: &str, input: &[u8]) {
    let claimed = read_expected_u64(fixture_rel);
    let derived = fnv1a_64(input);
    assert_eq!(
        claimed, derived,
        "\n{} declares expected = 0x{:016X}\nbut FNV-1a-64({:?}) per schema-registry.md §3 = 0x{:016X}\n\
         → correct the fixture to `pub let expected : u64 = 0x{:016X}`",
        fixture_rel, claimed, input, derived, derived
    );
}

#[test]
fn r220m5_hash_empty_matches_spec() {
    assert_fixture_matches_spec("pdx/str_hash_empty_vector.pdx", b"");
}

#[test]
fn r220m5_hash_a_matches_spec() {
    assert_fixture_matches_spec("pdx/str_hash_a_vector.pdx", b"a");
}

#[test]
fn r220m5_hash_ab_matches_spec() {
    assert_fixture_matches_spec("pdx/str_hash_ab_vector.pdx", b"ab");
}

#[test]
fn r220m5_hash_abc_matches_spec() {
    assert_fixture_matches_spec("pdx/str_hash_abc_vector.pdx", b"abc");
}

#[test]
fn r220m5_hash_foobar_matches_spec() {
    assert_fixture_matches_spec("pdx/str_hash_foobar_vector.pdx", b"foobar");
}

// -----------------------------------------------------------------------
// R220.M5 — 100-string collision-cluster property test.
//
// Generates 100 short pseudo-random byte strings from a deterministic
// seed (so failure is reproducible), hashes each with FNV-1a-64, and
// asserts every hash is unique. Two-way collisions on 100 random inputs
// under a 64-bit hash have probability ≈ 100·99/2 / 2^64 ≈ 2.7e-16, so a
// hit is a bug in the algorithm or its constants. This is not a
// randomized test in the sense that a fresh seed each run risks
// flakiness — the seed is fixed at 0xDEADBEEF_CAFEBABE.
// -----------------------------------------------------------------------

#[test]
fn r220m5_fnv1a_64_no_collisions_over_100_random_strings() {
    // xorshift64 for a deterministic PRNG — no external dep.
    let mut rng_state: u64 = 0xDEADBEEF_CAFEBABE;
    fn next(state: &mut u64) -> u64 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *state = x;
        x
    }

    let mut hashes: HashMap<u64, Vec<u8>> = HashMap::new();
    for _ in 0..100 {
        // Length 1..=24 to bias toward the "short shell command" case.
        let len = (next(&mut rng_state) as usize % 24) + 1;
        let mut buf = Vec::with_capacity(len);
        for _ in 0..len {
            buf.push((next(&mut rng_state) & 0xFF) as u8);
        }
        let h = fnv1a_64(&buf);
        if let Some(prev) = hashes.get(&h) {
            panic!(
                "FNV-1a-64 collision on 100-string cluster (seed 0xDEADBEEF_CAFEBABE):\n  \
                 {:?} and {:?} both hash to 0x{:016X}",
                prev, buf, h
            );
        }
        hashes.insert(h, buf);
    }
    assert_eq!(hashes.len(), 100, "expected 100 unique hashes");
}

// -----------------------------------------------------------------------
// R220.M4/M5 — source-of-truth checks (do not require the paideia-as
// binary; run under plain `cargo test`).
// -----------------------------------------------------------------------

#[test]
fn r220m4_str_eq_module_declares_signature_and_nfc_fixme() {
    let src = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/str_eq.pdx"),
    )
    .expect("str_eq.pdx must exist for R220.M4");
    assert!(src.contains("module StrEq"), "str_eq.pdx: missing `module StrEq`");
    assert!(
        src.contains("pub let str_eq"),
        "str_eq.pdx: missing `pub let str_eq` binding"
    );
    assert!(
        src.contains("FIXME(nfc)"),
        "str_eq.pdx: NFC deferral must be marked with `FIXME(nfc)` per SH-D9"
    );
}

#[test]
fn r220m5_str_hash_module_declares_signature_and_fixmes() {
    let src = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/str_hash.pdx"),
    )
    .expect("str_hash.pdx must exist for R220.M5");
    assert!(src.contains("module StrHash"), "str_hash.pdx: missing `module StrHash`");
    assert!(
        src.contains("pub let str_hash"),
        "str_hash.pdx: missing `pub let str_hash` binding"
    );
    assert!(
        src.contains("0xCBF29CE484222325"),
        "str_hash.pdx: FNV offset basis constant not present"
    );
    assert!(
        src.contains("0x100000001B3"),
        "str_hash.pdx: FNV prime constant not present"
    );
    assert!(
        src.contains("FIXME(nfc)") && src.contains("FIXME(blake3)"),
        "str_hash.pdx: must mark both NFC and BLAKE3 upgrade paths with FIXME tags"
    );
}
