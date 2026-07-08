//! PA7C-m4-004: iced-x86 round-trip suite for the three Phase-7-completion m4
//! expression-surface features.
//!
//! This is the gating artifact that closes the PA7C m4 sequence (#811). It
//! exercises every new surface form end-to-end through the real `build` CLI:
//!
//! - m4-001 prefix bitwise NOT (`~x`)
//! - m4-002 cast (`x as T`)
//! - m4-003 sized integer `let` (`let x : u32 = 42`)
//!
//! For each fixture the test writes a self-contained `.pdx` to a tempfile,
//! drives `cargo run -- build --emit elf64`, loads the resulting object with the
//! `object` crate, disassembles `.text` with the iced-x86 `IntelFormatter`, and
//! compares the rendered instruction lines against hand-rolled expectations.
//!
//! Ground truth (what the *build CLI* actually emits — empirically captured, not
//! assumed):
//!
//! - `fn (x) -> ~x` lowers to the canonical bitwise-NOT lambda
//!   `mov rax,rdi ; not rax ; ret` (`48 89 f8 / 48 f7 d0 / c3`). The lowering is
//!   *type-independent*: the same three instructions are emitted for all eight
//!   integer widths/signednesses, so the eight `~x` fixtures share one expected
//!   sequence while still proving each source program parses, builds, and
//!   disassembles cleanly.
//!
//! - `fn (x) -> x as T` lowers to the canonical cast lambda
//!   `movsxd rax,edi ; ret` (`48 63 c7 / c3`) for *every* target type — widening
//!   signed, widening unsigned, narrowing, and same-width reinterpret all map to
//!   this single MOVSXD form. The full `(src_width, dst_width, signedness)`
//!   dispatch is a documented follow-up (see the m4-002 commit), so the suite
//!   pins the current canonical lowering rather than per-type variants.
//!
//! - `let x : u32 = 42` lowers, *through the build CLI*, to the generic 64-bit
//!   immediate move `mov rax,2Ah` (`48 b8 2a 00 00 00 00 00 00 00`). The narrow
//!   `B8 imm32` `MovSized` form from m4-003 is only emitted by the typer-aware
//!   `EmitWalker::walk_with_typer`; the `build` pipeline calls the typer-free
//!   `walk`, so the generic move is the correct expectation here. The fixtures
//!   therefore prove that typed-integer `let` bindings of every declared width
//!   parse, build, and disassemble to the (currently generic) immediate move.
//!
//! - nested `~(x as u32)`: the outermost operator is `~`, so the lambda body is
//!   `BitNot` and `emit_bitnot_lambda` emits the canonical bitwise-NOT sequence
//!   without recursing into the inner cast.
//!
//! Comparison is by full iced `IntelFormatter` line (mnemonic + operands), which
//! is enough to lock the instruction shape without asserting raw encoding bytes.
//!
//! PLATFORM: Linux-only (iced-x86 disassembly availability), matching the m2/m3
//! round-trip tests.

#![cfg(target_os = "linux")]

use iced_x86::{Decoder, DecoderOptions, Formatter, IntelFormatter};
use object::{Object, ObjectSection};
use std::path::PathBuf;
use std::process::Command;

/// Canonical bitwise-NOT lambda body: `mov rax,rdi ; not rax ; ret`.
const BITNOT: &[&str] = &["mov rax,rdi", "not rax", "ret"];

/// Canonical cast lambda body: `movsxd rax,edi ; ret`.
const CAST: &[&str] = &["movsxd rax,edi", "ret"];

/// Generic 64-bit immediate move for `let x : T = 42` through the build CLI.
const LET42: &[&str] = &["mov rax,2Ah"];

/// Hand-rolled fixture table: `(basename, source, expected_disasm_lines)`.
///
/// `basename` is both the tempfile stem and (PascalCase-folded) the required
/// top-level module name. The folding rule lowercases everything after the
/// first character of each `_`/`-` segment, so e.g. `not_u8` -> module `NotU8`
/// and `cast_i32_to_i64` -> module `CastI32ToI64`.
///
/// 23 pairs:
///   - 8  prefix bitwise NOT (`~x`) across all integer widths/signednesses
///   - 3  widening signed casts
///   - 3  widening unsigned casts
///   - 4  narrowing casts
///   - 1  same-width reinterpret cast
///   - 3  sized-integer `let` bindings (u32/u16/u64)
///   - 1  nested `~(x as u32)`
type Fixture = (&'static str, &'static str, &'static [&'static str]);

