//! R222.M4 — HM-typed argv + flag parsing against `ArgSpec` / `FlagSpec`.
//!
//! # Contract
//!
//! Given a command's `arguments : Vec<ArgSpec>` and `flags :
//! Vec<FlagSpec>` (returned from the R222.M2 functor), plus the raw
//! argv the R222.M3 dispatcher split off the shell line, this module
//! parses argv into a `Vec<Value>` (one per positional arg) and flags
//! into a `HashMap<String, Value>`. Each parse checks the argv token
//! against the spec's [`sig::resolve_type_name`]-elaborated
//! `paideia_as_types::Type` before wrapping in a [`Value`]; on
//! mismatch the caller gets a structured error carrying the arg/flag
//! name, the expected type-name literal, the got Value tag, and the
//! byte-span the offending token occupied in the joined argv.
//!
//! # Byte-span semantics
//!
//! `span: (usize, usize)` is `(start, end)` in bytes into a
//! space-joined argv (the shape a downstream diagnostics layer will
//! reconstruct — R222.M4 does not yet wire a `miette::Diagnostic` for
//! these errors; that lands with R222.M8's `describe` support). The
//! start of the Nth argv token is `sum(len(argv[0..N])) + N` (one
//! space between tokens); the [`parse_argv`] driver tracks this
//! cursor as it walks. A missing-required error uses a zero-width
//! span at the point where the missing token would have appeared;
//! consumers should widen it to the whole command name for display.
//!
//! # Split vs joined flag form
//!
//! Long flags accept either `--name=value` (joined) or `--name value`
//! (split); short flags accept `-x=value`, `-x value`, or (Bool only)
//! bare `-x`. [`FlagSpec::parse_from`] handles the joined form and
//! bare-switch form only; [`parse_flags`] walks a flag-argv slice
//! with the one-token lookahead the split form needs.
//!
//! # What R222.M4 deliberately does NOT do
//!
//! * **Rich value types** — `Value` is `Int | Str | Bool` only. The
//!   `FileType`, `ByteSize`, `Lambda`, `FieldRef|Lambda` type-name
//!   literals from the R222 command surface all fall through
//!   [`sig::resolve_type_name`] to `Type::Str` and are wrapped as
//!   `Value::Str`. The command's `execute` op refines them from
//!   there. Growing `Value` before there is a real ByteSize /
//!   FileType / Lambda type in `paideia_as_types` would encode the
//!   parse shape twice.
//!
//! * **HM-scheme unification** — `paideia_as_types` does not yet
//!   expose a public `TypeScheme` (see the [`sig::resolve_type_name`]
//!   note); ArgSpec/FlagSpec are monomorphic, so plain `Type`
//!   equality is enough. When R225.M4 lands `TypeScheme`, this
//!   module's `expected` field type widens without changing the
//!   error variants.
//!
//! * **Quoted argv tokens** — the R222.M3 dispatcher already splits
//!   on whitespace with no quote handling; this module trusts the
//!   already-split argv. Quote handling lands with the R221.M5
//!   unified AST parser.
//!
//! * **`miette::Diagnostic` errors** — plain enum today; the derive
//!   arrives with R222.M8.

use std::collections::HashMap;

use paideia_as_types::Type;

use crate::sig::{resolve_type_name, ArgSpec, FlagSpec};

/// Parsed argv or flag value.
///
/// Minimal by design — the five R222 light commands (`find`, `where`,
/// `sort`, `head`, `count`) only need `Int`, `Str`, and `Bool` on
/// their arg/flag slots. See the module doc for why richer types
/// (`ByteSize`, `Lambda`, …) collapse to `Str` here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// A signed 64-bit integer. Covers `Int` / `I64` / `U*` / `I*`
    /// type-name literals (with the same width narrowing the shell
    /// declares on the spec).
    Int(i64),
    /// A raw UTF-8 string — the safe default for any type-name that
    /// does not currently have a dedicated `Value` variant.
    Str(String),
    /// A boolean.
    Bool(bool),
}

impl Value {
    /// The HM `Type` this value inhabits.
    ///
    /// For error diagnostics — a caller reporting a type mismatch
    /// shows `expected: <spec type-name>` and `got: <this>`.
    pub fn type_of(&self) -> Type {
        match self {
            Self::Int(_) => Type::SInt(64),
            Self::Str(_) => Type::Str,
            Self::Bool(_) => Type::Bool,
        }
    }

    /// Short tag string for error messages (`"Int"` / `"String"` /
    /// `"Bool"`) — matches the `type_name` shell literal so a
    /// mismatch report reads cleanly.
    pub fn tag(&self) -> &'static str {
        match self {
            Self::Int(_) => "Int",
            Self::Str(_) => "String",
            Self::Bool(_) => "Bool",
        }
    }
}

