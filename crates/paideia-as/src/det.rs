//! Build-determinism helpers.
//!
//! This module provides utilities to support reproducible builds by reading
//! environment variables per the Reproducible Builds spec and applying them
//! to build artifact metadata.

use std::path::{Path, PathBuf};

/// Read SOURCE_DATE_EPOCH from env, fall back to a fixed default (0).
///
/// Per the Reproducible Builds spec, SOURCE_DATE_EPOCH is the canonical
/// way to fix the timestamp embedded in build artifacts.
///
/// # Errors
///
/// Does not error; returns 0 if parsing fails or env is unset.
pub fn build_timestamp() -> u32 {
    build_timestamp_from(std::env::var("SOURCE_DATE_EPOCH").ok())
}

/// Parse SOURCE_DATE_EPOCH from an explicit value.
///
/// This is the testable form — it takes an optional string value
/// rather than reading the environment.
///
/// # Arguments
///
/// * `env_value` - The SOURCE_DATE_EPOCH environment variable value, or None.
///
/// # Returns
///
/// The parsed u32 timestamp, or 0 if parsing fails or value is None.
pub fn build_timestamp_from(env_value: Option<String>) -> u32 {
    env_value.and_then(|s| s.parse::<u32>().ok()).unwrap_or(0)
}

/// Map an absolute path to a build-relative form for embedding in debug info.
///
/// If PDX_PATH_PREFIX_MAP is set as "OLD=NEW", paths starting with OLD
/// are rewritten to start with NEW. Otherwise, returns the path as-is.
///
/// # Example
///
/// With `PDX_PATH_PREFIX_MAP="/home/user/build/=/build/"`:
/// - Input: `/home/user/build/src/foo.pdx` → Output: `/build/src/foo.pdx`
/// - Input: `/other/path/bar.pdx` → Output: `/other/path/bar.pdx` (unchanged)
#[allow(dead_code)]
pub fn map_path(path: &Path) -> PathBuf {
    map_path_with(path, std::env::var("PDX_PATH_PREFIX_MAP").ok())
}

/// Map a path using an explicit prefix-mapping string.
///
/// This is the testable form — it takes an optional mapping string
/// rather than reading the environment.
///
/// # Arguments
///
/// * `path` - The path to transform.
/// * `map` - The PDX_PATH_PREFIX_MAP environment value (e.g., "OLD=NEW"), or None.
///
/// # Returns
///
/// The transformed path if a matching prefix was found, otherwise the original.
#[allow(dead_code)]
pub fn map_path_with(path: &Path, map: Option<String>) -> PathBuf {
    if let Some(map_str) = map
        && let Some((old, new)) = map_str.split_once('=')
    {
        let s = path.to_string_lossy();
        if let Some(rest) = s.strip_prefix(old) {
            return PathBuf::from(format!("{new}{rest}"));
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_timestamp_returns_zero_when_env_unset() {
        let result = build_timestamp_from(None);
        assert_eq!(result, 0);
    }

    #[test]
    fn build_timestamp_returns_parsed_value_when_env_set() {
        let result = build_timestamp_from(Some("1700000000".to_string()));
        assert_eq!(result, 1700000000);
    }

    #[test]
    fn build_timestamp_returns_zero_on_invalid_parse() {
        let result = build_timestamp_from(Some("not_a_number".to_string()));
        assert_eq!(result, 0);
    }

    #[test]
    fn map_path_returns_unchanged_when_env_unset() {
        let input = Path::new("/home/user/src/foo.pdx");
        let result = map_path_with(input, None);
        assert_eq!(result, input);
    }

    #[test]
    fn map_path_rewrites_matching_prefix() {
        let input = Path::new("/home/user/src/foo.pdx");
        let map = Some("/home/user/=/build/".to_string());
        let result = map_path_with(input, map);
        assert_eq!(result, Path::new("/build/src/foo.pdx"));
    }

    #[test]
    fn map_path_returns_unchanged_when_prefix_doesnt_match() {
        let input = Path::new("/other/path/bar.pdx");
        let map = Some("/home/user/=/build/".to_string());
        let result = map_path_with(input, map);
        assert_eq!(result, input);
    }

    #[test]
    fn map_path_handles_malformed_map_string() {
        let input = Path::new("/home/user/src/foo.pdx");
        let map = Some("malformed_no_equals".to_string());
        let result = map_path_with(input, map);
        assert_eq!(result, input);
    }

    #[test]
    fn map_path_empty_old_prefix() {
        let input = Path::new("/home/user/src/foo.pdx");
        let map = Some("=/new/prefix/".to_string());
        let result = map_path_with(input, map);
        // Empty string matches everything at the start, so it should rewrite
        assert_eq!(result, Path::new("/new/prefix//home/user/src/foo.pdx"));
    }
}
