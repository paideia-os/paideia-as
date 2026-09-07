//! Operand parser for the unsafe-block surface (Phase 5, m3-002).
//!
//! This module implements parsing of x86_64 operands from the AST representation
//! used in unsafe blocks. It converts AST operand nodes into IR `Operand` values
//! with proper register encoding and memory addressing modes.
//!
//! # Mnemonic Resolution (Phase 5, m3-003)
//!
//! The MNEMONIC_TABLE constant (see `mnemonic_table`) provides a canonical
//! mapping from mnemonic string spellings (case-insensitive) to IR `Mnemonic`
//! enum variants, including proper disambiguation for variants with payloads:
//! - Jcc(Cond) forms: `je` → `Jcc(Cond::Eq)`, `jne` → `Jcc(Cond::Ne)`, etc.
//! - MovCr{write}: `mov_cr` → `MovCr{write:true}`, `mov_from_cr` → `MovCr{write:false}`
//! - MovDr{write}: `mov_dr` → `MovDr{write:true}`, `mov_from_dr` → `MovDr{write:false}`
//! - In{width}: `in_al` → `In{width:1}`, `in_ax` → `In{width:2}`, `in_eax` → `In{width:4}`
//! - Out{width}: `out_al` → `Out{width:1}`, `out_ax` → `Out{width:2}`, `out_eax` → `Out{width:4}`
//!
//! # UnsafeWalker (Phase 5, m3-004)
//!
//! The UnsafeWalker elaborates pending unsafe blocks emitted by the EmitWalker.
//! For each pending unsafe block, it walks the block's statement sequence, emitting
//! `Instruction` entries into the IR's InstructionSideTable keyed by StmtInstruction IrNodeId.
//!
//! Errors are handled per spec:
//! - Unknown mnemonic: emits U1605 diagnostic with mnemonic span; instruction skipped.
//! - Operand shape error: emits U1606 diagnostic with operand span; instruction skipped.
//!
//! # Register Encoding
//!
//! General-purpose registers and special registers use distinct sentinel ranges:
//! - GPR (rax–r15): `RegId(0..15)` (standard x86_64 encoding)
//! - Control registers (cr0–cr8): `RegId(16..24)` (compact encoding for m2-005 bridge)
//! - Debug registers (dr0–dr7): `RegId(25..32)` (compact encoding for m2-005 bridge)
//!
//! The m2-005 bridge reconciles these: if RegId >= 16 and < 25, extract cr_idx = RegId - 16;
//! if >= 25 and < 33, extract dr_idx = RegId - 25.
//!
//! # File layout
//!
//! Split out of `unsafe_walker.rs` (paideia-as #1403). The sub-files:
//! - `immediate` / `memory` / `register` / `symbol_ref` — operand-shape helpers.
//! - `mnemonic_table` — string → `Mnemonic` resolver (`resolve_mnemonic`).
//! - `operand` — top-level operand-parse dispatch (`parse_operand_from_ast`, `OperandError`).
//! - `diag` — U-category diagnostic code constants.
//! - `walker` — `UnsafeWalker` struct and its `run` driver.
//! - `process_stmt` — per-statement encoder inside `impl UnsafeWalker`.
//! - `tests` — unit tests (unchanged).

// --- Internal submodules ---
mod diag;
mod immediate;
mod memory;
mod mnemonic_table;
mod operand;
mod process_stmt;
mod register;
mod symbol_ref;
mod walker;

#[cfg(test)]
mod tests;

// Re-exports so the pre-refactor path `crate::unsafe_walker::extract_integer_from_span`
// (used by `lower/match_dispatch.rs`) still resolves.
pub(crate) use immediate::extract_integer_from_span;

// Public API — preserve the pre-refactor visibility surface for downstream
// crates and the top-level re-exports in `lib.rs`.
pub use diag::{
    U_DUPLICATE_LABEL, U_MALFORMED_OPERAND, U_SYMBOLREF_NOT_SUPPORTED, U_UNEXPECTED_OPERANDS,
    U_UNKNOWN_LABEL, U_UNKNOWN_MNEMONIC, U_UNRESOLVED_FIELD_OFFSET,
    U_UNSUPPORTED_STMT_IN_UNSAFE,
};
pub use mnemonic_table::resolve_mnemonic;
pub use operand::{parse_operand_from_ast, OperandError};
pub use walker::UnsafeWalker;
