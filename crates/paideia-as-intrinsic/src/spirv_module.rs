//! `@spirv_module(path)` — compile-time SPIR-V blob embed intrinsic.
//!
//! **Wave 0, Batch 4, row v0.30-M1-001 (paideia-as#1379).**
//!
//! First-class embed directive for SPIR-V shader / compute-kernel
//! modules that must be baked into the compiled artifact's `.rodata`
//! at build time. The intrinsic performs a strictly bounded
//! validation — file existence, regular-file shape, and the four-byte
//! SPIR-V magic word `0x07230203` — then hands the encoder a typed
//! [`SpirvModuleSpec`] plus a [`KindMemorySymbol`] descriptor.
//!
//! # Layering
//!
//! Two-phase pipeline mirroring the sibling
//! [`crate::include_signed`] intrinsic (v0.27-M1-004):
//!
//! 1. **Parse-time (this module).** Argument-shape validation, file
//!    probe, whole-file read, magic-word check. Emits the descriptor.
//! 2. **Encode-time (Wave-1 encoder work).** Consumes the descriptor,
//!    calls `intern_bytes(&spec.bytes)` to intern the payload into
//!    the `.rodata.spirv` output section at the descriptor's
//!    alignment, and stitches the resulting symbol into the AST as
//!    an addressable constant. Not implemented here — the descriptor
//!    is the sole handoff surface between the two phases.
//!
//! # Deliberate non-goals
//!
//! - **No structural parsing of the SPIR-V binary.** Only the
//!   4-byte magic word is inspected. Full validation (header word
//!   count, capability declarations, entry-point discovery, opcode
//!   linting) belongs downstream in the Vulkan / GPU driver layer,
//!   not in the assembler intrinsic. Layering it here would force
//!   this crate to grow a full SPIR-V decoder for something that
//!   the loader is going to re-validate anyway.
//! - **No `.rodata` emission.** The descriptor is a pure data record;
//!   the encoder consumes it verbatim in Wave-1.
//! - **No endianness normalisation of the payload.** SPIR-V modules
//!   are little-endian on every target paideia-as compiles for
//!   (x86_64, aarch64); the magic word is validated in the on-disk
//!   byte order and the payload is passed through unchanged.
//!
//! # Diagnostics (`I0200`–`I0206`)
//!
//! Codes live in a self-contained namespace on [`SpirvModuleError`]
//! (via [`SpirvModuleError::code`]) for the same reason as
//! [`crate::include_signed`]: `paideia-as-diagnostics::Category` does
//! not yet reserve an `I` (intrinsic) letter. When the category is
//! added upstream, the string codes here promote to a typed
//! `DiagnosticCode` without changing any call site.
//!
//! Current assignments — leave gaps closed and additive:
//!
//! | Code    | Condition                                                     |
//! |---------|---------------------------------------------------------------|
//! | I0200   | SPIR-V magic word mismatch (or file < 4 bytes)                |
//! | I0201   | Blob file not found on disk                                   |
//! | I0202   | Wrong arity (needs exactly 1 argument)                        |
//! | I0203   | Argument is not a string literal                              |
//! | I0204   | Blob path is empty                                            |
//! | I0205   | Blob path unreadable (permission or I/O error)                |
//! | I0206   | Blob path exists but is not a regular file                    |
//!
//! I0200 and I0201 are the two codes the task specification pins by
//! name; the surrounding four are the natural fail-fast siblings
//! (empty path, wrong arity, non-file, unreadable) that would
//! otherwise fall through to a confusing lower-level `io::Error`
//! render. Reserving them in one contiguous block keeps rustdoc's
//! intra-doc cross-links resolvable when a future encoder row lands.

use std::path::{Path, PathBuf};

// -----------------------------------------------------------------------------
// Wire constants
// -----------------------------------------------------------------------------

/// SPIR-V magic word, little-endian at bytes 0-3 of every valid module.
///
/// Defined by the SPIR-V specification (Khronos, revision 1.6, §2.3).
/// The endianness of the magic word within the file defines the
/// endianness of the rest of the module; paideia-as targets are all
/// little-endian, so anything but the LE spelling is a rejection
/// (I0200), not a byte-swap opportunity.
pub const SPIRV_MAGIC: u32 = 0x0723_0203;

