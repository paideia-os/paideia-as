//! Single integration-test binary for `paideia-as-encoder`.
//!
//! Cargo builds one binary per top-level `.rs` file under `tests/`, and
//! that link+start cost dominates a full test run when there are dozens
//! of tiny files. This module is the *only* top-level entry — every
//! test lives under a topical submodule below, so the whole suite links
//! once and shares one runtime.
//!
//! Layout: one directory per instruction family. Add new tests inside
//! the appropriate family and wire them via that family's `mod.rs`. If
//! a family doesn't fit, create a new sibling directory + `mod` entry
//! here — do NOT add another top-level `tests/foo.rs`, that would
//! reintroduce a second binary.

mod common;

mod addressing;
mod arith;
mod bitwise;
mod control_flow;
mod lock_prefix;
mod memory_ops;
mod mov;
mod simd;
mod system;
mod test_flags;
