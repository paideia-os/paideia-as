//! R227.M1 `.pds` header pragma parser.
//!
//! See the crate doc for the position of this parser in the wider
//! pipeline and the surface grammar. This module is a hand-written
//! line-by-line state machine — pure std, no lexer dependency.

use std::fmt;

use crate::cap_check::{self, CapCheckError, CapabilitySet};

/// Semantic version parsed from `#requires-paideia >= X.Y.Z`.
///
/// Three `u32` components; the M1 parser does not attempt any
/// pre-release / build-metadata suffix. Version comparison happens
/// in the runtime, not here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    /// Major version component.
    pub major: u32,
    /// Minor version component.
    pub minor: u32,
    /// Patch version component.
    pub patch: u32,
}

/// An `#import "path" as name` declaration.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Import {
    /// Filesystem-style path to the imported `.pds` module. Bytes as
    /// written in the source between the pair of `"`.
    pub path: String,
    /// The identifier the importing script uses to refer to the
    /// module. The `name` following `as`.
    pub alias: String,
}

/// A `#schema "SchemaName@version"` or `#schema "SchemaName"`
/// declaration.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SchemaRef {
    /// The schema name (the portion before `@`, or the whole quoted
    /// string when no `@` is present).
    pub name: String,
    /// The optional version pin (the portion after `@`); `None` when
    /// the pragma omits `@`.
    pub version: Option<String>,
}

/// Parsed `.pds` header block.
///
/// `body_offset` is the byte position in the original source where
/// the body starts — the caller feeds `src[header.body_offset ..]`
/// into `paideia-as-shell-lex`. It equals `src.len()` for a header-
/// only script (no body after the header terminator).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PdsHeader {
    /// Every `#capability "cap.name"` request, in source order.
    pub capabilities: Vec<String>,
    /// The `#requires-paideia >= X.Y.Z` pin, if the header carried
    /// one. Absent when no such pragma appeared.
    pub requires_paideia: Option<Version>,
    /// Every `#import "path" as name` declaration, in source order.
    pub imports: Vec<Import>,
    /// Every `#schema "..."` declaration, in source order. Names are
    /// de-duplicated at parse time (a repeat name is a
    /// `DuplicateSchema` error), so no two `SchemaRef` in this vector
    /// share a `name`.
    pub schemas: Vec<SchemaRef>,
    /// True iff the header contained at least one `#ascii` pragma.
    pub ascii: bool,
    /// Byte offset in the original source where the body starts. The
    /// body is `src[body_offset ..]`.
    pub body_offset: usize,
}

impl PdsHeader {
    /// Check this header's declared capabilities against an invoker
    /// [`CapabilitySet`] using R227.M2's subset rule.
    ///
    /// Thin forwarder to [`cap_check::check_subset`] — kept as a
    /// method on `PdsHeader` so the load-time call site reads as
    /// `header.check_against(&invoker)?;` without a second `use`.
    ///
    /// # Errors
    ///
    /// Returns [`CapCheckError::MissingCapabilities`] when at least
    /// one declared capability is absent from `invoker`. See
    /// [`cap_check`] for the exact-name / no-wildcard semantics.
    pub fn check_against(&self, invoker: &CapabilitySet) -> Result<(), CapCheckError> {
        cap_check::check_subset(&self.capabilities, invoker)
    }
}

/// Discriminated failure modes for [`parse_header`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PdsHeaderError {
    /// The line began with `#` but the pragma name did not match any
    /// recognised name. `line` is 1-based; `name` is the offending
    /// pragma name (without the leading `#`).
    UnknownPragma {
        /// Pragma name as written, minus the leading `#`.
        name: String,
        /// 1-based line number in the original source.
        line: usize,
    },
    /// `#requires-paideia` was present but its argument did not parse
    /// as `>= X.Y.Z` with three `u32` components.
    MalformedVersion {
        /// 1-based line number in the original source.
        line: usize,
        /// The rest-of-line argument text as written (post-`#requires-paideia`).
        got: String,
    },
    /// `#import` failed to parse. Common `reason` strings:
    ///   * `"missing quoted path"`
    ///   * `"missing 'as' keyword"`
    ///   * `"missing alias identifier"`
    ///   * `"trailing tokens after alias"`
    MalformedImport {
        /// 1-based line number in the original source.
        line: usize,
        /// A short human phrase naming which part of the pragma failed.
        reason: String,
    },
    /// `#capability` argument was not a quoted string, or contained
    /// trailing tokens after it.
    MalformedCapability {
        /// 1-based line number in the original source.
        line: usize,
        /// The argument text as written.
        got: String,
    },
    /// `#schema` argument was not a quoted `"Name"` or `"Name@version"`,
    /// or the quoted form was structurally invalid (empty name,
    /// misplaced `@`).
    MalformedSchema {
        /// 1-based line number in the original source.
        line: usize,
        /// The argument text as written.
        got: String,
    },
    /// A second `#schema "X"` appeared for the same schema name. The
    /// version pin is not consulted — two references to the same
    /// schema name are always a conflict at parse time (the runtime
    /// cannot reconcile two version pins for one schema without
    /// consulting the registry, so refuse it here).
    DuplicateSchema {
        /// The schema name that appeared twice.
        schema_name: String,
    },
}

