//! R222.M1 — `SchemasSig`: the ML-signature body a command's functor
//! parameterises over.
//!
//! # Shape
//!
//! Per `design/terminal/semantic-shell.md` §6.1 and
//! `design/terminal/schema-registry.md` §3, a command's functor is
//! declared as:
//!
//! ```text
//! module Find : CommandSig = functor (Schemas : SchemasSig) -> struct
//!   ...
//! end
//! ```
//!
//! `SchemasSig` names the schemas the command's `execute` op may see
//! on its `input` stream and produce on its `output` stream. Each
//! schema is identified by its **fingerprint** — the R222 substrate
//! only ever compares schemas by fingerprint, never by structural
//! equality on the field-descriptor bytes (schema-registry.md §3
//! "answering that in each library independently duplicates the table
//! 10 times and leaves the tools disagreeing"). The fingerprint is
//! FNV-1a-64 today, BLAKE3 on the paideia-as intrinsic landing; see
//! [`crate::fingerprint`].
//!
//! # Why we keep the `name` alongside the `fingerprint`
//!
//! Fingerprints are compact and fast to compare, but a REPL user
//! looking at `describe find` (R222.M8) wants to read `FileSchema@0.1`
//! not `0xa03c…`. The `name` is authoritative for diagnostics; the
//! fingerprint is authoritative for wire lookups. Storing both matches
//! `SchemaHandle` on the kernel side (`owner_pid` field carries
//! authorship, `schema_id` carries the wire identity; see
//! schema-registry.md §2.3).

use crate::fingerprint::fnv1a_64;

/// 64-bit schema fingerprint (schema-registry.md §3).
///
/// Compared by value; opaque otherwise — a caller must never derive
/// structural facts from the bits. Serialised on the wire as
/// little-endian u64 to match the R220 client library.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SchemaFingerprint(pub u64);

impl SchemaFingerprint {
    /// Compute the fingerprint from a schema's canonical name.
    #[inline]
    pub fn of_name(name: &str) -> Self {
        Self(fnv1a_64(name.as_bytes()))
    }
}

/// A reference to a schema, carrying both the human-readable canonical
/// name (for diagnostics + `describe`) and the wire fingerprint (for
/// lookup + equality).
///
/// The canonical name follows the `Name@<maj>.<min>` shape the R220
/// registry documents (§5) — `FileSchema@0.1`, `RawByteChunk@0.1`. A
/// caller that hands a name without a version to [`Self::of_name`]
/// gets a fingerprint over exactly those bytes; the registry will
/// refuse to register such a name at the daemon boundary, so
/// unversioned names never end up on the wire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaRef {
    /// Canonical name (e.g. `"FileSchema@0.1"`).
    pub name: String,
    /// Fingerprint of `name`.
    pub fingerprint: SchemaFingerprint,
}

impl SchemaRef {
    /// Construct from a canonical name; computes the fingerprint.
    #[inline]
    pub fn of_name(name: impl Into<String>) -> Self {
        let name = name.into();
        let fingerprint = SchemaFingerprint::of_name(&name);
        Self { name, fingerprint }
    }
}

/// R222.M1 — `SchemasSig`: the ML signature the functor consumes.
///
/// A command's `execute` op may declare an `input` schema (what it
/// reads off its input stream — `None` for source commands like
/// `find` that generate records without consuming any), and an
/// `output` schema (what it emits — `None` for sink commands like
/// `count` that emit a single scalar). Additional schemas the command
/// consumes internally (e.g. a `Path` schema for the argument
/// elaborator) travel in [`SchemasSig::extras`] so the functor can
/// bind against them without polluting the primary two fields.
///
/// The empty `extras` vector is the common case; the shape stays open
/// so the R222.M5 registry loader (which reads schema references from
/// the on-disk `commands.toml`) can present an unbounded set to the
/// functor without a breaking change to this struct.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SchemasSig {
    /// Schema the command reads on its input stream.
    pub input: Option<SchemaRef>,
    /// Schema the command writes to its output stream.
    pub output: Option<SchemaRef>,
    /// Any additional schemas the command's functor binds against.
    pub extras: Vec<SchemaRef>,
}

impl SchemasSig {
    /// Empty signature — the default a REPL session starts with; the
    /// R229 session bootstrapping code populates the real schemas
    /// from the registry.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Look up an extra schema by its canonical name.
    ///
    /// Returns `None` if no extra with that name is registered. `input`
    /// and `output` are intentionally NOT searched here — a functor
    /// that needs the input/output schema references them by field
    /// directly (they are the primary shape); `extras` is for the
    /// long tail.
    pub fn extra_by_name(&self, name: &str) -> Option<&SchemaRef> {
        self.extras.iter().find(|s| s.name == name)
    }

    /// Convenience: construct a session-level `SchemasSig` seeded with
    /// `FileSchema@0.1` and `RawByteChunk@0.1` — the two schemas the
    /// R220 client library (`libpdx-schema-registry` M1..M4) ships
    /// pre-registered per schema-registry.md §5. R222.M3's canary
    /// (`find .`) uses this seed to prove functor instantiation
    /// round-trips schema references end-to-end.
    pub fn r220_seed() -> Self {
        Self {
            input: None,
            output: None,
            extras: vec![
                SchemaRef::of_name("FileSchema@0.1"),
                SchemaRef::of_name("RawByteChunk@0.1"),
            ],
        }
    }
}
