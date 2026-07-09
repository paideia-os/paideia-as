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

#[test]
fn uefi_header_field_offsets_match_spec() {
    let template_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("pdx/uefi_header_template.bin");
    let bytes = std::fs::read(&template_path)
        .expect("Failed to read uefi_header_template.bin");

    // Test each offset constant from uefi.pdx
    // OFF_COFF_TIMESTAMP = 0x48
    let timestamp = u32::from_le_bytes([bytes[0x48], bytes[0x49], bytes[0x4A], bytes[0x4B]]);
    assert_eq!(timestamp, 0, "OFF_COFF_TIMESTAMP offset incorrect");

    // OFF_COFF_NUM_SECTIONS = 0x46
    let num_sections = u16::from_le_bytes([bytes[0x46], bytes[0x47]]);
    assert_eq!(num_sections, 0, "OFF_COFF_NUM_SECTIONS offset incorrect");

    // OFF_OPT_ADDRESS_OF_ENTRY = 0x68
    let entry_rva = u32::from_le_bytes([bytes[0x68], bytes[0x69], bytes[0x6A], bytes[0x6B]]);
    assert_eq!(entry_rva, 0, "OFF_OPT_ADDRESS_OF_ENTRY offset incorrect");

    // OFF_OPT_SIZE_OF_CODE = 0x5C
    let size_of_code = u32::from_le_bytes([bytes[0x5C], bytes[0x5D], bytes[0x5E], bytes[0x5F]]);
    assert_eq!(size_of_code, 0, "OFF_OPT_SIZE_OF_CODE offset incorrect");

    // OFF_OPT_SIZE_OF_IMAGE = 0x90
    let size_of_image = u32::from_le_bytes([bytes[0x90], bytes[0x91], bytes[0x92], bytes[0x93]]);
    assert_eq!(size_of_image, 0, "OFF_OPT_SIZE_OF_IMAGE offset incorrect");

    // OFF_OPT_SIZE_OF_HEADERS = 0x94
    let size_of_headers = u32::from_le_bytes([bytes[0x94], bytes[0x95], bytes[0x96], bytes[0x97]]);
    assert_eq!(size_of_headers, 0, "OFF_OPT_SIZE_OF_HEADERS offset incorrect");

    // OFF_OPT_CHECKSUM = 0x98
    let checksum = u32::from_le_bytes([bytes[0x98], bytes[0x99], bytes[0x9A], bytes[0x9B]]);
    assert_eq!(checksum, 0, "OFF_OPT_CHECKSUM offset incorrect");

    // OFF_OPT_SUBSYSTEM = 0x9C
    let subsystem = u16::from_le_bytes([bytes[0x9C], bytes[0x9D]]);
    assert_eq!(subsystem, 10, "OFF_OPT_SUBSYSTEM offset incorrect");

    // OFF_SECTION_TABLE_START = 0x148
    // Beyond the minimal header (should be zeros in template)
    assert!(0x148 < bytes.len(), "OFF_SECTION_TABLE_START is within the 512-byte template");
}