/// Output section for interned SPIR-V blobs.
///
/// Named `.rodata.spirv` (not plain `.rodata`) so the linker script
/// can place all shader payloads contiguously and the loader can
/// discover them by section name at runtime without a bespoke
/// symbol-table walk. Consumed by the Wave-1 encoder.
pub const SPIRV_SECTION: &str = ".rodata.spirv";

/// Alignment (bytes) for a SPIR-V payload in the output section.
///
/// SPIR-V is a stream of 32-bit words (spec §2.2.1); a 4-byte
/// alignment is the natural — and minimum-correct — placement. Any
/// higher alignment would waste `.rodata` space without a driver-side
/// benefit; any lower would let a bit-cast to `*const u32` produce an
/// unaligned load on architectures paideia-as may retarget to later.
pub const SPIRV_ALIGN: u64 = 4;

// -----------------------------------------------------------------------------
// Public spec
// -----------------------------------------------------------------------------

/// Descriptor for a memory-resident symbol the encoder will emit into
/// an output section.
///
/// Intentionally minimal and section-agnostic so the Wave-1 encoder
/// can reuse the same shape for the sibling `@wgsl_module` intrinsic
/// (v0.30-M1-002) and future `KIND_*` embed directives. When the
/// encoder crate lands the corresponding `Symbol` type this record
/// migrates upstream unchanged; keeping it local for now avoids
/// forcing a new cross-crate coupling before the consumer exists.
///
/// # Invariants
///
/// - `alignment` is always a power of two.
/// - `section` is a `&'static str` (linker-section name), never
///   allocated per-call; the encoder can equality-compare it against
///   its own table of known sections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KindMemorySymbol {
    /// Payload size in bytes. For SPIR-V this is the exact on-disk
    /// blob length — no padding, no length-prefix injection.
    pub size_bytes: u64,
    /// Placement alignment. Always [`SPIRV_ALIGN`] for this intrinsic.
    pub alignment: u64,
    /// Output linker section. Always [`SPIRV_SECTION`] for this
    /// intrinsic; carried as a field (rather than assumed by the
    /// consumer) so a hypothetical `@spirv_module_in_section(...)`
    /// variant can reuse the same descriptor type.
    pub section: &'static str,
}

/// Validated `@spirv_module(path)` invocation.
///
/// # Field discipline
///
/// - `source_path` is retained (post-normalization) so incremental
///   builds can re-stat the file and detect out-of-date payloads
///   without re-parsing the whole call site.
/// - `bytes` holds the entire blob. SPIR-V modules are typically
///   kilobytes to low megabytes; buffering the whole payload lets
///   the encoder call `intern_bytes(&spec.bytes)` in one shot and
///   keeps the descriptor self-contained (no file handle escapes the
///   parser). If future targets ship blobs large enough for the
///   pass-through cost to matter, this field promotes to an
///   `Arc<[u8]>` without changing the public shape of
///   `parse_spirv_module`.
/// - `symbol` is the encoder's handoff — see [`KindMemorySymbol`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpirvModuleSpec {
    /// Path the parser probed and read (as-supplied by the caller,
    /// wrapped in `PathBuf` without canonicalisation — sandboxing is
    /// the encoder's job, not the parser's).
    pub source_path: PathBuf,
    /// Whole-file payload, verbatim. First four bytes are guaranteed
    /// to be the little-endian encoding of [`SPIRV_MAGIC`] by
    /// construction.
    pub bytes: Vec<u8>,
    /// Encoder handoff — placement metadata for the `.rodata.spirv`
    /// symbol the encoder will emit from `bytes`.
    pub symbol: KindMemorySymbol,
}

// -----------------------------------------------------------------------------
// Argument surface
// -----------------------------------------------------------------------------

