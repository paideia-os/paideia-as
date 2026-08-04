//! Generic sparse side-table primitive shared by all `IrNodeId`-keyed
//! metadata containers in the IR crate.
//!
//! # Motivation
//!
//! The IR carries per-node metadata via `HashMap<K, V>`-backed side-tables:
//! literal values, addr-of info, borrow info, cast metadata, let metadata,
//! call metadata, field-access info, enum-cons info, record layouts, and so
//! on. Every one of those was hand-written with the same `new` / `insert` /
//! `get` / `get_mut` / `len` / `is_empty` / `iter` shape.
//!
//! `SparseSideTable<K, V>` is the shared primitive. Named table types are
//! declared with the `impl_named_side_table!` macro, which produces the
//! wrapper struct and delegates every method to the primitive.
//!
//! # Design constraints
//!
//! * Preserve the public API of the existing named tables method-for-method
//!   so migration is mechanical.
//! * Avoid forcing `Serialize` / `Deserialize` bounds on all tables — several
//!   named tables derive them, several deliberately don't. The generic
//!   primitive stays serde-free; named tables opt in per-declaration.
//! * `Debug` and `Clone` derived because every existing table derives them.

// #1253: use BTreeMap (deterministic key-ordered iteration) instead of HashMap
// (RandomState-permuted iteration). Root cause of paideia-os #669 kernel-link
// nondeterminism: DataSideTable iteration order was permuted across builds,
// which permuted .rodata/.data/.bss offsets and downstream reloc entries,
// producing a fresh kernel.elf hash per build. BTreeMap requires K: Ord (all
// current keys are IrNodeId / NonZeroU32 newtypes and derive Ord trivially).
use std::collections::BTreeMap;
use std::collections::btree_map::Iter;

/// Generic, sparse `K -> V` map used by every `IrNodeId`-keyed side-table.
///
/// Backed by [`BTreeMap`] for deterministic key-ordered iteration.
/// See #1253 for the emission-boundary hazard this defends against.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SparseSideTable<K, V>
where
    K: Ord,
{
    entries: BTreeMap<K, V>,
}

impl<K, V> Default for SparseSideTable<K, V>
where
    K: Ord,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> SparseSideTable<K, V>
where
    K: Ord,
{
    /// Construct an empty side-table.
    #[must_use]
    pub fn new() -> Self {
        Self { entries: BTreeMap::new() }
    }

    /// Construct an empty side-table with pre-allocated capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        // BTreeMap has no with_capacity — parameter kept for API compat.
        let _ = capacity;
        Self { entries: BTreeMap::new() }
    }

    /// Insert `value` at `key`. Returns any previous value at `key`.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.entries.insert(key, value)
    }

    /// Read the value at `key`, if present.
    pub fn get(&self, key: K) -> Option<&V> {
        self.entries.get(&key)
    }

    /// Mutably access the value at `key`, if present.
    pub fn get_mut(&mut self, key: K) -> Option<&mut V> {
        self.entries.get_mut(&key)
    }

    /// Remove and return the value at `key`, if present.
    pub fn remove(&mut self, key: K) -> Option<V> {
        self.entries.remove(&key)
    }

    /// Check whether the side-table has an entry for `key`.
    pub fn contains_key(&self, key: K) -> bool {
        self.entries.contains_key(&key)
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True if the side-table has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate `(&K, &V)` pairs in **ascending key order** (BTreeMap-backed).
    /// #1253: this deterministic order is load-bearing at the byte-emission
    /// boundary (`cmd_build/elf.rs`) — do not swap the backing store back to
    /// a hash map without also inserting an explicit sort at every consumer.
    pub fn iter(&self) -> Iter<'_, K, V> {
        self.entries.iter()
    }

    /// Direct access to the underlying map. Prefer the wrapper methods; this
    /// exists as an escape hatch for callers that need `BTreeMap` semantics
    /// the wrapper doesn't expose (e.g. `range()`, `entry` API).
    pub fn entries(&self) -> &BTreeMap<K, V> {
        &self.entries
    }
}