/// What can go wrong parsing a positional argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArgParseError {
    /// A required argument was not supplied and had no default.
    Missing {
        /// The `ArgSpec::name` of the missing slot.
        name: String,
        /// Byte-span into the space-joined argv where the missing
        /// token would have appeared (zero-width at insertion point).
        span: (usize, usize),
    },
    /// A supplied token did not parse into the spec's HM type.
    TypeMismatch {
        /// The `ArgSpec::name` whose slot failed.
        name: String,
        /// The expected type-name literal (e.g. `"Int"`).
        expected: String,
        /// The got Value tag (e.g. `"String"`).
        got: String,
        /// Byte-span of the offending token in the joined argv.
        span: (usize, usize),
    },
    /// argv had more positional tokens than specs consumed.
    ExtraPositional {
        /// Byte-span of the first extra token.
        span: (usize, usize),
    },
}

impl std::fmt::Display for ArgParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing { name, .. } => {
                write!(f, "missing required argument `{name}`")
            }
            Self::TypeMismatch {
                name, expected, got, ..
            } => write!(
                f,
                "argument `{name}`: expected {expected}, got {got}"
            ),
            Self::ExtraPositional { .. } => f.write_str("extra positional argument"),
        }
    }
}

impl std::error::Error for ArgParseError {}

/// What can go wrong parsing a flag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlagParseError {
    /// A flag-name literal did not match any registered [`FlagSpec`].
    Unknown {
        /// The parsed flag-name literal (stripped of leading dashes).
        name: String,
        /// Byte-span of the flag token in the joined flag-argv.
        span: (usize, usize),
    },
    /// A flag value did not parse into the spec's HM type.
    TypeMismatch {
        /// The `FlagSpec::name` whose value failed.
        name: String,
        /// The expected type-name literal (e.g. `"Int"`).
        expected: String,
        /// The got Value tag (e.g. `"String"`).
        got: String,
        /// Byte-span of the offending value token.
        span: (usize, usize),
    },
    /// A non-Bool flag was supplied without a value (neither `=value`
    /// joined nor a following split-form token).
    MissingValue {
        /// The `FlagSpec::name` whose value slot is empty.
        name: String,
        /// Byte-span of the flag token itself.
        span: (usize, usize),
    },
}

impl std::fmt::Display for FlagParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown { name, .. } => write!(f, "unknown flag `{name}`"),
            Self::TypeMismatch {
                name, expected, got, ..
            } => write!(f, "flag `{name}`: expected {expected}, got {got}"),
            Self::MissingValue { name, .. } => {
                write!(f, "flag `{name}` requires a value")
            }
        }
    }
}

impl std::error::Error for FlagParseError {}

// -------------------------------------------------------------------
// Positional arg parsing
// -------------------------------------------------------------------

impl ArgSpec {
    /// Parse ONE argument from `argv[0]` against this spec's type.
    ///
    /// The `argv` slice shape (rather than a single `&str`) matches
    /// the shape the R222.M4 brief specifies, and lets a future
    /// grouped-arg variant (e.g. a spec that consumes two tokens)
    /// widen without breaking callers. Today the method only ever
    /// reads `argv[0]`.
    ///
    /// Returns [`ArgParseError::Missing`] if `argv` is empty AND the
    /// spec has no default AND is required. If it has a default, the
    /// default is parsed instead. If it is optional with no default,
    /// [`ArgParseError::Missing`] is still returned — callers who
    /// want a "None on empty" shape use [`parse_argv`] which threads
    /// optional handling across a spec vector.
    pub fn parse_from(&self, argv: &[String]) -> Result<Value, ArgParseError> {
        match argv.first() {
            Some(raw) => parse_scalar(&self.type_name, raw, (0, raw.len())).map_err(
                |(expected, got, span)| ArgParseError::TypeMismatch {
                    name: self.name.clone(),
                    expected,
                    got,
                    span,
                },
            ),
            None => {
                if let Some(def) = &self.default {
                    parse_scalar(&self.type_name, def, (0, def.len())).map_err(
                        |(expected, got, span)| ArgParseError::TypeMismatch {
                            name: self.name.clone(),
                            expected,
                            got,
                            span,
                        },
                    )
                } else {
                    Err(ArgParseError::Missing {
                        name: self.name.clone(),
                        span: (0, 0),
                    })
                }
            }
        }
    }
}

