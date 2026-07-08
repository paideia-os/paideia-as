//! PA-r17-004 (#982): register-callee `call fptr` lowering.
//!
//! Verifies:
//!   1. Indirect call via register emits `mov r11, callee; mov args; call r11; ret`
//!   2. SysV argument marshalling routes arg0/arg1 into RDI/RSI at the call site
//!   3. Direct-call latent bug fix: two-function module resolves the correct callee
//!
//! Every positive test disassembles the actual .text bytes and asserts
//! byte-for-byte against a concrete literal — no OR-chains, no `.contains()`,
//! no `.text.size() > 0` weak assertions.

use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget};
use std::path::PathBuf;
use std::process::Command;

fn build_emit_data(name: &str) -> PathBuf {
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

fn extract_symbol_bytes(file: &object::File<'_>, symbol_name: &str) -> Option<Vec<u8>> {
    let mut text_bytes = Vec::new();
    for section in file.sections() {
        if section.name().unwrap_or("") == ".text" {
            text_bytes = section.data().unwrap_or(b"").to_vec();
            break;
        }
    }
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            if name == symbol_name {
                let sym_addr = sym.address() as usize;
                let sym_size = sym.size() as usize;
                if sym_size > 0 && sym_addr + sym_size <= text_bytes.len() {
                    return Some(text_bytes[sym_addr..sym_addr + sym_size].to_vec());
                }
            }
        }
    }
    None
}

fn build_and_assert(fixture_name: &str, symbol_name: &str, expected_bytes: &[u8]) {
    let input = build_emit_data(fixture_name);
    let tmp = std::env::temp_dir().join(format!("paideia_as_{}_emit.o", fixture_name));
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
        "build failed for {}: {}",
        fixture_name,
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");
    let actual = extract_symbol_bytes(&file, symbol_name)
        .unwrap_or_else(|| panic!("Symbol {} not found in {}", symbol_name, fixture_name));

    assert_eq!(
        actual, expected_bytes,
        "\n{}::{} byte mismatch\n  expected: {:02X?}\n  actual:   {:02X?}",
        fixture_name, symbol_name, expected_bytes, actual
    );

    let _ = std::fs::remove_file(&tmp);
}

/// (f)(x): `mov r11, rdi; mov rdi, rsi; call r11; ret`
#[test]
fn indirect_call_identity() {
    build_and_assert(
        "indirect_call_identity.pdx",
        "call_it",
        &[0x49, 0x89, 0xFB, 0x48, 0x89, 0xF7, 0x41, 0xFF, 0xD3, 0xC3],
    );
}

/// (f)(a, b): `mov r11, rdi; mov rdi, rsi; mov rsi, rdx; call r11; ret`
#[test]
fn indirect_call_two_arg() {
    build_and_assert(
        "indirect_call_two_arg.pdx",
        "call_it",
        &[
            0x49, 0x89, 0xFB, 0x48, 0x89, 0xF7, 0x48, 0x89, 0xD6, 0x41, 0xFF, 0xD3, 0xC3,
        ],
    );
}

/// (f)(): `mov r11, rdi; call r11; ret`
#[test]
fn indirect_call_zero_arg() {
    build_and_assert(
        "indirect_call_zero_arg.pdx",
        "call_it",
        &[0x49, 0x89, 0xFB, 0x41, 0xFF, 0xD3, 0xC3],
    );
}

/// Latent bug fix: two functions in one module, third calls the SECOND.
/// Old code picked the first Function symbol regardless. Fixed code must
/// resolve `second` by name and generate a reloc targeting `second`.
#[test]
fn direct_call_two_funcs_reloc_targets_second() {
    let input = build_emit_data("direct_call_two_funcs.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_direct_call_two_funcs_emit.o");
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
        "build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find the .text section and iterate its relocations.
    let text_section = file
        .sections()
        .find(|s| s.name().unwrap_or("") == ".text")
        .expect(".text section");

    let mut found_second_reloc = false;
    let mut found_first_reloc = false;
    for (_offset, reloc) in text_section.relocations() {
        if let RelocationTarget::Symbol(sym_idx) = reloc.target() {
            let sym = file.symbol_by_index(sym_idx).expect("symbol by index");
            let name = sym.name().unwrap_or("");
            if name == "second" {
                found_second_reloc = true;
            } else if name == "first" {
                found_first_reloc = true;
            }
        }
    }

    assert!(
        found_second_reloc,
        "expected reloc targeting `second`, none found (this was the latent bug)"
    );
    assert!(
        !found_first_reloc,
        "reloc unexpectedly targets `first` (the latent bug is back)"
    );

    let _ = std::fs::remove_file(&tmp);
}