impl<K, V> SparseSideTable<K, V>
where
    K: Ord,
{
    /// Clear all entries. Kept in a separate `impl` block so it doesn't
    /// perturb the mechanical grep of the old named-table APIs.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Declare a named side-table type over the shared `SparseSideTable` primitive.
///
/// Produces a struct with the given name, key, and value types, plus the full
/// `new` / `with_capacity` / `insert` / `get` / `get_mut` / `remove` /
/// `contains_key` / `len` / `is_empty` / `iter` / `entries` / `clear` API
/// delegated to the inner primitive. Derives `Debug`, `Clone`, `Default`.
///
/// The `Debug` bound on the value type is implied by the derive; if a caller
/// needs a value type that isn't `Debug`, drop `Debug` from the derive at the
/// call site.
///
/// # Example
///
/// ```ignore
/// use crate::impl_named_side_table;
/// use crate::instruction::RegId;
///
/// impl_named_side_table!(pub struct MyTable, RegId => u64);
///
/// let mut t = MyTable::new();
/// t.insert(RegId(1), 42);
/// assert_eq!(t.get(RegId(1)), Some(&42));
/// ```
#[macro_export]
macro_rules! impl_named_side_table {
    // Serde-aware variant: `impl_named_side_table!(@serde ...)` derives
    // Serialize + Deserialize on the wrapper by delegating through the
    // (also-serde-derived) inner primitive. Use for tables that ship in
    // .paideia note sections or otherwise round-trip through JSON.
    (
        @serde
        $(#[$outer:meta])*
        $vis:vis struct $name:ident, $key:ty => $val:ty
    ) => {
        $(#[$outer])*
        #[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
        #[serde(transparent)]
        $vis struct $name {
            inner: $crate::side_table::SparseSideTable<$key, $val>,
        }

        $crate::impl_named_side_table!(@methods $name, $key => $val);
    };

    (
        $(#[$outer:meta])*
        $vis:vis struct $name:ident, $key:ty => $val:ty
    ) => {
        $(#[$outer])*
        #[derive(Debug, Clone)]
        $vis struct $name {
            inner: $crate::side_table::SparseSideTable<$key, $val>,
        }

        $crate::impl_named_side_table!(@methods $name, $key => $val);
    };

    // Shared method block used by both variants. Not intended to be
    // called by hand — the outer arms delegate here.
    (@methods $name:ident, $key:ty => $val:ty) => {
        impl ::std::default::Default for $name {
            fn default() -> Self { Self::new() }
        }

        impl $name {
            /// Construct an empty table.
            #[must_use]
            pub fn new() -> Self {
                Self { inner: $crate::side_table::SparseSideTable::new() }
            }

            /// Construct an empty table with pre-allocated capacity.
            #[must_use]
            pub fn with_capacity(capacity: usize) -> Self {
                Self { inner: $crate::side_table::SparseSideTable::with_capacity(capacity) }
            }

            /// Insert `value` at `key`. Returns any previous value at `key`.
            pub fn insert(&mut self, key: $key, value: $val) -> Option<$val> {
                self.inner.insert(key, value)
            }

            /// Read the value at `key`, if present.
            pub fn get(&self, key: $key) -> Option<&$val> {
                self.inner.get(key)
            }

            /// Mutably access the value at `key`, if present.
            pub fn get_mut(&mut self, key: $key) -> Option<&mut $val> {
                self.inner.get_mut(key)
            }

            /// Remove and return the value at `key`, if present.
            pub fn remove(&mut self, key: $key) -> Option<$val> {
                self.inner.remove(key)
            }

            /// True if the table has an entry for `key`.
            pub fn contains_key(&self, key: $key) -> bool {
                self.inner.contains_key(key)
            }

            /// Number of entries.
            pub fn len(&self) -> usize {
                self.inner.len()
            }

            /// True if the table has no entries.
            pub fn is_empty(&self) -> bool {
                self.inner.is_empty()
            }

            /// Iterate `(&K, &V)` pairs in **ascending key order** (BTreeMap-backed, #1253).
            pub fn iter(&self) -> ::std::collections::btree_map::Iter<'_, $key, $val> {
                self.inner.iter()
            }

            /// Direct access to the underlying map. Escape hatch for callers
            /// that need `BTreeMap` semantics the wrapper doesn't expose.
            pub fn entries(&self) -> &::std::collections::BTreeMap<$key, $val> {
                self.inner.entries()
            }

            /// Clear all entries.
            pub fn clear(&mut self) {
                self.inner.clear();
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    // Drive-by: #1253 (SparseSideTable BTreeMap-backed) requires Ord on the
    // key. This test's TestKey wasn't updated when that migration landed,
    // so the whole side_table test module fails to compile.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
    struct TestKey(u32);

    #[test]
    fn new_is_empty() {
        let t: SparseSideTable<TestKey, u64> = SparseSideTable::new();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn insert_then_get() {
        let mut t = SparseSideTable::new();
        assert_eq!(t.insert(TestKey(1), 42u64), None);
        assert_eq!(t.get(TestKey(1)), Some(&42));
        assert_eq!(t.get(TestKey(2)), None);
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn insert_returns_previous() {
        let mut t = SparseSideTable::new();
        assert_eq!(t.insert(TestKey(1), 42u64), None);
        assert_eq!(t.insert(TestKey(1), 43u64), Some(42));
        assert_eq!(t.get(TestKey(1)), Some(&43));
    }

    #[test]
    fn get_mut_and_mutate() {
        let mut t = SparseSideTable::new();
        t.insert(TestKey(1), 42u64);
        *t.get_mut(TestKey(1)).unwrap() = 100;
        assert_eq!(t.get(TestKey(1)), Some(&100));
    }

    #[test]
    fn contains_key_and_remove() {
        let mut t = SparseSideTable::new();
        t.insert(TestKey(1), 42u64);
        assert!(t.contains_key(TestKey(1)));
        assert!(!t.contains_key(TestKey(2)));
        assert_eq!(t.remove(TestKey(1)), Some(42));
        assert!(!t.contains_key(TestKey(1)));
    }

    #[test]
    fn iter_visits_all_entries() {
        let mut t = SparseSideTable::new();
        t.insert(TestKey(1), 10u64);
        t.insert(TestKey(2), 20u64);
        t.insert(TestKey(3), 30u64);
        let mut collected: Vec<(TestKey, u64)> =
            t.iter().map(|(k, v)| (*k, *v)).collect();
        collected.sort_by_key(|(k, _)| k.0);
        assert_eq!(collected, vec![(TestKey(1), 10), (TestKey(2), 20), (TestKey(3), 30)]);
    }

    #[test]
    fn clear_empties() {
        let mut t = SparseSideTable::new();
        t.insert(TestKey(1), 42u64);
        t.insert(TestKey(2), 43u64);
        t.clear();
        assert!(t.is_empty());
    }

    // Macro smoke test — the same as the primitive tests but via a named
    // wrapper produced by `impl_named_side_table!`.
    crate::impl_named_side_table!(
        /// A named side-table just used to exercise the macro.
        pub struct MacroSmokeTable, TestKey => u64
    );

    #[test]
    fn macro_named_wrapper_delegates_new_and_len() {
        let t = MacroSmokeTable::new();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn macro_named_wrapper_delegates_insert_and_get() {
        let mut t = MacroSmokeTable::new();
        assert_eq!(t.insert(TestKey(1), 42u64), None);
        assert_eq!(t.get(TestKey(1)), Some(&42));
        assert_eq!(t.insert(TestKey(1), 43u64), Some(42));
        assert!(t.contains_key(TestKey(1)));
    }

    #[test]
    fn macro_named_wrapper_delegates_iter_and_clear() {
        let mut t = MacroSmokeTable::new();
        t.insert(TestKey(1), 10);
        t.insert(TestKey(2), 20);
        assert_eq!(t.iter().count(), 2);
        t.clear();
        assert!(t.is_empty());
    }

    #[test]
    fn macro_named_wrapper_default_is_empty() {
        let t: MacroSmokeTable = Default::default();
        assert!(t.is_empty());
    }
}
