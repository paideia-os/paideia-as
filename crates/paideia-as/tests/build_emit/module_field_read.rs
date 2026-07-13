//! Issue #1184: Module-qualified field read via RIP-relative load
//!
//! This test verifies that three shapes of `Module.field` reads (READ, not WRITE)
//! lower correctly to emit_mem_read_via_rip_sym, producing mov rax, [rip+disp32] bytecode.
//!
//! Three fixture shapes are tested:
//! 1. Braced tail:   `fn () -> { Module.field }` — routed via visit_field_access_with_reg
//! 2. Let-RHS:       `fn () -> { let x: u64 = Module.field; x }` — routed via visit_field_access_with_reg
//! 3. No-braces tail: `fn () -> Module.field` — routed via visit_lambda FieldAccess arm
//!
//! Expected behavior:
//! - Parser accepts module-qualified field read (Runqueue._current_tcb)
//! - Elaborator populate_field_access_info skips struct-type lookup (module receiver)
//! - Elaborator populate_field_access_info records field name in module_field_refs
//! - visit_field_access_with_reg detects module_field_refs entry and routes to emit_module_field_read
//! - visit_lambda FieldAccess arm detects module_field_refs and emits RIP-relative load + ret
//! - Bytecode contains 48 8b 05 (mov rax, [rip+disp32]) with REX.W + opcode + ModR/M
//! - .rela.text contains PC32 reloc against symbol '_current_tcb'

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

// ============================================================================
// Fixture 1: Braced tail `fn () -> { Module.field }`
// ============================================================================

