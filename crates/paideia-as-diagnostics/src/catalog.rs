use crate::code::{CodeParseError, DiagnosticCode};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use thiserror::Error;

/// Severity level for a diagnostic, matching the code module's definition.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Error severity.
    Error,
    /// Warning severity.
    Warning,
    /// Note severity.
    Note,
    /// Hint severity.
    Hint,
    /// Lint severity.
    Lint,
}

/// A single diagnostic entry in the catalog.
///
/// Deserialize from a `[diagnostic.CODE]` section in `catalog.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct CatalogEntry {
    /// Severity level for this diagnostic.
    pub severity: Severity,
    /// Category name (e.g., "lexer", "parser").
    pub category: String,
    /// One-line title describing the issue.
    pub title: String,
    /// Brief description (typically the error message text).
    pub brief: String,
    /// Extended description of the issue.
    #[serde(default)]
    pub description: String,
    /// Example of code that would trigger this diagnostic.
    #[serde(default, rename = "example_accept")]
    pub accept_example: Option<String>,
    /// Example of code that would not trigger this diagnostic.
    #[serde(default, rename = "example_reject")]
    pub reject_example: Option<String>,
    /// JSON schema for the diagnostic payload (if any).
    #[serde(default)]
    pub payload_schema: Option<String>,
    /// Suggested fix for this diagnostic.
    #[serde(default)]
    pub suggested_fix: Option<String>,
    /// Version when this diagnostic was introduced.
    #[serde(default)]
    pub since: Option<String>,
    /// Whether this diagnostic is deprecated.
    #[serde(default)]
    pub deprecated: bool,
    /// Deprecation note (if deprecated).
    #[serde(default)]
    pub deprecation_note: String,
}

/// The complete diagnostic catalog loaded from `catalog.toml`.
///
/// Provides access to diagnostic entries by code, with support for both
/// canonical form (e.g., "E0001") and shorthand form (e.g., "E1").
#[derive(Debug)]
pub struct Catalog {
    entries: BTreeMap<DiagnosticCode, CatalogEntry>,
    version: String,
}

/// Error type for catalog loading and validation.
#[derive(Debug, Error)]
pub enum CatalogError {
    /// Failed to read the catalog file.
    #[error("failed to read catalog at {path}")]
    Io {
        /// Path to the catalog file.
        path: PathBuf,
        /// Source I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Failed to parse catalog TOML.
    #[error("failed to parse catalog TOML")]
    Toml {
        /// Source TOML error.
        #[from]
        source: toml::de::Error,
    },

    /// Invalid diagnostic code key.
    #[error("invalid diagnostic code key '{key}': {source}")]
    InvalidCode {
        /// The problematic key.
        key: String,
        /// Source parsing error.
        #[source]
        source: CodeParseError,
    },

    /// Category name doesn't match the code letter.
    #[error(
        "category mismatch for {key}: key letter '{declared_letter}' \
         but category field maps to '{expected_letter}'"
    )]
    CategoryMismatch {
        /// The problematic key.
        key: String,
        /// The letter from the key.
        declared_letter: char,
        /// The letter expected from the category.
        expected_letter: char,
    },

    /// Duplicate code found post-parsing.
    #[error("duplicate code: {code}")]
    Duplicate {
        /// The duplicated code.
        code: String,
    },
}

/// Maps category names to their letter codes.
fn category_letter(name: &str) -> Option<char> {
    match name {
        "lexer" => Some('E'),
        "parser" => Some('P'),
        "module" => Some('M'),
        "type" => Some('T'),
        "substructural" => Some('S'),
        "effect" => Some('F'),
        "capability" => Some('C'),
        "optimization" => Some('O'),
        "unsafe" => Some('U'),
        "binary" => Some('B'),
        "dwarf" => Some('D'),
        "lint" => Some('L'),
        "workspace" => Some('W'),
        "runtime" => Some('R'),
        "experimental" => Some('Z'),
        _ => None,
    }
}

#[derive(Deserialize)]
struct CatalogHeader {
    version: String,
    #[serde(default)]
    #[allow(dead_code)]
    last_updated: String,
}

