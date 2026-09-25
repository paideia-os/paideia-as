//! R222.M5 — on-disk `commands.toml` loader.
//!
//! # What this module does
//!
//! The R222.M3 registry ([`crate::registry::CommandRegistry`]) is
//! seeded in-code by [`crate::registry::CommandRegistry::with_light_commands`].
//! R222.M5 replaces that pre-manifest fallback with a real load off
//! two well-known paths:
//!
//! * `/system/shell/commands.toml`   — the system-wide command manifest
//!   (mandatory; missing file is an error).
//! * `/users/<u>/shell/commands.toml` — the per-user override
//!   (optional; missing file is fine).
//!
//! Per `design/terminal/command-registry.md`, the user manifest is
//! loaded AFTER the system manifest, and a duplicate name in the user
//! manifest **shadows** the system entry silently. A duplicate name
//! WITHIN a single manifest is a load-time error — the on-disk source
//! itself is malformed, and letting the second entry win would smuggle
//! non-determinism into which body a shell name resolves to.
//!
//! # Wire form
//!
//! The TOML wire form ([`CommandsToml`] / [`CommandTomlEntry`]) carries
//! only the fields the on-disk manifest can meaningfully describe: the
//! command name, the input/output schema references (by canonical name;
//! the R220.M4 FNV fingerprint is computed at load time via
//! [`crate::SchemaRef::of_name`]), and the R222.M6 dispatch weight
//! (`"Light"` | `"Heavy"`). Everything else on the [`crate::CommandSig`]
//! — arguments, flags, effects, capabilities, `execute` — is intentionally
//! NOT on this v1 shape:
//!
//! * `arguments` / `flags` / `effects` / `required_capabilities` come
//!   from the command's paideia-as source (the elaborator emits them on
//!   the functor's return signature); the on-disk manifest is a shell-
//!   name → sig-shell mapping, not a duplicate of the functor's own
//!   declaration.
//! * `execute` is a process-local address; a TOML file cannot carry
//!   a function pointer meaningfully. Loaded sigs plant
//!   [`crate::wire::placeholder_execute`] on the field and rely on the
//!   R222.M6 heavy-dispatch path to spawn the real body.
//!
//! When the loader grows to consume the full spec surface (arguments +
//! flags + effects + caps) — a follow-up milestone once the R220 client
//! library can serialise those out of paideia-as source — [`CommandTomlEntry`]
//! grows fields without a shape change, because every field on it is
//! `Option`al today.
//!
//! # Error shape
//!
//! [`RegistryLoadError`] is a plain enum with `Display + Error`; each
//! variant carries just enough context to point a shell user at the
//! file and the row. The miette-diagnostic upgrade is deferred to the
//! landing that gives the shell a `SourceCache` for loaded manifests to
//! point into — R222.M5 does not need a source-span story to accept
//! its 10-test corpus.

use std::io;
use std::path::{Path, PathBuf};

use crate::registry::CommandRegistry;
use crate::schema::{SchemaFingerprint, SchemaRef};
use crate::sig::{CapSpec, CommandSig, CommandWeight, EffectRow};
use crate::wire::placeholder_execute;

/// The top-level TOML shape of a `commands.toml` manifest.
///
/// ```toml
/// [[commands]]
/// name = "find"
/// input_schema  = "FileSchema@0.1"     # optional
/// output_schema = "FileSchema@0.1"     # optional
/// weight        = "Light"              # optional, default = Light
/// ```
///
/// The `[[commands]]` array-of-tables shape lets the TOML author list
/// as many entries as they like without splitting the file across
/// section names. Serde parses duplicates into adjacent `Vec` entries,
/// which [`load_from`] then rejects at post-parse validation time.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CommandsToml {
    /// One entry per shell command the manifest declares.
    #[serde(default)]
    pub commands: Vec<CommandTomlEntry>,
}

/// A single `[[commands]]` entry.
///
/// Every field but `name` is `Option`al: an unset `input_schema` /
/// `output_schema` maps to a placeholder [`SchemaRef`] (see
/// [`fallback_schema`]) rather than a load error, matching the
/// module-level "grow without a shape change" invariant. An unset or
/// unrecognised `weight` defaults to [`CommandWeight::Light`] via
/// [`parse_weight`] — the safe fallback (see [`CommandWeight::default`]
/// for why the wrong direction to guess is `Heavy`).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CommandTomlEntry {
    /// Shell name the command is registered under.
    pub name: String,
    /// Canonical schema name the command reads on its input stream.
    pub input_schema: Option<String>,
    /// Canonical schema name the command writes to its output stream.
    pub output_schema: Option<String>,
    /// Dispatch weight — `"Light"` (in-process) or `"Heavy"` (spawn).
    #[serde(default)]
    pub weight: Option<String>,
}

