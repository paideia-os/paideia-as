//! Module-level metadata representation for IR nodes.
//!
//! Each `IrKind::Module` node carries structural information in the arena's
//! `children_table`. This module provides a side-table (`ModuleSideTable`)
//! mapping Module node ids to their full metadata: field definitions and
//! optional functor signature.
//!
//! Phase-1: the IR builder populates this table; phase-2+ elaborators use
//! it to type-check and emit module metadata sections (e.g., `.paideia.functors`).
//! The side-table design keeps `IrNodeData` compact while allowing unbounded
//! module field metadata.

use std::collections::HashMap;

use crate::IrArena;
use crate::node::IrNodeId;

/// Kind of a module field.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum FieldKind {
    /// A value-typed field.
    Value = 0x01,
    /// A type-typed field.
    Type = 0x02,
    /// A module field (nested).
    Module = 0x03,
    /// A functor-typed field.
    Functor = 0x04,
}

/// A single module field descriptor.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ModuleField {
    /// The name of the field.
    pub name: String,
    /// The kind of this field.
    pub kind: FieldKind,
    /// The IrNodeId of the field's definition.
    pub node_id: IrNodeId,
}

/// Functor signature metadata.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FunctorInfo {
    /// Hash of the parameter signature (caller-computed).
    pub param_signature_hash: u64,
    /// Hash of the result signature (caller-computed).
    pub result_signature_hash: u64,
    /// IrNodeId of the functor body.
    pub body_node_id: IrNodeId,
}

/// Metadata for a Module IR node.
///
/// Each Module node (indexed by `IrNodeId`) has a corresponding `ModuleInfo`
/// in the side-table. Phase-1 captures the static structure of the module:
/// its fields (values, types, modules, functors) and optional functor binding.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ModuleInfo {
    /// The name of this module.
    pub name: String,
    /// Fields declared in this module, in declaration order.
    pub fields: Vec<ModuleField>,
    /// If this module is a functor (parameterized module), its signature.
    /// If None, this is a plain (non-functor) module.
    pub functor: Option<FunctorInfo>,
}

/// Side-table mapping Module IrNodeIds to their metadata.
///
/// Parallels the arena's `children_table` pattern: uses a sparse HashMap
/// indexed by `IrNodeId` so that lookups are O(1) and entries are sparsely
/// distributed.
///
/// Phase-1: populated by the IR builder as modules are constructed.
/// Elaborators (phase-2+) read and mutate entries to populate type and
/// signature information.
#[derive(Default, Debug, Clone)]
pub struct ModuleSideTable {
    /// Sparse table: `table[Module.id()] = Some(ModuleInfo)`.
    /// Only Module nodes have entries; other nodes don't.
    table: HashMap<IrNodeId, ModuleInfo>,
}

impl ModuleSideTable {
    /// Construct an empty module side-table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) the metadata for a Module node.
    ///
    /// Returns the previous entry if one existed; useful for debugging
    /// duplicate-module errors.
    pub fn insert(&mut self, id: IrNodeId, info: ModuleInfo) -> Option<ModuleInfo> {
        self.table.insert(id, info)
    }

    /// Look up the metadata for a Module node.
    ///
    /// Returns `None` if the node was never registered or is not a Module node.
    #[must_use]
    pub fn get(&self, id: IrNodeId) -> Option<&ModuleInfo> {
        self.table.get(&id)
    }

    /// Look up (mutable) the metadata for a Module node.
    ///
    /// Allows elaborators to mutate the module's metadata (e.g., update
    /// field types or functor signature) without cloning.
    pub fn get_mut(&mut self, id: IrNodeId) -> Option<&mut ModuleInfo> {
        self.table.get_mut(&id)
    }

    /// Number of modules registered in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.table.len()
    }

    /// `true` iff no modules are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    /// Iterate over all `(IrNodeId, ModuleInfo)` pairs in the table.
    ///
    /// Iteration order is unspecified (HashMap order).
    pub fn iter(&self) -> impl Iterator<Item = (&IrNodeId, &ModuleInfo)> {
        self.table.iter()
    }
}

