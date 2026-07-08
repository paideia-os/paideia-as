//! PA10-013: Placeholder stub no-collision regression guard.
//!
//! Tests that placeholder stubs (let-fn bindings) are emitted as STB_LOCAL,
//! not STB_GLOBAL, so they don't collide with hand-written .globl definitions.
//!
//! Fixture: `let uart_puts = fn(_: ()) -> nop` (module-level placeholder stub).
//! Expected: uart_puts symbol in .o has STB_LOCAL binding.
//! Cross-file test: link .o with partner .S that has `.globl uart_puts` + real body.
//! Expect: link succeeds without "multiple definition" error.

use object::{Object, ObjectSymbol};
use std::env;
use std::process::Command;

#[test]
#[cfg(target_os = "linux")]
fn pa10_013_placeholder_stub_remains_local() {
    let temp_dir = env::temp_dir().join("pa10_013_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    // Fixture: Placeholder stub at module top-level.
    // This is the exact scenario from kernel_main.pdx that broke paideia_os_r1_5_r2_5_rebuild.
    let source = "module Pa10013 = structure { let uart_puts : (() -> ()) = fn (_: ()) -> nop ; let _start : (() -> ()) = fn () -> nop }";

    let source_file = temp_dir.join("Pa10013.pdx");
    std::fs::write(&source_file, source).expect("write source file");

    // Build the source to object file using paideia-as.
    let output_obj = temp_dir.join("Pa10013.o");
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run")
        .arg("--quiet")
        .arg("--")
        .arg("build")
        .arg(source_file.to_str().unwrap())
        .arg("--emit")
        .arg("elf64")
        .arg("-o")
        .arg(output_obj.to_str().unwrap());
    cmd.env("NO_COLOR", "1");
    let out = cmd.output().expect("paideia-as build");

    assert!(
        out.status.success(),
        "paideia-as build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(output_obj.exists(), "output .o not created");

    // Parse the object file.
    let obj_data = std::fs::read(&output_obj).expect("read .o");
    let obj = object::File::parse(&*obj_data).expect("parse .o");

    // Check that uart_puts is LOCAL and _start is GLOBAL.
    let mut uart_puts_binding = None;
    let mut start_binding = None;

    for symbol in obj.symbols() {
        if let Ok(name) = symbol.name() {
            if name == "uart_puts" {
                uart_puts_binding = Some(symbol);
            } else if name == "_start" {
                start_binding = Some(symbol);
            }
        }
    }

    let uart_puts_sym = uart_puts_binding.expect("symbol 'uart_puts' not found in object");

    // uart_puts must be defined (not undefined).
    assert!(
        !uart_puts_sym.is_undefined(),
        "symbol 'uart_puts' should be defined (not undefined)"
    );

    // uart_puts must be LOCAL (st_info >> 4 == 0).
    // The object crate provides scope() to check visibility.
    let uart_puts_scope = uart_puts_sym.scope();
    assert_eq!(
        uart_puts_scope,
        object::SymbolScope::Compilation,
        "uart_puts should have STB_LOCAL binding (SymbolScope::Compilation); got {:?}",
        uart_puts_scope
    );

    // _start, if present, must remain GLOBAL.
    if let Some(start_sym) = start_binding {
        assert!(
            !start_sym.is_undefined(),
            "symbol '_start' should be defined"
        );
        let start_scope = start_sym.scope();
        assert_eq!(
            start_scope,
            object::SymbolScope::Dynamic,
            "_start should have STB_GLOBAL binding (SymbolScope::Dynamic); got {:?}",
            start_scope
        );
    }

    // Cross-file test: link the .o with a partner .S that has .globl uart_puts.
    // This verifies that uart_puts being LOCAL doesn't collide.
    let partner_asm = temp_dir.join("partner.S");
    let partner_asm_text = r#"
.globl uart_puts
.type uart_puts, @function
uart_puts:
    mov $0, %rax
    ret
"#;
    std::fs::write(&partner_asm, partner_asm_text).expect("write partner.S");

    // Assemble partner.S to partner.o
    let partner_obj = temp_dir.join("partner.o");
    let assemble_cmd = Command::new("as")
        .arg(partner_asm.to_str().unwrap())
        .arg("-o")
        .arg(partner_obj.to_str().unwrap())
        .output()
        .expect("run assembler");

    if !assemble_cmd.status.success() {
        eprintln!(
            "assembler failed: {}",
            String::from_utf8_lossy(&assemble_cmd.stderr)
        );
        panic!("assembler failed");
    }

    // Link TestStub.o + partner.o using ld -r (partial linking).
    // Expect: success with no "multiple definition of uart_puts" error.
    let linked = temp_dir.join("linked.o");
    let link_cmd = Command::new("ld")
        .arg("-r")
        .arg("-o")
        .arg(linked.to_str().unwrap())
        .arg(output_obj.to_str().unwrap())
        .arg(partner_obj.to_str().unwrap())
        .output()
        .expect("run linker");

    if !link_cmd.status.success() {
        eprintln!(
            "linker failed: {}",
            String::from_utf8_lossy(&link_cmd.stderr)
        );
        panic!(
            "linker failed: {}",
            String::from_utf8_lossy(&link_cmd.stderr)
        );
    }

    // Verify the linked object has uart_puts GLOBAL (promoted from LOCAL during linking).
    let linked_obj = std::fs::read(&linked).expect("read linked.o");
    let linked_file = object::File::parse(&*linked_obj).expect("parse linked.o");

    let mut linked_uart_puts = None;
    for symbol in linked_file.symbols() {
        if let Ok(name) = symbol.name() {
            if name == "uart_puts" {
                linked_uart_puts = Some(symbol);
            }
        }
    }

    let linked_sym = linked_uart_puts.expect("uart_puts not found in linked.o");
    assert!(
        !linked_sym.is_undefined(),
        "uart_puts in linked.o should be defined"
    );

    // Cleanup.
    let _ = std::fs::remove_dir_all(&temp_dir);
}