/// Handle carrying the two on-disk paths the manifest loader consults.
///
/// The two `Option<PathBuf>` fields let a caller construct the client
/// without materialising a path they don't need (a bring-up harness
/// that only wants the system manifest passes `user_toml = None`; a
/// per-user shell process before `/system` is mounted may pass
/// `system_toml = None`, though that is not exercised at R222.M5). The
/// client is data-only; [`load_from`] is the free function that reads
/// the paths — the split keeps the loader testable without needing to
/// construct a `RegistryClient` for every fixture.
pub struct RegistryClient {
    /// Absolute path of the system-wide manifest.
    pub system_toml: Option<PathBuf>,
    /// Absolute path of the per-user override manifest, if any.
    pub user_toml: Option<PathBuf>,
}

impl RegistryClient {
    /// Construct with the two manifest paths (either may be `None`).
    pub fn new(system: Option<PathBuf>, user: Option<PathBuf>) -> Self {
        Self {
            system_toml: system,
            user_toml: user,
        }
    }
}

/// What can go wrong loading a `commands.toml`.
///
/// Kept as a plain enum rather than a `miette::Diagnostic` for the
/// reasons documented at module-level.
#[derive(Debug)]
pub enum RegistryLoadError {
    /// A manifest file was expected but could not be read (missing,
    /// permission denied, etc.). Carries the raw `io::Error` from the
    /// read.
    IoError(io::Error),
    /// A manifest file was read but the TOML parser rejected it. The
    /// `reason` is the parser's own message; the `path` is the file
    /// the parser was reading.
    TomlParse {
        /// Path of the manifest that failed to parse.
        path: PathBuf,
        /// Human-readable parser message.
        reason: String,
    },
    /// A single manifest file declared two `[[commands]]` entries with
    /// the same `name`. Shadowing across files (system → user) is
    /// intentional; within a single file it is a load-time error.
    DuplicateInSameFile {
        /// The name that appeared twice.
        name: String,
        /// The manifest that declared the duplicates.
        path: PathBuf,
    },
}

