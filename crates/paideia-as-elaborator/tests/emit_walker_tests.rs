//! Integration tests for EmitWalker (Phase 15 m2-002 and related).
//!
//! These tests exercise the EmitWalker component which produces instructions
//! from IR nodes, with emphasis on instruction mode propagation from module-level
//! #![bits=...] attributes.

mod emit_walker {
    pub mod mode_propagation;
}
