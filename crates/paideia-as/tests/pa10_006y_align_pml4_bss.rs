//! PA10-006y (issue #878): `@align(N)` per-symbol alignment directive.
//!
//! This integration test is the driving fixture for the feature: it verifies
//! end-to-end that `@align(4096)` on a `.bss` binding produces a 4096-aligned
//! `.bss` section with the aligned symbol present at a 4096-aligned offset.
//!
//! Fixture (written to a temp `.pdx` file):
//! ```text
//! module Pa10006yAlignPml4Bss = structure {
//!   pub let mut pml4 : [u64; 512] = uninit @align(4096)
//! }
//! ```

use object::{Object, ObjectSection, ObjectSymbol};
use std::process::Command;

const FIXTURE_SRC: &str = r#"module Pa10006yAlignPml4Bss = structure {
  pub let mut pml4 : [u64; 512] = uninit @align(4096)
}
"#;

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

#[test]
fn align_4096_produces_4096_aligned_bss_section_with_pml4_symbol() {
    // Write the fixture to a temp path. The file stem must PascalCase-convert to
    // the module name declared in FIXTURE_SRC (`Pa10006yAlignPml4Bss`), per the
    // M0305 file-module naming rule (§7.6).
    let src_path = std::env::temp_dir().join("pa10_006y_align_pml4_bss.pdx");
    std::fs::write(&src_path, FIXTURE_SRC).expect("should write fixture source");

    let obj_path = std::env::temp_dir().join("pa10_006y_align_pml4_bss.o");
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
        "build --emit elf64 failed for pa10_006y_align_pml4_bss fixture: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Read and parse the ELF file.
    let bytes = std::fs::read(&obj_path).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find the .bss section.
    let mut bss_section = None;
    for section in file.sections() {
        if section.name().unwrap_or("") == ".bss" {
            bss_section = Some(section);
            break;
        }
    }
    let bss_section = bss_section.expect(".bss section should exist");

    // Assert .bss is aligned to at least 4096 bytes.
    assert!(
        bss_section.align() >= 4096,
        ".bss section alignment should be >= 4096, got {}",
        bss_section.align()
    );

    // Find the pml4 symbol and assert it is global and lives in .bss.
    let bss_idx = bss_section.index();
    let mut pml4_symbol = None;
    for symbol in file.symbols() {
        if symbol.name().unwrap_or("") == "pml4" {
            pml4_symbol = Some(symbol);
            break;
        }
    }
    let pml4_symbol = pml4_symbol.expect("pml4 symbol should exist");

    assert!(
        pml4_symbol.is_global(),
        "pml4 symbol should be marked global"
    );
    assert_eq!(
        pml4_symbol.section_index(),
        Some(bss_idx),
        "pml4 symbol should reference the .bss section"
    );

    // Assert the pml4 symbol's offset within .bss is itself 4096-aligned.
    let pml4_offset = pml4_symbol.address();
    assert_eq!(
        pml4_offset % 4096,
        0,
        "pml4 offset ({}) within .bss should be 4096-aligned",
        pml4_offset
    );

    let _ = std::fs::remove_file(&src_path);
    let _ = std::fs::remove_file(&obj_path);
}
