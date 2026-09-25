//! R227.M7 `.pds` script-body binary cache fixture corpus.
//!
//! 8 tests, tagged `r227m7-cache-NN`, exercising the cache surface:
//!
//!   01 — Fresh cache lookup returns `None`.
//!   02 — Store, then lookup returns `Some(&cached_bytes)`.
//!   03 — Modified source misses (in-memory `HashMap` key mismatch).
//!   04 — Modified deps miss (deps_hash mismatch).
//!   05 — Explicit `invalidate` returns `true` and drops the entry.
//!   06 — Cache root is created lazily on first `store`.
//!   07 — Two `ScriptCache` instances have disjoint in-memory maps.
//!   08 — 1 KiB payload survives a store/lookup round-trip byte-for-byte.
//!
//! The corpus deliberately never asserts a specific FNV-1a-64 output —
//! locking a hash into the fixture set would freeze the interim FNV
//! choice into the observable API (the crate is expected to move to
//! BLAKE3 in a later release, in lockstep with
//! `load_fingerprint::fnv1a_64`). Instead the tests assert the
//! contracts of the cache (hit/miss, invalidation, isolation, byte-
//! identical round-trip) that any hash-based implementation must
//! uphold.

use std::fs;

use paideia_as_shell_pds::{CacheEntry, ScriptCache};
use tempfile::TempDir;

/// Convenience: shape-check that `CacheEntry` is publicly usable so a
/// future refactor that hides the type would trip the corpus. The
/// value produced here is deliberately not asserted against — the
/// point is compile-time reachability of the exported name.
fn _cache_entry_shape_check() -> CacheEntry {
    CacheEntry {
        source_hash: 0,
        deps_hash: 0,
        cached_bytes: Vec::new(),
        cached_at: 0,
    }
}

// ────────────────────────────────────────────────────────────────
// r227m7-cache-01 — fresh cache lookup returns None
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m7_cache_01_fresh_lookup_is_miss() {
    let tmp = TempDir::new().expect("r227m7-cache-01: tempdir");
    let cache = ScriptCache::new(tmp.path().to_path_buf());
    let hit = cache.lookup(b"echo hello\n", &[]);
    assert!(
        hit.is_none(),
        "r227m7-cache-01: an untouched cache must miss on every lookup, got {hit:?}"
    );
}

// ────────────────────────────────────────────────────────────────
// r227m7-cache-02 — store then lookup returns the same bytes
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m7_cache_02_store_then_lookup_hits() {
    let tmp = TempDir::new().expect("r227m7-cache-02: tempdir");
    let mut cache = ScriptCache::new(tmp.path().to_path_buf());
    let source: &[u8] = b"#capability \"fs.read\"\n\nls\n";
    let deps: [&[u8]; 1] = [b"module a bytes\n"];
    let cached: Vec<u8> = vec![0xC0, 0xDE, 0xCA, 0xFE];

    let stored_hash = cache.store(source, &deps, cached.clone());
    assert_ne!(
        stored_hash, 0,
        "r227m7-cache-02: sanity — a non-empty source should hash to non-zero"
    );

    let got = cache.lookup(source, &deps);
    assert_eq!(
        got,
        Some(&cached),
        "r227m7-cache-02: a stored (source, deps) pair must hit with the exact bytes"
    );
}

// ────────────────────────────────────────────────────────────────
// r227m7-cache-03 — modified source misses at the HashMap step
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m7_cache_03_modified_source_misses() {
    let tmp = TempDir::new().expect("r227m7-cache-03: tempdir");
    let mut cache = ScriptCache::new(tmp.path().to_path_buf());
    let deps: [&[u8]; 1] = [b"dep\n"];
    let original: &[u8] = b"echo one\n";
    let modified: &[u8] = b"echo two\n";
    cache.store(original, &deps, vec![1, 2, 3]);

    let got = cache.lookup(modified, &deps);
    assert!(
        got.is_none(),
        "r227m7-cache-03: a source-byte edit must miss (different source_hash key), got {got:?}"
    );

    // Sanity: the original still hits so we know the miss above is not
    // just an empty cache.
    let orig_hit = cache.lookup(original, &deps);
    assert!(
        orig_hit.is_some(),
        "r227m7-cache-03: sanity — the original source must still hit"
    );
}

// ────────────────────────────────────────────────────────────────
// r227m7-cache-04 — modified deps miss at the deps_hash comparison
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m7_cache_04_modified_deps_misses() {
    let tmp = TempDir::new().expect("r227m7-cache-04: tempdir");
    let mut cache = ScriptCache::new(tmp.path().to_path_buf());
    let source: &[u8] = b"echo stable\n";
    let original_deps: [&[u8]; 2] = [b"dep-a v1\n", b"dep-b v1\n"];
    let modified_deps: [&[u8]; 2] = [b"dep-a v2\n", b"dep-b v1\n"];
    cache.store(source, &original_deps, vec![9, 9, 9]);

    let got = cache.lookup(source, &modified_deps);
    assert!(
        got.is_none(),
        "r227m7-cache-04: a dep-byte edit must miss (deps_hash mismatch), got {got:?}"
    );

    let orig_hit = cache.lookup(source, &original_deps);
    assert!(
        orig_hit.is_some(),
        "r227m7-cache-04: sanity — the original deps must still hit"
    );
}

