//! Arena allocator for IR nodes (parallels [`paideia_as_ast::AstArena`]).
//!
//! [`paideia_as_ast::AstArena`]: paideia_as_ast::AstArena

use paideia_as_diagnostics::Span;
use smallvec::SmallVec;
use std::collections::HashSet;
use std::ops::Index;

use crate::addr_of::AddrOfSideTable;
use crate::binding_name::BindingNameTable;
use crate::call_meta::CallSideTable;
use crate::constant_pool::ConstantPoolTable;
use crate::data::DataSideTable;
use crate::enum_layout::{
    EnumConsSideTable, EnumDiscriminantSideTable, EnumVariantPayloadTable, EnumVariantPrimitiveWidthTable, FinalisedEnumLayoutTable, MatchArmMetaSideTable,
    MatchDispatchMetaSideTable, MatchJumpTableArmValuesSideTable, MatchScrutineeTable,
};
use crate::instruction::InstructionSideTable;
use crate::lambda_param::LambdaParamTable;
use crate::let_meta::LetMetaTable;
use crate::literal_bytes::LiteralBytesTable;
use crate::literal_value::LiteralValueTable;
use crate::loop_meta::LoopMetaTable;
use crate::node::{IrKind, IrNodeData, IrNodeId};
use crate::record_layout::{FieldAccessSideTable, RecordLayoutTable, FinalisedLayoutTable};
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
    /// Side-table: literal byte payloads indexed by StringLiteral node ID.
    /// PA10-002: populated by elaborator; consumed by emitter for string interning.
    literal_bytes_table: LiteralBytesTable,
    /// Side-table: data entries (.rodata/.data/.bss) indexed by Let node ID.
    data_table: DataSideTable,
    /// Side-table: binding names for Let nodes indexed by Let node ID.
    binding_name_table: BindingNameTable,
    /// Side-table: lambda parameter node IDs indexed by Lambda node ID.
    /// PA8-m1-001c: tracks the parameter pattern nodes for each Lambda.
    lambda_param_table: LambdaParamTable,
    /// Side-table: let binding mutability metadata indexed by Let node ID.
    /// Phase 6 m5-002: tracks whether a let binding is mutable (let mut x : T = ...).
    let_meta_table: LetMetaTable,
    /// Side-table: top-level binding symbol table.
    symbol_table: SymbolTable,
    /// Side-table: field access metadata indexed by FieldAccess node ID.
    /// Phase 6 m3-002: populated by elaborator, consumed by EmitWalker.
    field_access_table: FieldAccessSideTable,
    /// Side-table: record constructor type mapping indexed by RecordCons node ID.
    /// Phase 6 m3-004: maps RecordCons nodes to their RecordTypeId for layout lookup.
    record_layout_table: RecordLayoutTable,
    /// Side-table: enum constructor metadata indexed by EnumCons node ID.
    /// PA-r17-007: maps EnumCons nodes to their EnumTypeId and variant index.
    enum_cons_table: EnumConsSideTable,
    /// Side-table: finalised enum layouts indexed by EnumTypeId.
    /// PA-r17-007: populated during emission; consumed for register/stack form dispatch.
    enum_finalised_layouts: FinalisedEnumLayoutTable,
    /// Side-table: finalised record layouts indexed by RecordTypeId.
    /// Issue #1157: populated after record layout finalization; consumed for tight-pack encoding.
    finalised_record_layouts: FinalisedLayoutTable,
    /// Side-table: per-variant record payload type lookup indexed by (EnumTypeId, variant_idx).
    /// Issue #1054: populated during elaboration to enable type-directed code generation.
    enum_variant_payload_table: EnumVariantPayloadTable,
    /// Side-table: per-variant primitive payload width lookup indexed by (EnumTypeId, variant_idx).
    /// Issue #1160: populated during elaboration for tight-pack encoding of primitive payloads.
    enum_variant_primitive_widths: EnumVariantPrimitiveWidthTable,
    /// Side-table: enum discriminant extraction metadata indexed by EnumDiscriminant node ID.
    /// PA-r17-008: maps EnumDiscriminant nodes to their EnumTypeId for discriminant load emission.
    enum_disc_info_table: EnumDiscriminantSideTable,
    /// Side-table: match arm pattern metadata indexed by match arm IrNodeId.
    /// PA-r17-008: records variant_index, payload_binder, is_default for each arm.
    match_arm_meta_table: MatchArmMetaSideTable,
    /// Side-table: match scrutinee type mapping indexed by Match node ID.
    /// PA-r17-008: maps Match nodes to their scrutinee's EnumTypeId for layout dispatch.
    match_scrutinee_table: MatchScrutineeTable,
    /// Side-table: match dispatch strategy metadata indexed by Match node ID.
    /// PA-r15-009c (#1055): populated during elaboration for matches with @jump_table attribute.
    match_dispatch_meta_table: MatchDispatchMetaSideTable,
    /// Side-table: match jump-table arm values indexed by Match node ID.
    /// PA-r15-009b (#1032): populated during elaboration for matches with @jump_table attribute.
    /// Stores (arm_value, arm_idx) pairs for rodata jump-table synthesis.
    match_jump_table_arm_values_table: MatchJumpTableArmValuesSideTable,
    /// Side-table: address-of metadata indexed by Borrow node ID.
    /// PA10-006u: populated by elaborator for `& sym` in static initializers.
    addr_of_table: AddrOfSideTable,
    /// Side-table: set of Let node IDs marked as public (`pub let`).
    /// PA904: tracks which let bindings have explicit `pub` visibility for global export.
    public_lets: HashSet<IrNodeId>,
    /// Side-table: call site metadata indexed by App node ID.
    /// PA-r17-004: populated by pre-emit pass; consumed by emit_walker for indirect/direct dispatch.
    call_sites: CallSideTable,
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
            literal_bytes_table: LiteralBytesTable::new(),
            data_table: DataSideTable::new(),
            binding_name_table: BindingNameTable::new(),
            lambda_param_table: LambdaParamTable::new(),
            let_meta_table: LetMetaTable::new(),
            symbol_table: SymbolTable::new(),
            field_access_table: FieldAccessSideTable::new(),
            record_layout_table: RecordLayoutTable::new(),
            enum_cons_table: EnumConsSideTable::new(),
            enum_finalised_layouts: FinalisedEnumLayoutTable::new(),
            finalised_record_layouts: FinalisedLayoutTable::new(),
            enum_variant_payload_table: EnumVariantPayloadTable::new(),
            enum_variant_primitive_widths: EnumVariantPrimitiveWidthTable::new(),
            enum_disc_info_table: EnumDiscriminantSideTable::new(),
            match_arm_meta_table: MatchArmMetaSideTable::new(),
            match_scrutinee_table: MatchScrutineeTable::new(),
            match_dispatch_meta_table: MatchDispatchMetaSideTable::new(),
            match_jump_table_arm_values_table: MatchJumpTableArmValuesSideTable::new(),
            addr_of_table: AddrOfSideTable::new(),
            public_lets: HashSet::new(),
            call_sites: CallSideTable::new(),
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

    /// Borrow the literal bytes side-table (read-only).
    /// PA10-002: provides access to raw byte payloads for StringLiteral nodes.
    #[must_use]
    pub fn literal_bytes(&self) -> &LiteralBytesTable {
        &self.literal_bytes_table
    }

    /// Borrow the literal bytes side-table (mutable).
    pub fn literal_bytes_mut(&mut self) -> &mut LiteralBytesTable {
        &mut self.literal_bytes_table
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

    /// Borrow the lambda parameter side-table (read-only).
    /// PA8-m1-001c: provides access to parameter node IDs for Lambda nodes.
    #[must_use]
    pub fn lambda_params(&self) -> &LambdaParamTable {
        &self.lambda_param_table
    }

    /// Borrow the lambda parameter side-table (mutable).
    pub fn lambda_params_mut(&mut self) -> &mut LambdaParamTable {
        &mut self.lambda_param_table
    }

    /// Borrow the let metadata side-table (read-only).
    /// Phase 6 m5-002: provides access to LetInfo for Let nodes.
    #[must_use]
    pub fn let_meta(&self) -> &LetMetaTable {
        &self.let_meta_table
    }

    /// Borrow the let metadata side-table (mutable).
    pub fn let_meta_mut(&mut self) -> &mut LetMetaTable {
        &mut self.let_meta_table
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

    /// Borrow the record layout table (read-only).
    /// Phase 6 m3-004: provides RecordTypeId for RecordCons nodes.
    #[must_use]
    pub fn record_layout_table(&self) -> &RecordLayoutTable {
        &self.record_layout_table
    }

    /// Borrow the record layout table (mutable).
    pub fn record_layout_table_mut(&mut self) -> &mut RecordLayoutTable {
        &mut self.record_layout_table
    }

    /// Borrow the enum cons side-table (read-only).
    /// PA-r17-007: provides EnumConsInfo for EnumCons nodes.
    #[must_use]
    pub fn enum_cons_info(&self) -> &EnumConsSideTable {
        &self.enum_cons_table
    }

    /// Borrow the enum cons side-table (mutable).
    pub fn enum_cons_info_mut(&mut self) -> &mut EnumConsSideTable {
        &mut self.enum_cons_table
    }

    /// Borrow the enum layout table (read-only).
    /// PA-r17-007: provides EnumLayout for enum types.
    #[must_use]
    pub fn enum_layout_table(&self) -> &FinalisedEnumLayoutTable {
        &self.enum_finalised_layouts
    }

    /// Borrow the enum layout table (mutable).
    pub fn enum_layout_table_mut(&mut self) -> &mut FinalisedEnumLayoutTable {
        &mut self.enum_finalised_layouts
    }

    /// Borrow the finalised record layouts table (read-only).
    /// Issue #1157: provides RecordLayout for records used in tight-pack encoding.
    #[must_use]
    pub fn finalised_record_layouts(&self) -> &FinalisedLayoutTable {
        &self.finalised_record_layouts
    }

    /// Borrow the finalised record layouts table (mutable).
    pub fn finalised_record_layouts_mut(&mut self) -> &mut FinalisedLayoutTable {
        &mut self.finalised_record_layouts
    }

    /// Borrow the enum discriminant info side-table (read-only).
    /// PA-r17-008: provides EnumTypeId for EnumDiscriminant nodes.
    #[must_use]
    pub fn enum_disc_info(&self) -> &EnumDiscriminantSideTable {
        &self.enum_disc_info_table
    }

    /// Borrow the enum discriminant info side-table (mutable).
    pub fn enum_disc_info_mut(&mut self) -> &mut EnumDiscriminantSideTable {
        &mut self.enum_disc_info_table
    }

    /// Borrow the enum variant payload side-table (read-only).
    /// Issue #1054: provides RecordTypeId for per-variant payload lookup.
    #[must_use]
    pub fn enum_variant_payload_table(&self) -> &EnumVariantPayloadTable {
        &self.enum_variant_payload_table
    }

    /// Borrow the enum variant payload side-table (mutable).
    pub fn enum_variant_payload_table_mut(&mut self) -> &mut EnumVariantPayloadTable {
        &mut self.enum_variant_payload_table
    }

    /// Borrow the enum variant primitive width side-table (read-only).
    /// Issue #1160: provides byte width for per-variant primitive payload tight-pack encoding.
    #[must_use]
    pub fn enum_variant_primitive_widths(&self) -> &EnumVariantPrimitiveWidthTable {
        &self.enum_variant_primitive_widths
    }

    /// Borrow the enum variant primitive width side-table (mutable).
    pub fn enum_variant_primitive_widths_mut(&mut self) -> &mut EnumVariantPrimitiveWidthTable {
        &mut self.enum_variant_primitive_widths
    }

    /// Borrow the match arm metadata side-table (read-only).
    /// PA-r17-008: provides MatchArmMeta for match arm nodes.
    #[must_use]
    pub fn match_arm_meta(&self) -> &MatchArmMetaSideTable {
        &self.match_arm_meta_table
    }

    /// Borrow the match arm metadata side-table (mutable).
    pub fn match_arm_meta_mut(&mut self) -> &mut MatchArmMetaSideTable {
        &mut self.match_arm_meta_table
    }

    /// Borrow the match scrutinee type table (read-only).
    /// PA-r17-008: provides EnumTypeId for Match nodes.
    #[must_use]
    pub fn match_scrutinee_table(&self) -> &MatchScrutineeTable {
        &self.match_scrutinee_table
    }

    /// Borrow the match scrutinee type table (mutable).
    pub fn match_scrutinee_table_mut(&mut self) -> &mut MatchScrutineeTable {
        &mut self.match_scrutinee_table
    }

    /// Borrow the match dispatch metadata side-table (read-only).
    /// PA-r15-009c (#1055): provides MatchDispatchMeta for Match nodes with @jump_table.
    #[must_use]
    pub fn match_dispatch_meta(&self) -> &MatchDispatchMetaSideTable {
        &self.match_dispatch_meta_table
    }

    /// Borrow the match dispatch metadata side-table (mutable).
    pub fn match_dispatch_meta_mut(&mut self) -> &mut MatchDispatchMetaSideTable {
        &mut self.match_dispatch_meta_table
    }

    /// Borrow the match jump-table arm values side-table (read-only).
    /// PA-r15-009b (#1032): provides arm value/index pairs for rodata jump-table synthesis.
    #[must_use]
    pub fn match_jump_table_arm_values(&self) -> &MatchJumpTableArmValuesSideTable {
        &self.match_jump_table_arm_values_table
    }

    /// Borrow the match jump-table arm values side-table (mutable).
    pub fn match_jump_table_arm_values_mut(&mut self) -> &mut MatchJumpTableArmValuesSideTable {
        &mut self.match_jump_table_arm_values_table
    }

    /// Borrow the address-of side-table (read-only).
    /// PA10-006u: provides access to AddrOfMeta for Borrow nodes in static initializers.
    #[must_use]
    pub fn addr_of(&self) -> &AddrOfSideTable {
        &self.addr_of_table
    }

    /// Borrow the address-of side-table (mutable).
    pub fn addr_of_mut(&mut self) -> &mut AddrOfSideTable {
        &mut self.addr_of_table
    }

    /// Get the set of Let node IDs marked as public.
    #[must_use]
    pub fn public_lets(&self) -> &HashSet<IrNodeId> {
        &self.public_lets
    }

    /// Borrow the public_lets set (mutable).
    pub fn public_lets_mut(&mut self) -> &mut HashSet<IrNodeId> {
        &mut self.public_lets
    }

    /// Check if a Let node ID is marked as public.
    #[must_use]
    pub fn is_public_let(&self, id: IrNodeId) -> bool {
        self.public_lets.contains(&id)
    }

    /// Borrow the call sites side-table (read-only).
    /// PA-r17-004: provides access to CallMeta for App nodes.
    #[must_use]
    pub fn call_sites(&self) -> &CallSideTable {
        &self.call_sites
    }

    /// Borrow the call sites side-table (mutable).
    pub fn call_sites_mut(&mut self) -> &mut CallSideTable {
        &mut self.call_sites
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
