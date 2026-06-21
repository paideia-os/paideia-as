//! Arena allocator for IR nodes (parallels [`paideia_as_ast::AstArena`]).
//!
//! [`paideia_as_ast::AstArena`]: paideia_as_ast::AstArena

use paideia_as_diagnostics::Span;
use smallvec::SmallVec;
use std::ops::Index;

use crate::binding_name::BindingNameTable;
use crate::constant_pool::ConstantPoolTable;
use crate::data::DataSideTable;
use crate::instruction::InstructionSideTable;
use crate::literal_value::LiteralValueTable;
use crate::loop_meta::LoopMetaTable;
use crate::node::{IrKind, IrNodeData, IrNodeId};
use crate::record_layout::FieldAccessSideTable;
use crate::symbol::SymbolTable;

/// Slab-allocated IR storage for one source file.
///
/// Uses a side-table approach for child pointers: `IrNodeData` remains at
/// 48 bytes (preserving the budget), while children are stored separately
/// in `children_table`. This design keeps small nodes (≤4 children) inline
/// via SmallVec while avoiding the 16-byte budget constraint of adding
/// SmallVec directly to IrNodeData.
#[derive(Debug, Default)]
pub struct IrArena {
    nodes: Vec<IrNodeData>,
    /// Side-table: children_table[node.index()] = SmallVec of child IrNodeIds.
    /// The common case (≤4 children) is inline; spill to heap for larger nodes.
    children_table: Vec<SmallVec<[IrNodeId; 4]>>,
    /// Side-table: per-node instruction payloads (m9 opt passes).
    instruction_table: InstructionSideTable,
    /// Side-table: loop metadata (entry/exit labels) indexed by Loop node ID.
    loop_meta_table: LoopMetaTable,
    /// Side-table: constant pool for repeated 64-bit immediates (m1-010 pool-constants pass).
    constant_pool_table: ConstantPoolTable,
    /// Side-table: literal values (i64) indexed by Literal node ID.
    literal_value_table: LiteralValueTable,
    /// Side-table: data entries (.rodata/.data) indexed by Let node ID.
    data_table: DataSideTable,
    /// Side-table: binding names for Let nodes indexed by Let node ID.
    binding_name_table: BindingNameTable,
    /// Side-table: top-level binding symbol table.
    symbol_table: SymbolTable,
    /// Side-table: field access metadata indexed by FieldAccess node ID.
    /// Phase 6 m3-002: populated by elaborator, consumed by EmitWalker.
    field_access_table: FieldAccessSideTable,
}

impl IrArena {
    /// Construct an empty arena.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct an arena pre-reserved for `n` nodes.
    #[must_use]
    pub fn with_capacity(n: usize) -> Self {
        Self {
            nodes: Vec::with_capacity(n),
            children_table: Vec::with_capacity(n),
            instruction_table: InstructionSideTable::new(),
            loop_meta_table: LoopMetaTable::new(),
            constant_pool_table: ConstantPoolTable::new(),
            literal_value_table: LiteralValueTable::new(),
            data_table: DataSideTable::new(),
            binding_name_table: BindingNameTable::new(),
            symbol_table: SymbolTable::new(),
            field_access_table: FieldAccessSideTable::new(),
        }
    }

    /// Allocate a new node with the supplied kind and span. The new node
    /// inherits the default `lin_class = Unrestricted` and
    /// `effect_row = EMPTY`. The elaborator may mutate those fields in
    /// later passes. No children are initially set.
    ///
    /// Returns the freshly-allocated [`IrNodeId`].
    pub fn alloc(&mut self, kind: IrKind, span: Span) -> IrNodeId {
        let next = self.nodes.len() + 1;
        let id = IrNodeId::new(u32::try_from(next).expect("more than u32::MAX nodes"))
            .expect("non-zero next index");
        self.nodes.push(IrNodeData::new(kind, span));
        self.children_table.push(SmallVec::new());
        id
    }

    /// Allocate a new node with the supplied kind, span, and immediate children.
    /// The new node inherits the default `lin_class = Unrestricted` and
    /// `effect_row = EMPTY`. The elaborator may mutate those fields in
    /// later passes.
    ///
    /// Returns the freshly-allocated [`IrNodeId`].
    pub fn alloc_with_children<I>(&mut self, kind: IrKind, span: Span, children: I) -> IrNodeId
    where
        I: IntoIterator<Item = IrNodeId>,
    {
        let next = self.nodes.len() + 1;
        let id = IrNodeId::new(u32::try_from(next).expect("more than u32::MAX nodes"))
            .expect("non-zero next index");
        self.nodes.push(IrNodeData::new(kind, span));
        self.children_table.push(children.into_iter().collect());
        id
    }

