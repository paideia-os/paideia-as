//! Shared testkit for `EmitWalker` unit tests.
//!
//! Step 9 of the v0.17 refactor plan. Every emit-walker unit test
//! reconstructs the same three-part scaffold — an `IrArena`, an
//! `EmitWalker`, and a synthetic span — before it can even allocate the
//! nodes under test. Multiplied across 500+ tests, that scaffold is
//! ~1,500 lines of noise that obscures what each test actually asserts.
//!
//! `EmitFixture` bundles the scaffold plus builder methods for the most
//! common IR shapes: literals, let bindings, lambdas, walks, and
//! instruction inspection. New tests written against it read as
//! declarative sequences instead of setup boilerplate.
//!
//! Only compiled under `#[cfg(test)]` — the fixture is never linked into
//! production binaries.
//!
//! # Example
//!
//! ```ignore
//! use crate::emit_fixture::EmitFixture;
//! use paideia_as_ir::instruction::{Mnemonic, Operand};
//! use paideia_as_ir::abi;
//!
//! let mut f = EmitFixture::new();
//! let lit = f.literal(42);
//! let let_id = f.let_binding(lit);
//! f.walk();
//! let inst = f.instruction(let_id);
//! assert_eq!(inst.mnemonic, Mnemonic::Mov);
//! assert_eq!(inst.operands[0], Operand::Reg(abi::RAX));
//! assert_eq!(f.estimated_offset(), 7);
//! ```

#![cfg(test)]
#![allow(dead_code)]

use paideia_as_diagnostics::{FileId, Span};
use paideia_as_ir::instruction::Instruction;
use paideia_as_ir::record_layout::{RecordLayout, RecordTypeId};
use paideia_as_ir::{EnumLayout, EnumTypeId, IrArena, IrKind, IrNodeId};

use crate::emit_walker::EmitWalker;

/// Standard synthetic span used by every fixture-built node.
pub(crate) fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

/// Bundles a fresh `IrArena` and `EmitWalker` with builder methods for
/// the most common test shapes. Consumers construct one per test, chain
/// builder calls to allocate nodes, then call `walk()` and inspect the
/// resulting instruction table.
pub(crate) struct EmitFixture {
    pub(crate) arena: IrArena,
    pub(crate) walker: EmitWalker,
}

impl EmitFixture {
    /// Fresh arena + walker in default state.
    pub(crate) fn new() -> Self {
        Self {
            arena: IrArena::new(),
            walker: EmitWalker::new(),
        }
    }

    // ── Node allocation ─────────────────────────────────────────────────

    /// Allocate a `Literal` node with the given i64 value already
    /// registered in `literal_values`.
    pub(crate) fn literal(&mut self, value: i64) -> IrNodeId {
        let id = self.arena.alloc(IrKind::Literal, span());
        self.arena.literal_values_mut().insert(id, value);
        id
    }

    /// Allocate a `Var` node — the binding-name table is left untouched;
    /// callers can populate it with `bind_var_name` if needed.
    pub(crate) fn var(&mut self) -> IrNodeId {
        self.arena.alloc(IrKind::Var, span())
    }

    /// Record a binding name for a previously-allocated node (typically
    /// a `Var` or `Let`).
    pub(crate) fn bind_name(&mut self, node_id: IrNodeId, name: &str) {
        self.arena
            .binding_names_mut()
            .insert(node_id, name.to_string());
    }

    /// Allocate a `Let` binding whose sole child is `rhs`.
    pub(crate) fn let_binding(&mut self, rhs: IrNodeId) -> IrNodeId {
        self.arena
            .alloc_with_children(IrKind::Let, span(), [rhs])
    }

    /// Allocate a named `Let` binding — combines `let_binding` with
    /// `bind_name`.
    pub(crate) fn named_let(&mut self, name: &str, rhs: IrNodeId) -> IrNodeId {
        let id = self.let_binding(rhs);
        self.bind_name(id, name);
        id
    }

    /// Allocate a `Lambda` whose body is `body`.
    pub(crate) fn lambda(&mut self, body: IrNodeId) -> IrNodeId {
        self.arena
            .alloc_with_children(IrKind::Lambda, span(), [body])
    }

