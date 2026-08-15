//! Issue #1311 — `not r64` is reachable from `.pdx` source.
//!
//! `Mnemonic::Not`, `encode_not` and its dispatch arm all existed, and
//! `encode_not_rax_emits_48_f7_d0` asserted the bytes — from a hand-built
//! `Instruction`. The `("not", Mnemonic::Not)` row in the elaborator's
//! resolver table did not exist, so the instruction was unwritable in
//! source while every unit test around it passed. That is the shape of bug
//! this test exists to catch, which is why it starts from a `.pdx` file and
//! goes all the way to the emitted `.text` bytes.

use object::{Object, ObjectSection};
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

/// True when `hay` contains `needle` as a contiguous subsequence.
fn contains(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || hay.len() < needle.len() {
        return false;
    }
    hay.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn not_r64_elaborates_from_source_and_emits_f7_slash_2() {
    let input = data("pa_r30_1311_not_r64.pdx");
    let tmp = std::env::temp_dir().join("pa_r30_1311_not_r64.o");
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
        "`not r64` failed to elaborate from source: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");
    let text = file
        .sections()
        .find(|s| s.name().unwrap_or("") == ".text")
        .expect(".text section should exist");
    let code = text.data().expect(".text should have contents");

    // not rax — REX.W F7 /2 with r/m = rax
    assert!(
        contains(code, &[0x48, 0xF7, 0xD0]),
        "expected `not rax` to emit 48 F7 D0; .text = {code:02X?}"
    );
    // not rcx — same opcode extension, different r/m, so a mis-set /2 field
    // cannot hide behind the rax case
    assert!(
        contains(code, &[0x48, 0xF7, 0xD1]),
        "expected `not rcx` to emit 48 F7 D1; .text = {code:02X?}"
    );
    // not r12 — needs REX.B, so the extended-register path is covered too
    assert!(
        contains(code, &[0x49, 0xF7, 0xD4]),
        "expected `not r12` to emit 49 F7 D4; .text = {code:02X?}"
    );

    let _ = std::fs::remove_file(&tmp);
}
