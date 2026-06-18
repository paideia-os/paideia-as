//! End-to-end smoke: build a representative `.pdx` source into a valid
//! ELF64 object via `paideia-as build --emit elf64` and assert basic
//! invariants.
//!
//! Per the issue's design-doc reference, the §13 `ExampleDriver` source
//! in `syntax-reference.md` is the ultimate target. The reference
//! document does not yet contain a §13 example — when it does, the
//! `data/example_driver.pdx` fixture should be replaced with that
//! source. Until then this fixture exercises a representative subset of
//! parser surface (module + structure + lets).

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // tests/e2e/ → workspace root.
    p.pop();
    p.pop();
    p
}

fn fixture(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("data");
    p.push(name);
    p
}

#[test]
fn example_driver_builds_to_valid_elf64() {
    let input = fixture("example_driver.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_e2e_example_driver.o");
    let _ = std::fs::remove_file(&tmp);

    let mut cmd = Command::new(env!("CARGO"));
    cmd.current_dir(workspace_root());
    cmd.args([
        "run",
        "--quiet",
        "-p",
        "paideia-as",
        "--",
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        tmp.to_str().unwrap(),
    ]);
    cmd.env("NO_COLOR", "1");
    let out = cmd.output().expect("failed to spawn cargo run");

    assert!(
        out.status.success(),
        "paideia-as build exited with {:?}\nstdout: {}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    assert!(bytes.len() >= 64, "ELF header is 64 bytes minimum");
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic missing");
    assert_eq!(bytes[4], 2, "expected ELF64 (class 2)");
    assert_eq!(bytes[5], 1, "expected little-endian (data 1)");

    use object::{Object, ObjectSection};
    let file = object::File::parse(&*bytes).expect("object crate should parse the ELF");
    let section_names: Vec<String> = file
        .sections()
        .filter_map(|s| s.name().ok().map(String::from))
        .collect();
    // At least one of the standard sections must be present.
    assert!(
        section_names.iter().any(|n| n == ".text"),
        "expected .text in section list, got {section_names:?}"
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn example_driver_is_a_valid_fixture() {
    // Sanity check that the fixture file exists and is non-empty so
    // failures in the build step are unambiguous.
    let input = fixture("example_driver.pdx");
    let content = std::fs::read_to_string(&input).expect("fixture should exist");
    assert!(!content.is_empty(), "fixture should be non-empty");
    assert!(
        content.contains("module"),
        "fixture should declare a module"
    );
}
