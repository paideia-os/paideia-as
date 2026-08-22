//! paideia-as#1301 (v0.21-003c, phase-2): fixture guard for the elaborator
//! emit change that brackets accesses to a module-level `@atomic(SeqCst)`
//! binding with `mfence` (0F AE F0).
//!
//! Compiles `crates/paideia-stdlib/pdx/atomic_binding_smoke.pdx` (which
//! defines `pub let mut counter : u64 = 0 @atomic(SeqCst)` plus a
//! store-side and load-side accessor) and asserts:
//!
//!   1. `counter` lands in `.data` (mutable initialized) with 8-byte size.
//!   2. The emitted `.text` contains at least two `mfence` occurrences
//!      (`0F AE F0`) — one for the load-side pre-fence, one for the
//!      store-side post-fence.
//!   3. On the load side, the `mfence` bytes precede the `mov rax, [rip+…]`
//!      opcode bytes (`48 8B 05`); on the store side, the `mfence` bytes
//!      follow the `mov [rip+…], rdi` opcode bytes (`48 89 3D`) —
//!      matching the byte-shape recipe pinned in
//!      `crates/paideia-as-encoder/tests/memory_ops/atomic_ordering.rs`.
//!
//! Regression pin: if any future refactor decouples the two rip-relative
//! memory-op emit sites from the `atomic_bindings` lookup, these
//! assertions catch the miss before paideia-os spinlock/refcount
//! migration breaks.

use object::{Object, ObjectSection, ObjectSymbol};
use std::path::PathBuf;
use std::process::Command;

fn stdlib_pdx(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../paideia-stdlib/pdx");
    p.push(name);
    p
}

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

fn compile_pdx(input_path: &str, tmp_name: &str) -> Vec<u8> {
    let tmp = std::env::temp_dir().join(tmp_name);
    let _ = std::fs::remove_file(&tmp);

    let out = cargo_run(&[
        "build",
        input_path,
        "--emit",
        "elf64",
        "-o",
        tmp.to_str().unwrap(),
    ]);

    assert!(
        out.status.success(),
        "build --emit elf64 failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    std::fs::read(&tmp).expect("output ELF should exist")
}

fn text_bytes(elf_bytes: &[u8]) -> Vec<u8> {
    let elf = object::File::parse(elf_bytes).expect("valid ELF64");
    for section in elf.sections() {
        if section.name().unwrap_or("") == ".text" {
            return section.data().unwrap_or(&[]).to_vec();
        }
    }
    panic!(".text section missing from emitted ELF");
}

fn count_subsequence(hay: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() || hay.len() < needle.len() {
        return 0;
    }
    let mut n = 0;
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if &hay[i..i + needle.len()] == needle {
            n += 1;
            i += needle.len();
        } else {
            i += 1;
        }
    }
    n
}

fn assert_symbol_in_section(elf_bytes: &[u8], symbol_name: &str, expected_section: &str) {
    let elf = object::File::parse(elf_bytes).expect("valid ELF64");
    let symbols: Vec<_> = elf.symbols().collect();
    let sym = symbols
        .iter()
        .find(|s| s.name().unwrap_or("") == symbol_name)
        .unwrap_or_else(|| panic!("symbol '{}' should exist", symbol_name));
    let idx = sym
        .section_index()
        .unwrap_or_else(|| panic!("symbol '{}' should have section index", symbol_name));
    let section = elf
        .section_by_index(idx)
        .unwrap_or_else(|_| panic!("section index {} should exist", idx.0));
    let name = section.name().unwrap_or("<unnamed>");
    assert_eq!(
        name, expected_section,
        "symbol '{}' should live in '{}' section, but is in '{}'",
        symbol_name, expected_section, name
    );
}

const MFENCE: &[u8] = &[0x0F, 0xAE, 0xF0];
// mov rax, [rip+disp32]  (SeqCst-load RIP-relative recipe)
const LOAD_OPCODE: &[u8] = &[0x48, 0x8B, 0x05];
// mov [rip+disp32], rdi  (SeqCst-store RIP-relative recipe)
const STORE_OPCODE: &[u8] = &[0x48, 0x89, 0x3D];

#[test]
fn atomic_binding_smoke_counter_lives_in_data() {
    // `pub let mut counter : u64 = 0 @atomic(SeqCst)` is a mutable initialized
    // scalar and must live in `.data` — same routing as `pa_r13_008_mut_scalar`.
    let input = stdlib_pdx("atomic_binding_smoke.pdx");
    let elf = compile_pdx(input.to_str().unwrap(), "paideia_as_atomic_binding_smoke_data.o");
    assert_symbol_in_section(&elf, "counter", ".data");
}

#[test]
fn atomic_binding_smoke_text_contains_mfence_pair() {
    // Expect at least two mfence occurrences: pre-load fence in `load_it`
    // and post-store fence in `store_it`.
    let input = stdlib_pdx("atomic_binding_smoke.pdx");
    let elf = compile_pdx(input.to_str().unwrap(), "paideia_as_atomic_binding_smoke_text.o");
    let text = text_bytes(&elf);
    let n = count_subsequence(&text, MFENCE);
    assert!(
        n >= 2,
        "expected at least 2 mfence occurrences in .text (load-pre + store-post); got {}. .text = {:02X?}",
        n,
        text
    );
}

#[test]
fn atomic_binding_smoke_load_side_has_mfence_before_mov() {
    // The load-side recipe is: MFENCE ; mov rax, [rip+counter]
    // → assert that at least one occurrence of `48 8B 05` is directly
    // preceded by `0F AE F0` in the emitted `.text`.
    let input = stdlib_pdx("atomic_binding_smoke.pdx");
    let elf = compile_pdx(input.to_str().unwrap(), "paideia_as_atomic_binding_smoke_load.o");
    let text = text_bytes(&elf);
    let mut found = false;
    let win = MFENCE.len() + LOAD_OPCODE.len();
    if text.len() >= win {
        for i in 0..=text.len() - win {
            if &text[i..i + MFENCE.len()] == MFENCE
                && &text[i + MFENCE.len()..i + win] == LOAD_OPCODE
            {
                found = true;
                break;
            }
        }
    }
    assert!(
        found,
        "expected MFENCE immediately followed by `mov rax, [rip+...]` (48 8B 05) in .text; \
         got {:02X?}",
        text
    );
}

#[test]
fn atomic_binding_smoke_store_side_has_mfence_after_mov() {
    // The store-side recipe is: mov [rip+counter], rdi ; MFENCE
    // → assert that at least one occurrence of the 7-byte store opcode
    // `48 89 3D <disp32>` is directly followed by `0F AE F0` in .text.
    let input = stdlib_pdx("atomic_binding_smoke.pdx");
    let elf = compile_pdx(input.to_str().unwrap(), "paideia_as_atomic_binding_smoke_store.o");
    let text = text_bytes(&elf);
    // The store opcode is 3 bytes then 4 bytes of displacement, then mfence
    // (3 bytes). Scan for STORE_OPCODE, skip disp32, verify MFENCE follows.
    let store_len = STORE_OPCODE.len() + 4;
    let win = store_len + MFENCE.len();
    let mut found = false;
    if text.len() >= win {
        for i in 0..=text.len() - win {
            if &text[i..i + STORE_OPCODE.len()] == STORE_OPCODE
                && &text[i + store_len..i + win] == MFENCE
            {
                found = true;
                break;
            }
        }
    }
    assert!(
        found,
        "expected `mov [rip+...], rdi` (48 89 3D + disp32) immediately followed by MFENCE in .text; \
         got {:02X?}",
        text
    );
}