/// Apply the positional-arg specs to a raw argv, in order.
///
/// * If argv has fewer tokens than specs, each unfilled spec's
///   default is used; if the spec has no default AND is required,
///   [`ArgParseError::Missing`] fires.
/// * If argv has more tokens than specs consume, the first extra
///   token drives [`ArgParseError::ExtraPositional`].
/// * Optional specs with no default AND no argv token are simply
///   dropped from the output vector — the returned length equals the
///   number of specs that produced a `Value`.
///
/// The `span` on error is the byte-span into the space-joined argv;
/// see the module docs.
pub fn parse_argv(specs: &[ArgSpec], argv: &[String]) -> Result<Vec<Value>, ArgParseError> {
    let mut out = Vec::with_capacity(specs.len());
    let mut cursor = 0usize;
    for (i, spec) in specs.iter().enumerate() {
        match argv.get(i) {
            Some(raw) => {
                let span = (cursor, cursor + raw.len());
                out.push(parse_scalar(&spec.type_name, raw, span).map_err(
                    |(expected, got, sp)| ArgParseError::TypeMismatch {
                        name: spec.name.clone(),
                        expected,
                        got,
                        span: sp,
                    },
                )?);
                cursor = cursor.saturating_add(raw.len()).saturating_add(1);
            }
            None => match &spec.default {
                Some(def) => {
                    // Defaults parse against their own literal bytes;
                    // the argv-derived cursor is meaningless here so
                    // we report a zero-width span at the end.
                    let span = (cursor, cursor);
                    out.push(parse_scalar(&spec.type_name, def, span).map_err(
                        |(expected, got, sp)| ArgParseError::TypeMismatch {
                            name: spec.name.clone(),
                            expected,
                            got,
                            span: sp,
                        },
                    )?);
                }
                None if spec.required => {
                    return Err(ArgParseError::Missing {
                        name: spec.name.clone(),
                        span: (cursor, cursor),
                    });
                }
                None => {
                    // Optional, no default, no argv — drop silently.
                }
            },
        }
    }
    if let Some(extra) = argv.get(specs.len()) {
        return Err(ArgParseError::ExtraPositional {
            span: (cursor, cursor + extra.len()),
        });
    }
    Ok(out)
}

// -------------------------------------------------------------------
// Flag parsing
// -------------------------------------------------------------------

impl FlagSpec {
    /// Parse a single flag literal against THIS spec.
    ///
    /// Handles:
    ///
    /// * `--name=value` / `-x=value` — joined form; the value parses
    ///   into the spec's HM type.
    /// * `--name` / `-x` with `type_name = "Bool"` — bare switch,
    ///   yields `Value::Bool(true)`.
    /// * `--name` / `-x` with a non-Bool type — returns
    ///   [`FlagParseError::MissingValue`] (the split-form `--name
    ///   value` requires a lookahead which only [`parse_flags`]
    ///   provides; a single-literal caller must use the joined form).
    ///
    /// Returns [`FlagParseError::Unknown`] if the parsed name matches
    /// neither the spec's long-form name nor its short-form char.
    pub fn parse_from(&self, flag_lit: &str) -> Result<(String, Value), FlagParseError> {
        let span = (0, flag_lit.len());
        let (parsed_name, value_opt) = split_flag_lit(flag_lit, span)?;
        if !self.matches_name(&parsed_name) {
            return Err(FlagParseError::Unknown {
                name: parsed_name,
                span,
            });
        }
        match value_opt {
            Some(raw) => {
                // The value byte-span starts after `--name=` (or `-x=`);
                // count the dashes + name length + `=` sign.
                let value_start = flag_lit.len().saturating_sub(raw.len());
                let value_span = (value_start, flag_lit.len());
                parse_scalar(&self.type_name, &raw, value_span)
                    .map(|v| (self.name.clone(), v))
                    .map_err(|(expected, got, sp)| FlagParseError::TypeMismatch {
                        name: self.name.clone(),
                        expected,
                        got,
                        span: sp,
                    })
            }
            None => {
                if matches!(resolve_type_name(&self.type_name), Type::Bool) {
                    Ok((self.name.clone(), Value::Bool(true)))
                } else {
                    Err(FlagParseError::MissingValue {
                        name: self.name.clone(),
                        span,
                    })
                }
            }
        }
    }
}

