//! x86_64 instruction encoding for phase-1 smoke testing.
//!
//! This module provides a typed API for encoding a minimal x86_64 instruction set
//! to raw bytes. All encodings follow Intel SDM Vol 2A exactly.
//!
//! The encoder is stateless; callers maintain a `CodeBuffer` and pass it to
//! individual instruction functions.
//!
//! # Organization
//!
//! The encoder was originally a single 8 050-line `encode.rs`. It has been
//! split into focused sub-modules — one per mnemonic family — see the `mod`
//! declarations below. All previously-public items are re-exported unchanged,
//! so callers continue to reach them at `paideia_as_encoder::encode::…`.
//!
//! Shared types (`Reg64`, `Reg32`, `Cond`, `CodeBuffer`) and the internal REX /
//! ModR/M / SIB helpers (`rex`, `rex_w`, `emit_mem_base_disp`,
//! `emit_mem_sib_disp`) live in [`types`]; each sub-module pulls them in via
//! `use super::types::*;`.

mod types;

mod mov_arith;
mod bit_ops;
mod shift_logical;
mod unary_special;
mod cache_widen;
mod mem_moves;
mod lock_arith;
mod cmp_jmp;
mod abs_disp32;
mod cond_call;
mod system;

#[cfg(test)]
mod tests;

// ── Public re-exports ─────────────────────────────────────────────────────
//
// Every `pub` item that was reachable at `crate::encode::<name>` in the
// pre-split file must remain reachable at the same path. The `pub use` lines
// below preserve that surface exactly.

pub use types::{CodeBuffer, Cond, Reg32, Reg64};

// Internal helpers keep `pub(crate)` visibility (unchanged for
// `emit_mem_base_disp` / `emit_mem_sib_disp`, upgraded from private for `rex`
// / `rex_w` so sibling sub-modules can call them). `crate::encode::…`
// consumers such as `encode_instruction::cmp_test` continue to see the
// previously-exposed helpers at their existing paths.
pub(crate) use types::{emit_mem_base_disp, emit_mem_sib_disp};

pub use abs_disp32::*;
pub use bit_ops::*;
pub use cache_widen::*;
pub use cmp_jmp::*;
pub use cond_call::*;
pub use lock_arith::*;
pub use mem_moves::*;
pub use mov_arith::*;
pub use shift_logical::*;
pub use system::*;
pub use unary_special::*;