    /// Allocate an `Action` block that groups the given statement nodes.
    pub(crate) fn action(&mut self, children: impl IntoIterator<Item = IrNodeId>) -> IrNodeId {
        let ids: Vec<IrNodeId> = children.into_iter().collect();
        let action_id = self.arena.alloc(IrKind::Action, span());
        if let Some(existing) = self.arena.children_mut(action_id) {
            for id in ids {
                existing.push(id);
            }
        }
        action_id
    }

    /// Allocate a `Deref(child)` node.
    pub(crate) fn deref(&mut self, child: IrNodeId) -> IrNodeId {
        self.arena.alloc_with_children(IrKind::Deref, span(), [child])
    }

    // ── Layout registration ─────────────────────────────────────────────

    /// Inject a finalised record layout into the walker state.
    pub(crate) fn install_record_layout(
        &mut self,
        type_id: RecordTypeId,
        layout: RecordLayout,
    ) {
        self.walker.state_mut().insert_record_layout(type_id, layout);
    }

    /// Inject an enum layout into the walker state.
    pub(crate) fn install_enum_layout(&mut self, type_id: EnumTypeId, layout: EnumLayout) {
        self.walker.state_mut().insert_enum_layout(type_id, layout);
    }

    // ── Walk + inspection ───────────────────────────────────────────────

    /// Drive the walker over the fixture's arena.
    pub(crate) fn walk(&mut self) {
        self.walker.walk(&mut self.arena);
    }

    /// Look up the instruction emitted at `node_id`. Panics if none was
    /// registered — every test asserting shape wants that guarantee.
    pub(crate) fn instruction(&self, node_id: IrNodeId) -> &Instruction {
        self.walker
            .state()
            .instructions()
            .get(node_id)
            .expect("no instruction emitted at node")
            emission_order: 0,
}

    /// Look up the optional instruction at `node_id` without panicking.
    pub(crate) fn instruction_opt(&self, node_id: IrNodeId) -> Option<&Instruction> {
        self.walker.state().instructions().get(node_id)
    }

    /// Current `estimated_offset` — the walker's byte position estimate.
    pub(crate) fn estimated_offset(&self) -> u32 {
        self.walker.state().estimated_offset()
    }

    /// Read-only view of the diagnostics vector.
    pub(crate) fn diagnostics(&self) -> &[String] {
        self.walker.diagnostics()
    }

    /// Assert the walker produced no diagnostics. Preferred over
    /// `assert!(f.diagnostics().is_empty())` because the panic message
    /// echoes the offending diagnostic text.
    #[track_caller]
    pub(crate) fn assert_no_diagnostics(&self) {
        let diags = self.diagnostics();
        assert!(diags.is_empty(), "unexpected diagnostics: {:?}", diags);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ir::abi;
    use paideia_as_ir::instruction::{Mnemonic, Operand};

    #[test]
    fn fixture_literal_let_emits_mov_rax_imm() {
        let mut f = EmitFixture::new();
        let lit = f.literal(42);
        let let_id = f.let_binding(lit);
        f.walk();
        let inst = f.instruction(let_id);
        assert_eq!(inst.mnemonic, Mnemonic::Mov);
        assert_eq!(inst.operands[0], Operand::Reg(abi::RAX));
        assert_eq!(inst.operands[1], Operand::Imm64(42));
        assert_eq!(f.estimated_offset(), 7);
        f.assert_no_diagnostics();
    }

    #[test]
    fn fixture_walk_on_empty_arena_emits_zero_diagnostics() {
        let mut f = EmitFixture::new();
        f.walk();
        f.assert_no_diagnostics();
    }

    #[test]
    fn fixture_named_let_populates_binding_name() {
        let mut f = EmitFixture::new();
        let lit = f.literal(7);
        let let_id = f.named_let("x", lit);
        assert_eq!(
            f.arena.binding_names().get(let_id).as_deref(),
            Some("x")
        );
    }

    #[test]
    fn fixture_action_groups_children() {
        let mut f = EmitFixture::new();
        let a = f.literal(1);
        let b = f.literal(2);
        let block = f.action([a, b]);
        let kids = f.arena.children(block);
        assert_eq!(kids, &[a, b]);
    }

    #[test]
    fn fixture_lambda_wraps_body() {
        let mut f = EmitFixture::new();
        let lit = f.literal(3);
        let body = f.action([lit]);
        let lam = f.lambda(body);
        let kids = f.arena.children(lam);
        assert_eq!(kids, &[body]);
    }
}
