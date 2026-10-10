//! `.pdxfmt` TOML config discovery and loading (FMT-11, paideia-as#1575).
//!
//! # Role
//!
//! Formatter-run configuration (`max_width`, alignment flags, indent
//! width, trailing-comma + glyph policies) lives in a `.pdxfmt` TOML
//! file at the workspace root. This module:
//!
//! 1. [`discover`] — walks ancestor directories from a start path
//!    looking for the nearest `.pdxfmt`.
//! 2. [`load`] — reads + parses a single `.pdxfmt`.
//! 3. [`apply_to_options`] — merges parsed overrides onto a
//!    [`FormatOptions`] with the crate defaults preserved for any key
//!    the file omits.
//!
//! # TOML schema
//!
//! ```toml
//! [fmt]
//! max_width          = 100       # usize, default 100
//! indent_width       = 2         # usize, default 2
//! align_match_arms   = true      # bool,  default true
//! align_unsafe_fields = true     # bool,  default true
//! trailing_comma     = "always"  # "always" | "never" | "preserve"
//! glyph              = "preserve" # "preserve" | "unicode" | "ascii"
//! ```
//!
//! Every key is optional. An empty `.pdxfmt`, or one with only `[fmt]`
//! and no keys, is semantically equivalent to [`FormatOptions::default`]
//! after [`apply_to_options`].
//!
//! # Discovery semantics
//!
//! [`discover`] starts at `start` (treated as a directory) and walks
//! up ancestors until it finds a readable `.pdxfmt`. On a parse error
//! it returns `None` (the file is treated as if it did not exist); use
//! [`load`] directly when callers want to surface diagnostics.

use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::{FormatOptions, GlyphPolicy, TrailingCommaPolicy};

/// Canonical filename for a paideia-as formatter config file.
pub const CONFIG_FILENAME: &str = ".pdxfmt";

/// Parsed `.pdxfmt` content. Every field is optional so a partial file
/// merges cleanly over [`FormatOptions::default`] via
/// [`apply_to_options`].
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct FmtConfig {
    /// `[fmt]` section — all formatter knobs.
    #[serde(default)]
    pub fmt: FmtSection,
}

/// `[fmt]` TOML section. All keys optional.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct FmtSection {
    /// Override for [`FormatOptions::max_width`].
    #[serde(default)]
    pub max_width: Option<usize>,
    /// Override for [`FormatOptions::indent_width`].
    #[serde(default)]
    pub indent_width: Option<usize>,
    /// Override for [`FormatOptions::align_match_arms`].
    #[serde(default)]
    pub align_match_arms: Option<bool>,
    /// Override for [`FormatOptions::align_unsafe_fields`].
    #[serde(default)]
    pub align_unsafe_fields: Option<bool>,
    /// Override for [`FormatOptions::trailing_comma_policy`].
    #[serde(default)]
    pub trailing_comma: Option<TrailingCommaPolicy>,
    /// Override for [`FormatOptions::glyph_policy`].
    #[serde(default)]
    pub glyph: Option<GlyphPolicy>,
}

/// Errors from [`load`].
#[derive(Debug)]
pub enum ConfigError {
    /// I/O error reading the file (other than NotFound).
    Io(std::io::Error),
    /// TOML parse error.
    Parse(toml::de::Error),
    /// `.pdxfmt` not present at the requested path.
    NotFound,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "I/O error reading .pdxfmt: {}", e),
            ConfigError::Parse(e) => write!(f, "failed to parse .pdxfmt: {}", e),
            ConfigError::NotFound => write!(f, ".pdxfmt not found"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound {
            ConfigError::NotFound
        } else {
            ConfigError::Io(e)
        }
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(e: toml::de::Error) -> Self {
        ConfigError::Parse(e)
    }
}

/// Read + parse `.pdxfmt` at `path`.
///
/// `path` points at the file itself (not the containing directory).
/// Prefer [`discover`] when the caller only has a source-file location.
pub fn load(path: &Path) -> Result<FmtConfig, ConfigError> {
    let content = std::fs::read_to_string(path)?;
    let cfg: FmtConfig = toml::from_str(&content)?;
    Ok(cfg)
}