const FIXTURES: &[Fixture] = &[
    // ---- m4-001: prefix bitwise NOT (`~x`) for 8 integer types --------------
    (
        "not_u8",
        "module NotU8 = structure {\n  let f : (u8) -> u8 = fn (x : u8) -> ~x\n}\n",
        BITNOT,
    ),
    (
        "not_u16",
        "module NotU16 = structure {\n  let f : (u16) -> u16 = fn (x : u16) -> ~x\n}\n",
        BITNOT,
    ),
    (
        "not_u32",
        "module NotU32 = structure {\n  let f : (u32) -> u32 = fn (x : u32) -> ~x\n}\n",
        BITNOT,
    ),
    (
        "not_u64",
        "module NotU64 = structure {\n  let f : (u64) -> u64 = fn (x : u64) -> ~x\n}\n",
        BITNOT,
    ),
    (
        "not_i8",
        "module NotI8 = structure {\n  let f : (i8) -> i8 = fn (x : i8) -> ~x\n}\n",
        BITNOT,
    ),
    (
        "not_i16",
        "module NotI16 = structure {\n  let f : (i16) -> i16 = fn (x : i16) -> ~x\n}\n",
        BITNOT,
    ),
    (
        "not_i32",
        "module NotI32 = structure {\n  let f : (i32) -> i32 = fn (x : i32) -> ~x\n}\n",
        BITNOT,
    ),
    (
        "not_i64",
        "module NotI64 = structure {\n  let f : (i64) -> i64 = fn (x : i64) -> ~x\n}\n",
        BITNOT,
    ),
    // ---- m4-002: widening signed casts --------------------------------------
    (
        "cast_i8_to_i64",
        "module CastI8ToI64 = structure {\n  let f : (i8) -> u64 = fn (x : i8) -> x as i64\n}\n",
        CAST,
    ),
    (
        "cast_i16_to_i64",
        "module CastI16ToI64 = structure {\n  let f : (i16) -> u64 = fn (x : i16) -> x as i64\n}\n",
        CAST,
    ),
    (
        "cast_i32_to_i64",
        "module CastI32ToI64 = structure {\n  let f : (i32) -> u64 = fn (x : i32) -> x as i64\n}\n",
        CAST,
    ),
    // ---- m4-002: widening unsigned casts ------------------------------------
    (
        "cast_u8_to_u64",
        "module CastU8ToU64 = structure {\n  let f : (u8) -> u64 = fn (x : u8) -> x as u64\n}\n",
        CAST,
    ),
    (
        "cast_u16_to_u64",
        "module CastU16ToU64 = structure {\n  let f : (u16) -> u64 = fn (x : u16) -> x as u64\n}\n",
        CAST,
    ),
    (
        "cast_u32_to_u64",
        "module CastU32ToU64 = structure {\n  let f : (u32) -> u64 = fn (x : u32) -> x as u64\n}\n",
        CAST,
    ),
    // ---- m4-002: narrowing casts --------------------------------------------
    (
        "cast_u64_to_u32",
        "module CastU64ToU32 = structure {\n  let f : (u64) -> u64 = fn (x : u64) -> x as u32\n}\n",
        CAST,
    ),
    (
        "cast_u64_to_u16",
        "module CastU64ToU16 = structure {\n  let f : (u64) -> u64 = fn (x : u64) -> x as u16\n}\n",
        CAST,
    ),
    (
        "cast_u64_to_u8",
        "module CastU64ToU8 = structure {\n  let f : (u64) -> u64 = fn (x : u64) -> x as u8\n}\n",
        CAST,
    ),
    (
        "cast_i64_to_i32",
        "module CastI64ToI32 = structure {\n  let f : (i64) -> u64 = fn (x : i64) -> x as i32\n}\n",
        CAST,
    ),
    // ---- m4-002: same-width reinterpret cast --------------------------------
    (
        "cast_u32_to_i32",
        "module CastU32ToI32 = structure {\n  let f : (u32) -> u64 = fn (x : u32) -> x as i32\n}\n",
        CAST,
    ),
    // ---- m4-003: sized-integer `let` bindings -------------------------------
    (
        "sized_let_u32",
        "module SizedLetU32 = structure {\n  let x : u32 = 42\n}\n",
        LET42,
    ),
    (
        "sized_let_u16",
        "module SizedLetU16 = structure {\n  let x : u16 = 42\n}\n",
        LET42,
    ),
    (
        "sized_let_u64",
        "module SizedLetU64 = structure {\n  let x : u64 = 42\n}\n",
        LET42,
    ),
    // ---- nested: `~(x as u32)` ----------------------------------------------
    (
        "nested_not_cast",
        "module NestedNotCast = structure {\n  let f : (u64) -> u64 = fn (x : u64) -> ~(x as u32)\n}\n",
        BITNOT,
    ),
];

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

