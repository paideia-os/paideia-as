//! Closure metadata side-table — Issue #1233.
//!
//! Tracks metadata for closure body Lambdas: mangled symbol name, captured bindings,
//! and environment size. Used by the elaborator's emit pass to register closure body
//! symbols and emit fat-pointer + capture infrastructure.

use crate::node::IrNodeId;
use std::collections::HashMap;

/// Describes a single captured binding in a closure body.
///
/// Captures are inserted in symbol-sort order (per capture.rs analysis).
#[derive(Debug, Clone)]
pub struct CaptureMeta {
    /// Binding name (from the enclosing scope).
    pub name: String,
    /// Byte offset from environment buffer base (captured as [r14 + offset]).
    pub offset: i32,
    /// How the capture is used: by-value (Consume) or by-reference (Reference).
    pub kind: CaptureKind,
}

/// Capture usage semantics.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CaptureKind {
    /// Binding consumed in closure body (by-value capture).
    Consume,
    /// Binding referenced in closure body (by-reference / borrow capture).
    Reference,
}

/// Metadata for a closure body Lambda.
///
/// Keyed by the Lambda node ID of the closure body function.
/// Populated during elaboration (after capture analysis); consumed during emission.
#[derive(Debug, Clone)]
pub struct ClosureMeta {
    /// Mangled symbol name: format!("closure_{parent}_{lambda_id}").
    pub mangled_name: String,
    /// Captured bindings in symbol-sorted order.
    pub captures: Vec<CaptureMeta>,
    /// Total size of capture buffer (sum of 8-byte slots).
    pub env_size: u64,
}

/// Side-table mapping closure body Lambda IDs to their ClosureMeta.
/// Sparse: only entries for Lambdas that are closure bodies (not top-level functions).
#[derive(Debug, Default)]
pub struct ClosureMetaTable {
    entries: HashMap<IrNodeId, ClosureMeta>,
}

impl ClosureMetaTable {
    /// Construct an empty metadata table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Insert a closure body's metadata.
    pub fn insert(&mut self, lambda_id: IrNodeId, meta: ClosureMeta) {
        self.entries.insert(lambda_id, meta);
    }

    /// Retrieve a closure body's metadata by its Lambda ID.
    #[must_use]
    pub fn get(&self, lambda_id: IrNodeId) -> Option<&ClosureMeta> {
        self.entries.get(&lambda_id)
    }

    /// Iterate over all closure bodies and their metadata.
    pub fn iter(&self) -> impl Iterator<Item = (IrNodeId, &ClosureMeta)> {
        self.entries.iter().map(|(&id, meta)| (id, meta))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closure_meta_table_insert_and_get() {
        let mut table = ClosureMetaTable::new();
        let lambda_id = IrNodeId::new(42).unwrap();

        let meta = ClosureMeta {
            mangled_name: "closure_outer_42".to_string(),
            captures: vec![
                CaptureMeta {
                    name: "x".to_string(),
                    offset: 0,
                    kind: CaptureKind::Consume,
                },
                CaptureMeta {
                    name: "y".to_string(),
                    offset: 8,
                    kind: CaptureKind::Reference,
                },
            ],
            env_size: 16,
        };

        table.insert(lambda_id, meta.clone());
        let retrieved = table.get(lambda_id);
        assert!(retrieved.is_some());

        let m = retrieved.unwrap();
        assert_eq!(m.mangled_name, "closure_outer_42");
        assert_eq!(m.captures.len(), 2);
        assert_eq!(m.env_size, 16);
    }

    #[test]
    fn closure_meta_table_missing_entry_returns_none() {
        let table = ClosureMetaTable::new();
        let id = IrNodeId::new(99).unwrap();
        assert!(table.get(id).is_none());
    }

    #[test]
    fn closure_meta_table_iter() {
        let mut table = ClosureMetaTable::new();

        let id1 = IrNodeId::new(1).unwrap();
        let id2 = IrNodeId::new(2).unwrap();

        table.insert(
            id1,
            ClosureMeta {
                mangled_name: "closure_f_1".to_string(),
                captures: vec![],
                env_size: 0,
            },
        );
        table.insert(
            id2,
            ClosureMeta {
                mangled_name: "closure_f_2".to_string(),
                captures: vec![],
                env_size: 8,
            },
        );

        let pairs: Vec<_> = table.iter().collect();
        assert_eq!(pairs.len(), 2);
    }
}