/// Walk up from `start` (treated as a directory) looking for the
/// nearest `.pdxfmt`. Returns the file's path and the parsed config.
///
/// Returns `None` if no `.pdxfmt` is found between `start` and the
/// filesystem root, or if the found file cannot be parsed (callers
/// that need to surface parse errors should call [`load`] directly).
pub fn discover(start: &Path) -> Option<(PathBuf, FmtConfig)> {
    let mut current = start.to_path_buf();
    loop {
        let candidate = current.join(CONFIG_FILENAME);
        if candidate.is_file() {
            // Found a candidate; parse errors collapse to None per
            // the documented discovery semantics. Callers that want
            // to see the error should call `load` directly.
            if let Ok(cfg) = load(&candidate) {
                return Some((candidate, cfg));
            } else {
                return None;
            }
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Overlay the overrides in `cfg` onto `opts`.
///
/// Keys absent from `cfg` leave the corresponding `opts` field
/// untouched, so this composes cleanly: start from
/// [`FormatOptions::default`], apply any discovered config, apply any
/// per-run CLI overrides, in that order.
pub fn apply_to_options(cfg: &FmtConfig, opts: &mut FormatOptions) {
    if let Some(v) = cfg.fmt.max_width {
        opts.max_width = v;
    }
    if let Some(v) = cfg.fmt.indent_width {
        opts.indent_width = v;
    }
    if let Some(v) = cfg.fmt.align_match_arms {
        opts.align_match_arms = v;
    }
    if let Some(v) = cfg.fmt.align_unsafe_fields {
        opts.align_unsafe_fields = v;
    }
    if let Some(v) = cfg.fmt.trailing_comma {
        opts.trailing_comma_policy = v;
    }
    if let Some(v) = cfg.fmt.glyph {
        opts.glyph_policy = v;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// AC: no `.pdxfmt` in any ancestor → discover returns None.
    #[test]
    fn discover_returns_none_when_no_pdxfmt() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let nested = tmp.path().join("a").join("b");
        fs::create_dir_all(&nested).expect("mkdir");

        let got = discover(&nested);
        assert!(got.is_none());
    }

    /// AC: `.pdxfmt` with `max_width = 120` → FmtConfig captures the
    /// override and `apply_to_options` lifts it into FormatOptions.
    #[test]
    fn load_parses_max_width_override_and_applies() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(CONFIG_FILENAME);
        fs::write(&path, "[fmt]\nmax_width = 120\n").expect("write");

        let cfg = load(&path).expect("load");
        assert_eq!(cfg.fmt.max_width, Some(120));
        assert_eq!(cfg.fmt.indent_width, None);

        let mut opts = FormatOptions::default();
        apply_to_options(&cfg, &mut opts);
        assert_eq!(opts.max_width, 120);
        // Other fields untouched.
        assert_eq!(opts.indent_width, 2);
        assert!(opts.align_match_arms);
    }

    /// AC: invalid TOML surfaces as `ConfigError::Parse`.
    #[test]
    fn load_rejects_invalid_toml() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(CONFIG_FILENAME);
        fs::write(&path, "this = is = not valid toml\n").expect("write");

        let got = load(&path);
        assert!(matches!(got, Err(ConfigError::Parse(_))));
    }

    /// AC: discover walks up ancestor directories to find `.pdxfmt`.
    #[test]
    fn discover_walks_up_from_nested_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let nested = tmp.path().join("a").join("b").join("c");
        fs::create_dir_all(&nested).expect("mkdir");

        // Place .pdxfmt two levels up (at `a/`).
        let cfg_path = tmp.path().join("a").join(CONFIG_FILENAME);
        fs::write(&cfg_path, "[fmt]\nmax_width = 88\n").expect("write");

        let (found_path, cfg) = discover(&nested).expect("should find");
        assert_eq!(found_path, cfg_path);
        assert_eq!(cfg.fmt.max_width, Some(88));
    }

    /// AC: partial override — only the mentioned fields change; the
    /// rest keep crate defaults.
    #[test]
    fn apply_to_options_partial_merges_with_defaults() {
        let cfg = FmtConfig {
            fmt: FmtSection {
                align_match_arms: Some(false),
                glyph: Some(GlyphPolicy::Unicode),
                ..Default::default()
            },
        };

        let mut opts = FormatOptions::default();
        apply_to_options(&cfg, &mut opts);

        // Overridden:
        assert!(!opts.align_match_arms);
        assert_eq!(opts.glyph_policy, GlyphPolicy::Unicode);
        // Defaults preserved:
        assert_eq!(opts.max_width, 100);
        assert_eq!(opts.indent_width, 2);
        assert!(opts.align_unsafe_fields);
        assert_eq!(opts.trailing_comma_policy, TrailingCommaPolicy::Always);
    }

    /// AC: the full schema round-trips through TOML.
    #[test]
    fn load_parses_full_schema() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(CONFIG_FILENAME);
        let content = r#"[fmt]
max_width = 120
indent_width = 4
align_match_arms = false
align_unsafe_fields = false
trailing_comma = "never"
glyph = "ascii"
"#;
        fs::write(&path, content).expect("write");

        let cfg = load(&path).expect("load");
        assert_eq!(cfg.fmt.max_width, Some(120));
        assert_eq!(cfg.fmt.indent_width, Some(4));
        assert_eq!(cfg.fmt.align_match_arms, Some(false));
        assert_eq!(cfg.fmt.align_unsafe_fields, Some(false));
        assert_eq!(cfg.fmt.trailing_comma, Some(TrailingCommaPolicy::Never));
        assert_eq!(cfg.fmt.glyph, Some(GlyphPolicy::Ascii));

        let mut opts = FormatOptions::default();
        apply_to_options(&cfg, &mut opts);
        assert_eq!(opts.max_width, 120);
        assert_eq!(opts.indent_width, 4);
        assert!(!opts.align_match_arms);
        assert!(!opts.align_unsafe_fields);
        assert_eq!(opts.trailing_comma_policy, TrailingCommaPolicy::Never);
        assert_eq!(opts.glyph_policy, GlyphPolicy::Ascii);
    }

    /// AC: `load` on a missing path surfaces NotFound (not a panic).
    #[test]
    fn load_notfound_when_file_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let missing = tmp.path().join("does-not-exist.pdxfmt");

        let got = load(&missing);
        assert!(matches!(got, Err(ConfigError::NotFound)));
    }

    /// AC: an empty FmtConfig leaves every FormatOptions default alone.
    #[test]
    fn apply_to_options_empty_leaves_defaults() {
        let cfg = FmtConfig::default();
        let mut opts = FormatOptions::default();
        let baseline = FormatOptions::default();

        apply_to_options(&cfg, &mut opts);

        assert_eq!(opts.max_width, baseline.max_width);
        assert_eq!(opts.indent_width, baseline.indent_width);
        assert_eq!(opts.align_match_arms, baseline.align_match_arms);
        assert_eq!(opts.align_unsafe_fields, baseline.align_unsafe_fields);
        assert_eq!(opts.trailing_comma_policy, baseline.trailing_comma_policy);
        assert_eq!(opts.glyph_policy, baseline.glyph_policy);
    }

    /// AC: TrailingCommaPolicy variants deserialize from lowercase.
    #[test]
    fn trailing_comma_policy_lowercase_variants() {
        for (lit, expected) in [
            ("always", TrailingCommaPolicy::Always),
            ("never", TrailingCommaPolicy::Never),
            ("preserve", TrailingCommaPolicy::Preserve),
        ] {
            let src = format!("[fmt]\ntrailing_comma = \"{}\"\n", lit);
            let cfg: FmtConfig = toml::from_str(&src).expect("parse");
            assert_eq!(cfg.fmt.trailing_comma, Some(expected));
        }
    }

    /// AC: GlyphPolicy variants deserialize from lowercase.
    #[test]
    fn glyph_policy_lowercase_variants() {
        for (lit, expected) in [
            ("preserve", GlyphPolicy::Preserve),
            ("unicode", GlyphPolicy::Unicode),
            ("ascii", GlyphPolicy::Ascii),
        ] {
            let src = format!("[fmt]\nglyph = \"{}\"\n", lit);
            let cfg: FmtConfig = toml::from_str(&src).expect("parse");
            assert_eq!(cfg.fmt.glyph, Some(expected));
        }
    }

    /// AC: discover prefers the nearest ancestor when multiple
    /// ancestors have a `.pdxfmt` (deepest wins).
    #[test]
    fn discover_prefers_nearest_ancestor() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let outer_cfg = tmp.path().join(CONFIG_FILENAME);
        fs::write(&outer_cfg, "[fmt]\nmax_width = 80\n").expect("write outer");

        let inner_dir = tmp.path().join("inner");
        fs::create_dir_all(&inner_dir).expect("mkdir inner");
        let inner_cfg = inner_dir.join(CONFIG_FILENAME);
        fs::write(&inner_cfg, "[fmt]\nmax_width = 140\n").expect("write inner");

        let leaf = inner_dir.join("leaf");
        fs::create_dir_all(&leaf).expect("mkdir leaf");

        let (found_path, cfg) = discover(&leaf).expect("should find");
        assert_eq!(found_path, inner_cfg);
        assert_eq!(cfg.fmt.max_width, Some(140));
    }
}