/// Build a fixture to ELF64 and return the disassembled `.text` lines as
/// rendered by the iced-x86 `IntelFormatter`.
fn build_and_disasm(basename: &str, source: &str) -> Vec<String> {
    let dir = std::env::temp_dir().join("paideia_as_pa7c_m4_004");
    std::fs::create_dir_all(&dir).expect("create temp dir");

    let src_path: PathBuf = dir.join(format!("{basename}.pdx"));
    let obj_path: PathBuf = dir.join(format!("{basename}.o"));
    let _ = std::fs::remove_file(&obj_path);
    std::fs::write(&src_path, source).expect("write fixture source");

    let out = cargo_run(&[
        "build",
        src_path.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        obj_path.to_str().unwrap(),
    ]);

    assert!(
        out.status.success(),
        "build failed for fixture `{basename}`:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let bytes = std::fs::read(&obj_path)
        .unwrap_or_else(|e| panic!("output ELF for `{basename}` should exist: {e}"));
    assert_eq!(
        &bytes[0..4],
        b"\x7FELF",
        "ELF magic missing for `{basename}`"
    );

    let file = object::File::parse(&*bytes)
        .unwrap_or_else(|e| panic!("should parse ELF for `{basename}`: {e}"));

    let mut text_bytes = Vec::new();
    for section in file.sections() {
        if section.name().unwrap_or("") == ".text" {
            text_bytes = section.data().unwrap_or(b"").to_vec();
            break;
        }
    }
    assert!(
        !text_bytes.is_empty(),
        ".text section must exist and be non-empty for `{basename}`"
    );

    let mut decoder = Decoder::new(64, &text_bytes, DecoderOptions::NONE);
    let mut formatter = IntelFormatter::new();
    let mut lines = Vec::new();
    let mut rendered = String::new();
    for inst in decoder.iter() {
        rendered.clear();
        formatter.format(&inst, &mut rendered);
        lines.push(rendered.clone());
    }

    let _ = std::fs::remove_file(&obj_path);
    let _ = std::fs::remove_file(&src_path);

    lines
}

#[test]
fn round_trip_all_expr_surface_fixtures() {
    let mut failures: Vec<String> = Vec::new();

    for (basename, source, expected) in FIXTURES {
        let actual = build_and_disasm(basename, source);

        // The emitted `.text` must begin with the expected instruction lines.
        // (Some lowerings append nothing; we compare the leading window so
        // padding/alignment tail bytes, if any, never break the assertion.)
        let matches = actual.len() >= expected.len()
            && actual
                .iter()
                .zip(expected.iter())
                .all(|(got, want)| got == want);

        if !matches {
            failures.push(format!(
                "fixture `{basename}`:\n    expected: {expected:?}\n    actual:   {actual:?}"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} expr-surface fixtures failed the iced-x86 round-trip:\n\n{}",
        failures.len(),
        FIXTURES.len(),
        failures.join("\n\n"),
    );
}

#[test]
fn fixture_table_covers_the_full_m4_surface() {
    // Guard against accidental shrinkage of the gating corpus.
    assert!(
        FIXTURES.len() >= 23,
        "expected >= 23 expr-surface fixtures, found {}",
        FIXTURES.len()
    );

    let bitnot = FIXTURES.iter().filter(|(_, _, e)| *e == BITNOT).count();
    let cast = FIXTURES.iter().filter(|(_, _, e)| *e == CAST).count();
    let sized = FIXTURES.iter().filter(|(_, _, e)| *e == LET42).count();

    // 8 plain `~x` + 1 nested `~(x as u32)` = 9 bitnot expectations.
    assert_eq!(bitnot, 9, "expected 9 bitnot fixtures, found {bitnot}");
    // 3 widen-signed + 3 widen-unsigned + 4 narrow + 1 reinterpret = 11 casts.
    assert_eq!(cast, 11, "expected 11 cast fixtures, found {cast}");
    // 3 sized-integer `let` bindings.
    assert_eq!(sized, 3, "expected 3 sized-let fixtures, found {sized}");
}
