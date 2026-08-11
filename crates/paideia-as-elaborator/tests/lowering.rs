//! Integration tests: AST -> IR lowering.
//!
//! Consolidated binary that hosts every lowering-focused integration test:
//! `lower_ast_to_ir` over hand-built ASTs (unsafe blocks, pure control flow,
//! match-in-return, jump-table wildcard patterns) and stdlib-method lowering
//! recipes (Bytes, Checksum, MMIO, Pause, PerCpu).
//!
//! Each sibling file under `tests/lowering/` becomes a sub-module. Test-name
//! filtering (`cargo test -- <name>`) still resolves via substring match on
//! the fully-qualified path (`lowering::<file>::<test>`).

mod common;

mod lowering {
    pub mod let_meta_ty_populator;
    pub mod lower_unsafe;
    pub mod pa_r15_009c_jump_table_fixture;
    pub mod pa_r17_012_pure_control_flow;
    pub mod pa_r17_013_match_in_return;
    pub mod stdlib_barrier;
    pub mod stdlib_bitfield;
    pub mod stdlib_bulkmem;
    pub mod stdlib_bytes;
    pub mod stdlib_checksum;
    pub mod stdlib_mmio;
    pub mod stdlib_msr;
    pub mod stdlib_pause;
    pub mod stdlib_percpu;
    pub mod stdlib_refcount_bitmap;
    pub mod stdlib_tlb;
}
