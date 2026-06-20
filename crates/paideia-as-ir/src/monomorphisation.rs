//! Monomorphisation table — keyed on (function-id, [TypeId; arity]).
//!
//! Each generic function `fn foo<T, U>(...)` is monomorphised per
//! distinct (T, U) instantiation. The table records which monomorphic
//! variants have been generated and provides the lowering name.

use crate::node::IrNodeId;
use std::collections::HashMap;

/// A stable type identifier for monomorphisation instantiation.
///
/// Opaque wrapper around a u32; exact representation is determined by the
/// type interner in `paideia-as-types`. This crate uses it as a key component
/// in `MonoKey` without direct dependency on the types crate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct TypeId(pub u32);

impl TypeId {
    /// Construct a TypeId from an index (for testing).
    #[must_use]
    pub fn from_index(n: u32) -> Self {
        TypeId(n)
    }
}

/// Key for monomorphisation table lookup.
///
/// Identifies a unique monomorphic variant by function id and type arguments.
/// The `type_args` vector respects argument order — two keys with the same
/// type arguments in different order are distinct.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct MonoKey {
    /// The generic function being instantiated.
    pub function_id: IrNodeId,
    /// Type arguments for the instantiation (order-sensitive).
    pub type_args: Vec<TypeId>,
}

/// Side-table mapping (function-id, type-args) → monomorphic function id.
///
/// Pattern: m3-007 HandlerSideTable / m1-006 LoadStoreSideTable /
/// phase3-m2-004 InstructionSideTable. Keeps generated monomorphic variants
/// stable and deterministic.
///
/// The `insertion_order` vector maintains DDC-deterministic iteration order
/// across runs, ensuring reproducible code generation.
#[derive(Default, Debug, Clone)]
pub struct MonomorphisationTable {
    /// Sparse mapping: (function-id, type-args) → monomorphic function id.
    entries: HashMap<MonoKey, IrNodeId>,
    /// Stable insertion order for DDC determinism.
    insertion_order: Vec<MonoKey>,
}

impl MonomorphisationTable {
    /// Construct an empty monomorphisation table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern or retrieve a monomorphic variant.
    ///
    /// If the key already exists in the table, returns its monomorphic function id.
    /// Otherwise, calls the `generator` closure to create a new monomorphic variant,
    /// inserts it into the table with stable ordering, and returns its id.
    pub fn intern_or_get<F>(&mut self, key: MonoKey, generator: F) -> IrNodeId
    where
        F: FnOnce() -> IrNodeId,
    {
        if let Some(&id) = self.entries.get(&key) {
            return id;
        }
        let id = generator();
        self.insertion_order.push(key.clone());
        self.entries.insert(key, id);
        id
    }

    /// Look up a monomorphic variant by key.
    ///
    /// Returns `None` if the key has not been registered.
    #[must_use]
    pub fn get(&self, key: &MonoKey) -> Option<IrNodeId> {
        self.entries.get(key).copied()
    }

    /// Number of monomorphic variants registered in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no monomorphic variants are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over all (key, monomorphic-id) pairs in insertion order.
    ///
    /// This iterator preserves the insertion order for deterministic output
    /// across multiple runs (DDC compliance).
    pub fn iter_ordered(&self) -> impl Iterator<Item = (&MonoKey, IrNodeId)> {
        self.insertion_order
            .iter()
            .map(move |k| (k, self.entries[k]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a MonoKey with a single type argument.
    fn make_key(func_id: u32, type_id: u32) -> MonoKey {
        MonoKey {
            function_id: IrNodeId::new(func_id).unwrap(),
            type_args: vec![TypeId::from_index(type_id)],
        }
    }

    // Helper to create an IrNodeId.
    fn make_id(n: u32) -> IrNodeId {
        IrNodeId::new(n).unwrap()
    }

    #[test]
    fn mono_table_intern_or_get_returns_same_id_for_same_key() {
        let mut table = MonomorphisationTable::new();
        let key = make_key(1, 0);

        let id1 = table.intern_or_get(key.clone(), || make_id(100));
        let id2 = table.intern_or_get(key, || make_id(999)); // Should not call generator.

        assert_eq!(id1, id2);
        assert_eq!(id1, make_id(100));
    }

    #[test]
    fn mono_table_distinct_keys_get_distinct_ids() {
        let mut table = MonomorphisationTable::new();
        let key1 = make_key(1, 0);
        let key2 = make_key(1, 1);

        let id1 = table.intern_or_get(key1, || make_id(100));
        let id2 = table.intern_or_get(key2, || make_id(200));

        assert_ne!(id1, id2);
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn mono_table_iter_ordered_preserves_insertion_order() {
        let mut table = MonomorphisationTable::new();
        let key1 = make_key(1, 0);
        let key2 = make_key(1, 1);
        let key3 = make_key(2, 0);

        table.intern_or_get(key1.clone(), || make_id(100));
        table.intern_or_get(key2.clone(), || make_id(200));
        table.intern_or_get(key3.clone(), || make_id(300));

        let mut iter = table.iter_ordered();
        assert_eq!(iter.next().map(|(k, _)| k), Some(&key1));
        assert_eq!(iter.next().map(|(k, _)| k), Some(&key2));
        assert_eq!(iter.next().map(|(k, _)| k), Some(&key3));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn mono_key_same_args_in_different_order_distinct() {
        let key1 = MonoKey {
            function_id: make_id(1),
            type_args: vec![TypeId::from_index(0), TypeId::from_index(1)],
        };
        let key2 = MonoKey {
            function_id: make_id(1),
            type_args: vec![TypeId::from_index(1), TypeId::from_index(0)],
        };

        // Keys with type args in different order are distinct.
        assert_ne!(key1, key2);
    }

    #[test]
    fn mono_table_len_tracks_inserts() {
        let mut table = MonomorphisationTable::new();
        assert_eq!(table.len(), 0);

        table.intern_or_get(make_key(1, 0), || make_id(100));
        assert_eq!(table.len(), 1);

        table.intern_or_get(make_key(1, 1), || make_id(200));
        assert_eq!(table.len(), 2);

        // Reinserting the same key doesn't increase len.
        table.intern_or_get(make_key(1, 0), || make_id(999));
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn mono_table_get_returns_none_for_unknown_key() {
        let mut table = MonomorphisationTable::new();
        let key1 = make_key(1, 0);
        let key2 = make_key(1, 1);

        table.intern_or_get(key1, || make_id(100));
        assert_eq!(table.get(&key2), None);
    }
}
