//! Incremental elaboration engine.
//!
//! Hand-rolled Salsa-style query memoisation. Phase-2-m8-008 ships:
//! - Per-file parse query (genuinely incremental).
//! - Per-module elaborate query (coarse — m5-003's elaborate_structure
//!   is whole-module; per-definition granularity lands when the
//!   elaborator gains an elaborate_definition entry point).
//!
//! FUTURE(salsa-migration): the Query trait + memoisation HashMap +
//! revision counter map cleanly onto salsa-2022 if we migrate later.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

/// Definition key uniquely identifying a module or top-level definition.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DefinitionKey {
    /// Stringified URL for hashability.
    pub uri: String,
    /// "<module>" for whole-module entries, or definition name.
    pub name: String,
}

/// Snapshot of a source document.
#[derive(Clone, Debug)]
pub struct DocumentSnapshot {
    /// Revision counter (incremented on each change).
    pub rev: u64,
    /// BLAKE3 hash of the content.
    pub content_hash: [u8; 32],
    /// Source text, Arc-wrapped for efficient cloning.
    pub text: Arc<str>,
}

/// Snapshot of a parsed file.
#[derive(Clone, Debug)]
pub struct ParsedSnapshot {
    /// Revision at which this snapshot was computed.
    pub rev: u64,
    /// URIs imported by this file.
    pub deps_from: BTreeSet<String>,
    /// Phase-2-m8-008: just the source text length and hash for now;
    /// real AST is m8-009+. The engine memoizes the parse result; the
    /// consumer (LSP) can re-parse from text if needed.
    pub source_len: usize,
}

/// Snapshot of an elaborated module.
#[derive(Clone, Debug)]
pub struct ElaboratedSnapshot {
    /// Revision at which this snapshot was computed.
    pub rev: u64,
    /// Transitively closed set of definition keys this elaboration depends on.
    pub deps_from: BTreeSet<DefinitionKey>,
    /// Phase-2-m8-008: placeholder until per-definition elaboration lands.
    pub diagnostics_count: u32,
}

/// Query statistics for monitoring cache effectiveness.
#[derive(Default, Clone, Copy, Debug)]
pub struct QueryStats {
    /// Total parse query invocations.
    pub parse_calls: u64,
    /// Total elaborate query invocations.
    pub elab_calls: u64,
    /// Cache hits across all queries.
    pub hits: u64,
    /// Cache misses across all queries.
    pub misses: u64,
}

/// Incremental elaboration query engine.
#[derive(Default)]
pub struct QueryEngine {
    /// Global revision counter, incremented on each set_document.
    pub revision: u64,
    /// Document snapshots keyed by URI.
    pub documents: HashMap<String, DocumentSnapshot>,
    /// Parsed snapshots keyed by URI.
    pub parsed: HashMap<String, ParsedSnapshot>,
    /// Elaborated snapshots keyed by DefinitionKey.
    pub elaborated_definitions: HashMap<DefinitionKey, ElaboratedSnapshot>,
    /// Query statistics.
    pub stats: QueryStats,
}

impl QueryEngine {
    /// Create a new QueryEngine.
    pub fn new() -> Self {
        Self::default()
    }

    /// Ingest a document change. Updates the document snapshot, bumps revision,
    /// invalidates downstream.
    pub fn set_document(&mut self, uri: &str, text: &str) {
        let new_hash = content_hash(text);

        // Early exit if no change
        if let Some(doc) = self.documents.get(uri)
            && doc.content_hash == new_hash
        {
            return;
        }

        // Bump global revision
        self.revision += 1;

        // Insert new document snapshot
        self.documents.insert(
            uri.to_string(),
            DocumentSnapshot {
                rev: self.revision,
                content_hash: new_hash,
                text: Arc::from(text),
            },
        );

        // Invalidate parse cache and cascade invalidation
        self.parsed.remove(uri);
        self.invalidate_cascade(uri);
    }

