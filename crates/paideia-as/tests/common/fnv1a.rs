//! FNV-1a 64-bit hash implementation for test fixtures.
//!
//! This module provides the FNV-1a hash function to compute u64 keys from
//! command names at test setup time, which are then baked as u64 literals
//! into the .pdx fixtures (since the runtime cannot yet compute Str hashes).

pub const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
pub const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Compute FNV-1a 64-bit hash of a string.
pub fn fnv1a_u64(name: &str) -> u64 {
    let mut h = FNV_OFFSET;
    for b in name.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}
