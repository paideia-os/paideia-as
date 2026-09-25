//! R227.M7 `.pds` script-body binary cache.
//!
//! A small, best-effort key/value cache that maps a `.pds` script's
//! source bytes (plus the byte content of every module it imports) to
//! the compiled, ready-to-execute binary body the shell-eval layer
//! wants to hand to the runtime. The cache is a *hint*, never a source
//! of truth: a lookup miss simply forces the loader to re-compile;
//! filesystem I/O errors while persisting an entry are silently
//! swallowed so an unreadable / read-only cache directory can never
//! break a script that would otherwise load correctly.
//!
//! # Position in the pipeline
//!
//! ```text
//!   .pds source                   deps: [.pds import bytes, …]
//!       │                              │
//!       └────────── source_hash ◀──────┘─── deps_hash
//!                       │                        │
//!                       ▼                        ▼
//!                    ScriptCache::lookup(source, deps)
//!                       │
//!         hit ─────────►│                    ◄──── cached_bytes
//!         miss ────────►│──► compile ──► ScriptCache::store(..)
//!                       │                    │
//!                       ▼                    ▼
//!                    execute                 disk-persisted `.pdc`
//! ```
//!
//! # Key design
//!
//! Two hashes together form the cache key:
//!
//! * `source_hash` — FNV-1a-64 over the raw script bytes; this is the
//!   in-memory `HashMap` key, so a modified source misses at the very
//!   first lookup step (no need to compare deps).
//! * `deps_hash` — folded FNV-1a-64 across each dep's bytes in the
//!   caller-supplied order (`acc = 0; for d in deps: acc =
//!   acc.wrapping_add(fnv1a_64(d))`). The fold is deliberately
//!   **order-sensitive**: the same set of dep bytes in a different
//!   `#import` order is treated as a different cache key, because the
//!   R227.M3 import resolver already encodes import order into the
//!   compiled body (aliases bind in appearance order, and evaluation
//!   side-effects follow the same order). Freezing the fold as
//!   order-sensitive keeps the cache honest against that. The
//!   wrapping-add fold is chosen over XOR (which would collapse
//!   duplicated deps to zero) and over concatenation-then-hash (which
//!   would require an O(sum-of-dep-lengths) buffer allocation on every
//!   lookup) — wrapping-add over per-dep FNV keeps lookup allocation-
//!   free once the vector of dep-slice references is on hand.
//!
//! # Persistence
//!
//! Each `store` also attempts to write the cached bytes to
//! `root/<source_hash:016x>.pdc`. If the write (or the `create_dir_all`
//! that precedes it) fails, the in-memory entry is still populated and
//! the failure is discarded — the cache is a hint. R227.M7 does not
//! read entries back from disk on `lookup`: warm-up from disk into the
//! in-memory map is a separate concern the R227.M9+ warmer will drive.
//! Persistence here is purely so the next process (or the next warmer
//! pass) has bytes to consume.
//!
//! # Fingerprints
//!
//! The R227.M7 test corpus tags each fixture with `r227m7-cache-NN` so
//! the R220.M10 `@fingerprint` correlator can attribute pass/fail to a
//! specific fixture without re-parsing its name.

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use crate::load_fingerprint::fnv1a_64;

/// One entry in the [`ScriptCache`]'s in-memory map.
///
/// Cloneable so callers that want to snapshot the cache for a debug
/// pane can do so without holding a borrow across a later mutation.
/// `cached_at` is a caller-supplied timestamp slot — R227.M7 always
/// stores `0` (the crate has no clock and adding one would drag `std::
/// time` into the load path); a later milestone that wires the session
/// EDB in can populate it without changing the shape of the type.
#[derive(Clone, Debug)]
pub struct CacheEntry {
    /// FNV-1a-64 over the script's source bytes.
    pub source_hash: u64,
    /// Wrapping-add fold of FNV-1a-64 over each dep's bytes, in the
    /// order the caller supplied them.
    pub deps_hash: u64,
    /// The compiled script body this entry caches.
    pub cached_bytes: Vec<u8>,
    /// Caller-supplied timestamp slot. R227.M7 always writes `0`.
    pub cached_at: u64,
}

/// Best-effort binary cache keyed by (`source_hash`, `deps_hash`).
///
/// The in-memory map is authoritative for [`lookup`](Self::lookup);
/// disk persistence is a side effect of [`store`](Self::store) that
/// the R227.M7 lookup path deliberately does not consult. Two
/// `ScriptCache` instances with the same `root` therefore have
/// disjoint in-memory views even if their on-disk views overlap — the
/// R227.M9+ warmer is the module that will bridge the two.
#[derive(Debug)]
pub struct ScriptCache {
    root: PathBuf,
    in_memory: HashMap<u64, CacheEntry>,
}

