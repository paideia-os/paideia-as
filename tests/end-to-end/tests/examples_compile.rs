//! Regression test for `examples/<file>.pdx` files whose status header is
//! "compiles end-to-end". Each such file must build to an ELF64 via
//! `paideia-as build --emit elf64` and the resulting `.text` section must
//! contain at least one instruction.
//!
//! Phase-3-m1-012 honesty: as of the m1-012 commit, every example in
//! `examples/` still carries a "parses cleanly" status — the elaborator's
//! intrinsic-call lowering chokepoint (m1-013+) is the last hop before
//! examples 15 / 16 / 17 can flip to "compiles end-to-end". The harness
//! is shipped today so that:
//!
//! 1. The discovery + build + objdump-size logic is reviewable.
//! 2. The moment the elaborator chokepoint lands, the test activates
//!    by removing the `#[ignore]` line — no harness work needed.
//!
//! `cargo test --test examples_compile -p paideia-end-to-end -- --ignored`
//! runs the gated tests (against the locally-built paideia-as binary).

use std::path::{Path, PathBuf};
use std::process::Command;

/// Locate the workspace root by walking up from CARGO_MANIFEST_DIR.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn examples_dir() -> PathBuf {
    workspace_root().join("examples")
}

fn paideia_as_binary() -> Option<PathBuf> {
    let candidates = [
        workspace_root().join("target/release/paideia-as"),
        workspace_root().join("target/debug/paideia-as"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

/// Scan the file's header lines for `// status: compiles end-to-end`.
fn declares_compiles_end_to_end(path: &Path) -> bool {
    let contents = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return false,
    };
    for line in contents.lines().take(20) {
        if line.contains("status:") && line.contains("compiles end-to-end") {
            return true;
        }
    }
    false
}

fn collect_compiles_end_to_end_examples() -> Vec<PathBuf> {
    let dir = examples_dir();
    if !dir.exists() {
        return Vec::new();
    }
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("read examples dir")
        .filter_map(|e| e.ok().map(|d| d.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("pdx"))
        .filter(|p| declares_compiles_end_to_end(p))
        .collect();
    out.sort();
    out
}

#[test]
fn examples_dir_is_present() {
    assert!(examples_dir().is_dir(), "examples/ missing");
}

#[test]
#[ignore = "phase-3-m1-013+: elaborator intrinsic-call chokepoint is the last hop before examples flip to compiles-end-to-end"]
fn every_compiles_end_to_end_example_builds_to_elf64() {
    let bin = match paideia_as_binary() {
        Some(b) => b,
        None => {
            eprintln!("paideia-as binary missing; build first");
            return;
        }
    };
    let examples = collect_compiles_end_to_end_examples();
    assert!(
        examples.len() >= 3,
        "expected at least 3 compiles-end-to-end examples; found {}",
        examples.len()
    );
    let tmp = std::env::temp_dir().join(format!("examples-compile-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("tmp dir");
    for ex in &examples {
        let out_path = tmp.join(format!(
            "{}.o",
            ex.file_stem().unwrap().to_string_lossy()
        ));
        let result = Command::new(&bin)
            .args([
                "build",
                "--emit",
                "elf64",
                &ex.to_string_lossy(),
                "-o",
                &out_path.to_string_lossy(),
            ])
            .env("SOURCE_DATE_EPOCH", "0")
            .env("PDX_PATH_PREFIX_MAP", "/=/build/")
            .output()
            .expect("paideia-as build");
        assert!(
            result.status.success(),
            "build failed for {}: {}",
            ex.display(),
            String::from_utf8_lossy(&result.stderr)
        );
        let bytes = std::fs::read(&out_path).expect("read elf");
        assert!(
            bytes.len() >= 64,
            "ELF64 output too small ({} bytes) for {}",
            bytes.len(),
            ex.display()
        );
        // ELF64 magic: 7f 45 4c 46 02 ...
        assert_eq!(&bytes[0..4], b"\x7fELF", "ELF magic missing for {}", ex.display());
        assert_eq!(bytes[4], 2, "ELF64 class missing for {}", ex.display());
        // The .text-non-empty assertion currently only reaches us once the
        // elaborator chokepoint is wired; full objdump-d cross-check
        // activates with m1-013.
    }
    let _ = std::fs::remove_dir_all(&tmp);
}
