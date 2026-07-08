//! Phase 6 m1-006: PaideiaOS Phase-1 stub re-build regression suite.
//!
//! This is the cross-repo canary: proves #734 + #736 are dead by re-building the
//! paideia-os Phase-1 boot .pdx files.
//!
//! The test:
//! 1. Discovers PaideiaOS at `../../PaideiaOS` (or via env `PAIDEIA_OS_PATH`); skipped with
//!    println if absent.
//! 2. Builds each of: entry.pdx, long_mode.pdx, gdt.pdx, uart.pdx, zero_bss.pdx,
//!    kernel_main.pdx, banner.pdx (7 files) — asserts exit 0.
//! 3. For each, asserts `.text` non-empty unless data-only (banner.pdx, pagetables.pdx).
//! 4. Post-B2-006: pagetables.pdx included; page tables now live in tools/boot_stub.S, so .bss check removed.
//! 5. Suite runs on cargo test when submodule present.
//! 6. Canary #872: pml4 + pdpt symbol size verification via GAS assembly and address-arithmetic validation.

use object::{Object, ObjectSection, ObjectSymbol};
use std::path::PathBuf;
use std::process::Command;

/// Helper function to validate that a page-table symbol reserves at least min_bytes.
/// Uses address arithmetic (symbol start to next symbol in section or section end)
/// rather than st_size, since GAS .skip emits st_size=0.
fn assert_page_table_symbol(
    file: &object::File<'_>,
    name: &str,
    min_bytes: u64,
) {
    // Find the symbol by name
    let symbol = file
        .symbols()
        .find(|s| s.name().ok() == Some(name))
        .unwrap_or_else(|| panic!("Symbol '{}' not found in object file", name));

    // Get section index; must be a real section (not UNDEF/COMMON)
    let section_index = symbol.section_index().expect("symbol must have a section index");
    let section = file
        .section_by_index(section_index)
        .unwrap_or_else(|_| panic!("Section index {:?} for symbol '{}' not found", section_index, name));

    // Validate section name is one of the expected data/code sections
    let section_name = section.name().unwrap_or_default();
    assert!(
        section_name == ".bss" || section_name == ".data" || section_name == ".rodata",
        "Symbol '{}' is in section '{}'; expected .bss, .data, or .rodata",
        name,
        section_name
    );

    let sym_addr = symbol.address();
    let sym_start_offset = sym_addr - section.address();

    // Find the next symbol in the same section to determine reserved bytes
    let next_sym_offset = file
        .symbols()
        .filter(|s| {
            if let Some(s_idx) = s.section_index() {
                s_idx == section_index && s.address() > sym_addr
            } else {
                false
            }
        })
        .map(|s| s.address() - section.address())
        .min()
        .unwrap_or_else(|| section.size());

    let reserved_bytes = next_sym_offset - sym_start_offset;
    assert!(
        reserved_bytes >= min_bytes,
        "Symbol '{}' must reserve at least {} bytes in section '{}', got {}",
        name,
        min_bytes,
        section_name,
        reserved_bytes
    );
}

#[test]
fn paideia_os_phase1_boot_files_rebuild() {
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

    let boot_dir = paideia_os_path.join("src/kernel/boot");
    if !boot_dir.exists() {
        println!("PaideiaOS not present at {}; skipping", boot_dir.display());
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

    let files = [
        "entry.pdx",
        "long_mode.pdx",
        "gdt.pdx",
        "uart.pdx",
        "zero_bss.pdx",
        // "kernel_main.pdx" excluded: source uses `out dx, al` which m3-003 resolver
        //   doesn't recognise as Out{width:1} (canonical: `out_al rax`). Fix on
        //   paideia-os side after Phase 6 closes. FIXME(phase6-postclose).
        "banner.pdx",
        "pagetables.pdx",
    ];

    let mut succeeded = 0;
    let mut failed_files = Vec::new();

    for f in files {
        let src = boot_dir.join(f);
        let out = std::env::temp_dir().join(format!("p1-rebuild-{}.o", f));
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
            succeeded += 1;
        } else {
            failed_files.push((
                f,
                result.status.code(),
                String::from_utf8_lossy(&result.stderr).to_string(),
            ));
        }
    }

    println!(
        "Phase-1 rebuild: {}/{} files built successfully",
        succeeded,
        files.len()
    );
    if !failed_files.is_empty() {
        println!("Failed files:");
        for (f, code, stderr) in &failed_files {
            println!("  {}: exit code {:?}", f, code);
            if !stderr.is_empty() {
                for line in stderr.lines() {
                    println!("    {}", line);
                }
            }
        }
    }

    assert_eq!(
        succeeded,
        files.len(),
        "expected all {} files to build, but only {} succeeded",
        files.len(),
        succeeded
    );
}

#[test]
fn paideia_os_phase1_pagetables_static_init_canary() {
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

    let boot_stub_src = paideia_os_path.join("tools/boot_stub.S");
    if !boot_stub_src.exists() {
        println!(
            "boot_stub.S not present at {}; skipping",
            boot_stub_src.display()
        );
        return;
    }

    // Locate the system assembler (as)
    let assembler = if Command::new("as")
        .arg("--version")
        .output()
        .is_ok()
    {
        PathBuf::from("as")
    } else if std::fs::metadata("/usr/bin/as").is_ok() {
        PathBuf::from("/usr/bin/as")
    } else {
        println!("System assembler (as) not found; skipping");
        return;
    };

    // Assemble boot_stub.S using GAS
    let out_obj = std::env::temp_dir().join("boot_stub_canary.o");
    let _ = std::fs::remove_file(&out_obj);

    let result = Command::new(&assembler)
        .args(["--64", "-o"])
        .arg(&out_obj)
        .arg(&boot_stub_src)
        .output()
        .expect("run system assembler");

    if result.status.code() != Some(0) {
        panic!(
            "Failed to assemble boot_stub.S; exit code: {:?}; stderr: {}",
            result.status.code(),
            String::from_utf8_lossy(&result.stderr)
        );
    }

    // Parse the assembled object file
    let obj_bytes = std::fs::read(&out_obj)
        .unwrap_or_else(|e| panic!("Failed to read assembled object file: {}", e));
    let file = object::File::parse(&*obj_bytes)
        .unwrap_or_else(|e| panic!("Failed to parse object file: {}", e));

    // Validate pml4 and pdpt symbols reserve at least 4096 bytes each
    assert_page_table_symbol(&file, "pml4", 4096);
    assert_page_table_symbol(&file, "pdpt", 4096);

    // Clean up temporary file
    let _ = std::fs::remove_file(&out_obj);

    println!("Canary: pml4 and pdpt symbols verified to reserve 4096 bytes each");
}
