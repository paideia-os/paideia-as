//! paideia-os.toml workspace manifest reader.
//!
//! This module provides utilities for discovering and parsing workspace manifest files
//! (`paideia-os.toml`) that define workspace configuration, source roots, ABI versions,
//! and signing requirements for paideia-as projects.
//!
//! When a manifest cannot be found, the LSP treats this as "no workspace" and uses
//! default settings, rather than crashing.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// A parsed paideia-os.toml workspace manifest.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct WorkspaceManifest {
    /// Workspace configuration.
    #[serde(default)]
    pub workspace: WorkspaceConfig,
}

/// Configuration section from [workspace] in paideia-os.toml.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct WorkspaceConfig {
    /// Workspace name.
    #[serde(default)]
    pub name: String,

    /// Source roots (directory paths relative to the manifest).
    #[serde(default)]
    pub source_roots: Vec<PathBuf>,

    /// Default ABI version for this workspace (paideia-as ABI).
    #[serde(default)]
    pub abi_version: Option<u32>,

    /// Required signing scope for artifacts in this workspace.
    #[serde(default)]
    pub signing: Option<SigningConfig>,
}

/// Signing configuration for workspace artifacts.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct SigningConfig {
    /// Effect ids the signing key must subsume.
    #[serde(default)]
    pub required_scope: Vec<u32>,
}

/// Errors that can occur when reading or discovering workspace manifests.
#[derive(Debug)]
pub enum ManifestError {
    /// I/O error (e.g., file read failure).
    Io(std::io::Error),

    /// TOML parsing error.
    Parse(toml::de::Error),

    /// Manifest file not found.
    NotFound,
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::Io(e) => write!(f, "I/O error reading manifest: {}", e),
            ManifestError::Parse(e) => write!(f, "Failed to parse manifest: {}", e),
            ManifestError::NotFound => write!(f, "Manifest file not found"),
        }
    }
}

impl std::error::Error for ManifestError {}

impl From<std::io::Error> for ManifestError {
    fn from(e: std::io::Error) -> Self {
        ManifestError::Io(e)
    }
}

impl From<toml::de::Error> for ManifestError {
    fn from(e: toml::de::Error) -> Self {
        ManifestError::Parse(e)
    }
}

const MANIFEST_FILENAME: &str = "paideia-os.toml";

impl WorkspaceManifest {
    /// Load and parse `paideia-os.toml` from the given directory.
    ///
    /// # Returns
    ///
    /// - `Ok(manifest)` if the file exists and parses successfully.
    /// - `Err(ManifestError::NotFound)` if the file does not exist in the directory.
    /// - `Err(ManifestError::Io(_))` if a read error occurs.
    /// - `Err(ManifestError::Parse(_))` if the TOML content cannot be parsed.
    pub fn load_from_dir(dir: &Path) -> Result<Self, ManifestError> {
        let manifest_path = dir.join(MANIFEST_FILENAME);

        match std::fs::read_to_string(&manifest_path) {
            Ok(content) => {
                let manifest = toml::from_str(&content)?;
                Ok(manifest)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(ManifestError::NotFound),
            Err(e) => Err(ManifestError::Io(e)),
        }
    }

    /// Walk up from the start path looking for `paideia-os.toml`.
    ///
    /// # Returns
    ///
    /// - `Ok((manifest, directory))` where `directory` is the path where the manifest was found.
    /// - `Err(ManifestError::NotFound)` if no manifest is found up to the filesystem root.
    /// - `Err(ManifestError::Parse(_))` if the manifest is found but cannot be parsed.
    /// - `Err(ManifestError::Io(_))` if a read error occurs (other than NotFound).
    ///
    /// This method walks up the directory hierarchy from `start`, checking each ancestor
    /// directory for a `paideia-os.toml` file until one is found or the filesystem root
    /// is reached.
    pub fn discover(start: &Path) -> Result<(Self, PathBuf), ManifestError> {
        let mut current = start.to_path_buf();

        loop {
            match Self::load_from_dir(&current) {
                Ok(manifest) => return Ok((manifest, current)),
                Err(ManifestError::NotFound) => {
                    // Continue walking up
                    if !current.pop() {
                        // Reached filesystem root
                        return Err(ManifestError::NotFound);
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// AC 2: missing manifest produces NotFound, not a crash.
    #[test]
    fn load_from_dir_returns_not_found_for_missing_file() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let result = WorkspaceManifest::load_from_dir(temp_dir.path());

        assert!(matches!(result, Err(ManifestError::NotFound)));
    }

    /// Load a minimal manifest with only a name.
    #[test]
    fn load_from_dir_parses_minimal_manifest() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let manifest_path = temp_dir.path().join(MANIFEST_FILENAME);

        let content = "[workspace]\nname = \"demo\"\n";
        fs::write(&manifest_path, content).expect("failed to write manifest");

        let result = WorkspaceManifest::load_from_dir(temp_dir.path());

        assert!(result.is_ok());
        let manifest = result.unwrap();
        assert_eq!(manifest.workspace.name, "demo");
        assert!(manifest.workspace.source_roots.is_empty());
        assert!(manifest.workspace.abi_version.is_none());
        assert!(manifest.workspace.signing.is_none());
    }

    /// AC 1: Load workspace with three source roots.
    #[test]
    fn load_from_dir_parses_workspace_with_three_source_roots() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let manifest_path = temp_dir.path().join(MANIFEST_FILENAME);

        let content = r#"[workspace]
name = "multi-root"
source_roots = ["a", "b", "c"]
abi_version = 42
"#;
        fs::write(&manifest_path, content).expect("failed to write manifest");

        let result = WorkspaceManifest::load_from_dir(temp_dir.path());

        assert!(result.is_ok());
        let manifest = result.unwrap();

        // Snapshot assertions (AC 3)
        assert_eq!(manifest.workspace.name, "multi-root");
        assert_eq!(
            manifest.workspace.source_roots,
            vec![PathBuf::from("a"), PathBuf::from("b"), PathBuf::from("c")]
        );
        assert_eq!(manifest.workspace.abi_version, Some(42));
        assert!(manifest.workspace.signing.is_none());
    }

    /// Discover walks up from a nested directory to find the manifest.
    #[test]
    fn discover_walks_up_from_nested_dir() {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let nested = temp_dir.path().join("a").join("b").join("c");
        fs::create_dir_all(&nested).expect("failed to create nested dirs");

        let manifest_path = temp_dir.path().join("a").join(MANIFEST_FILENAME);
        let content = "[workspace]\nname = \"discovered\"\n";
        fs::write(&manifest_path, content).expect("failed to write manifest");

        let result = WorkspaceManifest::discover(&nested);

        assert!(result.is_ok());
        let (manifest, found_dir) = result.unwrap();
        assert_eq!(manifest.workspace.name, "discovered");
        assert_eq!(found_dir, temp_dir.path().join("a"));
    }

    /// Discover returns NotFound when starting from a dir with no manifest up to root.
    #[test]
    fn discover_returns_not_found_at_filesystem_root() {
        // Use a temporary directory that we know has no manifest.
        // We'll start from a deeply nested path within it.
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let nested = temp_dir.path().join("deep").join("nested").join("path");
        fs::create_dir_all(&nested).expect("failed to create nested dirs");

        let result = WorkspaceManifest::discover(&nested);

        assert!(matches!(result, Err(ManifestError::NotFound)));
    }
}
