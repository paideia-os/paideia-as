//! Symbol table for runtime resolution: external symbol name → address.
//!
//! Caller-owned map from external symbol name to resolved address.
//! Populated by the caller (WASM jail, loader, dynamic linker) with addresses
//! known before `resolve_symbols` runs. Addresses are `u64` absolute values
//! in the process's virtual address space at the point where the emitted code
//! will execute.
//!
//! Uses `BTreeMap` (not `HashMap`) for two reasons:
//! 1. `no_std + alloc`-only compatibility (`HashMap` requires `std`).
//! 2. Deterministic iteration order — supports byte-identical emit across
//!    hosts (v0.20 exit criterion).

use alloc::collections::BTreeMap;
use alloc::string::String;

/// Caller-owned map from external symbol name → resolved address.
///
/// Populated by the caller (WASM jail, loader, dynamic linker) with addresses
/// known before `resolve_symbols` runs. Addresses are `u64` absolute values
/// in the process's virtual address space at the point where the emitted code
/// will execute.
///
/// Uses `BTreeMap` (not `HashMap`) for two reasons:
/// 1. `no_std + alloc`-only compatibility (`HashMap` requires `std`).
/// 2. Deterministic iteration order — supports byte-identical emit across
///    hosts (v0.20 exit criterion).
#[derive(Default, Debug, Clone)]
pub struct SymbolTable {
    entries: BTreeMap<String, u64>,
}

impl SymbolTable {
    /// Create a new empty symbol table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a symbol with its resolved address.
    pub fn insert(&mut self, name: impl Into<String>, address: u64) {
        self.entries.insert(name.into(), address);
    }

    /// Look up a symbol by name.
    pub fn get(&self, name: &str) -> Option<u64> {
        self.entries.get(name).copied()
    }

    /// Check if a symbol is present in the table.
    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// Return the number of symbols in the table.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the table is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over symbol names and addresses.
    pub fn iter(&self) -> impl Iterator<Item = (&str, u64)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), *v))
    }
}
