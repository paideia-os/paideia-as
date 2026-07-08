//! PA10-006y (issue #878): `@align(N)` per-symbol alignment directive.
//!
//! Regression guard: verifies that WITHOUT an `@align(N)` suffix, `.bss`
//! alignment stays at the softarch-specified default of 8 bytes (not 4096,
//! not some other value picked up incidentally by the alignment feature).
//!
//! Fixture (written to a temp `.pdx` file):
//! ```text
//! module Pa10006yAlignDefault = structure {
//!   let mut buf : [u64; 4] = uninit
//! }
//! ```

use object::{Object, ObjectSection, ObjectSymbol};
use std::process::Command;

const FIXTURE_SRC: &str = r#"module Pa10006yAlignDefault = structure {
  let mut buf : [u64; 4] = uninit
}
"#;

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

#[test]
fn no_align_attribute_keeps_default_bss_alignment_at_8() {
    // Write the fixture to a temp path. The file stem must PascalCase-convert to
    // the module name declared in FIXTURE_SRC (`Pa10006yAlignDefault`), per the
    // M0305 file-module naming rule (§7.6).
    let src_path = std::env::temp_dir().join("pa10_006y_align_default.pdx");
    std::fs::write(&src_path, FIXTURE_SRC).expect("should write fixture source");

    let obj_path = std::env::temp_dir().join("pa10_006y_align_default.o");
    let _ = std::fs::remove_file(&obj_path);

    // Build the fixture into ELF64 format.
    let out = cargo_run(&[
        "build",
        src_path.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        obj_path.to_str().unwrap(),
    ]);

    assert!(
        out.status.success(),
        "build --emit elf64 failed for pa10_006y_align_default fixture: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Read and parse the ELF file.
    let bytes = std::fs::read(&obj_path).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find the .bss section and assert its alignment is the default of 8.
    let mut bss_section = None;
    for section in file.sections() {
        if section.name().unwrap_or("") == ".bss" {
            bss_section = Some(section);
            break;
        }
    }
    let bss_section = bss_section.expect(".bss section should exist");

    assert_eq!(
        bss_section.align(),
        8,
        ".bss section alignment should default to 8 when no @align(N) is present, got {}",
        bss_section.align()
    );

    // Assert the buf symbol exists.
    let mut found_buf = false;
    for symbol in file.symbols() {
        if symbol.name().unwrap_or("") == "buf" {
            found_buf = true;
            break;
        }
    }
    assert!(found_buf, "buf symbol should exist in the .bss section");

    let _ = std::fs::remove_file(&src_path);
    let _ = std::fs::remove_file(&obj_path);
}
