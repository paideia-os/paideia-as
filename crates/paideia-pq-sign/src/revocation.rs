//! Revocation list: JSON-lines format, per-line revocation record.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// A single revocation entry record.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RevocationEntry {
    /// Hex string of hybrid public key BLAKE3 prefix (first 16 chars).
    pub key_id: String,
    /// ISO 8601 / RFC 3339 timestamp when revocation was issued.
    pub revoked_at: String,
    /// Free-text reason for revocation (e.g., "compromised", "rotation").
    pub reason: String,
}

/// In-memory revocation list indexed by key_id for O(1) lookup.
#[derive(Default, Clone, Debug)]
pub struct RevocationList {
    entries: HashMap<String, RevocationEntry>,
}

impl RevocationList {
    /// Create a new empty revocation list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load revocation list from a JSON-lines file.
    ///
    /// Each line is expected to be a single JSON object representing a
    /// [`RevocationEntry`]. Lines are parsed independently.
    pub fn load_from_jsonl(path: &Path) -> Result<Self, RevocationError> {
        let content = std::fs::read_to_string(path)?;
        let mut list = Self::new();

        for (line_num, line) in content.lines().enumerate() {
            let line_idx = line_num + 1;

            // Skip empty lines and comments
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let entry: RevocationEntry =
                serde_json::from_str(trimmed).map_err(|e| RevocationError::Parse {
                    line: line_idx,
                    msg: e.to_string(),
                })?;

            list.entries.insert(entry.key_id.clone(), entry);
        }

        Ok(list)
    }

    /// Check if a key_id is revoked and return the entry if found.
    pub fn is_revoked(&self, key_id: &str) -> Option<&RevocationEntry> {
        self.entries.get(key_id)
    }

    /// Add a revocation entry to the list (overwrites if key_id already exists).
    pub fn add(&mut self, entry: RevocationEntry) {
        self.entries.insert(entry.key_id.clone(), entry);
    }

    /// Return the number of entries in the revocation list.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return true if the revocation list is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Errors that can occur during revocation list operations.
#[derive(Debug, thiserror::Error)]
pub enum RevocationError {
    /// IO error (file read, etc.).
    #[error("revocation list IO: {0}")]
    Io(#[from] std::io::Error),

    /// JSON parse error on a specific line.
    #[error("revocation list parse: line {line}: {msg}")]
    Parse {
        /// Line number where the parse error occurred.
        line: usize,
        /// Error message describing the parse failure.
        msg: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_revocation_list_loads_from_valid_jsonl() {
        // Create a temporary file with valid JSON-lines
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            "{{\"key_id\": \"abc123\", \"revoked_at\": \"2026-01-15T10:00:00Z\", \"reason\": \"compromised\"}}"
        )
        .unwrap();
        writeln!(
            file,
            "{{\"key_id\": \"def456\", \"revoked_at\": \"2026-02-01T00:00:00Z\", \"reason\": \"rotation\"}}"
        )
        .unwrap();

        let list = RevocationList::load_from_jsonl(file.path()).unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_revocation_list_rejects_unknown_key_id() {
        let list = RevocationList::new();
        assert!(list.is_revoked("unknown_key").is_none());
    }

    #[test]
    fn test_revocation_list_returns_entry_for_known_key_id() {
        let mut list = RevocationList::new();
        let entry = RevocationEntry {
            key_id: "abc123".to_string(),
            revoked_at: "2026-01-15T10:00:00Z".to_string(),
            reason: "compromised".to_string(),
        };
        list.add(entry.clone());

        let found = list.is_revoked("abc123").unwrap();
        assert_eq!(found.key_id, "abc123");
        assert_eq!(found.reason, "compromised");
    }

    #[test]
    fn test_revocation_list_handles_empty_file() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();
        // Write nothing to file, file is automatically empty

        let list = RevocationList::load_from_jsonl(&path).unwrap();
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn test_revocation_list_parse_error_includes_line_number() {
        // Create a file with invalid JSON on line 2
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            "{{\"key_id\": \"abc123\", \"revoked_at\": \"2026-01-15T10:00:00Z\", \"reason\": \"compromised\"}}"
        )
        .unwrap();
        writeln!(file, "{{invalid json}}").unwrap();

        let result = RevocationList::load_from_jsonl(file.path());
        assert!(result.is_err());

        if let Err(RevocationError::Parse { line, .. }) = result {
            assert_eq!(line, 2);
        } else {
            panic!("Expected Parse error with line number");
        }
    }

    #[test]
    fn test_revocation_list_skips_comments_and_empty_lines() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "# This is a comment").unwrap();
        writeln!(file).unwrap(); // empty line
        writeln!(
            file,
            "{{\"key_id\": \"abc123\", \"revoked_at\": \"2026-01-15T10:00:00Z\", \"reason\": \"compromised\"}}"
        )
        .unwrap();

        let list = RevocationList::load_from_jsonl(file.path()).unwrap();
        assert_eq!(list.len(), 1);
    }
}
