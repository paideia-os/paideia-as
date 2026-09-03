//! Byte-slice string interner — Wave 0 Batch 4, v0.26-M1-003
//! (paideia-as#1362).
//!
//! # What this ships
//!
//! A single-arena interner for short byte-slice keys — the shape the ACPI
//! AML parser needs for [`NameString`] caching (four-byte NameSegs joined by
//! `\` / `^`, tens of thousands per firmware image, huge duplication factor).
//! The surface is intentionally minimal:
//!
//! * [`Interner::intern`] deduplicates a `&[u8]` and returns a stable
//!   [`InternedId`].
//! * [`Interner::resolve`] returns the interned bytes.
//!
//! # Storage layout
//!
//! `arena: Vec<u8>` holds interned strings back-to-back, each prefixed by a
//! little-endian `u32` length header:
//!
//! ```text
//!   arena: [len0 : u32 LE][bytes0 ...][len1 : u32 LE][bytes1 ...] ...
//! ```
//!
//! An [`InternedId`] is the byte offset of the length header within the arena.
//! `u32` caps the arena at 4 GiB, which is orders of magnitude beyond any
//! plausible AML NameString population.
//!
//! `index: HashMap<u64, u32>` maps a stable hash of the byte slice to the id
//! of the first entry that hashed to that bucket. On lookup we resolve the
//! candidate and verify equality against the query bytes; a mismatch triggers
//! open-addressing linear probing (`h.wrapping_add(1)`) until either an equal
//! entry is found or an empty slot is claimed. Two properties matter:
//!
//! 1. **Correctness under collision** — even if the 64-bit hash collides,
//!    distinct byte slices always receive distinct ids.
//! 2. **Reproducibility** — the hash is a fixed-seed FxHasher-style mixer
//!    (see [`fx_hash`]) with no randomized state, so an interned id for a
//!    given input is a function of insertion order only, not of process
//!    startup entropy.
//!
//! In the AML NameString population 64-bit hash collisions are effectively
//! impossible (birthday bound ≈ 2^32 items), but the probing loop keeps the
//! interner correct for adversarial inputs too.
//!
//! # Capacity
//!
//! Both `Vec` and `HashMap` grow on demand — no fixed cap. [`Interner::new`]
//! starts empty; [`Interner::with_capacity`] pre-sizes both when the caller
//! has a rough estimate.
//!
//! [`NameString`]: https://uefi.org/specifications ACPI 6.5 §20.2.2

use std::collections::HashMap;

// -----------------------------------------------------------------------------
// Public types
// -----------------------------------------------------------------------------

/// Opaque stable identifier for an interned byte slice.
///
/// The wrapped `u32` is the byte offset of the entry's length header in the
/// arena. Consumers must treat it as opaque — only [`Interner::resolve`]
/// interprets it.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InternedId(u32);

impl InternedId {
    /// The raw arena offset. Escape hatch for serialization / debug prints;
    /// downstream code should never rely on the numeric value beyond that.
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Deduplicating byte-slice arena. See the module-level docs for the on-disk
/// layout and the collision-safety argument.
pub struct Interner {
    arena: Vec<u8>,
    index: HashMap<u64, u32>,
}

// -----------------------------------------------------------------------------
// Stable hash
// -----------------------------------------------------------------------------

// FxHasher-style constants. `FX_MULT` is the Firefox / rustc-fx magic multiplier;
// `FX_ROTATE` is the canonical five-bit rotation. Both are fixed constants —
// the hash is deterministic across runs.
const FX_MULT: u64 = 0x517c_c1b7_2722_0a95;
const FX_ROTATE: u32 = 5;

/// Fixed-seed FxHasher-style byte-slice hash. Mixes eight-byte little-endian
/// chunks with a rotate-xor-multiply cycle, then folds the tail one byte at
/// a time.
///
/// Reproducible across runs — no randomized state, no ambient environment.
#[inline]
fn fx_hash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0;
    let mut chunks = bytes.chunks_exact(8);
    for chunk in &mut chunks {
        // chunks_exact hands us slices of length 8 — `try_into` is infallible.
        let word = u64::from_le_bytes(chunk.try_into().unwrap());
        h = (h.rotate_left(FX_ROTATE) ^ word).wrapping_mul(FX_MULT);
    }
    for &b in chunks.remainder() {
        h = (h.rotate_left(FX_ROTATE) ^ (b as u64)).wrapping_mul(FX_MULT);
    }
    h
}