/// Walk a flag-argv slice and parse each flag against the registered
/// specs; returns a name-keyed value map.
///
/// * `--name=value` and `-x=value` — joined form (single token).
/// * `--name value` and `-x value` — split form (two tokens); the
///   driver consumes the following token as the value.
/// * Bare `--name` / `-x` for a Bool-typed spec yields
///   `Value::Bool(true)`; for a non-Bool spec, the next token is
///   consumed as the value; if none remains, [`FlagParseError::MissingValue`].
///
/// Duplicate flags overwrite — last-one-wins, matching the eventual
/// substrate-side elaborator behaviour (there is no shell-user
/// affordance for "reject duplicates" that we want to encode here
/// before R222.M8 spans it in `describe` output).
pub fn parse_flags(
    specs: &[FlagSpec],
    flag_argv: &[String],
) -> Result<HashMap<String, Value>, FlagParseError> {
    let mut out = HashMap::with_capacity(specs.len());
    let mut i = 0usize;
    let mut cursor = 0usize;
    while i < flag_argv.len() {
        let lit = &flag_argv[i];
        let lit_span = (cursor, cursor + lit.len());
        let (parsed_name, value_opt) = split_flag_lit(lit, lit_span)?;
        let spec = specs
            .iter()
            .find(|s| s.matches_name(&parsed_name))
            .ok_or_else(|| FlagParseError::Unknown {
                name: parsed_name.clone(),
                span: lit_span,
            })?;
        let value = match value_opt {
            Some(raw) => {
                let value_start = cursor + lit.len().saturating_sub(raw.len());
                let value_span = (value_start, cursor + lit.len());
                parse_scalar(&spec.type_name, &raw, value_span).map_err(
                    |(expected, got, sp)| FlagParseError::TypeMismatch {
                        name: spec.name.clone(),
                        expected,
                        got,
                        span: sp,
                    },
                )?
            }
            None => {
                if matches!(resolve_type_name(&spec.type_name), Type::Bool) {
                    // Bare switch → true. Advance past the flag only.
                    cursor = cursor.saturating_add(lit.len()).saturating_add(1);
                    i += 1;
                    out.insert(spec.name.clone(), Value::Bool(true));
                    continue;
                }
                // Split form: consume the next argv token as the value.
                let next_i = i + 1;
                let next_cursor = cursor.saturating_add(lit.len()).saturating_add(1);
                let next = flag_argv
                    .get(next_i)
                    .ok_or_else(|| FlagParseError::MissingValue {
                        name: spec.name.clone(),
                        span: lit_span,
                    })?;
                let next_span = (next_cursor, next_cursor + next.len());
                let v = parse_scalar(&spec.type_name, next, next_span).map_err(
                    |(expected, got, sp)| FlagParseError::TypeMismatch {
                        name: spec.name.clone(),
                        expected,
                        got,
                        span: sp,
                    },
                )?;
                // Advance past both flag and value.
                cursor = next_cursor + next.len() + 1;
                i = next_i + 1;
                out.insert(spec.name.clone(), v);
                continue;
            }
        };
        out.insert(spec.name.clone(), value);
        cursor = cursor.saturating_add(lit.len()).saturating_add(1);
        i += 1;
    }
    Ok(out)
}

// -------------------------------------------------------------------
// Internal helpers
// -------------------------------------------------------------------

/// Parse `raw` into a [`Value`] against the type resolved from
/// `type_name`. On mismatch returns `(expected_type_name, got_tag,
/// span)` so callers can wrap in either [`ArgParseError::TypeMismatch`]
/// or [`FlagParseError::TypeMismatch`] with the right name field.
fn parse_scalar(
    type_name: &str,
    raw: &str,
    span: (usize, usize),
) -> Result<Value, (String, String, (usize, usize))> {
    match resolve_type_name(type_name) {
        Type::SInt(_) => raw.parse::<i64>().map(Value::Int).map_err(|_| {
            (
                type_name.to_owned(),
                classify_raw(raw).to_owned(),
                span,
            )
        }),
        Type::UInt(_) => raw
            .parse::<u64>()
            .map(|u| Value::Int(u as i64))
            .map_err(|_| {
                (
                    type_name.to_owned(),
                    classify_raw(raw).to_owned(),
                    span,
                )
            }),
        Type::Bool => match raw {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            other => Err((
                type_name.to_owned(),
                classify_raw(other).to_owned(),
                span,
            )),
        },
        _ => Ok(Value::Str(raw.to_owned())),
    }
}

/// Best-guess tag for a raw token — used in mismatch error messages
/// so the reader sees what the parser thought the input looked like.
fn classify_raw(raw: &str) -> &'static str {
    if raw.parse::<i64>().is_ok() {
        "Int"
    } else if raw == "true" || raw == "false" {
        "Bool"
    } else {
        "String"
    }
}

/// Strip the leading `--` (long) or `-` (short) from a flag literal
/// and split off an optional `=value` tail. Returns
/// `(name, Some(value))` for joined form, `(name, None)` for bare form.
///
/// Returns [`FlagParseError::Unknown`] with the raw literal if the
/// input has no leading dash — a positional token snuck into the
/// flag-argv, which is a driver-level invariant break.
fn split_flag_lit(
    lit: &str,
    span: (usize, usize),
) -> Result<(String, Option<String>), FlagParseError> {
    let stripped = if let Some(s) = lit.strip_prefix("--") {
        s
    } else if let Some(s) = lit.strip_prefix('-') {
        s
    } else {
        return Err(FlagParseError::Unknown {
            name: lit.to_owned(),
            span,
        });
    };
    if let Some((n, v)) = stripped.split_once('=') {
        Ok((n.to_owned(), Some(v.to_owned())))
    } else {
        Ok((stripped.to_owned(), None))
    }
}
