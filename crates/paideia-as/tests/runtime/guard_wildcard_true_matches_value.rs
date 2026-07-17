//! Issue #1000 — guarded wildcard with `if true` must match its arm (return 99),
//! NOT silently fall through to the next default arm (return 7).
//!
//! Regression test for the L130 PatIdent branch fix in match_arm.rs. Debugger
//! caught during #1000 verification that the initial workerbee fix patched only
//! the L122 PatWildcard branch, which the real parser never produces —
//! parse_pattern.rs constructs PatIdent for `_`. Without the L130 fix,
//! guarded wildcards silently drop their guard.
//!
//! Calculation: `match r { _ if true => 99u64, _ => 7u64 }` → 99.

use super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn guard_wildcard_true_matches() {
    let poison = [
        Poison { reg: Reg::Rcx, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF02 },
        Poison { reg: Reg::R8, value: 0xDEADBEEF03 },
        Poison { reg: Reg::R9, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "guard_wildcard_true_matches.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 99,
    };
    run_and_verify(&case);
}