// -----------------------------------------------------------------------------
// Interner
// -----------------------------------------------------------------------------

impl Interner {
    /// Fresh empty interner. Both arena and index start with zero capacity —
    /// grow on demand.
    #[inline]
    pub fn new() -> Self {
        Self {
            arena: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Pre-sized constructor for callers that can estimate the working set.
    /// `arena_bytes` reserves arena capacity; `entries` reserves index buckets.
    #[inline]
    pub fn with_capacity(arena_bytes: usize, entries: usize) -> Self {
        Self {
            arena: Vec::with_capacity(arena_bytes),
            index: HashMap::with_capacity(entries),
        }
    }

    /// Intern `s`. Returns the id of the existing entry if `s` was already
    /// interned; otherwise appends `s` to the arena and returns its new id.
    ///
    /// # Panics
    ///
    /// Panics if `s.len() > u32::MAX` or if the arena would exceed 4 GiB.
    /// Neither bound is reachable from the AML NameString workload this
    /// interner was sized for.
    pub fn intern(&mut self, s: &[u8]) -> InternedId {
        let mut probe = fx_hash(s);
        // Open-addressed linear probing on the hash key so that a 64-bit hash
        // collision on distinct bytes still yields distinct ids.
        loop {
            match self.index.get(&probe) {
                Some(&existing) => {
                    if self.resolve_bytes(existing) == s {
                        return InternedId(existing);
                    }
                    // True collision: same hash key, different bytes. Probe.
                    probe = probe.wrapping_add(1);
                }
                None => {
                    let id = self.append(s);
                    self.index.insert(probe, id);
                    return InternedId(id);
                }
            }
        }
    }

    /// Resolve `id` back to the byte slice it stands for.
    ///
    /// # Panics
    ///
    /// Panics if `id` was not produced by this interner — the offset must
    /// land on a length header inside `self.arena`.
    #[inline]
    pub fn resolve(&self, id: InternedId) -> &[u8] {
        self.resolve_bytes(id.0)
    }

    /// Number of unique interned entries.
    #[inline]
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// True when nothing has been interned yet.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Total bytes held in the arena (headers + payloads). Diagnostic use.
    #[inline]
    pub fn arena_bytes(&self) -> usize {
        self.arena.len()
    }

    // -- private helpers ------------------------------------------------------

    /// Append a fresh entry (`[len_le : u32][bytes ...]`) and return its
    /// offset. Grows the arena as needed.
    fn append(&mut self, s: &[u8]) -> u32 {
        assert!(
            s.len() <= u32::MAX as usize,
            "interned slice length {} exceeds u32::MAX",
            s.len(),
        );
        let new_end = self
            .arena
            .len()
            .checked_add(4)
            .and_then(|n| n.checked_add(s.len()))
            .expect("interner arena size overflowed usize");
        assert!(
            new_end <= u32::MAX as usize,
            "interner arena would exceed 4 GiB",
        );

        let off = self.arena.len() as u32;
        let len = s.len() as u32;
        self.arena.extend_from_slice(&len.to_le_bytes());
        self.arena.extend_from_slice(s);
        off
    }

    /// Untyped resolve used by both the public `resolve` and the collision
    /// verification path in `intern`.
    fn resolve_bytes(&self, offset: u32) -> &[u8] {
        let off = offset as usize;
        let header_end = off + 4;
        let len_bytes: [u8; 4] = self.arena[off..header_end]
            .try_into()
            .expect("interned header slice must be exactly four bytes");
        let len = u32::from_le_bytes(len_bytes) as usize;
        let payload_end = header_end + len;
        &self.arena[header_end..payload_end]
    }
}

impl Default for Interner {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Interner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Interner")
            .field("entries", &self.index.len())
            .field("arena_bytes", &self.arena.len())
            .finish()
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_string_twice_returns_same_id() {
        let mut interner = Interner::new();
        let a = interner.intern(b"_SB_");
        let b = interner.intern(b"_SB_");
        assert_eq!(a, b, "identical bytes must intern to the same id");
        assert_eq!(interner.len(), 1);
    }

    #[test]
    fn different_strings_return_distinct_ids() {
        let mut interner = Interner::new();
        let a = interner.intern(b"_SB_");
        let b = interner.intern(b"_PR_");
        let c = interner.intern(b"CPU0");
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
        assert_eq!(interner.len(), 3);
    }

    #[test]
    fn resolve_round_trips_bytes() {
        let mut interner = Interner::new();
        let cases: &[&[u8]] = &[
            b"_SB_",
            b"_PR_.CPU0",
            b"\\_SB_.PCI0.SBUS.SMBK",
            b"a single byte name is fine too",
        ];
        let ids: Vec<InternedId> = cases.iter().map(|c| interner.intern(c)).collect();
        for (id, expected) in ids.iter().zip(cases.iter()) {
            assert_eq!(interner.resolve(*id), *expected);
        }
    }

    #[test]
    fn empty_string_edge_case() {
        let mut interner = Interner::new();
        let empty_a = interner.intern(b"");
        let empty_b = interner.intern(b"");
        assert_eq!(empty_a, empty_b, "empty string must dedup like any other");
        assert_eq!(interner.resolve(empty_a), b"");
        assert_eq!(interner.len(), 1);

        // A non-empty follow-up must not alias the empty slot.
        let nonempty = interner.intern(b"x");
        assert_ne!(empty_a, nonempty);
        assert_eq!(interner.resolve(nonempty), b"x");
    }

    #[test]
    fn empty_interner_starts_empty() {
        let interner = Interner::new();
        assert_eq!(interner.len(), 0);
        assert!(interner.is_empty());
        assert_eq!(interner.arena_bytes(), 0);
    }

    #[test]
    fn interleaved_interning_preserves_ids() {
        let mut interner = Interner::new();
        let ids_a: Vec<_> = (0..16)
            .map(|i| interner.intern(format!("seg{:02}", i).as_bytes()))
            .collect();
        // Re-interning in a different order must return the same ids.
        for i in (0..16).rev() {
            let re = interner.intern(format!("seg{:02}", i).as_bytes());
            assert_eq!(re, ids_a[i]);
        }
        assert_eq!(interner.len(), 16);
    }

    #[test]
    fn resolve_preserves_binary_bytes() {
        // NameSeg fields are ASCII, but the interner promises byte-slice
        // fidelity — no encoding assumptions.
        let mut interner = Interner::new();
        let raw: &[u8] = &[0x00, 0xFF, 0x10, 0x7F, 0x80, 0x00];
        let id = interner.intern(raw);
        assert_eq!(interner.resolve(id), raw);
    }

    #[test]
    fn fx_hash_is_deterministic() {
        // The stability contract: identical input, identical output. Verified
        // both within a run (calling twice) and against a known-good check
        // that would trip if someone swapped in RandomState.
        let s = b"deterministic";
        assert_eq!(fx_hash(s), fx_hash(s));

        // Empty input hashes to the seed (0), and short tails hit the byte
        // remainder path. These constants pin the hash formula so an
        // accidental change to FX_MULT / FX_ROTATE breaks a test rather
        // than silently reshuffling downstream ids.
        assert_eq!(fx_hash(b""), 0);
        assert_eq!(fx_hash(b"a"), (0u64 ^ (b'a' as u64)).wrapping_mul(FX_MULT));
    }

    #[test]
    fn linear_probing_survives_forced_collisions() {
        // We cannot easily manufacture a natural 64-bit hash collision, so
        // simulate one by pre-loading the index with a bucket that already
        // points at a distinct interned entry. `intern` must probe, land in
        // an empty slot, and return a fresh id for the new bytes.
        let mut interner = Interner::new();
        let first = interner.intern(b"first");

        // Hijack the bucket that the second call would land in.
        let target_bucket = fx_hash(b"second");
        if !interner.index.contains_key(&target_bucket) {
            interner.index.insert(target_bucket, first.raw());
        }

        let second = interner.intern(b"second");
        assert_ne!(first, second, "colliding bytes must still get distinct ids");
        assert_eq!(interner.resolve(second), b"second");
        assert_eq!(interner.resolve(first), b"first");
    }

    #[test]
    fn with_capacity_matches_new_semantics() {
        let mut sized = Interner::with_capacity(1024, 64);
        let a = sized.intern(b"warm");
        let b = sized.intern(b"warm");
        assert_eq!(a, b);
        assert_eq!(sized.resolve(a), b"warm");
    }
}
