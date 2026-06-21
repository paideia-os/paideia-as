//! Phase 6 m3-002: Byte-sequence assertion test for field access lowering.
//!
//! This test would verify that field access expressions emit the correct x86-64 bytes
//! when the parser supports struct types and field access syntax:
//! - (*p).kind (u64 at offset 0) → 48 8B 07 (mov rax, [rdi])
//! - (*p).field4 (offset 24) → 48 8B 47 18 (mov rax, [rdi + 24])
//! - (*p).u32field (u32 field) → mov eax, [rdi + offset]
//! - (*p).u8field (u8 field) → movzx rax, byte [rdi + offset]
//!
//! Current status: Phase 6 parser does not yet support inline struct type definitions
//! or field access syntax. This test is deferred to a later phase when parser support
//! is available. The unit tests in emit_walker.rs validate the core lowering logic.
//!
//! TODO: Update when parser adds struct type + field access support.

#[test]
fn field_access_fixture_deferred_pending_parser_support() {
    // This is a placeholder. The real test would build cap_read_kind.pdx
    // and verify that (*p).kind emits mov rax, [rdi] (48 8B 07 c3).
    // For now, we skip this until the parser supports the required syntax.
}
