//! Issue #999 (pa-r18-006) — runtime verification for command-dispatch pattern reference fixture.
//!
//! Tests that command dispatch via enum-tag match with @jump_table correctly routes
//! the Echo command to its handler and returns exit code 3.
//!
//! Expected: 3

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn command_dispatch_shell_echo_returns_3() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "pa_r18_006_command_dispatch_shell.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 3,
    };
    run_and_verify(&case);
}
