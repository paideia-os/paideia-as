//! Phase-1 placeholder emitter (deliverable 4 closure).
//!
//! The placeholder emitter writes a single text artifact containing a
//! BLAKE3 hash of the IR's pretty-printed form. It is **not** a real
//! object file — the ELF emitter arrives at deliverable 8 (T8). It
//! exists so the phase-1 pipeline can produce *some* artifact and the
//! end-to-end smoke test has something to verify.
//!
//! Determinism: equal IR arenas produce identical hashes (the
//! pretty-printer is pure over the arena snapshot).

use blake3::Hasher;
use paideia_as_ir::{IrArena, pretty};

/// Compute the placeholder content for an IR arena.
///
/// The returned string ends with a trailing newline so downstream tools
/// can `cat` it without artifacts.
#[must_use]
pub fn placeholder_for(ir: &IrArena) -> String {
    let dump = pretty::dump(ir);
    let mut hasher = Hasher::new();
    hasher.update(dump.as_bytes());
    let hash = hasher.finalize();
    format!("paideia-as placeholder v0\nblake3 {}\n", hash.to_hex())
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{FileId, Span};
    use paideia_as_ir::IrKind;

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn placeholder_is_deterministic() {
        let mut a = IrArena::new();
        a.alloc(IrKind::Placeholder, span());
        let mut b = IrArena::new();
        b.alloc(IrKind::Placeholder, span());
        assert_eq!(placeholder_for(&a), placeholder_for(&b));
    }

    #[test]
    fn placeholder_changes_with_different_ir() {
        let mut a = IrArena::new();
        a.alloc(IrKind::Placeholder, span());
        let mut b = IrArena::new();
        b.alloc(IrKind::Module, span());
        assert_ne!(placeholder_for(&a), placeholder_for(&b));
    }

    #[test]
    fn placeholder_has_header_and_hash() {
        let arena = IrArena::new();
        let out = placeholder_for(&arena);
        assert!(out.starts_with("paideia-as placeholder v0"));
        assert!(out.contains("blake3 "));
    }
}
