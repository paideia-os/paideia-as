//! R227.M8 `.pds` script-load fingerprint emitter.
//!
//! Emits a stable identifier for a `.pds` script's byte content at
//! load time, so downstream tooling — trace correlators, session
//! ledgers, cache warmers — can pin observations to the exact bytes
//! that were loaded, without having to re-hash the script itself at
//! every join point. The fingerprint is FNV-1a-64 over the raw script
//! bytes; the emitted tag is the ASCII string `pds.load.` followed by
//! the hash rendered as 16 lower-case hexadecimal digits (zero-padded).
//!
//! # Position in the pipeline
//!
//! ```text
//!   .pds source (raw bytes)
//!       │
//!       ├──▶ emit_load(sink, script_bytes)          ← this module
//!       │        │
//!       │        └── sink.emit("pds.load.{hash:016x}")
//!       │                          ↳ correlator / ledger / debug pane
//!       ▼
//!   parse_header  ── PdsHeader { … }
//!       │
//!       ▼
//!   check_version  →  check_capabilities  →  shell-lex body
//! ```
//!
//! # Choice of hash
//!
//! FNV-1a-64 is the same interim algorithm the schema registry ships
//! with (`paideia-as-shell-cmd::fingerprint`, per
//! `design/terminal/schema-registry.md` §3). Both fingerprints will
//! move together to `BLAKE3(..)[0..8]` when the paideia-as BLAKE3
//! intrinsic lands; keeping the algorithm identical here lets that
//! migration flip one helper across both call sites in a single
//! release without diverging tag formats.
//!
//! The offset basis and prime are inlined at module scope rather than
//! taken from a sibling crate so that `paideia-as-shell-pds` stays
//! dependency-free (matching the M1..M7 posture that the header path
//! avoids pulling `paideia-as-shell-*` peers into a `.pds` load).
//!
//! # Sinks
//!
//! [`LoadSink`] is a small trait so the caller picks the sink at
//! runtime — [`NullLoadSink`] for release binaries that don't want to
//! carry the observation, [`CollectingLoadSink`] for tests and
//! diagnostic UIs that want the raw tag string back. A future
//! milestone can add a `ChannelLoadSink` that forwards each tag onto
//! the session EDB (R226) without touching this module.
//!
//! # Fingerprints
//!
//! The R227.M8 test corpus tags each fixture with `r227m8-load-NN` so
//! the R220.M10 `@fingerprint` correlator can attribute pass/fail to a
//! specific fixture without re-parsing its name.

use std::sync::Mutex;

/// FNV-1a-64 offset basis — `0xCBF29CE484222325`.
///
/// Matches `paideia-as-shell-cmd::fingerprint::FNV_OFFSET_BASIS` byte
/// for byte; both call sites are expected to move to BLAKE3 in the
/// same release, at which point this constant retires.
const FNV_OFFSET_BASIS: u64 = 0xCBF2_9CE4_8422_2325;

/// FNV-1a-64 prime — `0x00000100000001B3`.
const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;

/// Compute FNV-1a-64 over `bytes`.
///
/// Standard byte-at-a-time algorithm: `hash = (hash ^ byte) * prime`.
/// The empty slice hashes to [`FNV_OFFSET_BASIS`] by definition.
///
/// Exposed at `pub(crate)` visibility so sibling modules (R227.M7
/// `script_cache`) can reuse the identical algorithm without a
/// second implementation drifting out of sync — when the crate
/// migrates to BLAKE3, both call sites move together by editing
/// this one helper.
#[inline]
pub(crate) fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut h: u64 = FNV_OFFSET_BASIS;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

/// Receiver for load-fingerprint tags emitted by [`emit_load`].
///
/// Kept intentionally minimal: a single string argument, no return
/// value, no error channel. Sinks that need to fail (a network
/// forwarder, say) swallow the failure internally — the load path
/// must not abort on observation-side trouble.
pub trait LoadSink {
    /// Record one emitted tag.
    ///
    /// The tag has the shape `pds.load.{hash:016x}` (25 ASCII bytes
    /// total: `pds.load.` prefix plus 16 lower-case hex digits).
    /// Implementations should treat it as an opaque identifier.
    fn emit(&self, tag: &str);
}

/// No-op sink: discards every tag.
///
/// The default choice for release binaries that don't want the
/// observation to leave any trace. Kept as a zero-sized type so it
/// costs nothing to instantiate at each `.pds` load.
#[derive(Clone, Copy, Debug, Default)]
pub struct NullLoadSink;

impl LoadSink for NullLoadSink {
    #[inline]
    fn emit(&self, _tag: &str) {
        // deliberate no-op
    }
}

/// In-memory sink: appends every tag onto an internal vector.
///
/// Intended for tests and diagnostic surfaces (a shell REPL's "why
/// was this .pds loaded?" pane, say). Interior mutability via
/// [`Mutex`] keeps the sink shareable across threads without forcing
/// callers to hold a `&mut` — the load path is fundamentally a `&self`
/// operation and a mutex here is the least surprising way to keep it
/// that way.
#[derive(Debug, Default)]
pub struct CollectingLoadSink {
    collected: Mutex<Vec<String>>,
}

impl CollectingLoadSink {
    /// Construct an empty collecting sink.
    #[must_use]
    pub fn new() -> Self {
        Self {
            collected: Mutex::new(Vec::new()),
        }
    }

    /// Return a snapshot of every tag emitted into this sink so far,
    /// in emission order. Subsequent [`LoadSink::emit`] calls do not
    /// mutate the returned vector — it is a copy.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex has been poisoned by a previous
    /// panic inside [`LoadSink::emit`] — which cannot happen along the
    /// current code paths (the emit body is a single `push`), but is
    /// documented here for the same reason the standard library
    /// documents mutex poisoning: a future extension that runs
    /// arbitrary code under the lock inherits this behaviour.
    #[must_use]
    pub fn snapshot(&self) -> Vec<String> {
        self.collected
            .lock()
            .expect("CollectingLoadSink mutex poisoned")
            .clone()
    }
}

impl LoadSink for CollectingLoadSink {
    fn emit(&self, tag: &str) {
        self.collected
            .lock()
            .expect("CollectingLoadSink mutex poisoned")
            .push(tag.to_owned());
    }
}

/// Compute the FNV-1a-64 fingerprint of `script_bytes` and emit the
/// `pds.load.{hash:016x}` tag into `sink`.
///
/// Deterministic: two calls with byte-equal `script_bytes` emit the
/// same tag. The function does not itself examine `script_bytes`
/// beyond the hash — a leading `#!` shebang, `\r\n` line endings, or
/// stray trailing whitespace are all part of the fingerprint, because
/// the fingerprint's job is to name *exactly the bytes that were
/// loaded*, not the normalised script.
pub fn emit_load(sink: &dyn LoadSink, script_bytes: &[u8]) {
    let hash = fnv1a_64(script_bytes);
    let tag = format!("pds.load.{hash:016x}");
    sink.emit(&tag);
}
