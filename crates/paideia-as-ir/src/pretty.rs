//! Debug pretty-printer for IR trees.
//!
//! Produces an indented S-expression-style dump for snapshot tests. Each
//! line shows the variant name, `LinClass`, and the effect-row id.

use std::fmt::Write;

use crate::arena::IrArena;
use crate::node::IrNodeId;

/// Dump every node in `arena` as an indented S-expression-style list.
///
/// Phase-1: walks the arena in allocation order and prints each node's
/// kind + `LinClass` + `EffectRowId`. Children-traversal will arrive once
/// the IR carries structural links (PR-29+).
#[must_use]
pub fn dump(arena: &IrArena) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "(ir-arena nodes={})", arena.len());
    for (i, node) in arena.as_slice().iter().enumerate() {
        let id = IrNodeId::new((i + 1) as u32).expect("non-zero");
        let _ = writeln!(
            out,
            "  {} {:?} class={:?} effects=#{}",
            id, node.kind, node.lin_class, node.effect_row.0
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{IrKind, LinClass};
    use paideia_as_diagnostics::{FileId, Span};

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn dump_empty_arena() {
        let arena = IrArena::new();
        let s = dump(&arena);
        assert!(s.contains("(ir-arena nodes=0)"));
    }

    #[test]
    fn dump_includes_kind_and_class() {
        let mut arena = IrArena::new();
        let id = arena.alloc(IrKind::Lambda, span());
        arena.get_mut(id).unwrap().lin_class = LinClass::Linear;
        let s = dump(&arena);
        assert!(s.contains("Lambda"));
        assert!(s.contains("Linear"));
        assert!(s.contains("i1"));
    }

    #[test]
    fn dump_two_nodes_in_order() {
        let mut arena = IrArena::new();
        arena.alloc(IrKind::Module, span());
        arena.alloc(IrKind::Let, span());
        let s = dump(&arena);
        let module_pos = s.find("Module").unwrap();
        let let_pos = s.find("Let").unwrap();
        assert!(module_pos < let_pos);
    }
}
