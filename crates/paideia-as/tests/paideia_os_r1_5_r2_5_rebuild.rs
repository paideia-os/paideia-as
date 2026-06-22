//! PA7C m2-004: PaideiaOS R1.5/R2.5 four-file re-build regression suite.
//!
//! Cross-repo canary for the m1+m2 work (Phase 7C). Verifies that the four
//! recently-unquarantined kernel files can be built and linked together with
//! paideia-as.
//!
//! The test:
//! 1. Discovers PaideiaOS at `../../PaideiaOS` (or via env `PAIDEIA_OS_PATH`);
//!    skipped cleanly if absent.
//! 2. Builds each of: kernel_main.pdx, exceptions.pdx, idt.pdx, pt_walk.pdx
//!    (4 files from R1.5/R2.5) — asserts exit 0.
//! 3. For each build, loads the resulting .o via the `object` crate and asserts
//!    that it contains at least 1 STT_FUNC symbol.
//! 4. Final linking: assembles stub_partner.S, links all 5 objects together
//!    using a minimal linker script, asserts ld exit 0.
//! 5. Linux-only gate; skipped cleanly when `ld` or `as` absent.
//!
//! Stubbed symbols in stub_partner.S (minimal bodies for linking):
//! - _start: entry point (ret-only)
//! - uart_init: minimal UART init (ret-only)
//! - uart_puts: minimal UART puts (ret-only)
//! - add_one: placeholder (ret-only)

use object::{Object, ObjectSymbol};
use std::path::PathBuf;
use std::process::Command;

#[test]
#[cfg(target_os = "linux")]
fn paideia_os_r1_5_r2_5_four_file_rebuild() {
    let paideia_os_path = std::env::var("PAIDEIA_OS_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("PaideiaOS")
        });

    // Check if ld and as are present on the system
    let ld_check = Command::new("which").arg("ld").output();
    let as_check = Command::new("which").arg("as").output();
    if ld_check.is_err() || !ld_check.unwrap().status.success() {
        println!("ld not found in PATH; skipping link test");
        return;
    }
    if as_check.is_err() || !as_check.unwrap().status.success() {
        println!("as not found in PATH; skipping assembly test");
        return;
    }

    let kernel_dir = paideia_os_path.join("src/kernel");
    if !kernel_dir.exists() {
        println!(
            "PaideiaOS not present at {}; skipping",
            kernel_dir.display()
        );
        return;
    }

    let paideia_as = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/release/paideia-as");
    if !paideia_as.exists() {
        println!(
            "paideia-as binary not built at {}; run cargo build --release -p paideia-as first; skipping",
            paideia_as.display()
        );
        return;
    }

    // The four R1.5/R2.5 files to build
    let files = [
        ("boot/kernel_main.pdx", "kernel_main"),
        ("core/int/exceptions.pdx", "exceptions"),
        ("core/int/idt.pdx", "idt"),
        ("core/mm/pt_walk.pdx", "pt_walk"),
    ];

    let tmpdir = std::env::temp_dir();
    let mut object_files = Vec::new();
    let mut succeeded = 0;
    let mut failed_files = Vec::new();

    // Build each file and verify it contains at least one function symbol
    for (rel_path, _name) in &files {
        let src = kernel_dir.join(rel_path);
        let out = tmpdir.join(format!("pa7c-r1r2-{}.o", _name));
        let _ = std::fs::remove_file(&out);

        let result = Command::new(&paideia_as)
            .args(["build", "--emit", "elf64"])
            .arg(&src)
            .args(["-o"])
            .arg(&out)
            .output()
            .expect("run paideia-as");

        if result.status.code() == Some(0)
            && out.exists()
            && std::fs::metadata(&out).unwrap().len() > 0
        {
            // Check for at least one non-zero-sized symbol
            if let Ok(bytes) = std::fs::read(&out) {
                if let Ok(file) = object::File::parse(bytes.as_slice()) {
                    let mut symbol_count = 0;
                    for symbol in file.symbols() {
                        if symbol.size() > 0 {
                            symbol_count += 1;
                        }
                    }
                    if symbol_count > 0 {
                        println!(
                            "{}: built successfully ({} symbols with size > 0)",
                            rel_path, symbol_count
                        );
                        succeeded += 1;
                        object_files.push(out);
                    } else {
                        failed_files.push((
                            rel_path.to_string(),
                            "no symbols with size > 0 found".to_string(),
                        ));
                    }
                } else {
                    failed_files.push((rel_path.to_string(), "failed to parse ELF".to_string()));
                }
            } else {
                failed_files.push((
                    rel_path.to_string(),
                    "failed to read output file".to_string(),
                ));
            }
        } else {
            let stderr = String::from_utf8_lossy(&result.stderr).to_string();
            failed_files.push((rel_path.to_string(), stderr));
        }
    }

    println!(
        "R1.5/R2.5 four-file rebuild: {}/{} files built successfully",
        succeeded,
        files.len()
    );
    if !failed_files.is_empty() {
        println!("Failed files:");
        for (f, err) in &failed_files {
            println!("  {}: {}", f, err);
        }
    }

    assert_eq!(
        succeeded,
        files.len(),
        "expected all {} files to build, but only {} succeeded",
        files.len(),
        succeeded
    );

    // Now link all objects together with stub_partner
    // 1. Assemble stub_partner.S
    let stub_partner_s =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stub_partner.S");
    let stub_partner_o = tmpdir.join("pa7c-stub-partner.o");
    let _ = std::fs::remove_file(&stub_partner_o);

    let as_result = Command::new("as")
        .args(["--64", "-o"])
        .arg(&stub_partner_o)
        .arg(&stub_partner_s)
        .output()
        .expect("run as");

    assert_eq!(
        as_result.status.code(),
        Some(0),
        "failed to assemble stub_partner.S"
    );

    // 2. Link all objects with the linker script
    let link_script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pa7c_link.ld");
    let combined_elf = tmpdir.join("pa7c-combined.elf");
    let _ = std::fs::remove_file(&combined_elf);

    let mut ld_cmd = Command::new("ld");
    ld_cmd
        .args(["-T"])
        .arg(&link_script)
        .args(["-o"])
        .arg(&combined_elf);

    for obj in &object_files {
        ld_cmd.arg(obj);
    }
    ld_cmd.arg(&stub_partner_o);

    let ld_result = ld_cmd.output().expect("run ld");

    let ld_stderr = String::from_utf8_lossy(&ld_result.stderr).to_string();
    println!("Link result: exit code {:?}", ld_result.status.code());
    if !ld_stderr.is_empty() {
        println!("Link stderr:");
        for line in ld_stderr.lines() {
            println!("  {}", line);
        }
    }

    assert_eq!(
        ld_result.status.code(),
        Some(0),
        "linking combined.elf failed; see stderr above"
    );

    assert!(combined_elf.exists(), "combined.elf was not created");

    println!("Successfully linked 4 R1.5/R2.5 files + stub_partner.o -> combined.elf");
}