// ────────────────────────────────────────────────────────────────
// r227m7-cache-05 — invalidate returns true; subsequent lookup misses
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m7_cache_05_invalidate_drops_entry() {
    let tmp = TempDir::new().expect("r227m7-cache-05: tempdir");
    let mut cache = ScriptCache::new(tmp.path().to_path_buf());
    let source: &[u8] = b"echo drop\n";
    let deps: [&[u8]; 0] = [];
    let hash = cache.store(source, &deps, vec![0x42]);

    let removed = cache.invalidate(hash);
    assert!(
        removed,
        "r227m7-cache-05: invalidate on a live entry must return true"
    );

    let post = cache.lookup(source, &deps);
    assert!(
        post.is_none(),
        "r227m7-cache-05: after invalidate the entry must be gone, got {post:?}"
    );

    // A second invalidate on the same (now-absent) hash must report
    // false — the return value is the "was there something to
    // invalidate?" signal, not a "does the key still exist?" ping.
    let second = cache.invalidate(hash);
    assert!(
        !second,
        "r227m7-cache-05: invalidate on an already-gone entry must return false"
    );
}

// ────────────────────────────────────────────────────────────────
// r227m7-cache-06 — cache root is created lazily on first store
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m7_cache_06_root_created_on_first_store() {
    // Point the cache at a subdirectory that does NOT yet exist. `new`
    // must not create it (no I/O in the constructor), but the first
    // `store` must — the persistence path is documented as best
    // effort, but on a writable parent it should succeed.
    let tmp = TempDir::new().expect("r227m7-cache-06: tempdir");
    let root = tmp.path().join("nested").join("cache-dir");
    assert!(
        !root.exists(),
        "r227m7-cache-06: sanity — the nested cache root should not exist yet"
    );

    let mut cache = ScriptCache::new(root.clone());
    assert!(
        !root.exists(),
        "r227m7-cache-06: `new` must be I/O-free — the root should still not exist"
    );

    let source: &[u8] = b"echo dir\n";
    let hash = cache.store(source, &[], vec![0xAA, 0xBB]);
    assert!(
        root.is_dir(),
        "r227m7-cache-06: the first store must create the cache root directory"
    );

    // The on-disk file is a best-effort hint; on a writable tempdir it
    // should exist and hold the cached bytes.
    let pdc = root.join(format!("{hash:016x}.pdc"));
    assert!(
        pdc.is_file(),
        "r227m7-cache-06: expected an on-disk `.pdc` companion at {pdc:?}"
    );
    let on_disk = fs::read(&pdc).expect("r227m7-cache-06: read .pdc");
    assert_eq!(
        on_disk,
        vec![0xAA, 0xBB],
        "r227m7-cache-06: on-disk `.pdc` must hold the exact stored bytes"
    );
}

// ────────────────────────────────────────────────────────────────
// r227m7-cache-07 — two ScriptCache instances have disjoint maps
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m7_cache_07_instances_are_isolated() {
    // Point two caches at two different roots; storing into one must
    // not populate the other's in-memory map. The R227.M7 lookup path
    // is authoritative for the in-memory map only, so this holds even
    // if the two roots ever pointed at the same directory (they
    // don't, here, so we also implicitly cover the disk-side isolation).
    let tmp_a = TempDir::new().expect("r227m7-cache-07: left tempdir");
    let tmp_b = TempDir::new().expect("r227m7-cache-07: right tempdir");
    let mut left = ScriptCache::new(tmp_a.path().to_path_buf());
    let right = ScriptCache::new(tmp_b.path().to_path_buf());

    let source: &[u8] = b"echo only-left\n";
    left.store(source, &[], vec![0x11]);

    assert!(
        left.lookup(source, &[]).is_some(),
        "r227m7-cache-07: sanity — left must see its own store"
    );
    assert!(
        right.lookup(source, &[]).is_none(),
        "r227m7-cache-07: right must NOT see left's store — instances are isolated"
    );
}

// ────────────────────────────────────────────────────────────────
// r227m7-cache-08 — 1 KiB payload survives a byte-identical round-trip
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m7_cache_08_kib_payload_roundtrips() {
    // 1024 bytes with a mildly non-trivial pattern so a stray truncation
    // or off-by-one in the persistence path can't hide behind a
    // repeating-byte payload.
    let mut payload = Vec::with_capacity(1024);
    for i in 0..1024u32 {
        payload.push((i.wrapping_mul(31) ^ 0xA5) as u8);
    }
    assert_eq!(
        payload.len(),
        1024,
        "r227m7-cache-08: sanity — payload should be exactly 1 KiB"
    );

    let tmp = TempDir::new().expect("r227m7-cache-08: tempdir");
    let mut cache = ScriptCache::new(tmp.path().to_path_buf());
    let source: &[u8] = b"echo big\n";
    let deps: [&[u8]; 1] = [b"one dep\n"];

    cache.store(source, &deps, payload.clone());
    let hit = cache
        .lookup(source, &deps)
        .expect("r227m7-cache-08: freshly stored entry must hit on lookup");

    assert_eq!(
        hit.len(),
        payload.len(),
        "r227m7-cache-08: round-tripped payload length must match"
    );
    assert_eq!(
        hit, &payload,
        "r227m7-cache-08: round-tripped payload must be byte-identical"
    );
}