    /// Number of nodes allocated so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// `true` iff no nodes have been allocated.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Borrow the underlying slice of node data.
    #[must_use]
    pub fn as_slice(&self) -> &[IrNodeData] {
        &self.nodes
    }

    /// Return `None` if `id` was not minted by this arena.
    #[must_use]
    pub fn get(&self, id: IrNodeId) -> Option<&IrNodeData> {
        self.nodes.get(id.index())
    }

    /// Mutable access to the node data; the elaborator updates `lin_class`
    /// and `effect_row` through this.
    pub fn get_mut(&mut self, id: IrNodeId) -> Option<&mut IrNodeData> {
        self.nodes.get_mut(id.index())
    }

    /// Return the immediate children of a node in source order.
    /// Returns an empty slice if the node does not exist or has no children.
    #[must_use]
    pub fn children(&self, id: IrNodeId) -> &[IrNodeId] {
        self.children_table
            .get(id.index())
            .map(|sv| sv.as_slice())
            .unwrap_or(&[])
    }

    /// Return mutable access to the immediate children of a node.
    /// Allows builders to populate children after node creation.
    pub fn children_mut(&mut self, id: IrNodeId) -> Option<&mut SmallVec<[IrNodeId; 4]>> {
        self.children_table.get_mut(id.index())
    }

    /// Borrow the instruction side-table (read-only).
    #[must_use]
    pub fn instructions(&self) -> &InstructionSideTable {
        &self.instruction_table
    }

    /// Borrow the instruction side-table (mutable).
    pub fn instructions_mut(&mut self) -> &mut InstructionSideTable {
        &mut self.instruction_table
    }

    /// Borrow the loop metadata side-table (read-only).
    #[must_use]
    pub fn loop_meta(&self) -> &LoopMetaTable {
        &self.loop_meta_table
    }

    /// Borrow the loop metadata side-table (mutable).
    pub fn loop_meta_mut(&mut self) -> &mut LoopMetaTable {
        &mut self.loop_meta_table
    }

    /// Borrow the constant pool side-table (read-only).
    #[must_use]
    pub fn constant_pool(&self) -> &ConstantPoolTable {
        &self.constant_pool_table
    }

    /// Borrow the constant pool side-table (mutable).
    pub fn constant_pool_mut(&mut self) -> &mut ConstantPoolTable {
        &mut self.constant_pool_table
    }

    /// Borrow the literal value side-table (read-only).
    #[must_use]
    pub fn literal_values(&self) -> &LiteralValueTable {
        &self.literal_value_table
    }

    /// Borrow the literal value side-table (mutable).
    pub fn literal_values_mut(&mut self) -> &mut LiteralValueTable {
        &mut self.literal_value_table
    }

    /// Borrow the data side-table (read-only).
    #[must_use]
    pub fn data(&self) -> &DataSideTable {
        &self.data_table
    }

    /// Borrow the data side-table (mutable).
    pub fn data_mut(&mut self) -> &mut DataSideTable {
        &mut self.data_table
    }

    /// Borrow the binding name side-table (read-only).
    #[must_use]
    pub fn binding_names(&self) -> &BindingNameTable {
        &self.binding_name_table
    }

    /// Borrow the binding name side-table (mutable).
    pub fn binding_names_mut(&mut self) -> &mut BindingNameTable {
        &mut self.binding_name_table
    }

    /// Borrow the symbol table (read-only).
    #[must_use]
    pub fn symbols(&self) -> &SymbolTable {
        &self.symbol_table
    }

    /// Borrow the symbol table (mutable).
    pub fn symbols_mut(&mut self) -> &mut SymbolTable {
        &mut self.symbol_table
    }

    /// Borrow the field access table (read-only).
    /// Phase 6 m3-002: provides access to FieldAccessInfo for FieldAccess nodes.
    #[must_use]
    pub fn field_access_info(&self) -> &FieldAccessSideTable {
        &self.field_access_table
    }

    /// Borrow the field access table (mutable).
    pub fn field_access_info_mut(&mut self) -> &mut FieldAccessSideTable {
        &mut self.field_access_table
    }
}

impl Index<IrNodeId> for IrArena {
    type Output = IrNodeData;