/// Input token for the single `@spirv_module` argument.
///
/// Modeled as an enum (rather than a bare `&str`) so a non-string
/// argument at the surface parser can be routed through this module's
/// diagnostic surface — the surface parser attaches the offending
/// token's span to the resulting [`SpirvModuleError::NonStringArg`].
/// Matches the shape used by [`crate::include_signed::IncludeSignedArg`]
/// so future intrinsic-call adapters can treat both surfaces
/// identically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpirvModuleArg<'a> {
    /// A parsed string literal — `s` is the already-unescaped
    /// content.
    Str(&'a str),
    /// Anything else the surface parser saw (integer literal,
    /// identifier, nested directive, …). Carries no payload; the
    /// surface parser owns the span it will attach to the resulting
    /// diagnostic.
    NonString,
}

// -----------------------------------------------------------------------------
// Error surface
// -----------------------------------------------------------------------------

/// Parse-time failure modes for `@spirv_module`.
///
/// Every variant carries the data needed to render a human message
/// with no reach-back into the surface parser. See the module-level
/// table for the code assignments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpirvModuleError {
    /// I0200 — SPIR-V magic word mismatch.
    ///
    /// `observed` is `Some(word)` when the file was at least 4 bytes
    /// long (little-endian read of bytes 0-3), or `None` when the
    /// file is shorter than the magic word — collapsing the two
    /// cases into one code keeps the operator's mental model as
    /// "this is not a SPIR-V module", which is the accurate
    /// interpretation of either failure. The distinction survives in
    /// the rendered message.
    BadMagic {
        /// The exact path the parser probed.
        path: PathBuf,
        /// LE-decoded first four bytes, or `None` for a truncated
        /// file.
        observed: Option<u32>,
    },
    /// I0201 — blob file not found on disk. Fail-fast: a mistyped
    /// path is a source-code error the operator can fix at their
    /// terminal, so we surface it at parse time rather than
    /// deferring to the encoder.
    FileMissing {
        /// The exact path the parser probed.
        path: PathBuf,
    },
    /// I0202 — arity mismatch. Includes the observed count so the
    /// surface parser can render `expected 1, got N`.
    Arity {
        /// Number of arguments actually seen (may be 0).
        got: usize,
    },
    /// I0203 — non-string-literal argument at the given zero-based
    /// position (always 0 for this single-argument intrinsic, but
    /// modeled uniformly with the sibling intrinsics for
    /// future-proofing).
    NonStringArg {
        /// 0 for this single-arg intrinsic.
        position: usize,
    },
    /// I0204 — blob path is the empty string.
    EmptyPath,
    /// I0205 — blob path could not be probed or read (permission
    /// denied or transient I/O error). Distinct from I0201 because
    /// retry semantics differ: a NotFound is a source-code fix, an
    /// I/O error is an environment fix.
    FileUnreadable {
        /// The exact path the parser probed.
        path: PathBuf,
        /// Human-readable OS error, captured so tests can assert
        /// against a stable prefix without depending on
        /// `io::Error`'s non-`PartialEq` shape.
        reason: String,
    },
    /// I0206 — blob path exists but is not a regular file
    /// (directory, device node, symlink loop). Distinct from I0201
    /// so the operator's mental model — "file present vs. entry
    /// present" — matches the diagnostic they see.
    NotFile {
        /// The exact path the parser probed.
        path: PathBuf,
    },
}

impl SpirvModuleError {
    /// Stable string diagnostic code (`"I0200"`–`"I0206"`).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            SpirvModuleError::BadMagic { .. } => "I0200",
            SpirvModuleError::FileMissing { .. } => "I0201",
            SpirvModuleError::Arity { .. } => "I0202",
            SpirvModuleError::NonStringArg { .. } => "I0203",
            SpirvModuleError::EmptyPath => "I0204",
            SpirvModuleError::FileUnreadable { .. } => "I0205",
            SpirvModuleError::NotFile { .. } => "I0206",
        }
    }

    /// One-line human-readable message, safe to render directly in
    /// terminal diagnostics.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            SpirvModuleError::BadMagic { path, observed } => match observed {
                Some(word) => format!(
                    "@spirv_module: bad SPIR-V magic word 0x{word:08X} \
                     (expected 0x{SPIRV_MAGIC:08X}) in {}",
                    path.display(),
                ),
                None => format!(
                    "@spirv_module: file is shorter than the 4-byte SPIR-V \
                     magic word: {}",
                    path.display(),
                ),
            },
            SpirvModuleError::FileMissing { path } => format!(
                "@spirv_module: file not found on disk: {}",
                path.display()
            ),
            SpirvModuleError::Arity { got } => format!(
                "@spirv_module expects exactly 1 argument (path), got {got}"
            ),
            SpirvModuleError::NonStringArg { position } => format!(
                "@spirv_module argument #{n} must be a string literal",
                n = position + 1,
            ),
            SpirvModuleError::EmptyPath => {
                "@spirv_module: path is empty".to_string()
            }
            SpirvModuleError::FileUnreadable { path, reason } => format!(
                "@spirv_module: file unreadable: {} ({reason})",
                path.display(),
            ),
            SpirvModuleError::NotFile { path } => format!(
                "@spirv_module: path is not a regular file: {}",
                path.display()
            ),
        }
    }
}

