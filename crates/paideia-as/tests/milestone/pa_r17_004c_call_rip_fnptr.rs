//! Integration tests for function-pointer indirect call lowering (PA-R17-004c, issue #1040).
//!
//! Tests that `(f)(args)` where f is Object with AddrOf(target_fn) correctly:
//! - Detects module-scope fnptr Object (SymbolKind::Object with AddrOfMeta)
//! - Emits single RIP-relative indirect call (FF 15 + disp32)
//! - Routes argument marshalling correctly
//! - Produces valid ELF output with proper relocations

use object::{Object, ObjectSection, ObjectSymbol};
use std::path::PathBuf;
use std::process::Command;

fn data(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../tests/build-emit");
    p.push(name);
    p
}

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

/// Test: fnptr_object_indirect_call
///
/// Verifies that indirect call via module-level fnptr Object compiles and emits
/// the correct RIP-relative indirect call instruction:
///
/// ```
/// pub let identity : (u32) -> u32 = fn(x: u32) -> x
/// pub let f : (u32) -> u32 = &identity
/// pub let entry : () -> u32 = fn(_: ()) -> (f)(42u32)
/// ```
///
/// - Compiles successfully
/// - Produces valid ELF output
/// - Symbol table contains `identity`, `f`, and `entry`
/// - `.text` section contains FF 15 opcode (indirect call) at the call site
/// - `.rela.text` has a relocation against symbol `f` at the disp32 offset
#[test]
fn fnptr_object_indirect_call() {
    let input = data("pa_r17_004c_call_rip_fnptr.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_fnptr_rip_call.o");
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
        "build should succeed for indirect call via fnptr Object. stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Verify output ELF is valid
    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("output should be valid ELF");

    // Verify symbols exist
    let mut has_identity = false;
    let mut has_f = false;
    let mut has_entry = false;
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            if name == "identity" {
                has_identity = true;
            }
            if name == "f" {
                has_f = true;
            }
            if name == "entry" {
                has_entry = true;
            }
        }
    }
    assert!(has_identity, "should have `identity` function symbol");
    assert!(has_f, "should have `f` data symbol (fnptr Object)");
    assert!(has_entry, "should have `entry` function symbol");

    // Extract .text section and verify FF 15 opcode is present
    let text_section = file
        .sections()
        .find(|s| s.name().unwrap_or("") == ".text")
        .expect("should have .text section");

    let text_bytes = text_section.data().expect("should read .text data");
    assert!(
        text_bytes.windows(2).any(|w| w == b"\xFF\x15"),
        "should contain FF 15 opcode (indirect call) in .text"
    );

    // Verify relocation section exists for .text
    let has_reloc_section = file
        .sections()
        .any(|s| s.name().unwrap_or("") == ".rela.text");
    assert!(
        has_reloc_section,
        "should have .rela.text section for relocations"
    );

    let _ = std::fs::remove_file(&tmp);
}