impl ScriptCache {
    /// Construct a fresh cache rooted at `root`.
    ///
    /// Performs no I/O — the directory is created lazily on the first
    /// [`store`](Self::store) call so a caller that only ever misses
    /// (a cold shell that never compiles anything) leaves no
    /// filesystem trace.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            in_memory: HashMap::new(),
        }
    }

    /// Insert `cached_bytes` under the (`source`, `deps`) key and
    /// attempt to persist it to disk. Returns the computed
    /// `source_hash` so callers that want to later `invalidate` a
    /// specific entry can do so without re-hashing the source.
    ///
    /// I/O failures while creating the cache directory or writing the
    /// `.pdc` file are silently swallowed — the in-memory entry is
    /// still populated and returned. The cache is a hint, not a
    /// correctness surface; a read-only or full disk must not be able
    /// to break a script that would otherwise load.
    pub fn store(
        &mut self,
        source: &[u8],
        deps: &[&[u8]],
        cached_bytes: Vec<u8>,
    ) -> u64 {
        let source_hash = fnv1a_64(source);
        let deps_hash = fold_deps(deps);

        // Best-effort persistence. `create_dir_all` treats an existing
        // directory as success, but we still tolerate any other error
        // silently — the on-disk copy is a warmer hint.
        match fs::create_dir_all(&self.root) {
            Ok(()) => {
                let path = self.root.join(format!("{source_hash:016x}.pdc"));
                let _ = fs::write(path, &cached_bytes);
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                // Redundant against create_dir_all's own semantics, but
                // documents the contract we rely on if the std impl
                // ever tightens.
                let path = self.root.join(format!("{source_hash:016x}.pdc"));
                let _ = fs::write(path, &cached_bytes);
            }
            Err(_) => {
                // Cache is a hint — persist failure is not a load
                // failure. Fall through and still populate in-memory.
            }
        }

        self.in_memory.insert(
            source_hash,
            CacheEntry {
                source_hash,
                deps_hash,
                cached_bytes,
                cached_at: 0,
            },
        );

        source_hash
    }

    /// Return the cached body for (`source`, `deps`) if the in-memory
    /// map holds one AND that entry's `deps_hash` matches the freshly
    /// folded dep hash.
    ///
    /// A source-byte edit misses at the `HashMap` step (different
    /// `source_hash` key); a dep-byte edit misses at the `deps_hash`
    /// comparison. R227.M7 does not touch disk on lookup — a bytes-on-
    /// disk-only hit is a miss until the R227.M9+ warmer pre-populates
    /// the in-memory map.
    #[must_use]
    pub fn lookup(&self, source: &[u8], deps: &[&[u8]]) -> Option<&Vec<u8>> {
        let source_hash = fnv1a_64(source);
        let entry = self.in_memory.get(&source_hash)?;
        let deps_hash = fold_deps(deps);
        if entry.deps_hash == deps_hash {
            Some(&entry.cached_bytes)
        } else {
            None
        }
    }

    /// Drop the in-memory entry for `source_hash` and, if one was
    /// present, attempt to unlink the on-disk `.pdc` companion.
    ///
    /// Returns `true` iff the in-memory map had an entry for this key
    /// before the call — the disk-side result is deliberately not
    /// surfaced, matching the "cache is a hint" posture of
    /// [`store`](Self::store). A caller that wants to know whether the
    /// on-disk copy is really gone can `fs::metadata` the file itself.
    pub fn invalidate(&mut self, source_hash: u64) -> bool {
        let had = self.in_memory.remove(&source_hash).is_some();
        if had {
            let path = self.root.join(format!("{source_hash:016x}.pdc"));
            let _ = fs::remove_file(path);
        }
        had
    }
}

/// Fold a slice of dep byte-slices into a single order-sensitive
/// `u64` by wrapping-adding each per-dep FNV-1a-64.
///
/// Pulled out as a private helper so `store` and `lookup` agree on the
/// fold by construction. A future BLAKE3 migration edits `fnv1a_64` in
/// `load_fingerprint`; the fold shape stays the same.
#[inline]
fn fold_deps(deps: &[&[u8]]) -> u64 {
    let mut acc: u64 = 0;
    for dep in deps {
        acc = acc.wrapping_add(fnv1a_64(dep));
    }
    acc
}
