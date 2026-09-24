//! Per-turn wire fingerprint emission — the elaborator side of
//! `@fingerprint("<name>")` (R220.M10, paideia-as#1424).
//!
//! # What this module is
//!
//! When a paideia-as module carries one or more R220.M10 attributes:
//!
//! ```text
//! pub let turn_marker : u64 = 0
//!     @fingerprint("test.turn.001");
//!
//! pub let batch_marker : u64 = 1
//!     @fingerprint("r220m10-fp-01");
//! ```
//!
//! the parser records each `(let_node_id, "<name>")` pair on
//! [`paideia_as_ast::ItemFingerprintTable`]. This module lifts every entry
//! into a standalone [`paideia_as_ir::DataEntry`] holding the raw
//! **NUL-terminated** name bytes and pushes it onto
//! [`paideia_as_ir::IrArena::fingerprints`], staged for `.rodata` emission
//! by the ELF / PE back ends under the synthetic symbol `fp_<name>`.
//!
//! # Why NUL-terminated
//!
//! A hosted DSL / REPL that dispatches per turn uses `@fingerprint("<name>")`
//! to stamp a wire tag the debugger recognises by **substring search of
//! the compiled ELF's rodata payload** — the anti-fabrication pattern
//! kernel-side per `feedback_workerbee_verify_claims.md`. Trailing `\0`
//! matches the C-string shape the debugger already looks for in
//! `src/kernel/core/klog/keys.pdx`, so the same finder utilities work
//! byte-for-byte on paideia-as artefacts and paideia-os kernel images
//! alike.
//!
//! # Determinism
//!
//! `ItemFingerprintTable` is HashMap-backed, so iteration order is
//! unspecified. Every build must produce byte-identical `.rodata`, so we
//! collect entries, sort by originating Let `NodeId::get()`, and push in
//! that order. The resulting `.rodata` layout is deterministic across
//! builds and across `cargo test` runs.
//!
//! # Fingerprint tags
//!
//! Tests in this module carry `r220m10-fp-NN` fingerprints per the batch
//! discipline in `.plans/scratch/CHANGELOG-fingerprint-intrinsic.md`.

use paideia_as_ast::{AstArena, NodeId};
use paideia_as_ir::{DataEntry, IrArena};

/// Symbol-name prefix stamped onto every `.rodata` fingerprint entry.
///
/// Kept as a constant so downstream tests / consumers (debugger,
/// fingerprint finder utilities) can `starts_with` against a single
/// canonical value rather than re-encoding it themselves.
pub const FINGERPRINT_SYMBOL_PREFIX: &str = "fp_";

/// Byte alignment for every fingerprint entry.
///
/// `.rodata` C-strings have no natural alignment beyond byte; keeping
/// this at 1 lets the linker tightly pack consecutive fingerprints and
/// keeps a per-turn fingerprint's cost to `name.len() + 1` bytes exactly.
pub const FINGERPRINT_ALIGN: u32 = 1;

/// Scan `ast.item_fingerprint()` and push one `.rodata` entry per
/// `@fingerprint("<name>")` attribute onto `ir.fingerprints_mut()`.
///
/// Each entry:
/// - **bytes** = `<name>` UTF-8 bytes followed by a NUL terminator
///   (`name.as_bytes()` are already ASCII by parser validation, so the
///   NUL-terminated payload is byte-identical to a C string of the tag).
/// - **symbol_name** = `format!("fp_{}", name)`.
/// - **section** = `SectionKind::Rodata` (via [`DataEntry::new_rodata`]).
/// - **align** = [`FINGERPRINT_ALIGN`] (= 1).
///
/// Entries are sorted by originating Let `NodeId::get()` before pushing
/// so `.rodata` layout is byte-stable across builds.
///
/// This pass is a no-op when the side-table is empty — the common case
/// for modules that do not use hosted-DSL / REPL fingerprinting.
pub fn populate_fingerprints(ast: &AstArena, ir: &mut IrArena) {
    if ast.item_fingerprint().is_empty() {
        return;
    }

    // Collect + sort by originating Let NodeId for byte-stable layout.
    let mut ordered: Vec<(NodeId, String)> = ast
        .item_fingerprint()
        .iter()
        .map(|(id, name)| (id, name.to_string()))
        .collect();
    ordered.sort_by_key(|(id, _)| id.get());

    for (_let_id, name) in ordered {
        // NUL-terminate the tag bytes — see module docs for rationale.
        let mut bytes = Vec::with_capacity(name.len() + 1);
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(0u8);

        let symbol_name = format!("{}{}", FINGERPRINT_SYMBOL_PREFIX, name);
        let entry = DataEntry::new_rodata(bytes, symbol_name, FINGERPRINT_ALIGN);
        ir.fingerprints_mut().push(entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ir::SectionKind;

    fn make_ast_with_fp(entries: &[(u32, &str)]) -> AstArena {
        let mut ast = AstArena::new();
        for (raw_id, name) in entries {
            let id = NodeId::new(*raw_id).expect("non-zero");
            ast.item_fingerprint_mut().insert(id, name.to_string());
        }
        ast
    }

    /// Fingerprint tag: r220m10-fp-16 — single attach lands one rodata entry.
    #[test]
    fn single_entry_populates_rodata() {
        let ast = make_ast_with_fp(&[(3, "test.turn.001")]);
        let mut ir = IrArena::new();
        populate_fingerprints(&ast, &mut ir);

        assert_eq!(ir.fingerprints().len(), 1);
        let e = &ir.fingerprints()[0];
        assert_eq!(e.section, SectionKind::Rodata);
        assert_eq!(e.symbol_name, "fp_test.turn.001");
        // 13 bytes of "test.turn.001" + 1 NUL = 14 bytes.
        assert_eq!(e.bytes, b"test.turn.001\0".to_vec());
        assert_eq!(e.align, FINGERPRINT_ALIGN);
    }

    /// Fingerprint tag: r220m10-fp-17 — multiple attaches land in NodeId order.
    #[test]
    fn multiple_entries_are_sorted_by_node_id() {
        // Insert out of order: id=7 first, then id=2, then id=5.
        let ast = make_ast_with_fp(&[
            (7, "gamma-turn.03"),
            (2, "alpha-turn.01"),
            (5, "beta-turn.02"),
        ]);
        let mut ir = IrArena::new();
        populate_fingerprints(&ast, &mut ir);

        assert_eq!(ir.fingerprints().len(), 3);
        let syms: Vec<&str> = ir
            .fingerprints()
            .iter()
            .map(|e| e.symbol_name.as_str())
            .collect();
        // Sorted by NodeId::get(): 2 → 5 → 7.
        assert_eq!(
            syms,
            vec!["fp_alpha-turn.01", "fp_beta-turn.02", "fp_gamma-turn.03"]
        );
        // Each carries its own NUL-terminated tag.
        assert_eq!(ir.fingerprints()[0].bytes, b"alpha-turn.01\0".to_vec());
        assert_eq!(ir.fingerprints()[1].bytes, b"beta-turn.02\0".to_vec());
        assert_eq!(ir.fingerprints()[2].bytes, b"gamma-turn.03\0".to_vec());
    }

    /// Fingerprint tag: r220m10-fp-18 — empty side-table → no-op.
    #[test]
    fn empty_side_table_is_noop() {
        let ast = AstArena::new();
        let mut ir = IrArena::new();
        populate_fingerprints(&ast, &mut ir);
        assert!(ir.fingerprints().is_empty());
    }
}
