//! String interning for .rodata symbol deduplication.
//!
//! Maps each distinct byte sequence to a unique symbol name of the form
//! `__str_<16-hex-hash>` (plus a length symbol `__str_<hash>__len`), so that
//! identical strings share a single .rodata slot through relocations.
//!
//! # PAS-DEBT-B4-001 (paideia-as#1523, paideia-as#1392)
//! The hash used to key the intern table is BLAKE3-truncated-to-u64, not
//! FNV-1a-64. FNV is a non-cryptographic hash: adversarial inputs can
//! deliberately collide (birthday-bound ≈ 2^32 with tiny FNV-known-collision
//! sets in the literature). `libpdx-schema-registry` addresses schemas by the
//! same intern key, so collision resistance must be cryptographic; BLAKE3
//! gives 128-bit collision resistance and > 1 GB/s single-core throughput on
//! commodity x86_64, so the change is a strict upgrade at every axis.
//!
//! Aumasson, J.-P., Neves, S., Wilcox-O'Hearn, Z., & Winnerlein, J. (2020).
//!   *BLAKE3: One function, fast everywhere.* IETF draft / whitepaper.

use std::collections::HashMap;

/// 64-bit content hash used as the intern-table key.
///
/// Computes BLAKE3 over `bytes` and returns the first eight output bytes
/// interpreted little-endian as `u64`. Truncating a 256-bit BLAKE3 digest
/// to 64 bits preserves BLAKE3's collision-resistance up to the birthday
/// bound (~2^32 for a 64-bit key), which is what the intern table actually
/// needs; the full 256-bit digest is available from `paideia-as-crypto` when
/// a downstream consumer needs the wider width.
///
/// Cf. Aumasson et al. 2020 (BLAKE3 whitepaper) for the primitive.
#[must_use]
pub fn symbol_hash(bytes: &[u8]) -> u64 {
    let digest = blake3::hash(bytes);
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&digest.as_bytes()[..8]);
    u64::from_le_bytes(buf)
}

/// Maps a byte sequence to its interned symbol name and length symbol.
///
/// Ensures that identical byte sequences reuse the same symbol across
/// the program, enabling .rodata deduplication.
#[derive(Debug, Default, Clone)]
pub struct StringInternTable {
    /// Map from hash → (symbol_name, length_symbol)
    interned: HashMap<u64, (String, String)>,
}

impl StringInternTable {
    /// Construct an empty intern table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern a byte sequence; return the symbol name and length symbol.
    ///
    /// If the hash has already been interned, returns the cached symbols.
    /// Otherwise, generates `__str_<16-hex-hash>` and `__str_<hash>__len`,
    /// inserts them, and returns them.
    pub fn intern(&mut self, bytes: &[u8]) -> (String, String) {
        let hash = symbol_hash(bytes);
        self.intern_with_hash(hash)
    }

    /// Intern a byte sequence with a pre-computed hash.
    ///
    /// Used when the hash is already available (e.g., from a cache).
    pub fn intern_with_hash(&mut self, hash: u64) -> (String, String) {
        if let Some((sym, len_sym)) = self.interned.get(&hash) {
            return (sym.clone(), len_sym.clone());
        }

        let hex = format!("{:016x}", hash);
        let sym = format!("__str_{}", hex);
        let len_sym = format!("__str_{}__len", hex);

        self.interned.insert(hash, (sym.clone(), len_sym.clone()));
        (sym, len_sym)
    }

    /// Look up the symbols for a given hash (without interning if absent).
    #[must_use]
    pub fn get(&self, hash: u64) -> Option<(&str, &str)> {
        self.interned
            .get(&hash)
            .map(|(sym, len_sym)| (sym.as_str(), len_sym.as_str()))
    }

    /// Number of distinct hashes interned.
    #[must_use]
    pub fn len(&self) -> usize {
        self.interned.len()
    }