    /// Lookup parsed snapshot. Recomputes if missing or stale.
    /// Returns mutable reference to the snapshot (caller responsible for filling).
    pub fn query_parse(&mut self, uri: &str) -> Option<ParsedSnapshot> {
        self.stats.parse_calls += 1;

        // Check if already cached
        if let Some(parsed) = self.parsed.get(uri) {
            // Verify it's not stale by checking the document's revision
            if let Some(doc) = self.documents.get(uri)
                && parsed.rev == doc.rev
            {
                self.stats.hits += 1;
                return Some(parsed.clone());
            }
        }

        self.stats.misses += 1;

        // Recompute: create a default snapshot (caller will populate with actual parse results)
        let doc = self.documents.get(uri)?.clone();
        let parsed = ParsedSnapshot {
            rev: doc.rev,
            deps_from: BTreeSet::new(),
            source_len: doc.text.len(),
        };

        self.parsed.insert(uri.to_string(), parsed.clone());
        Some(parsed)
    }

    /// Lookup elaborated snapshot. Recomputes if missing or any dep has a
    /// newer revision than the cached entry.
    pub fn query_elab(&mut self, key: &DefinitionKey) -> Option<ElaboratedSnapshot> {
        self.stats.elab_calls += 1;

        // Check if already cached and dependencies are fresh
        if let Some(elab) = self.elaborated_definitions.get(key) {
            let mut deps_stale = false;

            // Verify all dependencies are still at the recorded revision
            for dep_key in &elab.deps_from {
                if let Some(dep_elab) = self.elaborated_definitions.get(dep_key)
                    && dep_elab.rev > elab.rev
                {
                    deps_stale = true;
                    break;
                }
            }

            if !deps_stale {
                self.stats.hits += 1;
                return Some(elab.clone());
            }
        }

        self.stats.misses += 1;

        // Recompute: create a default snapshot (caller will populate with actual results)
        let elaborated = ElaboratedSnapshot {
            rev: self.revision,
            deps_from: BTreeSet::new(),
            diagnostics_count: 0,
        };

        self.elaborated_definitions
            .insert(key.clone(), elaborated.clone());
        Some(elaborated)
    }

