//! Allowlist parser for documented non-determinism.

use serde::Deserialize;

/// An allowlist rule that documents a known source of non-determinism.
#[derive(Clone, Debug, Deserialize)]
pub struct AllowlistRule {
    /// Human-readable name describing what this rule covers.
    pub name: String,
    /// Byte offset where range starts (inclusive).
    pub start: u64,
    /// Byte offset where range ends (inclusive).
    pub end: u64,
    /// Documented reason (e.g., "build timestamp embedded in ELF .note section").
    pub reason: String,
}

/// Collection of allowlist rules that documents sources of non-determinism.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Allowlist {
    /// Rules that cover known non-deterministic ranges.
    pub rules: Vec<AllowlistRule>,
}

impl Allowlist {
    /// Parse an allowlist from a TOML string.
    pub fn parse(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }

    /// Load an allowlist from a TOML file.
    pub fn load(path: &std::path::Path) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::parse(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
    }

    /// Check whether an offset is covered by any rule. Returns
    /// (true, Some(name)) on match, (false, None) otherwise.
    pub fn check(&self, offset: u64) -> (bool, Option<String>) {
        for rule in &self.rules {
            if offset >= rule.start && offset <= rule.end {
                return (true, Some(rule.name.clone()));
            }
        }
        (false, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_check_returns_true_for_in_range_offset() {
        let allowlist = Allowlist {
            rules: vec![AllowlistRule {
                name: "test-rule".to_string(),
                start: 10,
                end: 20,
                reason: "test".to_string(),
            }],
        };

        let (covered, rule) = allowlist.check(15);
        assert!(covered);
        assert_eq!(rule, Some("test-rule".to_string()));
    }

    #[test]
    fn allowlist_check_returns_false_for_out_of_range_offset() {
        let allowlist = Allowlist {
            rules: vec![AllowlistRule {
                name: "test-rule".to_string(),
                start: 10,
                end: 20,
                reason: "test".to_string(),
            }],
        };

        let (covered, rule) = allowlist.check(25);
        assert!(!covered);
        assert_eq!(rule, None);
    }
}