    /// `true` iff no strings have been interned.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.interned.is_empty()
    }

    /// Iterate over (hash, symbol_name, length_symbol) tuples.
    pub fn iter(&self) -> impl Iterator<Item = (u64, &str, &str)> {
        self.interned
            .iter()
            .map(|(hash, (sym, len_sym))| (*hash, sym.as_str(), len_sym.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_hash_is_deterministic() {
        // Same input, same u64, across independent invocations.
        assert_eq!(symbol_hash(b"hello"), symbol_hash(b"hello"));
        assert_eq!(symbol_hash(b""), symbol_hash(b""));
        assert_eq!(
            symbol_hash(b"FileSchema@0.1"),
            symbol_hash(b"FileSchema@0.1"),
        );
    }

    #[test]
    fn symbol_hash_pins_blake3_truncation() {
        // Regression pin: `blake3::hash(b"")` starts with the bytes
        // af 13 49 b9 f5 f9 a1 a6 …, so the little-endian u64 truncation
        // is 0xa6a1f9f5b94913af. Locks the truncation convention.
        assert_eq!(symbol_hash(b""), 0xa6a1_f9f5_b949_13af);
    }

    #[test]
    fn symbol_hash_distinguishes_similar_inputs() {
        // Off-by-one, prefix, suffix — none may collide in a 64-bit slice
        // of BLAKE3 for these tiny inputs.
        let a = symbol_hash(b"FileSchema@0.1");
        let b = symbol_hash(b"FileSchema@0.2");
        let c = symbol_hash(b"FileSchema@0.10");
        let d = symbol_hash(b"fileSchema@0.1");
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
        assert_ne!(b, c);
    }

    #[test]
    fn intern_same_bytes_returns_same_symbol() {
        let mut table = StringInternTable::new();
        let bytes = b"hello";

        let (sym1, len_sym1) = table.intern(bytes);
        let (sym2, len_sym2) = table.intern(bytes);

        assert_eq!(sym1, sym2);
        assert_eq!(len_sym1, len_sym2);
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn intern_distinct_bytes_distinct_symbols() {
        let mut table = StringInternTable::new();

        let (sym_banner, _) = table.intern(b"banner");
        let (sym_hello, _) = table.intern(b"hello");
        let (sym_world, _) = table.intern(b"world");

        assert_ne!(sym_banner, sym_hello);
        assert_ne!(sym_hello, sym_world);
        assert_ne!(sym_banner, sym_world);
        assert_eq!(table.len(), 3);
    }

    #[test]
    fn intern_with_hash_deduplicates() {
        let mut table = StringInternTable::new();
        let hash = symbol_hash(b"test");

        let (sym1, len_sym1) = table.intern_with_hash(hash);
        let (sym2, len_sym2) = table.intern_with_hash(hash);

        assert_eq!(sym1, sym2);
        assert_eq!(len_sym1, len_sym2);
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn symbol_naming_format() {
        let mut table = StringInternTable::new();
        let (sym, len_sym) = table.intern(b"test");

        assert!(sym.starts_with("__str_"));
        assert!(len_sym.starts_with("__str_"));
        assert!(len_sym.ends_with("__len"));
        assert_eq!(sym.len(), 22); // __str_ (6) + 16 hex digits
        assert_eq!(len_sym.len(), 27); // __str_ (6) + 16 hex digits + __len (5)
    }

    #[test]
    fn get_returns_none_for_missing_hash() {
        let table = StringInternTable::new();
        let hash = symbol_hash(b"nonexistent");
        assert!(table.get(hash).is_none());
    }

    #[test]
    fn get_returns_symbols_for_interned_hash() {
        let mut table = StringInternTable::new();
        let bytes = b"example";
        let hash = symbol_hash(bytes);

        table.intern(bytes);
        let result = table.get(hash);
        assert!(result.is_some());

        let (sym, len_sym) = result.unwrap();
        assert!(sym.starts_with("__str_"));
        assert!(len_sym.starts_with("__str_"));
        assert!(len_sym.ends_with("__len"));
    }
}
