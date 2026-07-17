//! Shared test harness for `paideia-as` integration tests.
//!
//! Every top-level `tests/*.rs` binary declares `mod common;`. Cargo compiles
//! one copy per binary; the source is centralized here so a change to build
//! invocation (new flag, in-process compile) is a one-file edit.
//!
//! Sub-modules:
//!   * `harness` — run the CLI, capture stdout/stderr, own a scratch temp dir
//!   * `fixture` — resolve fixture paths under `tests/build-emit/` and friends
//!   * `elf`     — thin ELF-parsing helpers on top of the `object` crate
//!   * `expected` — parse `*.expected_bytes.txt` sidecars
//!
//! Each sub-module carries `#![allow(dead_code)]` because Cargo builds one
//! copy per test binary and each binary uses only a slice of the surface.

#![allow(dead_code)]

pub mod elf;
pub mod expected;
pub mod fixture;
pub mod fnv1a;
pub mod harness;