    /// Invalidate everything depending on `uri`. Walks the elaborated_definitions
    /// map and removes entries whose deps_from includes `uri`'s default DefinitionKey.
    pub fn invalidate_cascade(&mut self, uri: &str) {
        let uri_default_key = DefinitionKey {
            uri: uri.to_string(),
            name: "<module>".to_string(),
        };

        // Collect all keys to remove (to avoid borrow checker issues)
        let keys_to_remove: Vec<DefinitionKey> = self
            .elaborated_definitions
            .iter()
            .filter_map(|(key, elab)| {
                if elab.deps_from.contains(&uri_default_key) {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        // Remove all stale entries
        for key in keys_to_remove {
            self.elaborated_definitions.remove(&key);
        }
    }

    /// Return current query statistics.
    pub fn stats(&self) -> QueryStats {
        self.stats
    }
}

/// Compute BLAKE3 hash of text.
fn content_hash(text: &str) -> [u8; 32] {
    *blake3::hash(text.as_bytes()).as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memoise_hit_miss() {
        let mut engine = QueryEngine::new();

        // First set: should be a miss
        engine.set_document("file:///test.pdx", "x = 0");
        let parsed1 = engine.query_parse("file:///test.pdx");
        assert!(parsed1.is_some());
        assert_eq!(engine.stats.misses, 1);
        assert_eq!(engine.stats.hits, 0);

        // Second set with same text: should be no-op
        engine.set_document("file:///test.pdx", "x = 0");
        let parsed2 = engine.query_parse("file:///test.pdx");
        assert!(parsed2.is_some());
        // No new call since set_document returned early
        assert_eq!(engine.stats.parse_calls, 2); // Both queries increment
        assert_eq!(engine.stats.hits, 1); // Second query hits the cache
    }

    #[test]
    fn revision_monotonic_on_insert() {
        let mut engine = QueryEngine::new();

        assert_eq!(engine.revision, 0);

        for i in 1..=5 {
            engine.set_document("file:///test.pdx", &format!("revision {}", i));
            assert_eq!(engine.revision, i as u64);
        }
    }

    #[test]
    fn cascade_invalidate_same_file() {
        let mut engine = QueryEngine::new();

        // Set up document and elaborated entry
        engine.set_document("file:///a.pdx", "module A");
        let key = DefinitionKey {
            uri: "file:///a.pdx".to_string(),
            name: "main".to_string(),
        };

        // Create an elaborated entry with a dependency on the file itself
        let mut elab = ElaboratedSnapshot {
            rev: engine.revision,
            deps_from: BTreeSet::new(),
            diagnostics_count: 0,
        };
        let file_key = DefinitionKey {
            uri: "file:///a.pdx".to_string(),
            name: "<module>".to_string(),
        };
        elab.deps_from.insert(file_key);
        engine.elaborated_definitions.insert(key.clone(), elab);

        assert_eq!(engine.elaborated_definitions.len(), 1);

        // Mutate document: should cascade-invalidate
        engine.set_document("file:///a.pdx", "module A modified");
        assert_eq!(engine.elaborated_definitions.len(), 0);
    }

    #[test]
    fn cross_file_invalidate() {
        let mut engine = QueryEngine::new();

        // Set up two files: A imports B
        engine.set_document("file:///a.pdx", "import B");
        engine.set_document("file:///b.pdx", "module B");

        // Create elaborated entry for A that depends on B
        let key_a = DefinitionKey {
            uri: "file:///a.pdx".to_string(),
            name: "main".to_string(),
        };

        let key_b = DefinitionKey {
            uri: "file:///b.pdx".to_string(),
            name: "<module>".to_string(),
        };

        let mut elab_a = ElaboratedSnapshot {
            rev: engine.revision,
            deps_from: BTreeSet::new(),
            diagnostics_count: 0,
        };
        elab_a.deps_from.insert(key_b);
        engine.elaborated_definitions.insert(key_a.clone(), elab_a);

        assert_eq!(engine.elaborated_definitions.len(), 1);

        // Mutate B: should invalidate A
        engine.set_document("file:///b.pdx", "module B modified");
        assert_eq!(engine.elaborated_definitions.len(), 0);
    }

    #[test]
    fn stats_counters_track_calls() {
        let mut engine = QueryEngine::new();

        engine.set_document("file:///test.pdx", "x = 0");
        assert_eq!(engine.stats.parse_calls, 0);
        assert_eq!(engine.stats.elab_calls, 0);

        let _ = engine.query_parse("file:///test.pdx");
        assert_eq!(engine.stats.parse_calls, 1);

        let key = DefinitionKey {
            uri: "file:///test.pdx".to_string(),
            name: "main".to_string(),
        };
        let _ = engine.query_elab(&key);
        assert_eq!(engine.stats.elab_calls, 1);

        // Second parse query should hit
        let _ = engine.query_parse("file:///test.pdx");
        assert_eq!(engine.stats.parse_calls, 2);
        assert_eq!(engine.stats.hits, 1);
    }

    #[test]
    fn hundred_sequential_edits_bounded() {
        let mut engine = QueryEngine::new();

        for i in 0..100 {
            engine.set_document("file:///test.pdx", &format!("rev {}", i));
        }

        assert_eq!(engine.documents.len(), 1);
        assert!(engine.elaborated_definitions.len() <= engine.documents.len());
    }

    #[test]
    #[ignore]
    fn throughput_1000_line_fixture() {
        let mut engine = QueryEngine::new();
        let fixture = "x = 0\n".repeat(1000);

        let start = std::time::Instant::now();
        engine.set_document("file:///test.pdx", &fixture);
        let elapsed = start.elapsed();

        println!("1000-line set_document took: {:?}", elapsed);
    }

    #[test]
    #[ignore]
    fn latency_probe_single_char() {
        let mut engine = QueryEngine::new();
        let fixture = "x = 0\n".repeat(1000);

        engine.set_document("file:///test.pdx", &fixture);

        // Single char edit
        let mut modified = fixture.clone();
        modified.push('y');

        let start = std::time::Instant::now();
        engine.set_document("file:///test.pdx", &modified);
        let elapsed = start.elapsed();

        println!("Single-char edit latency: {:?}", elapsed);

        if cfg!(not(debug_assertions)) {
            assert!(
                elapsed.as_millis() < 100,
                "single-char edit took too long: {:?}",
                elapsed
            );
        }
    }
}
