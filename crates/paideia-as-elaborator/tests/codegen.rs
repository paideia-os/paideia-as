//! Integration tests: IR -> instruction codegen surface.
//!
//! Consolidated binary that hosts every emit-side integration test: the
//! `EmitWalker` mode-propagation and `pub let` visibility flows, `emit_call`
//! recipe-label mangling and SysV-regs marshalling, `byte_offset_in_text`
//! population, and string-literal interning.
//!
//! Each sibling file under `tests/codegen/` becomes a sub-module. `emit_walker`
//! stays nested with its existing sub-files unchanged.

mod codegen {
    pub mod byte_offset;
    pub mod emit_call_recipe_labels;
    pub mod emit_call_sysvregs;
    pub mod emit_walker {
        pub mod mode_propagation;
        pub mod pub_let;
    }
    pub mod string_lit_emit;
}