impl fmt::Display for PdsHeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPragma { name, line } => {
                write!(f, "unknown pragma `#{name}` on line {line}")
            }
            Self::MalformedVersion { line, got } => {
                write!(
                    f,
                    "malformed `#requires-paideia` on line {line}: expected `>= X.Y.Z`, got `{got}`"
                )
            }
            Self::MalformedImport { line, reason } => {
                write!(f, "malformed `#import` on line {line}: {reason}")
            }
            Self::MalformedCapability { line, got } => {
                write!(
                    f,
                    "malformed `#capability` on line {line}: expected quoted capability name, got `{got}`"
                )
            }
            Self::MalformedSchema { line, got } => {
                write!(
                    f,
                    "malformed `#schema` on line {line}: expected quoted `Name` or `Name@version`, got `{got}`"
                )
            }
            Self::DuplicateSchema { schema_name } => {
                write!(f, "duplicate `#schema` for `{schema_name}`")
            }
        }
    }
}

impl std::error::Error for PdsHeaderError {}

/// Parse the header block of a `.pds` script.
///
/// See the crate documentation for the surface grammar. On success
/// returns a [`PdsHeader`] whose `body_offset` names the byte position
/// where the body begins (equal to `src.len()` if no body follows).
///
/// # Errors
///
/// Returns a [`PdsHeaderError`] the first time a pragma line fails to
/// parse. No partial header is returned on failure.
pub fn parse_header(src: &str) -> Result<PdsHeader, PdsHeaderError> {
    let mut header = PdsHeader::default();
    let mut cursor: usize = 0;
    let mut line_no: usize = 0;

    // Optional shebang: strictly the first line, must start with `#!`.
    if let Some(rest) = src.strip_prefix("#!") {
        let nl = rest.find('\n').map_or(rest.len(), |i| i + 1);
        cursor = 2 + nl;
        line_no = 1;
    }

    // Header pragma lines.
    while cursor < src.len() {
        line_no += 1;
        let remainder = &src[cursor..];
        let line_len = remainder.find('\n').map_or(remainder.len(), |i| i + 1);
        let raw_line = &remainder[..line_len];

        // A line whose trimmed body is empty terminates the header,
        // and the blank line itself is consumed (not part of body).
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            cursor += line_len;
            header.body_offset = cursor;
            return Ok(header);
        }

        // A line whose first non-whitespace byte is not `#`
        // terminates the header. That line is not consumed; it
        // belongs to the body.
        if !trimmed.starts_with('#') {
            header.body_offset = cursor;
            return Ok(header);
        }

        // Strip the leading `#`, then dispatch on the pragma name.
        // We match on the trimmed body (leading whitespace before `#`
        // is tolerated for readability — the design doc examples all
        // have `#pragma` in the first column, but the grammar
        // doesn't forbid indentation).
        let after_hash = &trimmed[1..];
        let (name, args) = split_name_args(after_hash);

        match name {
            "capability" => {
                let cap = parse_quoted_arg(args).ok_or_else(|| {
                    PdsHeaderError::MalformedCapability {
                        line: line_no,
                        got: args.to_owned(),
                    }
                })?;
                if cap.is_empty() {
                    return Err(PdsHeaderError::MalformedCapability {
                        line: line_no,
                        got: args.to_owned(),
                    });
                }
                header.capabilities.push(cap);
            }
            "requires-paideia" => {
                let version = parse_requires_paideia_args(args).ok_or_else(|| {
                    PdsHeaderError::MalformedVersion {
                        line: line_no,
                        got: args.to_owned(),
                    }
                })?;
                header.requires_paideia = Some(version);
            }
            "import" => {
                let import = parse_import_args(args, line_no)?;
                header.imports.push(import);
            }
            "schema" => {
                let schema = parse_schema_args(args, line_no)?;
                if header.schemas.iter().any(|s| s.name == schema.name) {
                    return Err(PdsHeaderError::DuplicateSchema {
                        schema_name: schema.name,
                    });
                }
                header.schemas.push(schema);
            }
            "ascii" => {
                // Argument must be empty (module-doc: "no args").
                if !args.trim().is_empty() {
                    return Err(PdsHeaderError::UnknownPragma {
                        name: format!("ascii {}", args.trim()),
                        line: line_no,
                    });
                }
                header.ascii = true;
            }
            other => {
                return Err(PdsHeaderError::UnknownPragma {
                    name: other.to_owned(),
                    line: line_no,
                });
            }
        }

        cursor += line_len;
    }

    // Source ran out inside the header block (no blank terminator,
    // no non-`#` line). All bytes consumed; the body is empty.
    header.body_offset = cursor;
    Ok(header)
}