/// Pretty-print a module's metadata.
///
/// Returns a human-readable debug string showing the module name, fields,
/// and functor presence. Useful for debugging and snapshot tests.
///
/// # Arguments
///
/// * `arena` - The arena (reserved for future use if node kinds are needed).
/// * `table` - The side-table to look up module info.
/// * `id` - The IrNodeId of the module.
///
/// # Example
///
/// ```ignore
/// let module_info = ModuleInfo {
///     name: "TestModule".to_string(),
///     fields: vec![
///         ModuleField {
///             name: "x".to_string(),
///             kind: FieldKind::Value,
///             node_id: i1,
///         },
///     ],
///     functor: None,
/// };
/// table.insert(module_id, module_info);
/// let s = pretty_module(&arena, &table, module_id);
/// // Outputs something like:
/// // Module<TestModule>:
/// //   fields:
/// //     - x: kind = Value, node = i1
/// //   functor: None
/// ```
#[must_use]
pub fn pretty_module(_arena: &IrArena, table: &ModuleSideTable, id: IrNodeId) -> String {
    let mut s = String::new();

    if let Some(info) = table.get(id) {
        s.push_str(&format!("Module<{}>:\n", info.name));
        s.push_str("  fields:\n");
        for field in &info.fields {
            let kind_str = match field.kind {
                FieldKind::Value => "Value",
                FieldKind::Type => "Type",
                FieldKind::Module => "Module",
                FieldKind::Functor => "Functor",
            };
            s.push_str(&format!(
                "    - {}: kind = {}, node = {}\n",
                field.name, kind_str, field.node_id
            ));
        }
        if let Some(functor_info) = &info.functor {
            s.push_str(&format!(
                "  functor: Some(param_hash={:#x}, result_hash={:#x}, body={})\n",
                functor_info.param_signature_hash,
                functor_info.result_signature_hash,
                functor_info.body_node_id
            ));
        } else {
            s.push_str("  functor: None\n");
        }
    } else {
        s.push_str("Module<?>: [not found in table]\n");
    }

    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_side_table_empty_by_default() {
        let table = ModuleSideTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());
    }

    #[test]
    fn module_info_struct_with_3_fields_inserts_and_retrieves() {
        let mut table = ModuleSideTable::new();
        let module_id = IrNodeId::new(1).unwrap();

        let field1 = ModuleField {
            name: "x".to_string(),
            kind: FieldKind::Value,
            node_id: IrNodeId::new(10).unwrap(),
        };
        let field2 = ModuleField {
            name: "T".to_string(),
            kind: FieldKind::Type,
            node_id: IrNodeId::new(11).unwrap(),
        };
        let field3 = ModuleField {
            name: "sub".to_string(),
            kind: FieldKind::Module,
            node_id: IrNodeId::new(12).unwrap(),
        };

        let info = ModuleInfo {
            name: "TestModule".to_string(),
            fields: vec![field1.clone(), field2.clone(), field3.clone()],
            functor: None,
        };

        table.insert(module_id, info.clone());
        let retrieved = table.get(module_id);
        assert!(retrieved.is_some());
        let retrieved_info = retrieved.unwrap();
        assert_eq!(retrieved_info.name, "TestModule");
        assert_eq!(retrieved_info.fields.len(), 3);
        assert_eq!(retrieved_info.fields[0], field1);
        assert_eq!(retrieved_info.fields[1], field2);
        assert_eq!(retrieved_info.fields[2], field3);
        assert_eq!(retrieved_info.functor, None);
    }

    #[test]
    fn module_info_functor_carries_param_hash_and_body_id() {
        let mut table = ModuleSideTable::new();
        let module_id = IrNodeId::new(5).unwrap();
        let body_id = IrNodeId::new(50).unwrap();

        let functor_info = FunctorInfo {
            param_signature_hash: 0xDEADBEEF,
            result_signature_hash: 0xCAFEBABE,
            body_node_id: body_id,
        };

        let info = ModuleInfo {
            name: "FunctorModule".to_string(),
            fields: vec![],
            functor: Some(functor_info.clone()),
        };

        table.insert(module_id, info);
        let retrieved = table.get(module_id).unwrap();
        assert_eq!(retrieved.functor, Some(functor_info));
        assert_eq!(
            retrieved.functor.as_ref().unwrap().param_signature_hash,
            0xDEADBEEF
        );
        assert_eq!(
            retrieved.functor.as_ref().unwrap().result_signature_hash,
            0xCAFEBABE
        );
        assert_eq!(retrieved.functor.as_ref().unwrap().body_node_id, body_id);
    }

    #[test]
    fn pretty_module_snapshot_contains_name_fields_and_functor_marker() {
        let mut table = ModuleSideTable::new();
        let arena = IrArena::new();
        let module_id = IrNodeId::new(1).unwrap();
        let body_id = IrNodeId::new(99).unwrap();

        let field1 = ModuleField {
            name: "value_field".to_string(),
            kind: FieldKind::Value,
            node_id: IrNodeId::new(10).unwrap(),
        };
        let field2 = ModuleField {
            name: "type_field".to_string(),
            kind: FieldKind::Type,
            node_id: IrNodeId::new(11).unwrap(),
        };

        let functor_info = FunctorInfo {
            param_signature_hash: 0x1111111111111111,
            result_signature_hash: 0x2222222222222222,
            body_node_id: body_id,
        };

        let info = ModuleInfo {
            name: "PrettyTestModule".to_string(),
            fields: vec![field1, field2],
            functor: Some(functor_info),
        };

        table.insert(module_id, info);
        let formatted = pretty_module(&arena, &table, module_id);

        // Verify string contains expected parts
        assert!(
            formatted.contains("PrettyTestModule"),
            "should contain module name, got: {}",
            formatted
        );
        assert!(formatted.contains("fields:"));
        assert!(formatted.contains("value_field"));
        assert!(formatted.contains("type_field"));
        assert!(formatted.contains("kind = Value"));
        assert!(formatted.contains("kind = Type"));
        assert!(formatted.contains("functor: Some("));
        assert!(formatted.contains("param_hash=0x1111111111111111"));
        assert!(formatted.contains("result_hash=0x2222222222222222"));

        // Snapshot-style line count check
        let lines: Vec<&str> = formatted.lines().collect();
        assert!(
            lines.len() >= 5,
            "expected at least 5 lines in formatted output, got: {}",
            lines.len()
        );
    }
}