impl std::fmt::Display for SpirvModuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code(), self.message())
    }
}

impl std::error::Error for SpirvModuleError {}

// -----------------------------------------------------------------------------
// Parser
// -----------------------------------------------------------------------------

/// Validate a `@spirv_module(path)` invocation and load the blob.
///
/// Returns a [`SpirvModuleSpec`] the encoder can consume, or a
/// [`SpirvModuleError`] describing the first violation seen.
///
/// # Validation order
///
/// Deterministic so downstream tests and rustdoc snapshots stay stable
/// across refactors:
///
/// 1. arity (I0202)
/// 2. argument shape (I0203)
/// 3. non-empty path (I0204)
/// 4. on-disk probe: I0201 (missing) → I0206 (not-a-file) → I0205
///    (unreadable)
/// 5. whole-file read (I0205)
/// 6. magic-word check (I0200)
///
/// # Errors
///
/// See [`SpirvModuleError`] for the full failure surface. Only one
/// error is returned per call — the caller (v0.30-M2 elaborator
/// wiring) forwards it to the diagnostic sink.
pub fn parse_spirv_module(
    args: &[SpirvModuleArg<'_>],
) -> Result<SpirvModuleSpec, SpirvModuleError> {
    // (1) arity.
    if args.len() != 1 {
        return Err(SpirvModuleError::Arity { got: args.len() });
    }

    // (2) argument shape.
    let path_str = match args[0] {
        SpirvModuleArg::Str(s) => s,
        SpirvModuleArg::NonString => {
            return Err(SpirvModuleError::NonStringArg { position: 0 });
        }
    };

    // (3) non-empty.
    if path_str.is_empty() {
        return Err(SpirvModuleError::EmptyPath);
    }

    let path = PathBuf::from(path_str);

    // (4) + (5) + (6) — probe, read, validate.
    load_and_validate(&path)
}

/// Convenience adapter for callers that already hold a `&str`
/// (encoder tests, internal build tooling). Forwards to
/// [`parse_spirv_module`] with the same argument-shape and
/// path-validation guarantees.
///
/// Not a shortcut past the diagnostics — the surface parser must
/// still route through [`parse_spirv_module`] so `NonString` /
/// arity failures render with the correct spans.
pub fn parse_spirv_module_path(path: &str) -> Result<SpirvModuleSpec, SpirvModuleError> {
    parse_spirv_module(&[SpirvModuleArg::Str(path)])
}

/// Do the filesystem-touching half of the pipeline in isolation.
///
/// Kept private so the three probe-classification paths (I0201,
/// I0205, I0206) and the magic-word branch (I0200) can be exercised
/// through the public [`parse_spirv_module`] surface without a second
/// entry point drifting.
fn load_and_validate(path: &Path) -> Result<SpirvModuleSpec, SpirvModuleError> {
    // (4) — probe classification. Distinguish NotFound → I0201,
    // non-file → I0206, other I/O → I0205. Doing this *before* the
    // whole-file read keeps the diagnostic pinned to the correct
    // category even on platforms where `std::fs::read` maps a
    // directory-open to a generic InvalidInput rather than IsADirectory.
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() => {}
        Ok(_) => {
            return Err(SpirvModuleError::NotFile {
                path: path.to_path_buf(),
            });
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(SpirvModuleError::FileMissing {
                path: path.to_path_buf(),
            });
        }
        Err(e) => {
            return Err(SpirvModuleError::FileUnreadable {
                path: path.to_path_buf(),
                reason: e.to_string(),
            });
        }
    }

    // (5) — whole-file read. Any I/O error here falls into I0205
    // (a race between the metadata probe and the read is rare but
    // possible on shared filesystems; classifying it under the
    // environment-fix bucket is the safer default).
    let bytes = std::fs::read(path).map_err(|e| SpirvModuleError::FileUnreadable {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;

    // (6) — magic word. Read four bytes explicitly little-endian
    // (SPIR-V spec §2.3). A file under 4 bytes cannot carry the
    // magic word at all; collapse that case into the same I0200
    // bucket with observed=None (the message distinguishes them).
    if bytes.len() < 4 {
        return Err(SpirvModuleError::BadMagic {
            path: path.to_path_buf(),
            observed: None,
        });
    }
    let observed =
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if observed != SPIRV_MAGIC {
        return Err(SpirvModuleError::BadMagic {
            path: path.to_path_buf(),
            observed: Some(observed),
        });
    }

    // `bytes.len()` is at most `usize::MAX` which on every paideia-as
    // target is <= `u64::MAX`; the `as` cast is total on 16/32/64-bit
    // targets and would need explicit truncation only on hypothetical
    // >64-bit platforms that this crate does not compile for.
    let size_bytes = bytes.len() as u64;

    Ok(SpirvModuleSpec {
        source_path: path.to_path_buf(),
        bytes,
        symbol: KindMemorySymbol {
            size_bytes,
            alignment: SPIRV_ALIGN,
            section: SPIRV_SECTION,
        },
    })
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Little-endian byte spelling of [`SPIRV_MAGIC`] — placed at the
    /// head of any fixture that should be accepted as a SPIR-V blob.
    const MAGIC_LE: [u8; 4] = [0x03, 0x02, 0x23, 0x07];

    /// Materialize a real regular file inside a `tempfile::TempDir`
    /// with the given prefix + suffix bytes. Returns the path. The
    /// caller must keep the TempDir alive across the parse call —
    /// dropping it before the parser stats the path yields I0201
    /// instead of the intended outcome.
    fn touch(dir: &tempfile::TempDir, name: &str, body: &[u8]) -> PathBuf {
        let p = dir.path().join(name);
        let mut f = std::fs::File::create(&p).expect("create fixture");
        f.write_all(body).expect("write fixture");
        p
    }

    // --- Happy path -----------------------------------------------------------

    #[test]
    fn valid_magic_is_accepted() {
        // Fixture: 4-byte magic word only. The magic word alone is a
        // legal (if degenerate) SPIR-V "module" — the parser
        // deliberately does not validate the SPIR-V header beyond
        // the magic word (see module docs).
        let dir = tempfile::tempdir().expect("tempdir");
        let path = touch(&dir, "hello.spv", &MAGIC_LE);
        let path_s = path.to_str().expect("utf-8 fixture path");

        let spec = parse_spirv_module(&[SpirvModuleArg::Str(path_s)])
            .expect("valid magic must parse");

        assert_eq!(spec.source_path, path);
        assert_eq!(spec.bytes, MAGIC_LE);
        assert_eq!(spec.symbol.size_bytes, 4);
        assert_eq!(spec.symbol.alignment, SPIRV_ALIGN);
        assert_eq!(spec.symbol.section, SPIRV_SECTION);
    }

    #[test]
    fn valid_magic_with_trailing_payload_is_accepted() {
        // A more realistic fixture: magic word + a few plausible
        // header words. Payload is passed through unchanged.
        let mut body = Vec::from(MAGIC_LE);
        body.extend_from_slice(&0x0001_0000_u32.to_le_bytes()); // version 1.0
        body.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]); // arbitrary
        let dir = tempfile::tempdir().expect("tempdir");
        let path = touch(&dir, "shader.spv", &body);
        let path_s = path.to_str().unwrap();

        let spec =
            parse_spirv_module_path(path_s).expect("valid magic must parse via convenience adapter");

        assert_eq!(spec.symbol.size_bytes, body.len() as u64);
        assert_eq!(spec.bytes, body, "payload must be passed through byte-for-byte");
    }

    #[test]
    fn parse_spirv_module_path_matches_arg_surface() {
        // The convenience adapter must produce byte-identical output
        // to the arg-surface entry point so encoder tests can pick
        // whichever is more ergonomic without behavioural drift.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = touch(&dir, "same.spv", &MAGIC_LE);
        let path_s = path.to_str().unwrap();

        let via_args = parse_spirv_module(&[SpirvModuleArg::Str(path_s)])
            .expect("arg surface must parse");
        let via_path = parse_spirv_module_path(path_s).expect("path adapter must parse");
        assert_eq!(via_args, via_path);
    }

    // --- I0200 bad magic ------------------------------------------------------

    #[test]
    fn invalid_magic_is_rejected_with_i0200() {
        let bogus = [0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x00, 0x00, 0x00];
        let dir = tempfile::tempdir().expect("tempdir");
        let path = touch(&dir, "not-spirv.bin", &bogus);
        let path_s = path.to_str().unwrap();

        let err = parse_spirv_module(&[SpirvModuleArg::Str(path_s)])
            .expect_err("bogus magic must be rejected");
        assert_eq!(err.code(), "I0200");

        match &err {
            SpirvModuleError::BadMagic { path: p, observed } => {
                assert_eq!(p, &path);
                assert_eq!(
                    *observed,
                    Some(u32::from_le_bytes([0xDE, 0xAD, 0xBE, 0xEF])),
                    "observed magic must be LE decode of bytes 0-3",
                );
            }
            other => panic!("expected BadMagic, got {other:?}"),
        }
        assert!(err.message().contains("bad SPIR-V magic"));
    }

    #[test]
    fn big_endian_spelling_of_magic_is_still_i0200() {
        // The magic word is defined little-endian; a file that spells
        // it big-endian (0x07, 0x23, 0x02, 0x03) is a different
        // 32-bit word and must fail. Guards against a well-meaning
        // future maintainer adding endianness auto-detection.
        let be = [0x07, 0x23, 0x02, 0x03];
        let dir = tempfile::tempdir().expect("tempdir");
        let path = touch(&dir, "be.spv", &be);
        let err = parse_spirv_module_path(path.to_str().unwrap()).unwrap_err();
        assert_eq!(err.code(), "I0200");
        assert!(matches!(
            err,
            SpirvModuleError::BadMagic { observed: Some(w), .. } if w == 0x0302_2307
        ));
    }

    #[test]
    fn file_shorter_than_magic_word_is_i0200_with_none() {
        // A 3-byte file cannot carry a magic word at all. Collapse
        // into I0200 with observed=None; the message distinguishes
        // the two cases for the operator.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = touch(&dir, "truncated.spv", &[0x03, 0x02, 0x23]);
        let err = parse_spirv_module_path(path.to_str().unwrap()).unwrap_err();
        assert_eq!(err.code(), "I0200");
        assert!(matches!(
            err,
            SpirvModuleError::BadMagic { observed: None, .. }
        ));
        assert!(err.message().contains("shorter than"));
    }

    #[test]
    fn empty_file_is_i0200_with_none() {
        // Boundary case: a 0-byte file. Same classification as the
        // 3-byte case above.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = touch(&dir, "zero.spv", &[]);
        let err = parse_spirv_module_path(path.to_str().unwrap()).unwrap_err();
        assert_eq!(err.code(), "I0200");
        assert!(matches!(
            err,
            SpirvModuleError::BadMagic { observed: None, .. }
        ));
    }

    // --- I0201 missing file ---------------------------------------------------

    #[test]
    fn missing_file_is_rejected_with_i0201() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ghost = dir.path().join("never-created.spv");
        assert!(!ghost.exists(), "precondition: fixture path must not exist");

        let err = parse_spirv_module_path(ghost.to_str().unwrap())
            .expect_err("missing file must be rejected");
        assert_eq!(err.code(), "I0201");
        match &err {
            SpirvModuleError::FileMissing { path } => assert_eq!(path, &ghost),
            other => panic!("expected FileMissing, got {other:?}"),
        }
        assert!(err.message().contains("never-created.spv"));
    }

    // --- I0202 arity ----------------------------------------------------------

    #[test]
    fn wrong_arity_is_rejected_with_i0202() {
        // Zero args.
        let err = parse_spirv_module(&[]).expect_err("zero-arg must be rejected");
        assert_eq!(err.code(), "I0202");
        assert!(matches!(err, SpirvModuleError::Arity { got: 0 }));

        // Two args.
        let err = parse_spirv_module(&[
            SpirvModuleArg::Str("a.spv"),
            SpirvModuleArg::Str("b.spv"),
        ])
        .expect_err("two-arg must be rejected");
        assert!(matches!(err, SpirvModuleError::Arity { got: 2 }));
    }

    // --- I0203 non-string arg -------------------------------------------------

    #[test]
    fn non_string_arg_is_rejected_with_i0203() {
        let err = parse_spirv_module(&[SpirvModuleArg::NonString])
            .expect_err("non-string arg must be rejected");
        assert_eq!(err.code(), "I0203");
        assert!(matches!(
            err,
            SpirvModuleError::NonStringArg { position: 0 }
        ));
    }

    // --- I0204 empty path -----------------------------------------------------

    #[test]
    fn empty_path_is_rejected_with_i0204() {
        let err = parse_spirv_module(&[SpirvModuleArg::Str("")])
            .expect_err("empty path must be rejected");
        assert_eq!(err.code(), "I0204");
        assert!(matches!(err, SpirvModuleError::EmptyPath));
    }

    // --- I0206 not a file -----------------------------------------------------

    #[test]
    fn directory_path_is_rejected_with_i0206() {
        // Distinct from I0201: the path exists but is a directory,
        // not a regular file. Diagnostics should tell the operator
        // "you pointed at a directory, did you mean a file inside?"
        let dir = tempfile::tempdir().expect("tempdir");
        let subdir = dir.path().join("subdir");
        std::fs::create_dir(&subdir).expect("mk fixture dir");

        let err = parse_spirv_module_path(subdir.to_str().unwrap())
            .expect_err("directory path must be rejected");
        assert_eq!(err.code(), "I0206");
        assert!(matches!(err, SpirvModuleError::NotFile { .. }));
    }

    // --- Metadata and static shape --------------------------------------------

    #[test]
    fn constants_have_stable_values() {
        // Pinned so a well-meaning refactor cannot silently change
        // the wire contract these values represent.
        assert_eq!(SPIRV_MAGIC, 0x0723_0203);
        assert_eq!(SPIRV_ALIGN, 4);
        assert_eq!(SPIRV_SECTION, ".rodata.spirv");
        assert!(SPIRV_ALIGN.is_power_of_two());
    }

    #[test]
    fn every_variant_has_distinct_code_in_reserved_range() {
        // Guards against two failure paths accidentally collapsing
        // onto the same wire code, and against a code drifting
        // outside the reserved I0200..=I0206 block.
        let codes = [
            SpirvModuleError::BadMagic {
                path: PathBuf::from("x"),
                observed: None,
            }
            .code(),
            SpirvModuleError::FileMissing {
                path: PathBuf::from("x"),
            }
            .code(),
            SpirvModuleError::Arity { got: 0 }.code(),
            SpirvModuleError::NonStringArg { position: 0 }.code(),
            SpirvModuleError::EmptyPath.code(),
            SpirvModuleError::FileUnreadable {
                path: PathBuf::from("x"),
                reason: "y".into(),
            }
            .code(),
            SpirvModuleError::NotFile {
                path: PathBuf::from("x"),
            }
            .code(),
        ];
        let mut sorted: Vec<&&str> = codes.iter().collect();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), codes.len(), "codes must be distinct");
        for c in &codes {
            assert!(c.starts_with('I'), "code {c} must be in I-band");
            let n: u16 = c[1..].parse().unwrap();
            assert!(
                (200..=206).contains(&n),
                "code {c} out of I0200..=I0206 range"
            );
        }
    }

    #[test]
    fn display_impl_prepends_code() {
        // The Display impl is what non-diagnostic-aware callers
        // (bin drivers, `dbg!`) will surface; pin the shape.
        let err = SpirvModuleError::EmptyPath;
        let rendered = format!("{err}");
        assert!(rendered.starts_with("[I0204]"), "got: {rendered}");
    }
}
