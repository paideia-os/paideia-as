//! Issue #999 (pa-r18-006) — build-emit test for command-dispatch pattern reference fixture.
//!
//! Verifies that the command-dispatch shell fixture compiles successfully and
//! emits valid ELF64 output with an _start entry point.

use crate::common::elf::assert_elf64_magic;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;
use object::{Object, ObjectSymbol};

#[test]
fn pa_r18_006_command_dispatch_shell_builds() {
    let out = run_build(build_emit("pa_r18_006_command_dispatch_shell.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    // Verify entry symbol exists
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");
    let entry_found = file
        .symbols()
        .any(|s| s.name().ok() == Some("entry"));
    assert!(entry_found, "entry symbol not found in ELF");
}