/// Split a `#`-stripped pragma line into (name, rest).
///
/// The name is the leading run of `[A-Za-z0-9_-]`; the rest starts at
/// the first byte after that run, verbatim (including any leading
/// whitespace, which the per-pragma parser then trims as it likes).
fn split_name_args(after_hash: &str) -> (&str, &str) {
    let end = after_hash
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(after_hash.len());
    (&after_hash[..end], &after_hash[end..])
}

/// Extract the single quoted argument of `#capability "…"`.
///
/// Returns `None` if the argument doesn't strictly match
/// `whitespace* "..." whitespace*`. Inner double-quote escaping is
/// not supported at M1; capability names never contain `"`.
fn parse_quoted_arg(args: &str) -> Option<String> {
    let trimmed = args.trim();
    let inner = trimmed.strip_prefix('"')?.strip_suffix('"')?;
    if inner.contains('"') {
        return None;
    }
    Some(inner.to_owned())
}

/// Parse the argument of `#requires-paideia`. Must match exactly
/// `>= X.Y.Z` (with any amount of whitespace around the `>=` and
/// after `X.Y.Z`).
fn parse_requires_paideia_args(args: &str) -> Option<Version> {
    let trimmed = args.trim();
    let rest = trimmed.strip_prefix(">=")?.trim_start();
    // Everything after the version triple must be whitespace.
    let (triple, tail) = match rest.find(|c: char| c.is_whitespace()) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    if !tail.trim().is_empty() {
        return None;
    }
    let mut parts = triple.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(Version {
        major,
        minor,
        patch,
    })
}

/// Parse the argument of `#import`.
///
/// Grammar: `"path" as name` — the alias is an ASCII identifier
/// (`[A-Za-z_][A-Za-z0-9_]*`).
fn parse_import_args(args: &str, line: usize) -> Result<Import, PdsHeaderError> {
    let trimmed = args.trim();
    // The quoted path.
    let after_open = trimmed
        .strip_prefix('"')
        .ok_or_else(|| PdsHeaderError::MalformedImport {
            line,
            reason: "missing quoted path".to_owned(),
        })?;
    let close = after_open
        .find('"')
        .ok_or_else(|| PdsHeaderError::MalformedImport {
            line,
            reason: "missing quoted path".to_owned(),
        })?;
    let path = &after_open[..close];
    let after_path = after_open[close + 1..].trim_start();

    // The `as` keyword.
    let after_as = after_path
        .strip_prefix("as")
        .ok_or_else(|| PdsHeaderError::MalformedImport {
            line,
            reason: "missing 'as' keyword".to_owned(),
        })?;
    // Enforce word-boundary after `as`.
    if !after_as
        .chars()
        .next()
        .is_none_or(|c| c.is_whitespace())
    {
        return Err(PdsHeaderError::MalformedImport {
            line,
            reason: "missing 'as' keyword".to_owned(),
        });
    }
    let after_as = after_as.trim_start();

    // The alias identifier.
    if after_as.is_empty() {
        return Err(PdsHeaderError::MalformedImport {
            line,
            reason: "missing alias identifier".to_owned(),
        });
    }
    let alias_end = after_as
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(after_as.len());
    let alias = &after_as[..alias_end];
    if alias.is_empty() {
        return Err(PdsHeaderError::MalformedImport {
            line,
            reason: "missing alias identifier".to_owned(),
        });
    }
    if let Some(first) = alias.chars().next()
        && !(first.is_ascii_alphabetic() || first == '_')
    {
        return Err(PdsHeaderError::MalformedImport {
            line,
            reason: "missing alias identifier".to_owned(),
        });
    }
    let tail = &after_as[alias_end..];
    if !tail.trim().is_empty() {
        return Err(PdsHeaderError::MalformedImport {
            line,
            reason: "trailing tokens after alias".to_owned(),
        });
    }

    Ok(Import {
        path: path.to_owned(),
        alias: alias.to_owned(),
    })
}

/// Parse the argument of `#schema`.
///
/// Grammar: `"Name"` or `"Name@version"`. The `@` split is at the
/// first `@`; `Name` must be non-empty; version (if present) must
/// also be non-empty.
fn parse_schema_args(args: &str, line: usize) -> Result<SchemaRef, PdsHeaderError> {
    let inner = parse_quoted_arg(args).ok_or_else(|| PdsHeaderError::MalformedSchema {
        line,
        got: args.to_owned(),
    })?;
    if inner.is_empty() {
        return Err(PdsHeaderError::MalformedSchema {
            line,
            got: args.to_owned(),
        });
    }
    match inner.split_once('@') {
        None => Ok(SchemaRef {
            name: inner,
            version: None,
        }),
        Some((name, version)) => {
            if name.is_empty() || version.is_empty() {
                return Err(PdsHeaderError::MalformedSchema {
                    line,
                    got: args.to_owned(),
                });
            }
            Ok(SchemaRef {
                name: name.to_owned(),
                version: Some(version.to_owned()),
            })
        }
    }
}
