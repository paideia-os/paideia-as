//! Integration tests: `UnsafeWalker` elaboration.
//!
//! Consolidated binary that hosts every UnsafeWalker-focused integration test:
//! the core walker top-level tests (mnemonic retarget, arity checks, operand
//! parsing), the imm64 bit-op expansion path, symbol memory-operand parsing,
//! and zero-arity mnemonic dispatch.

mod common;

mod unsafe_walker {
    pub mod imm64_bitops;
    pub mod symbol_memory;
    pub mod top_level;
    pub mod zero_arity;
}