#[derive(Deserialize)]
struct RawCatalog {
    catalog: CatalogHeader,
    diagnostic: BTreeMap<String, CatalogEntry>,
}

impl Catalog {
    /// Loads the embedded catalog from the compiled binary.
    ///
    /// Returns a reference to the static, once-initialized catalog.
    pub fn embedded() -> &'static Catalog {
        static EMBEDDED: OnceLock<Catalog> = OnceLock::new();
        EMBEDDED.get_or_init(|| {
            let text = include_str!("../catalog.toml");
            Catalog::parse(text).expect("embedded catalog is valid")
        })
    }

    /// Loads the catalog from a file path.
    pub fn load(path: &Path) -> Result<Self, CatalogError> {
        let text = std::fs::read_to_string(path).map_err(|source| CatalogError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&text)
    }

    /// Parses a catalog from a TOML string.
    fn parse(text: &str) -> Result<Self, CatalogError> {
        let raw: RawCatalog = toml::from_str(text)?;
        Self::validate_and_build(raw)
    }

    /// Validates and builds a catalog from parsed TOML.
    fn validate_and_build(raw: RawCatalog) -> Result<Self, CatalogError> {
        let mut entries = BTreeMap::new();

        for (key, entry) in raw.diagnostic {
            // Parse the code key.
            let code =
                key.parse::<DiagnosticCode>()
                    .map_err(|source| CatalogError::InvalidCode {
                        key: key.clone(),
                        source,
                    })?;

            // Verify category name matches the code's letter.
            let expected_letter =
                category_letter(&entry.category).ok_or_else(|| CatalogError::InvalidCode {
                    key: key.clone(),
                    source: CodeParseError::BadFormat {
                        reason: "unknown category name",
                    },
                })?;

            let declared_letter = code.category().letter();
            if declared_letter != expected_letter {
                return Err(CatalogError::CategoryMismatch {
                    key,
                    declared_letter,
                    expected_letter,
                });
            }

            entries.insert(code, entry);
        }

        Ok(Catalog {
            entries,
            version: raw.catalog.version,
        })
    }

    /// Looks up a catalog entry by code string (canonical or shorthand).
    ///
    /// Accepts both "E0001" and "E1" forms. Returns `None` if the code
    /// is invalid or not found.
    pub fn lookup(&self, code: &str) -> Option<&CatalogEntry> {
        code.parse::<DiagnosticCode>()
            .ok()
            .and_then(|c| self.entries.get(&c))
    }

    /// Looks up a catalog entry by parsed diagnostic code.
    pub fn lookup_code(&self, code: DiagnosticCode) -> Option<&CatalogEntry> {
        self.entries.get(&code)
    }

    /// Iterates over all entries in the catalog.
    pub fn iter(&self) -> impl Iterator<Item = (&DiagnosticCode, &CatalogEntry)> {
        self.entries.iter()
    }

    /// Returns the number of entries in the catalog.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the version string of the catalog.
    pub fn version(&self) -> &str {
        &self.version
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process;

    /// Test 1: Embedded catalog is non-empty.
    #[test]
    fn embedded_catalog_non_empty() {
        let catalog = Catalog::embedded();
        assert!(
            catalog.len() >= 18,
            "embedded catalog has fewer than 18 entries"
        );
    }

    /// Test 2: Embedded catalog contains E0001 through E0018.
    #[test]
    fn embedded_contains_e0001_through_e0018() {
        let catalog = Catalog::embedded();
        for n in 1..=18 {
            let code_str = format!("E{:04}", n);
            assert!(
                catalog.lookup(&code_str).is_some(),
                "catalog missing {}",
                code_str
            );
        }
    }

    /// Test 3: Lookup with canonical form succeeds.
    #[test]
    fn lookup_canonical_form() {
        let catalog = Catalog::embedded();
        assert!(catalog.lookup("E0001").is_some());
    }

    /// Test 4: Lookup with shorthand form matches canonical.
    #[test]
    fn lookup_shorthand_matches_canonical() {
        let catalog = Catalog::embedded();
        let canon = catalog.lookup("E0001").unwrap();
        let short = catalog.lookup("E1").unwrap();
        assert!(std::ptr::eq(canon as *const _, short as *const _));
    }

    /// Test 5: Lookup of unknown code returns None.
    #[test]
    fn lookup_unknown_returns_none() {
        let catalog = Catalog::embedded();
        assert!(catalog.lookup("Z9099").is_none());
    }

    /// Test 6: Lookup of invalid strings returns None.
    #[test]
    fn lookup_invalid_string_returns_none() {
        let catalog = Catalog::embedded();
        assert!(catalog.lookup("not-a-code").is_none());
        assert!(catalog.lookup("").is_none());
    }

    /// Test 7: E0001 entry has expected fields.
    #[test]
    fn e0001_fields() {
        let catalog = Catalog::embedded();
        let entry = catalog.lookup("E0001").unwrap();
        assert_eq!(entry.severity, Severity::Error);
        assert_eq!(entry.category, "lexer");
        assert!(
            entry.title.to_lowercase().contains("utf-8"),
            "E0001 title should mention UTF-8"
        );
    }

    /// Test 8: Iterator yields entries in sorted order.
    #[test]
    fn iter_is_sorted() {
        let catalog = Catalog::embedded();
        let mut prev_code: Option<DiagnosticCode> = None;
        for (code, _entry) in catalog.iter() {
            if let Some(p) = prev_code {
                assert!(
                    p < *code,
                    "catalog iteration not sorted: {:?} not less than {:?}",
                    p,
                    code
                );
            }
            prev_code = Some(*code);
        }
    }

    /// Test 9: Catalog version is present.
    #[test]
    fn version_present() {
        let catalog = Catalog::embedded();
        assert_eq!(catalog.version(), "0.1.0");
    }

    /// Test 10: Load from path roundtrip.
    #[test]
    fn load_from_path_roundtrip() {
        let embedded = Catalog::embedded();
        let temp_path =
            std::env::temp_dir().join(format!("paideia-as-catalog-test-{}.toml", process::id()));

        // Write the embedded catalog's TOML to a temp file.
        let toml_str = include_str!("../catalog.toml");
        std::fs::write(&temp_path, toml_str).expect("write temp catalog");

        // Load it back.
        let loaded = Catalog::load(&temp_path).expect("load temp catalog");

        // Verify it matches.
        assert_eq!(loaded.len(), embedded.len());
        assert_eq!(loaded.version(), embedded.version());

        // Clean up.
        let _ = std::fs::remove_file(&temp_path);
    }

    /// Test 11: Load rejects category mismatch.
    #[test]
    fn load_rejects_category_mismatch() {
        let toml_str = r#"
[catalog]
version = "0.1.0"

[diagnostic.E0001]
severity = "error"
category = "parser"
title = "x"
brief = "x"
"#;
        let temp_path = std::env::temp_dir().join(format!(
            "paideia-as-catalog-mismatch-{}.toml",
            process::id()
        ));
        std::fs::write(&temp_path, toml_str).expect("write temp catalog");

        let result = Catalog::load(&temp_path);
        assert!(
            matches!(result, Err(CatalogError::CategoryMismatch { .. })),
            "expected CategoryMismatch error, got {:?}",
            result
        );

        let _ = std::fs::remove_file(&temp_path);
    }

    /// Test 12: Load rejects invalid code key.
    #[test]
    fn load_rejects_invalid_code_key() {
        let toml_str = r#"
[catalog]
version = "0.1.0"

[diagnostic.XYZ]
severity = "error"
category = "lexer"
title = "x"
brief = "x"
"#;
        let temp_path =
            std::env::temp_dir().join(format!("paideia-as-catalog-badkey-{}.toml", process::id()));
        std::fs::write(&temp_path, toml_str).expect("write temp catalog");

        let result = Catalog::load(&temp_path);
        assert!(
            matches!(result, Err(CatalogError::InvalidCode { .. })),
            "expected InvalidCode error, got {:?}",
            result
        );

        let _ = std::fs::remove_file(&temp_path);
    }
}
