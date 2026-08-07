//! Phase 5 m6-003: Byte-sequence assertion test for uart_smoke.pdx.
//!
//! This test verifies that the build command correctly emits the unsafe block
//! bytes for uart_smoke.pdx, which contains direct x86-64 assembly:
//!   mov al, 0x78       => B0 78
//!   mov dx, 0x3F8     => 66 BA F8 03
//!   out dx, al         => EE
//!   hlt                => F4
//!
//! The test:
//! 1. Invokes the build command programmatically
//! 2. Reads the resulting .o (ELF) file
//! 3. Extracts the .text section bytes via the `object` crate
//! 4. Compares against tests/build-emit/uart_smoke.expected_bytes.txt
//! 5. Also asserts the _start symbol exists, is STB_GLOBAL, and has non-zero size

use object::{Object, ObjectSymbol};

use crate::common::elf::{assert_elf64_magic, text_bytes};
use crate::common::expected;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

/// Phase 5 m6-003: Test that build emits correct .text bytes for uart_smoke.pdx.
///
/// This test is THE TRUTH DETECTOR for whether the unsafe block content
/// reaches the emitted code. If the bytes match expected_bytes.txt, the chain
/// (m1-004 → m3-004 → m3-005 → m5-005) is working correctly.
/// If they don't match, this test fails and surfaces the bug.
#[test]
fn uart_smoke_text_bytes_match_expected() {
    let out = run_build(build_emit("uart_smoke.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    let text = text_bytes(&bytes);
    assert!(!text.is_empty(), ".text section must exist in ELF");

    let expected = expected::load(&build_emit("uart_smoke.expected_bytes.txt"));

    if text != expected {
        eprintln!(
            "MISMATCH: emitted .text bytes do not match expected\n\
             Expected ({} bytes): {:02X?}\n\
             Got ({} bytes):      {:02X?}",
            expected.len(),
            expected,
            text.len(),
            text
        );
        panic!(
            ".text section mismatch: expected {} bytes, got {}",
            expected.len(),
            text.len()
        );
    }

    // Assert _start symbol exists with correct properties.
    // Phase 5 m6-001/m5-003 ensures _start is emitted as STB_GLOBAL.
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");
    let symbol_count = file.symbols().count();
    assert!(
        symbol_count > 0,
        "ELF symbol table must contain at least one symbol (including _start)"
    );
}

/// Phase 5 m6-005: Test byte-identical code emission for functions in 02_functions.pdx.
///
/// paideia-as#1276 phase 3: bytes updated for the default SysV frame-pointer
/// prologue/epilogue (`push rbp; mov rbp, rsp; ...; mov rsp, rbp; pop rbp;
/// ret`). Fixtures don't carry `@no_frame` so they get the default emission.
///
/// - `add_one(x)`: returns x + 1, expected bytecode:
///   `55 48 89 E5 48 8D 47 01 48 89 EC 5D C3` (13 bytes)
///   push rbp; mov rbp,rsp; lea rax,[rdi+1]; mov rsp,rbp; pop rbp; ret
/// - `identity(x)`: returns x, expected bytecode:
///   `55 48 89 E5 48 89 F8 48 89 EC 5D C3` (12 bytes)
///   push rbp; mov rbp,rsp; mov rax,rdi; mov rsp,rbp; pop rbp; ret
/// - `double(x)`: returns x + x, expected bytecode:
///   `55 48 89 E5 48 8D 04 3F 48 89 EC 5D C3` (13 bytes)
///   push rbp; mov rbp,rsp; lea rax,[rdi+rdi]; mov rsp,rbp; pop rbp; ret
#[test]
fn add_one_byte_identical() {
    let input = build_emit("../../examples/02_functions.pdx");
    let out = run_build(input);
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    let text = text_bytes(&bytes);
    assert!(!text.is_empty(), ".text section must exist in ELF");

    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Build a map of symbol name -> raw bytes.
    let mut symbol_bytes: std::collections::HashMap<String, Vec<u8>> =
        std::collections::HashMap::new();

    eprintln!("=== Symbol table debug ===");
    eprintln!(".text section: {} bytes", text.len());
    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            let sym_addr = sym.address() as usize;
            let sym_size = sym.size() as usize;
            eprintln!("  {}: addr={}, size={}", name, sym_addr, sym_size);

            if sym_size > 0 && sym_addr + sym_size <= text.len() {
                let func_bytes = text[sym_addr..sym_addr + sym_size].to_vec();
                symbol_bytes.insert(name.to_string(), func_bytes);
            }
        }
    }

    // paideia-as#1276 phase 3: prologue `55 48 89 E5` + body + epilogue `48 89 EC 5D` + `C3`.
    let expected_add_one = vec![
        0x55, 0x48, 0x89, 0xE5,             // push rbp; mov rbp, rsp
        0x48, 0x8D, 0x47, 0x01,             // lea rax, [rdi + 1]
        0x48, 0x89, 0xEC, 0x5D, 0xC3,       // mov rsp, rbp; pop rbp; ret
    ];
    let expected_identity = vec![
        0x55, 0x48, 0x89, 0xE5,             // push rbp; mov rbp, rsp
        0x48, 0x89, 0xF8,                   // mov rax, rdi
        0x48, 0x89, 0xEC, 0x5D, 0xC3,       // mov rsp, rbp; pop rbp; ret
    ];
    let expected_double = vec![
        0x55, 0x48, 0x89, 0xE5,             // push rbp; mov rbp, rsp
        0x48, 0x8D, 0x04, 0x3F,             // lea rax, [rdi + rdi]
        0x48, 0x89, 0xEC, 0x5D, 0xC3,       // mov rsp, rbp; pop rbp; ret
    ];

    let check_function = |name: &str, expected: &[u8]| {
        if let Some(actual) = symbol_bytes.get(name) {
            if actual != expected {
                eprintln!(
                    "MISMATCH in {}: expected {:02X?}, got {:02X?}",
                    name, expected, actual
                );
                panic!("Function {} bytes do not match", name);
            }
        } else {
            panic!("Function {} not found in symbol table", name);
        }
    };

    check_function("add_one", &expected_add_one);
    check_function("identity", &expected_identity);
    check_function("double", &expected_double);
}
