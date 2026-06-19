//! Parse cache for incremental elaboration.

use std::num::NonZeroUsize;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use tower_lsp::lsp_types::{Diagnostic, Url};

/// Cache entry: parsed result + the diagnostics produced.
#[derive(Clone, Debug)]
pub struct CacheEntry {
    /// BLAKE3 hash of the source text.
    pub content_hash: [u8; 32],
    /// Diagnostics produced from parsing the source.
    pub diagnostics: Vec<Diagnostic>,
}

/// LRU cache for parsed ASTs keyed by (URI, content_hash).
/// Cache hits skip re-parse on no-op re-elaborations.
#[derive(Debug)]
pub struct ParseCache {
    entries: Mutex<lru::LruCache<Url, CacheEntry>>,
    hits: AtomicU64,
    misses: AtomicU64,
}

/// Default capacity for the parse cache (64 entries).
pub const DEFAULT_CAPACITY: usize = 64;

impl ParseCache {
    /// Create a new parse cache with the specified capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Mutex::new(lru::LruCache::new(
                NonZeroUsize::new(capacity).expect("capacity must be > 0"),
            )),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Create a new parse cache with the default capacity.
    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }

    /// Look up an entry by (uri, current content hash). Returns Some only
    /// if both match (URI present AND hash equal).
    pub fn lookup(&self, uri: &Url, content_hash: &[u8; 32]) -> Option<CacheEntry> {
        let mut entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.get(uri)
            && &entry.content_hash == content_hash
        {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return Some(entry.clone());
        }
        self.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Insert an entry into the cache.
    pub fn insert(&self, uri: Url, entry: CacheEntry) {
        let mut entries = self.entries.lock().unwrap();
        entries.put(uri, entry);
    }

    /// Invalidate a single cached entry by URI.
    pub fn invalidate(&self, uri: &Url) {
        let mut entries = self.entries.lock().unwrap();
        entries.pop(uri);
    }

    /// Invalidate ALL entries — used on cross-file dependency changes.
    /// Phase-2-m8-005 minimum: any didChange anywhere invalidates all.
    /// m8-007+ refines with a per-file dep graph.
    pub fn invalidate_all(&self) {
        let mut entries = self.entries.lock().unwrap();
        entries.clear();
    }

    /// Return the number of cache hits.
    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    /// Return the number of cache misses.
    pub fn misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }

    /// Return the number of entries currently in the cache.
    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Compute the BLAKE3 hash of source text.
pub fn content_hash(source: &str) -> [u8; 32] {
    *blake3::hash(source.as_bytes()).as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_miss_on_empty_cache() {
        let cache = ParseCache::with_default_capacity();
        let uri = Url::parse("file:///test.pdx").unwrap();
        let hash = content_hash("test content");

        let result = cache.lookup(&uri, &hash);
        assert!(result.is_none(), "expected lookup to miss on empty cache");
        assert_eq!(cache.misses(), 1);
        assert_eq!(cache.hits(), 0);
    }

    #[test]
    fn insert_then_lookup_returns_entry() {
        let cache = ParseCache::with_default_capacity();
        let uri = Url::parse("file:///test.pdx").unwrap();
        let source = "test content";
        let hash = content_hash(source);
        let diagnostics = vec![];

        let entry = CacheEntry {
            content_hash: hash,
            diagnostics: diagnostics.clone(),
        };

        cache.insert(uri.clone(), entry);

        let result = cache.lookup(&uri, &hash);
        assert!(result.is_some(), "expected lookup to hit after insert");
        let retrieved = result.unwrap();
        assert_eq!(retrieved.content_hash, hash);
        assert_eq!(retrieved.diagnostics, diagnostics);
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 0);
    }

    #[test]
    fn lookup_with_different_hash_returns_none() {
        let cache = ParseCache::with_default_capacity();
        let uri = Url::parse("file:///test.pdx").unwrap();
        let source1 = "content 1";
        let source2 = "content 2";
        let hash1 = content_hash(source1);
        let hash2 = content_hash(source2);

        let entry = CacheEntry {
            content_hash: hash1,
            diagnostics: vec![],
        };

        cache.insert(uri.clone(), entry);

        // Try to lookup with different hash
        let result = cache.lookup(&uri, &hash2);
        assert!(
            result.is_none(),
            "expected lookup to miss with different hash"
        );
        assert_eq!(cache.misses(), 1);
        assert_eq!(cache.hits(), 0);
    }

    #[test]
    fn cache_hit_counter_increments() {
        let cache = ParseCache::with_default_capacity();
        let uri = Url::parse("file:///test.pdx").unwrap();
        let source = "test content";
        let hash = content_hash(source);

        let entry = CacheEntry {
            content_hash: hash,
            diagnostics: vec![],
        };

        cache.insert(uri.clone(), entry);

        // First lookup should be a hit
        let _ = cache.lookup(&uri, &hash);
        assert_eq!(cache.hits(), 1, "expected 1 hit after first lookup");

        // Second lookup should also be a hit
        let _ = cache.lookup(&uri, &hash);
        assert_eq!(cache.hits(), 2, "expected 2 hits after second lookup");
    }

    #[test]
    fn cache_eviction_respects_capacity() {
        let cache = ParseCache::new(2); // capacity of 2
        let uri1 = Url::parse("file:///test1.pdx").unwrap();
        let uri2 = Url::parse("file:///test2.pdx").unwrap();
        let uri3 = Url::parse("file:///test3.pdx").unwrap();

        let hash1 = content_hash("content 1");
        let hash2 = content_hash("content 2");
        let hash3 = content_hash("content 3");

        let entry1 = CacheEntry {
            content_hash: hash1,
            diagnostics: vec![],
        };
        let entry2 = CacheEntry {
            content_hash: hash2,
            diagnostics: vec![],
        };
        let entry3 = CacheEntry {
            content_hash: hash3,
            diagnostics: vec![],
        };

        cache.insert(uri1.clone(), entry1);
        assert_eq!(cache.len(), 1);

        cache.insert(uri2.clone(), entry2);
        assert_eq!(cache.len(), 2);

        // Inserting a third entry should evict the first (LRU)
        cache.insert(uri3.clone(), entry3);
        assert_eq!(cache.len(), 2, "expected cache to maintain capacity of 2");

        // The first entry should no longer be in the cache
        let result = cache.lookup(&uri1, &hash1);
        assert!(result.is_none(), "expected first entry to be evicted");
    }

    #[test]
    fn invalidate_all_clears_cache() {
        let cache = ParseCache::with_default_capacity();
        let uri1 = Url::parse("file:///test1.pdx").unwrap();
        let uri2 = Url::parse("file:///test2.pdx").unwrap();

        let hash1 = content_hash("content 1");
        let hash2 = content_hash("content 2");

        let entry1 = CacheEntry {
            content_hash: hash1,
            diagnostics: vec![],
        };
        let entry2 = CacheEntry {
            content_hash: hash2,
            diagnostics: vec![],
        };

        cache.insert(uri1.clone(), entry1);
        cache.insert(uri2.clone(), entry2);
        assert_eq!(cache.len(), 2);

        cache.invalidate_all();
        assert_eq!(
            cache.len(),
            0,
            "expected cache to be empty after invalidate_all"
        );

        // Further lookups should miss
        let result1 = cache.lookup(&uri1, &hash1);
        let result2 = cache.lookup(&uri2, &hash2);
        assert!(result1.is_none());
        assert!(result2.is_none());
    }
}
