//! Canary integration tests for `@fingerprint("<name>")` (R220.M10,
//! paideia-as#1424).
//!
//! Drives the full pipeline (parse → elaborate → ELF emit) on a
//! multi-fingerprint fixture and asserts:
//!
//! 1. **Bytes land in `.rodata`.** Every `@fingerprint("<name>")` attribute
//!    contributes a NUL-terminated byte string (`"<name>\0"`) to the
//!    compiled ELF's `.rodata` section — matching the shape the debugger
//!    substring-matches (anti-fabrication pattern per
//!    `feedback_workerbee_verify_claims.md`).
//! 2. **Multiple fingerprints coexist.** The fixture carries two
//!    `@fingerprint(...)` attributes; both appear in the same `.rodata`
//!    payload with their distinct tag bytes.
//! 3. **Symbol names are stable.** Each entry is exported under the
//!    `fp_<name>` symbol so a linker / disassembler has a stable handle
//!    on it (same shape the elaborator's `fingerprint_emit` promises).
//!
//! Fingerprint tags in this file: r220m10-fp-19, r220m10-fp-20, r220m10-fp-21.

use object::{Object, ObjectSection, ObjectSymbol};
use std::path::PathBuf;
use std::process::Command;

fn data(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/data");
    p.push(name);
    p
}

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

fn build_fingerprint_object() -> Vec<u8> {
    let input = data("fingerprint_intrinsic.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_r220m10_fp.o");
    let _ = std::fs::remove_file(&tmp);

    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        tmp.to_str().unwrap(),
    ]);

    assert!(
        out.status.success(),
        "build --emit elf64 failed for @fingerprint fixture:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let _ = std::fs::remove_file(&tmp);
    bytes
}

/// Fingerprint tag: r220m10-fp-19 — both NUL-terminated tags appear in `.rodata`.
///
/// Debugger contract: a byte-for-byte substring search of the compiled
/// ELF's `.rodata` finds each tag exactly as the source spelled it. This
/// is the load-bearing assertion for the anti-fabrication use — a hosted
/// DSL / REPL trusts that the tag it saw at elaboration time is the one
/// the debugger will confirm at wire time.
#[test]
fn fingerprint_bytes_land_in_rodata() {
    let bytes = build_fingerprint_object();

    let elf = object::read::elf::ElfFile64::<object::Endianness>::parse(bytes.as_slice())
        .expect("should parse as valid ELF64");

    let mut rodata_payload: Vec<u8> = Vec::new();
    for section in elf.sections() {
        if section.name().unwrap_or("") == ".rodata" {
            let data = section.data().expect(".rodata should have data");
            rodata_payload.extend_from_slice(data);
        }
    }
    assert!(
        !rodata_payload.is_empty(),
        "compiled ELF should have a non-empty .rodata (@fingerprint entries land here)"
    );

    // Each fingerprint entry is the tag bytes plus a NUL terminator.
    let tag1 = b"test.turn.001\0";
    let tag2 = b"r220m10-fp-01\0";

    assert!(
        rodata_payload
            .windows(tag1.len())
            .any(|w| w == tag1),
        "expected NUL-terminated \"test.turn.001\" in .rodata; payload = {:?}",
        rodata_payload
    );
    assert!(
        rodata_payload
            .windows(tag2.len())
            .any(|w| w == tag2),
        "expected NUL-terminated \"r220m10-fp-01\" in .rodata; payload = {:?}",
        rodata_payload
    );
}

/// Fingerprint tag: r220m10-fp-20 — each entry exports its `fp_<name>` symbol.
#[test]
fn fingerprint_symbols_are_exported() {
    let bytes = build_fingerprint_object();

    let elf = object::read::elf::ElfFile64::<object::Endianness>::parse(bytes.as_slice())
        .expect("should parse as valid ELF64");

    let mut have_fp1 = false;
    let mut have_fp2 = false;
    for sym in elf.symbols() {
        let name = sym.name().unwrap_or("");
        if name == "fp_test.turn.001" {
            have_fp1 = true;
        }
        if name == "fp_r220m10-fp-01" {
            have_fp2 = true;
        }
    }
    assert!(have_fp1, "expected fp_test.turn.001 symbol in ELF");
    assert!(have_fp2, "expected fp_r220m10-fp-01 symbol in ELF");
}

/// Fingerprint tag: r220m10-fp-21 — no `@fingerprint` → nothing extra in rodata.
///
/// Guards against a regression where `populate_fingerprints` runs even
/// when the side-table is empty and silently appends stray bytes. Uses
/// the already-shipping `hello.pdx` fixture which carries no
/// `@fingerprint` attributes; the compiled object must not carry any
/// `fp_*` symbols.
#[test]
fn no_fingerprint_symbols_when_attribute_absent() {
    let input = data("hello.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_r220m10_fp_absent.o");
    let _ = std::fs::remove_file(&tmp);

    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        tmp.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "hello.pdx build must succeed");
    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let _ = std::fs::remove_file(&tmp);

    let elf = object::read::elf::ElfFile64::<object::Endianness>::parse(bytes.as_slice())
        .expect("should parse as valid ELF64");

    for sym in elf.symbols() {
        let name = sym.name().unwrap_or("");
        assert!(
            !name.starts_with("fp_"),
            "no @fingerprint in source → no fp_* symbols expected, found {:?}",
            name
        );
    }
}