    fn index(&self, id: IrNodeId) -> &Self::Output {
        &self.nodes[id.index()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn alloc_returns_increasing_ids() {
        let mut a = IrArena::new();
        let a1 = a.alloc(IrKind::Placeholder, span());
        let a2 = a.alloc(IrKind::Module, span());
        assert_eq!(a1.get(), 1);
        assert_eq!(a2.get(), 2);
    }

    #[test]
    fn index_returns_node_data() {
        let mut a = IrArena::new();
        let id = a.alloc(IrKind::Lambda, span());
        assert_eq!(a[id].kind, IrKind::Lambda);
        assert_eq!(a[id].span, span());
    }

    #[test]
    fn get_mut_allows_elaborator_to_update_class_and_effects() {
        let mut a = IrArena::new();
        let id = a.alloc(IrKind::Let, span());
        let d = a.get_mut(id).unwrap();
        d.lin_class = crate::node::LinClass::Linear;
        d.effect_row = crate::node::EffectRowId(7);
        assert_eq!(a[id].lin_class, crate::node::LinClass::Linear);
        assert_eq!(a[id].effect_row, crate::node::EffectRowId(7));
    }

    #[test]
    fn len_and_empty_track_state() {
        let mut a = IrArena::new();
        assert!(a.is_empty());
        a.alloc(IrKind::Placeholder, span());
        assert_eq!(a.len(), 1);
    }

    #[test]
    fn var_and_literal_have_no_children() {
        let mut a = IrArena::new();
        let var_id = a.alloc(IrKind::Var, span());
        let lit_id = a.alloc(IrKind::Literal, span());
        assert!(a.children(var_id).is_empty());
        assert!(a.children(lit_id).is_empty());
    }

    #[test]
    fn let_has_one_child() {
        let mut a = IrArena::new();
        let value_id = a.alloc(IrKind::Var, span());
        let let_id = a.alloc_with_children(IrKind::Let, span(), [value_id]);
        assert_eq!(a.children(let_id).len(), 1);
        assert_eq!(a.children(let_id)[0], value_id);
    }

    #[test]
    fn app_has_callee_plus_args() {
        let mut a = IrArena::new();
        let callee_id = a.alloc(IrKind::Var, span());
        let arg1_id = a.alloc(IrKind::Literal, span());
        let arg2_id = a.alloc(IrKind::Literal, span());
        let app_id = a.alloc_with_children(IrKind::App, span(), [callee_id, arg1_id, arg2_id]);
        let children = a.children(app_id);
        assert_eq!(children.len(), 3);
        assert_eq!(children[0], callee_id);
        assert_eq!(children[1], arg1_id);
        assert_eq!(children[2], arg2_id);
    }

    #[test]
    fn empty_module_has_no_items_children() {
        let mut a = IrArena::new();
        let mod_id = a.alloc(IrKind::Module, span());
        assert!(a.children(mod_id).is_empty());
    }

    #[test]
    fn module_with_items_has_item_children() {
        let mut a = IrArena::new();
        let item1 = a.alloc(IrKind::Var, span());
        let item2 = a.alloc(IrKind::Literal, span());
        let item3 = a.alloc(IrKind::Var, span());
        let mod_id = a.alloc_with_children(IrKind::Module, span(), [item1, item2, item3]);
        let children = a.children(mod_id);
        assert_eq!(children.len(), 3);
        assert_eq!(children[0], item1);
        assert_eq!(children[1], item2);
        assert_eq!(children[2], item3);
    }

    #[test]
    fn children_mut_allows_building_children_after_alloc() {
        let mut a = IrArena::new();
        let child1 = a.alloc(IrKind::Var, span());
        let child2 = a.alloc(IrKind::Literal, span());
        let parent_id = a.alloc(IrKind::Let, span());

        // Add children after allocation
        {
            let children = a.children_mut(parent_id).unwrap();
            children.push(child1);
            children.push(child2);
        }

        let result_children = a.children(parent_id);
        assert_eq!(result_children.len(), 2);
        assert_eq!(result_children[0], child1);
        assert_eq!(result_children[1], child2);
    }

    #[test]
    fn size_budget_assertion() {
        // IrNodeData is 20 bytes (u8 + u8 + u32 + 12-byte Span with alignment).
        // Phase-1 AC budget: ≤ 48 bytes. Side-table keeps IrNodeData clean.
        assert!(std::mem::size_of::<IrNodeData>() <= 48);
    }
}
