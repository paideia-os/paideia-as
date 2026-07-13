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
    let f_bytes = crate::common::elf::symbol_bytes(&bytes, "f")
        .expect("symbol 'f' should exist in .text");

    assert_eq!(
        f_bytes.len(),
        8,
        "f must be exactly 8 bytes (load + ret); got {} bytes: {:02X?}",
        f_bytes.len(),
        f_bytes
    );
    assert_eq!(
        &f_bytes[..3],
        &[0x48u8, 0x8b, 0x05],
        "f must begin with 48 8b 05 (mov rax, [rip+disp32]); got {:02X?}",
        f_bytes
    );
    assert_eq!(
        f_bytes[7], 0xC3,
        "f must end with C3 (ret); got {:02X?}",
        f_bytes
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
    // mov rcx, [rip+...] (load into rcx via scratch) AND mov rax, rcx (setup return)
    // Expected byte sequences: 48 8b 0d (mov rcx, [rip+...]) for the RIP-relative load,
    // then 48 89 c8 c3 (mov rax, rcx; ret) to return the value.
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
    let g_bytes = crate::common::elf::symbol_bytes(&bytes, "g")
        .expect("symbol 'g' should exist in .text");

    assert_eq!(
        g_bytes.len(),
        11,
        "g must be exactly 11 bytes; got {} bytes: {:02X?}",
        g_bytes.len(),
        g_bytes
    );
    assert_eq!(
        &g_bytes[..3],
        &[0x48u8, 0x8b, 0x0d],
        "g must begin with 48 8b 0d (mov rcx, [rip+disp32]); got {:02X?}",
        g_bytes
    );
    assert_eq!(
        &g_bytes[7..11],
        &[0x48u8, 0x89, 0xc8, 0xc3],
        "g bytes [7..11) must be 48 89 c8 c3 (mov rax, rcx; ret); got {:02X?}",
        &g_bytes[7..11]
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
// Issue #1184-corr: The no-braces lambda body shape now correctly emits code via
// visit_lambda's FieldAccess arm, which has been restructured to test module_field_refs
// first (before the struct-typed guard that would always fail for module receivers).
// ============================================================================

#[test]
fn module_field_read_no_braces_builds_successfully() {
    // Issue #1184-corr AC1: module_field_read_no_braces.pdx builds without error.
    // Verifies that:
    // - Parser accepts module-qualified field read in lambda no-braces tail
    // - module_field_refs side-table is populated during elaboration
    // - No U1644 error even though FieldAccessInfo is not populated
    let input = build_emit_data("module_field_read_no_braces.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_module_field_read_no_braces.o",
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "module_field_read_no_braces should build successfully. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn module_field_read_no_braces_emits_rip_rel_load_rax() {
    // Issue #1184-corr AC2: fn () -> Runqueue._current_tcb emits mov rax, [rip+...] (u64)
    // Expected byte sequence: 48 8b 05 (mov rax, [rip+...]) with REX.W prefix + opcode + ModR/M
    // This verifies that visit_lambda's restructured FieldAccess arm correctly routes
    // no-braces module field reads to emit_module_field_read.
    let input = build_emit_data("module_field_read_no_braces.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_module_field_read_no_braces_emit.o");
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
        "build failed for module_field_read_no_braces: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");

    // Extract function h's bytes via symbol lookup (h is the no-braces lambda in the fixture)
    let h_bytes = crate::common::elf::symbol_bytes(&bytes, "h")
        .expect("symbol 'h' should exist in .text");

    // Assert h contains mov rax, [rip+disp32] — should be 3 bytes of the instruction prefix
    let load_rax_seq = [0x48u8, 0x8b, 0x05];
    assert!(
        h_bytes.windows(3).any(|w| w == load_rax_seq),
        "h should contain 48 8b 05 (mov rax, [rip+...]) instruction, got: {:02X?}",
        h_bytes
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn module_field_read_no_braces_has_reloc_to_current_tcb() {
    // Issue #1184-corr AC3: .rela.text contains PC32 relocation against symbol '_current_tcb'
    // This verifies that the no-braces lambda correctly generates a relocation record
    // for the RIP-relative reference to the external module symbol.
    let input = build_emit_data("module_field_read_no_braces.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_module_field_read_no_braces_reloc.o");
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
        "build failed for module_field_read_no_braces: {}",
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
        ".rela.text should contain relocation against symbol '_current_tcb' for no-braces lambda"
    );

    let _ = std::fs::remove_file(&tmp);
}

// ============================================================================
// Fixture 4: Write-then-read tail `fn (v: u64) -> { M.f = v; M.f }`
//
// Issue #1187: Combined module-qualified field write + read exercises both
// emit_module_field_write and emit_module_field_read within the same function.
// Verifies that both paths correctly place their instructions inside the
// function symbol range.
// ============================================================================

#[test]
fn module_field_write_then_read_tail_emits_write_then_load_and_ret() {
    let input = build_emit_data("module_field_write_then_read_tail.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_1187_write_then_read.o");
    let _ = std::fs::remove_file(&tmp);
    let out = cargo_run(&[
        "build", input.to_str().unwrap(), "--emit", "elf64",
        "-o", tmp.to_str().unwrap(),
    ]);
    assert!(out.status.success(),
        "build failed: {}", String::from_utf8_lossy(&out.stderr));

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let f_bytes = crate::common::elf::symbol_bytes(&bytes, "f")
        .expect("symbol 'f' should exist in .text");

    assert_eq!(
        f_bytes.len(), 15,
        "f must be exactly 15 bytes (store + load + ret); got {} bytes: {:02X?}",
        f_bytes.len(), f_bytes
    );
    assert_eq!(&f_bytes[..3], &[0x48u8, 0x89, 0x3d],
        "f[..3] must be 48 89 3d; got {:02X?}", f_bytes);
    assert_eq!(&f_bytes[7..10], &[0x48u8, 0x8b, 0x05],
        "f[7..10] must be 48 8b 05; got {:02X?}", f_bytes);
    assert_eq!(f_bytes[14], 0xC3,
        "f[14] must be C3; got {:02X?}", f_bytes);

    let _ = std::fs::remove_file(&tmp);
}
