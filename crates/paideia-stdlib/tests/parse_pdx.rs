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
fn system_alloc_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/system_alloc.pdx");
    let result = Command::new(bin)
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
fn system_alloc_decl_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/system_alloc_decl.pdx");
    let result = Command::new(bin)
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
fn system_alloc_in_linux_block_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/system_alloc_in_linux_block.pdx");
    let result = Command::new(bin)
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
fn vec_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/vec.pdx");
    let result = Command::new(bin)
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
fn vec_new_creates_empty_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/vec_new_creates_empty.pdx");
    let result = Command::new(bin)
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
fn vec_with_capacity_preallocates_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/vec_with_capacity_preallocates.pdx");
    let result = Command::new(bin)
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
fn vec_push_grows_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/vec_push_grows.pdx");
    let result = Command::new(bin)
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
fn vec_pop_shrinks_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/vec_pop_shrinks.pdx");
    let result = Command::new(bin)
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
fn vec_get_returns_option_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/vec_get_returns_option.pdx");
    let result = Command::new(bin)
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
fn vec_get_mut_returns_option_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/vec_get_mut_returns_option.pdx");
    let result = Command::new(bin)
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
fn vec_len_returns_count_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/vec_len_returns_count.pdx");
    let result = Command::new(bin)
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
fn vec_iter_yields_each_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/vec_iter_yields_each.pdx");
    let result = Command::new(bin)
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
fn vec_into_iter_consumes_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/vec_into_iter_consumes.pdx");
    let result = Command::new(bin)
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
fn vec_push_then_pop_returns_value_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/vec_push_then_pop_returns_value.pdx");
    let result = Command::new(bin)
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
fn vec_push_capacity_doubles_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/vec_push_capacity_doubles.pdx");
    let result = Command::new(bin)
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
fn vec_empty_pop_returns_none_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/vec_empty_pop_returns_none.pdx");
    let result = Command::new(bin)
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
fn vec_get_out_of_bounds_returns_none_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("pdx/vec_get_out_of_bounds_returns_none.pdx");
    let result = Command::new(bin)
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
fn vec_for_loop_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/vec_for_loop.pdx");
    let result = Command::new(bin)
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
fn vec_chained_push_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/vec_chained_push.pdx");
    let result = Command::new(bin)
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
fn vec_with_strings_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/vec_with_strings.pdx");
    let result = Command::new(bin)
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
fn hashmap_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/hashmap.pdx");
    let result = Command::new(bin)
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
fn hashmap_new_creates_empty_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/hashmap_new_creates_empty.pdx");
    let result = Command::new(bin)
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
fn hashmap_with_capacity_preallocates_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("pdx/hashmap_with_capacity_preallocates.pdx");
    let result = Command::new(bin)
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
fn hashmap_insert_returns_none_on_new_key_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("pdx/hashmap_insert_returns_none_on_new_key.pdx");
    let result = Command::new(bin)
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
fn hashmap_insert_returns_some_on_overwrite_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("pdx/hashmap_insert_returns_some_on_overwrite.pdx");
    let result = Command::new(bin)
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
fn hashmap_get_returns_value_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/hashmap_get_returns_value.pdx");
    let result = Command::new(bin)
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
fn hashmap_get_missing_returns_none_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/hashmap_get_missing_returns_none.pdx");
    let result = Command::new(bin)
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
fn hashmap_remove_returns_value_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/hashmap_remove_returns_value.pdx");
    let result = Command::new(bin)
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
fn hashmap_contains_key_true_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/hashmap_contains_key_true.pdx");
    let result = Command::new(bin)
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
fn hashmap_contains_key_false_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/hashmap_contains_key_false.pdx");
    let result = Command::new(bin)
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
fn hashmap_len_tracks_inserts_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/hashmap_len_tracks_inserts.pdx");
    let result = Command::new(bin)
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
fn hashmap_iter_yields_pairs_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/hashmap_iter_yields_pairs.pdx");
    let result = Command::new(bin)
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
fn hashmap_open_addressing_handles_collision_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("pdx/hashmap_open_addressing_handles_collision.pdx");
    let result = Command::new(bin)
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
fn file_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/file.pdx");
    let result = Command::new(bin)
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
fn file_open_returns_result_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/file_open_returns_result.pdx");
    let result = Command::new(bin)
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
fn file_create_returns_result_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/file_create_returns_result.pdx");
    let result = Command::new(bin)
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
fn file_read_into_buf_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/file_read_into_buf.pdx");
    let result = Command::new(bin)
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
fn file_read_to_string_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/file_read_to_string.pdx");
    let result = Command::new(bin)
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
fn file_write_all_succeeds_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/file_write_all_succeeds.pdx");
    let result = Command::new(bin)
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
fn file_flush_sync_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/file_flush_sync.pdx");
    let result = Command::new(bin)
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
fn iterator_pdx_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/iterator.pdx");
    let result = Command::new(bin)
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
fn iterator_next_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/iterator_next.pdx");
    let result = Command::new(bin)
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
fn iterator_map_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/iterator_map.pdx");
    let result = Command::new(bin)
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
fn iterator_filter_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/iterator_filter.pdx");
    let result = Command::new(bin)
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
fn iterator_fold_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/iterator_fold.pdx");
    let result = Command::new(bin)
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
fn iterator_collect_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/iterator_collect.pdx");
    let result = Command::new(bin)
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
fn iterator_for_each_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/iterator_for_each.pdx");
    let result = Command::new(bin)
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
fn iterator_count_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/iterator_count.pdx");
    let result = Command::new(bin)
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
fn iterator_chained_adapters_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/iterator_chained_adapters.pdx");
    let result = Command::new(bin)
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
fn iterator_map_filter_fold_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/iterator_map_filter_fold.pdx");
    let result = Command::new(bin)
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
fn iterator_for_loop_consumes_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/iterator_for_loop_consumes.pdx");
    let result = Command::new(bin)
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
fn iterator_vec_into_iter_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/iterator_vec_into_iter.pdx");
    let result = Command::new(bin)
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
fn iterator_default_method_impl_parses_cleanly() {
    let bin = paideia_as_bin().expect("paideia-as binary not built");
    let pdx =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/iterator_default_method_impl.pdx");
    let result = Command::new(bin)
        .args(["check", &pdx.to_string_lossy()])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
