//! LSP-side wrapper around the elaborator's QueryEngine.

use std::sync::RwLock;

use paideia_as_elaborator::incremental::{QueryEngine, QueryStats};

/// Thread-safe wrapper around the incremental elaboration query engine.
pub struct IncrementalEngine {
    inner: RwLock<QueryEngine>,
}

impl IncrementalEngine {
    /// Create a new incremental engine.
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(QueryEngine::new()),
        }
    }

    /// Ingest a document change and invalidate downstream dependencies.
    pub fn set_document(&self, uri: &str, text: &str) {
        let mut engine = self.inner.write().unwrap();
        engine.set_document(uri, text);
    }

    /// Return current query statistics.
    pub fn stats(&self) -> QueryStats {
        let engine = self.inner.read().unwrap();
        engine.stats()
    }

    /// Return the number of documents currently tracked.
    pub fn document_count(&self) -> usize {
        self.inner.read().unwrap().documents.len()
    }
}

impl Default for IncrementalEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_engine_set_document_increments_count() {
        let engine = IncrementalEngine::new();
        assert_eq!(engine.document_count(), 0);

        engine.set_document("file:///test1.pdx", "x = 0");
        assert_eq!(engine.document_count(), 1);

        engine.set_document("file:///test2.pdx", "y = 1");
        assert_eq!(engine.document_count(), 2);
    }

    #[test]
    fn incremental_engine_thread_safe_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let engine = Arc::new(IncrementalEngine::new());
        let engine1 = engine.clone();
        let engine2 = engine.clone();

        let handle1 = thread::spawn(move || {
            for i in 0..10 {
                engine1.set_document(&format!("file:///thread1_{}.pdx", i), "content");
            }
        });

        let handle2 = thread::spawn(move || {
            for i in 0..10 {
                engine2.set_document(&format!("file:///thread2_{}.pdx", i), "content");
            }
        });

        handle1.join().unwrap();
        handle2.join().unwrap();

        // Both threads should have added documents without deadlock
        assert_eq!(engine.document_count(), 20);
    }

    #[test]
    fn incremental_engine_stats_thread_safe_read() {
        use std::sync::Arc;
        use std::thread;

        let engine = Arc::new(IncrementalEngine::new());
        let engine1 = engine.clone();
        let engine2 = engine.clone();

        engine.set_document("file:///test.pdx", "x = 0");

        let handle1 = thread::spawn(move || {
            for _ in 0..5 {
                let _ = engine1.stats();
            }
        });

        let handle2 = thread::spawn(move || {
            for _ in 0..5 {
                let _ = engine2.stats();
            }
        });

        handle1.join().unwrap();
        handle2.join().unwrap();

        // Stats should be readable without panics
        let stats = engine.stats();
        assert_eq!(stats.parse_calls, 0); // No parse queries made
    }
}
