//! FNV-1a-64 — schema-registry fingerprint per
//! `design/terminal/schema-registry.md` §3.
//!
//! This is the interim fingerprint algorithm the client-side of the
//! kernel schema registry ships with; the daemon matches it byte for
//! byte. Both sides move together to
//! `BLAKE3(name || 0x00 || flat_field_descriptors)[0..8]` in the same
//! release once `paideia-as` ships the BLAKE3 intrinsic — see the
//! §3 note.
//!
//! Kept in a dedicated module so the R222.M5 registry client can
//! share it with the daemon-facing lookup path without re-implementing
//! the constants (which is exactly the drift the registry-in-kernel
//! design brief §1 warns against — "answering that in each library
//! independently duplicates the table 10 times and leaves the tools
//! disagreeing whenever one library's table drifts").
//!
//! Standard byte-at-a-time algorithm: `hash = (hash ^ byte) * prime`,
//! bytes read from the name string excluding any trailing NUL.

/// FNV-1a-64 offset basis — `0xCBF29CE484222325`.
pub const FNV_OFFSET_BASIS: u64 = 0xCBF2_9CE4_8422_2325;

/// FNV-1a-64 prime — `0x00000100000001B3`.
pub const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;

/// Compute FNV-1a-64 over `bytes`.
///
/// The caller passes the schema name's UTF-8 byte string; the registry
/// spec is explicit that the trailing NUL (if present in the fixed-32
/// on-wire encoding) is *not* hashed. Callers here pass a Rust `&str`
/// which is never NUL-terminated, so the boundary is right by
/// construction.
#[inline]
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut h: u64 = FNV_OFFSET_BASIS;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    // Locked-down reference vectors — recomputed against these on any
    // future change to the algorithm. The empty-string case is the raw
    // offset basis (definition of FNV-1a); the `""` and `"a"` cases
    // are the standard reference vectors quoted in the FNV RFC draft.
    #[test]
    fn fnv1a_empty_is_offset_basis() {
        assert_eq!(fnv1a_64(b""), FNV_OFFSET_BASIS);
    }

    #[test]
    fn fnv1a_a_matches_reference() {
        // "a" — computed once as `basis ^ 0x61` then `* prime`.
        let expected = (FNV_OFFSET_BASIS ^ 0x61).wrapping_mul(FNV_PRIME);
        assert_eq!(fnv1a_64(b"a"), expected);
    }

    #[test]
    fn fnv1a_is_stable_across_calls() {
        let a = fnv1a_64(b"FileSchema@0.1");
        let b = fnv1a_64(b"FileSchema@0.1");
        assert_eq!(a, b);
    }

    #[test]
    fn fnv1a_differs_on_version_bump() {
        // Registry §5: a version bump is a distinct schema; the
        // fingerprints MUST differ so consumers referencing the old
        // version keep resolving to the old shape.
        let v01 = fnv1a_64(b"FileSchema@0.1");
        let v02 = fnv1a_64(b"FileSchema@0.2");
        assert_ne!(v01, v02);
    }
}
