//! Issue #1003 (pa-r18-010) — build-emit tests for hash-based command dispatch.
//!
//! Verifies that the four hash-dispatch fixtures compile successfully and emit
//! valid ELF64 output with an entry entry point.
//!
//! Fixtures:
//! - `pa_r18_010_hash_dispatch_smoke.pdx`: 4 commands, dispatch echo, expect exit 3.
//! - `pa_r18_010_hash_dispatch_miss.pdx`: 4 commands, dispatch absent key, expect exit 999.
//! - `pa_r18_010_hash_dispatch_collision.pdx`: 2 colliding names, linear probe, expect exit 20.
//! - `pa_r18_010_hash_dispatch_30cmd.pdx`: 30 commands, dispatch cmd14, expect exit 14.

use crate::common::elf::assert_elf64_magic;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;
use object::{Object, ObjectSymbol};

#[test]
fn pa_r18_010_hash_dispatch_smoke_builds() {
    let out = run_build(build_emit("hash_dispatch/pa_r18_010_hash_dispatch_smoke.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    let file = object::File::parse(&*bytes).expect("object should parse the ELF");
    let entry_found = file
        .symbols()
        .any(|s| s.name().ok() == Some("entry"));
    assert!(entry_found, "entry symbol not found in ELF");
}

#[test]
fn pa_r18_010_hash_dispatch_miss_builds() {
    let out = run_build(build_emit("hash_dispatch/pa_r18_010_hash_dispatch_miss.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    let file = object::File::parse(&*bytes).expect("object should parse the ELF");
    let entry_found = file
        .symbols()
        .any(|s| s.name().ok() == Some("entry"));
    assert!(entry_found, "entry symbol not found in ELF");
}

#[test]
fn pa_r18_010_hash_dispatch_collision_builds() {
    let out = run_build(build_emit("hash_dispatch/pa_r18_010_hash_dispatch_collision.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    let file = object::File::parse(&*bytes).expect("object should parse the ELF");
    let entry_found = file
        .symbols()
        .any(|s| s.name().ok() == Some("entry"));
    assert!(entry_found, "entry symbol not found in ELF");
}

#[test]
fn pa_r18_010_hash_dispatch_30cmd_builds() {
    let out = run_build(build_emit("hash_dispatch/pa_r18_010_hash_dispatch_30cmd.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    let file = object::File::parse(&*bytes).expect("object should parse the ELF");
    let entry_found = file
        .symbols()
        .any(|s| s.name().ok() == Some("entry"));
    assert!(entry_found, "entry symbol not found in ELF");
}
