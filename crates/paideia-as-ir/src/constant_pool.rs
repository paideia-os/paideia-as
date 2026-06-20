//! Side-table for interned 64-bit constants in the constant pool.
//!
//! When the pool-constants opt pass detects repeated 64-bit immediates,
//! it interns them in this table to generate PC-relative load offsets.
//! The actual `.rodata` section threading and encode-time emission are
//! deferred to m2 (paideia-link phase).

use std::collections::HashMap;

/// A constant-pool entry holds a 64-bit value and tracks its offset
/// within the `.rodata` section.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolEntry {
    /// The 64-bit constant value.
    pub value: i64,
    /// Offset within the constant pool (in bytes, 8-byte aligned).
    pub offset: u64,
}

/// Side-table mapping 64-bit constant values to their pool offsets.
///
/// This table is populated during the pool-constants opt pass (m1-010).
/// Each unique constant is interned exactly once, and its offset
/// (in units of 8-byte aligned slots) is stored for later PC-relative
/// load emission.
#[derive(Clone, Debug, Default)]
pub struct ConstantPoolTable {
    /// List of pool entries in allocation order.
    entries: Vec<i64>,
    /// Map from constant value to its offset in bytes.
    offsets: HashMap<i64, u64>,
}

impl ConstantPoolTable {
    /// Construct a new empty constant pool table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern a 64-bit constant value and return its offset in bytes.
    ///
    /// If the value is already in the pool, returns the existing offset.
    /// Otherwise, appends it to the pool and returns the new offset.
    pub fn intern(&mut self, value: i64) -> u64 {
        if let Some(&offset) = self.offsets.get(&value) {
            return offset;
        }
        let offset = (self.entries.len() * 8) as u64;
        self.entries.push(value);
        self.offsets.insert(value, offset);
        offset
    }

    /// Return the pool entries in allocation order.
    #[must_use]
    pub fn entries(&self) -> &[i64] {
        &self.entries
    }

    /// Look up the offset of a constant value.
    #[must_use]
    pub fn offset_of(&self, value: i64) -> Option<u64> {
        self.offsets.get(&value).copied()
    }

    /// Return the number of entries in the pool.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the pool is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Return the total size in bytes (assuming 8-byte entries).
    #[must_use]
    pub fn size_bytes(&self) -> u64 {
        (self.entries.len() as u64) * 8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_pool_table_intern_new_value() {
        let mut pool = ConstantPoolTable::new();
        let offset = pool.intern(0x1234_5678_9abc_ddefi64);
        assert_eq!(offset, 0, "First entry should be at offset 0");
    }

    #[test]
    fn constant_pool_table_intern_deterministic() {
        let mut pool = ConstantPoolTable::new();
        let value = 0x1111_2222_3333_4444i64;
        let offset1 = pool.intern(value);
        let offset2 = pool.intern(value);
        assert_eq!(offset1, offset2, "Same value should return same offset");
        assert_eq!(pool.len(), 1, "Pool should contain exactly one entry");
    }

    #[test]
    fn constant_pool_table_multiple_entries() {
        let mut pool = ConstantPoolTable::new();
        let v1 = 0x1111_1111_1111_1111i64;
        let v2 = 0x2222_2222_2222_2222i64;
        let v3 = 0x3333_3333_3333_3333i64;

        let off1 = pool.intern(v1);
        let off2 = pool.intern(v2);
        let off3 = pool.intern(v3);

        assert_eq!(off1, 0);
        assert_eq!(off2, 8);
        assert_eq!(off3, 16);
        assert_eq!(pool.len(), 3);
    }

    #[test]
    fn constant_pool_table_offset_of() {
        let mut pool = ConstantPoolTable::new();
        let value = 0x1234567890abcdefi64;
        pool.intern(value);
        assert_eq!(pool.offset_of(value), Some(0));
        assert_eq!(pool.offset_of(0x5555_6666_7777_8888i64), None);
    }

    #[test]
    fn constant_pool_table_size_bytes() {
        let mut pool = ConstantPoolTable::new();
        pool.intern(1i64);
        pool.intern(2i64);
        pool.intern(3i64);
        assert_eq!(pool.size_bytes(), 24, "Three 8-byte entries = 24 bytes");
    }

    #[test]
    fn constant_pool_table_empty_at_construction() {
        let pool = ConstantPoolTable::new();
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
        assert_eq!(pool.size_bytes(), 0);
    }

    #[test]
    fn constant_pool_table_entries_order() {
        let mut pool = ConstantPoolTable::new();
        let values = vec![0x1111i64, 0x2222i64, 0x3333i64];
        for v in &values {
            pool.intern(*v);
        }
        assert_eq!(pool.entries(), &values[..]);
    }
}
