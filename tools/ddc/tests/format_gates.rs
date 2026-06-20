//! Determinism gate corpus: each fixture must build to a byte-identical
//! artifact on two successive invocations (no toolchain diversity required;
//! deterministic within a single toolchain is the m10-004 baseline).

use std::path::PathBuf;
use std::process::Command;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn paideia_as_binary() -> PathBuf {
    // The integration test build directory.
    let cargo_target = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            p.push("..");
            p.push("..");
            p.push("target");
            p
        });
    // Use whichever build matches the test's profile.
    if cfg!(debug_assertions) {
        cargo_target.join("debug/paideia-as")
    } else {
        cargo_target.join("release/paideia-as")
    }
}

fn build_twice_and_diff(fixture: &str, emit: &str) {
    let bin = paideia_as_binary();
    if !bin.exists() {
        eprintln!(
            "paideia-as binary not built at {}; skipping (run cargo build first)",
            bin.display()
        );
        return;
    }
    let fixture_path = fixtures_dir().join(fixture);
    if !fixture_path.exists() {
        panic!("fixture missing: {}", fixture_path.display());
    }

    let tmp = std::env::temp_dir().join(format!("ddc-fmt-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    let out_a = tmp.join(format!("a.{}", emit));
    let out_b = tmp.join(format!("b.{}", emit));

    let run = |out: &PathBuf| {
        Command::new(&bin)
            .args([
                "build",
                "--emit",
                emit,
                &fixture_path.to_string_lossy(),
                "-o",
                &out.to_string_lossy(),
            ])
            .env("SOURCE_DATE_EPOCH", "0")
            .env("PDX_PATH_PREFIX_MAP", "/=/build/")
            .output()
            .expect("paideia-as run")
    };

    let result_a = run(&out_a);
    let result_b = run(&out_b);

    if !result_a.status.success() {
        eprintln!(
            "build A failed: {}",
            String::from_utf8_lossy(&result_a.stderr)
        );
        return;
    }
    if !result_b.status.success() {
        eprintln!(
            "build B failed: {}",
            String::from_utf8_lossy(&result_b.stderr)
        );
        return;
    }

    let bytes_a = std::fs::read(&out_a).expect("read a");
    let bytes_b = std::fs::read(&out_b).expect("read b");

    assert_eq!(
        bytes_a,
        bytes_b,
        "fixture {} emit {} produced divergent bytes between runs (lengths {} vs {})",
        fixture,
        emit,
        bytes_a.len(),
        bytes_b.len()
    );
}

const FIXTURES: &[&str] = &[
    "empty_module.pdx",
    "single_val.pdx",
    "two_vals.pdx",
    "type_decl.pdx",
    "module_with_inner.pdx",
    "let_binding.pdx",
    "function_decl.pdx",
    "match_expr.pdx",
    "capability_use.pdx",
    "handler_install.pdx",
];

#[test]
#[ignore = "requires `cargo build` to populate target/debug/paideia-as first; gated on local toolchain"]
fn format_gate_corpus_pe_coff() {
    for fx in FIXTURES {
        build_twice_and_diff(fx, "pe-coff");
    }
}

#[test]
#[ignore = "requires paideia-as binary built first"]
fn format_gate_corpus_elf64() {
    for fx in FIXTURES {
        build_twice_and_diff(fx, "elf64");
    }
}

#[test]
#[ignore = "requires paideia-as binary built first"]
fn format_gate_corpus_pax() {
    for fx in FIXTURES {
        build_twice_and_diff(fx, "pax");
    }
}

#[test]
fn format_gate_fixtures_count_is_ten() {
    let dir = fixtures_dir();
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|s| s.to_str()) == Some("pdx") {
                count += 1;
            }
        }
    }
    assert_eq!(count, 10, "expected 10 fixtures, found {count}");
}