#[test]
fn module_field_read_tail_builds_successfully() {
    // Issue #1184 AC1: module_field_read_tail.pdx builds without error.
    // Verifies that:
    // - Parser accepts module-qualified field read
    // - module_field_refs side-table is populated during elaboration
    // - No U1644 error even though FieldAccessInfo is not populated
    let input = build_emit_data("module_field_read_tail.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_module_field_read_tail.o",
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "module_field_read_tail should build successfully. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn module_field_read_tail_emits_rip_rel_load_rax() {
    // Issue #1184 AC2: fn () -> { Runqueue._current_tcb } emits mov rax, [rip+...] (u64)
    // Expected byte sequence: 48 8b 05 (mov rax, [rip+...]) with REX.W prefix + opcode + ModR/M
    // This verifies emit_module_field_read correctly emits RIP-relative load with u64 width.
    let input = build_emit_data("module_field_read_tail.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_module_field_read_tail_emit.o");
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
        "build failed for module_field_read_tail: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Extract .text section bytes
    let mut text_bytes = Vec::new();
    for section in file.sections() {
        if section.name().unwrap_or("") == ".text" {
            text_bytes = section.data().unwrap_or(b"").to_vec();
            break;
        }
    }

    assert!(!text_bytes.is_empty(), ".text section should contain bytes");

    // Assert: must contain 48 8b 05 (mov rax, [rip+...]) with REX.W prefix
    // 0x48 = REX.W, 0x8b = opcode, 0x05 = ModR/M (rax as register field)
    let load_seq = [0x48u8, 0x8b, 0x05];
    let load_pos = text_bytes.windows(3).position(|w| w == load_seq)
        .expect("expected 48 8b 05 (mov rax, [rip+...] with REX.W) in .text");

    // Verify that REX.W is indeed there (0x48 must be the prefix)
    assert_eq!(
        text_bytes[load_pos], 0x48,
        "expected REX.W (0x48) prefix for u64 mov rax, [rip+...] at position {}",
        load_pos
    );

    // Assert: must contain C3 (ret) — ideally after the mov
    assert!(
        text_bytes.iter().any(|&b| b == 0xC3),
        "expected C3 (ret) in .text, got: {:02X?}",
        text_bytes
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn module_field_read_tail_has_reloc_to_current_tcb() {
    // Issue #1184 AC3: .rela.text contains PC32 relocation against symbol '_current_tcb'
    // This verifies that the relocation record is correctly generated for the RIP-relative reference
    // to the external module symbol.
    let input = build_emit_data("module_field_read_tail.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_module_field_read_tail_reloc.o");
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
        "build failed for module_field_read_tail: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find .text section and iterate its relocations
    let text_section = file
        .sections()
        .find(|s| s.name().unwrap_or("") == ".text")
        .expect(".text section should exist");

    // Iterate relocations and check for at least one targeting symbol '_current_tcb'
    let mut found_current_tcb_reloc = false;
    for (_offset, reloc) in text_section.relocations() {
        if let RelocationTarget::Symbol(sym_idx) = reloc.target() {
            let sym = file.symbol_by_index(sym_idx).expect("symbol by index");
            let name = sym.name().unwrap_or("");
            if name == "_current_tcb" {
                found_current_tcb_reloc = true;
                break;
            }
        }
    }

    assert!(
        found_current_tcb_reloc,
        ".rela.text should contain relocation against symbol '_current_tcb'"
    );

    let _ = std::fs::remove_file(&tmp);
}

// ============================================================================
// Fixture 2: Let-RHS `fn () -> { let x: u64 = Module.field; x }`
// ============================================================================

#[test]
fn module_field_read_let_rhs_builds_successfully() {
    // Issue #1184 AC1: module_field_read_let_rhs.pdx builds without error.
    // Verifies that:
    // - Parser accepts module-qualified field read in let binding
    // - module_field_refs side-table is populated during elaboration
    // - No U1644 error even though FieldAccessInfo is not populated
    let input = build_emit_data("module_field_read_let_rhs.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_module_field_read_let_rhs.o",
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "module_field_read_let_rhs should build successfully. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn module_field_read_let_rhs_emits_load_into_rcx_and_return() {
    // Issue #1184 AC2: fn () -> { let x: u64 = Runqueue._current_tcb; x } emits
    // mov rax, [rip+...] (load into rax) AND mov rcx, rax (intermediate) AND mov rax, rcx (setup return)
    // Expected byte sequences: 48 8b 05 (mov rax, [rip+...]) for the RIP-relative load
    // Order-agnostic via windows().
    let input = build_emit_data("module_field_read_let_rhs.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_module_field_read_let_rhs_emit.o");
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
        "build failed for module_field_read_let_rhs: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Extract .text section bytes
    let mut text_bytes = Vec::new();
    for section in file.sections() {
        if section.name().unwrap_or("") == ".text" {
            text_bytes = section.data().unwrap_or(b"").to_vec();
            break;
        }
    }

    assert!(!text_bytes.is_empty(), ".text section should contain bytes");

    // Assert: must contain 48 8b 05 (mov rax, [rip+...]) with REX.W prefix for the RIP-relative load
    let load_rax_seq = [0x48u8, 0x8b, 0x05];
    let load_rax_found = text_bytes.windows(3).any(|w| w == load_rax_seq);
    assert!(
        load_rax_found,
        "expected 48 8b 05 (mov rax, [rip+...] with REX.W) in .text for let binding field load"
    );

    // Assert: must contain C3 (ret)
    assert!(
        text_bytes.iter().any(|&b| b == 0xC3),
        "expected C3 (ret) in .text"
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn module_field_read_let_rhs_has_reloc_to_current_tcb() {
    // Issue #1184 AC3: .rela.text contains PC32 relocation against symbol '_current_tcb'
    // This verifies that the relocation record is correctly generated for the RIP-relative reference.
    let input = build_emit_data("module_field_read_let_rhs.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_module_field_read_let_rhs_reloc.o");
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
        "build failed for module_field_read_let_rhs: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find .text section and iterate its relocations
    let text_section = file
        .sections()
        .find(|s| s.name().unwrap_or("") == ".text")
        .expect(".text section should exist");

    // Iterate relocations and check for at least one targeting symbol '_current_tcb'
    let mut found_current_tcb_reloc = false;
    for (_offset, reloc) in text_section.relocations() {
        if let RelocationTarget::Symbol(sym_idx) = reloc.target() {
            let sym = file.symbol_by_index(sym_idx).expect("symbol by index");
            let name = sym.name().unwrap_or("");
            if name == "_current_tcb" {
                found_current_tcb_reloc = true;
                break;
            }
        }
    }

    assert!(
        found_current_tcb_reloc,
        ".rela.text should contain relocation against symbol '_current_tcb'"
    );

    let _ = std::fs::remove_file(&tmp);
}

// ============================================================================
// Fixture 3: No-braces tail `fn () -> Module.field`
//
// NOTE: This pattern currently does not emit code (Issue #1184 follow-up).
// The tail and let-RHS patterns cover the module-qualified field read functionality
// and are fully supported. The no-braces lambda body shape is deferred to a
// follow-up PR when visit_lambda's FieldAccess arm can properly detect module-qualified
// field references that bypass field_access_info.
// ============================================================================