impl std::fmt::Display for RegistryLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "registry-client: I/O error: {e}"),
            Self::TomlParse { path, reason } => {
                write!(
                    f,
                    "registry-client: TOML parse error in {}: {reason}",
                    path.display()
                )
            }
            Self::DuplicateInSameFile { name, path } => {
                write!(
                    f,
                    "registry-client: duplicate command name `{name}` in single manifest {}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for RegistryLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::IoError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for RegistryLoadError {
    fn from(e: io::Error) -> Self {
        Self::IoError(e)
    }
}

/// Placeholder [`SchemaRef`] planted on a loaded sig whose manifest
/// entry omitted `input_schema` / `output_schema`.
///
/// Distinct fingerprint (`SchemaFingerprint(0)`) rather than an
/// FNV-of-`"unknown"` value so a downstream inspector can tell the
/// placeholder from a legitimate schema named `"unknown"`. The R220.M4
/// registry never returns fingerprint `0` for a legitimate schema
/// (FNV-1a-64 seeded at `0xcbf29ce484222325` cannot collapse a
/// non-empty byte string to 0), so the sentinel value is safe.
fn fallback_schema() -> SchemaRef {
    SchemaRef {
        name: "unknown".to_owned(),
        fingerprint: SchemaFingerprint(0),
    }
}

/// Elaborate a TOML `weight` string into a [`CommandWeight`].
///
/// Unknown or missing strings collapse to [`CommandWeight::Light`] —
/// the safe fallback (see [`CommandWeight::default`]).
fn parse_weight(s: Option<&str>) -> CommandWeight {
    match s {
        Some("Heavy") | Some("heavy") => CommandWeight::Heavy,
        Some("Light") | Some("light") => CommandWeight::Light,
        _ => CommandWeight::default(),
    }
}

/// Parse the TOML text at `path` into an ordered
/// `Vec<CommandTomlEntry>`, checking for in-file duplicates.
///
/// A duplicate name inside the SAME file surfaces as
/// [`RegistryLoadError::DuplicateInSameFile`]. Shadowing across files
/// is a merge-time concern, handled in [`load_from`].
fn parse_manifest(path: &Path) -> Result<Vec<CommandTomlEntry>, RegistryLoadError> {
    let text = std::fs::read_to_string(path)?;
    let manifest: CommandsToml = toml::from_str(&text).map_err(|e| RegistryLoadError::TomlParse {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;

    // Check for duplicates inside this single file.
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for entry in &manifest.commands {
        if !seen.insert(entry.name.as_str()) {
            return Err(RegistryLoadError::DuplicateInSameFile {
                name: entry.name.clone(),
                path: path.to_path_buf(),
            });
        }
    }

    Ok(manifest.commands)
}

/// Turn a single TOML entry into a fully-formed [`CommandSig`] with
/// the R222.M5 placeholder body.
fn entry_to_sig(entry: CommandTomlEntry) -> CommandSig {
    let input_schema = entry
        .input_schema
        .as_deref()
        .map(SchemaRef::of_name)
        .or_else(|| Some(fallback_schema()));
    let output_schema = entry
        .output_schema
        .as_deref()
        .map(SchemaRef::of_name)
        .or_else(|| Some(fallback_schema()));
    let weight = parse_weight(entry.weight.as_deref());

    CommandSig {
        name: entry.name,
        input_schema,
        output_schema,
        arguments: Vec::new(),
        flags: Vec::new(),
        effects: EffectRow::default(),
        required_capabilities: CapSpec::default(),
        execute: placeholder_execute,
        weight,
    }
}

/// Load a system manifest (mandatory) and an optional user manifest,
/// returning the merged `Vec<CommandSig>` in system-then-user order.
///
/// # Merge semantics
///
/// The returned vector preserves the system manifest's declaration
/// order. A user-manifest entry whose `name` matches a system entry
/// **replaces the system entry in place** (i.e. at the same vector
/// index) rather than appending — this keeps `describe`-style
/// enumerations stable across shadowing. A user entry whose `name` is
/// not in the system manifest appends at the end in the order the user
/// file declared it.
///
/// # Errors
///
/// * The system file cannot be read → [`RegistryLoadError::IoError`].
///   A missing system file is an error at R222.M5 (a shell without a
///   command manifest cannot dispatch); the caller may choose to fall
///   back to the in-code seed via
///   [`crate::registry::CommandRegistry::with_light_commands`].
/// * Either file fails to parse → [`RegistryLoadError::TomlParse`]
///   carrying the offending path.
/// * Either file declares the same name twice →
///   [`RegistryLoadError::DuplicateInSameFile`].
/// * A missing user file (with the system file present) is NOT an
///   error — the user manifest is optional per the module docs.
pub fn load_from(
    system_path: &Path,
    user_path: Option<&Path>,
) -> Result<Vec<CommandSig>, RegistryLoadError> {
    let system_entries = parse_manifest(system_path)?;

    // Build the base list from system, remembering each name's index so
    // a later user-side shadow can replace in place.
    let mut merged: Vec<CommandSig> = system_entries.into_iter().map(entry_to_sig).collect();
    let mut index_by_name: std::collections::HashMap<String, usize> = merged
        .iter()
        .enumerate()
        .map(|(i, sig)| (sig.name.clone(), i))
        .collect();

    // Optional user manifest. A missing file is fine; any OTHER I/O
    // error (permission denied, etc.) still surfaces as IoError.
    if let Some(user_path) = user_path {
        match std::fs::metadata(user_path) {
            Ok(_) => {
                let user_entries = parse_manifest(user_path)?;
                for entry in user_entries {
                    let sig = entry_to_sig(entry);
                    if let Some(&idx) = index_by_name.get(&sig.name) {
                        merged[idx] = sig;
                    } else {
                        index_by_name.insert(sig.name.clone(), merged.len());
                        merged.push(sig);
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // Optional file, silently absent.
            }
            Err(e) => return Err(RegistryLoadError::IoError(e)),
        }
    }

    Ok(merged)
}

/// Seed a [`CommandRegistry`] from the loaded `Vec<CommandSig>`,
/// returning the count of sigs actually inserted.
///
/// Uses [`CommandRegistry::register_sig`], so a repeat name silently
/// overwrites (shadow-in-registry semantics). The returned count is
/// simply `loaded.len()` — no de-dup happens here; the loader's own
/// merge already ensured every name is unique across the input slice.
pub fn seed_registry(loaded: &[CommandSig], reg: &mut CommandRegistry) -> usize {
    let mut n = 0;
    for sig in loaded {
        reg.register_sig(sig.clone());
        n += 1;
    }
    n
}
