//! `@include_bytes_signed(path, keyring)` — signed firmware blob intrinsic.
//!
//! **Wave 0, Batch 3, row v0.27-M1-004 (paideia-as#1368).**
//! First-class embed directive for firmware payloads that must carry a
//! post-quantum signature verified against a caller-supplied keyring.
//! Enforces the D1.a dual-signature discipline (Ed25519 || ML-DSA-65
//! hybrid) at compile time — the encoder cannot forget to verify, the
//! programmer cannot ship an unsigned blob past the compiler.
//!
//! # Layering
//!
//! This module owns the **parse-time** half of the pipeline:
//!
//! 1. Argument-shape validation — exactly two string-literal paths.
//! 2. Path-syntax validation only — the blob is **not** read here.
//! 3. Fail-fast keyring probe — `keyring` must exist on disk at parse
//!    time so a mistyped `keyring = "…/dev.key"` fails during
//!    `pdxc build` instead of during a downstream signature check on
//!    a target where the operator has no ability to correct it.
//!
//! Actual blob read + signature verification against the keyring is
//! deferred to the encoder, which calls into `paideia-as-crypto::sig`
//! (v0.27-M2). The [`IncludeBytesSignedSpec`] returned from
//! [`parse_include_bytes_signed`] is the sole handoff surface between
//! the two phases; the encoder consumes the spec verbatim.
//!
//! # Diagnostics (`I0140`–`I0150`)
//!
//! Codes live in a self-contained namespace on [`IncludeSignedError`]
//! (via [`IncludeSignedError::code`]) because the shared diagnostic
//! catalog in `paideia-as-diagnostics` does not yet reserve an `I`
//! (intrinsic) category. Batch 3 populates the sibling intrinsic
//! stubs (`atomic128`, `dma_buffer`) with the same shape; when the
//! category is added upstream, the string codes here promote to a
//! typed `DiagnosticCode` without changing any call site.
//!
//! Current assignments — leave gaps closed and additive:
//!
//! | Code    | Condition                                                 |
//! |---------|-----------------------------------------------------------|
//! | I0140   | Wrong arity (needs exactly 2 arguments)                   |
//! | I0141   | Argument is not a string literal                          |
//! | I0142   | Blob path is empty                                        |
//! | I0143   | Keyring path is empty                                     |
//! | I0144   | Keyring file not found on disk                            |
//! | I0145   | Keyring path exists but is not a regular file             |
//! | I0146   | Keyring path unreadable (permission or I/O error)         |
//!
//! I0147–I0150 are reserved for the M2 encoder half (algorithm
//! mismatch, malformed keyring, signature verification failure, and
//! blob-read failure respectively). Reserving them here keeps the
//! whole intrinsic's error surface contiguous and lets rustdoc's
//! cross-file `intra-doc` links resolve once M2 lands.

use std::path::{Path, PathBuf};

// -----------------------------------------------------------------------------
// Public spec
// -----------------------------------------------------------------------------

/// Post-quantum signature algorithm expected on a firmware blob.
///
/// D1.a mandates hybrid (Ed25519 || ML-DSA-65) for any firmware payload
/// loaded by the paideia-as toolchain. The other two variants exist so
/// developer tooling (test fixtures, transitional bring-up firmware from
/// hardware vendors that have not yet published a hybrid keyring) can
/// declare a downgraded expectation explicitly rather than silently.
/// The parser always yields [`SigAlgo::Hybrid`]; downgrades are set by
/// higher layers (a per-crate build-manifest override) and land here
/// through the encoder's spec-transform pass, not through surface
/// syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SigAlgo {
    /// Classical Ed25519 (RFC 8032).
    Ed25519,
    /// Post-quantum ML-DSA-65 (FIPS 204, category-3 parameter set).
    MlDsa65,
    /// D1.a hybrid: Ed25519 signature concatenated with ML-DSA-65
    /// signature. Both components must verify. This is the default
    /// and the only algorithm the parser will emit unmodified.
    Hybrid,
}

impl SigAlgo {
    /// Short stable label used in diagnostics and manifest files.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SigAlgo::Ed25519 => "ed25519",
            SigAlgo::MlDsa65 => "mldsa65",
            SigAlgo::Hybrid => "hybrid",
        }
    }
}

/// Validated `@include_bytes_signed(path, keyring)` invocation.
///
/// This is a pure data record — no file handles, no cached bytes. The
/// encoder reads `blob_path` and verifies against `keyring_path` at
/// emit time; carrying the paths (not the contents) keeps the AST
/// small and lets the encoder re-check timestamps for incremental
/// builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeBytesSignedSpec {
    /// Path to the firmware blob relative to the source directory
    /// (or absolute — the parser does not restrict shape, only that
    /// the string is non-empty; sandboxing is the encoder's job so it
    /// can honor the workspace's `pdx.toml` `include_roots =` list).
    pub blob_path: PathBuf,
    /// Path to the keyring the encoder must load and select the
    /// verification key from. Guaranteed by [`parse_include_bytes_signed`]
    /// to exist and be a regular file at parse time.
    pub keyring_path: PathBuf,
    /// Signature algorithm the encoder must enforce. Parser always
    /// emits [`SigAlgo::Hybrid`].
    pub expected_algo: SigAlgo,
}

// -----------------------------------------------------------------------------
// Argument surface
// -----------------------------------------------------------------------------

/// Input token for a single `@include_bytes_signed` argument.
///
/// The surface parser (in `paideia-as-parser`) will lift each argument
/// into one of these variants before handing the slice to
/// [`parse_include_bytes_signed`]. Modeling the "not a string" case
/// as a variant (rather than pre-filtering) is deliberate: it lets
/// this module attach the [`IncludeSignedError::NonStringArg`]
/// diagnostic (I0141) with a precise argument index, which the
/// surface parser wraps with the offending token's span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncludeSignedArg<'a> {
    /// A parsed string literal — `s` is the already-unescaped content.
    Str(&'a str),
    /// Anything else the surface parser saw (integer literal, identifier,
    /// nested directive, …). Carries no payload; the surface parser
    /// owns the span it will attach to the resulting diagnostic.
    NonString,
}

// -----------------------------------------------------------------------------
// Error surface
// -----------------------------------------------------------------------------

/// Parse-time failure modes for `@include_bytes_signed`.
///
/// Every variant carries the data needed to render a human message —
/// no reach-back into the surface parser required. This lets the
/// module be exercised from unit tests without a `Diagnostic` sink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncludeSignedError {
    /// I0140 — arity mismatch. Includes the observed count so the
    /// surface parser can render `expected 2, got N`.
    Arity {
        /// Number of arguments actually seen (may be 0).
        got: usize,
    },
    /// I0141 — non-string-literal argument at the given zero-based
    /// position. The surface parser attaches this to the offending
    /// token's span.
    NonStringArg {
        /// 0 for the blob path, 1 for the keyring path.
        position: usize,
    },
    /// I0142 — blob path is the empty string.
    EmptyBlobPath,
    /// I0143 — keyring path is the empty string.
    EmptyKeyringPath,
    /// I0144 — keyring path resolves to a filesystem entry that does
    /// not exist. Fail-fast: a mistyped keyring path is a config
    /// error, not a security event; we surface it at parse time so
    /// `pdxc build` fails at the caller's terminal instead of at a
    /// downstream verification step on hardware they can't reach.
    KeyringMissing {
        /// The exact path the parser probed (post-normalization).
        path: PathBuf,
    },
    /// I0145 — keyring path exists but is not a regular file
    /// (directory, device node, symlink loop). Distinct from I0144
    /// so the operator's mental model —"file present vs. entry
    /// present"— matches the diagnostic they see.
    KeyringNotFile {
        /// The exact path the parser probed.
        path: PathBuf,
    },
    /// I0146 — keyring path could not be probed (permission denied
    /// or transient I/O error). Distinct from I0144 because retry
    /// semantics differ: a NotFound is a source-code fix, an I/O
    /// error is an environment fix.
    KeyringUnreadable {
        /// The exact path the parser probed.
        path: PathBuf,
        /// Human-readable OS error, captured so tests can assert
        /// against a stable prefix without depending on `io::Error`'s
        /// non-`PartialEq` shape.
        reason: String,
    },
}

impl IncludeSignedError {
    /// Stable string diagnostic code (`"I0140"`–`"I0146"`).
    ///
    /// See the module-level table for the full mapping and the
    /// reserved-but-unused range (`I0147`–`I0150`).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            IncludeSignedError::Arity { .. } => "I0140",
            IncludeSignedError::NonStringArg { .. } => "I0141",
            IncludeSignedError::EmptyBlobPath => "I0142",
            IncludeSignedError::EmptyKeyringPath => "I0143",
            IncludeSignedError::KeyringMissing { .. } => "I0144",
            IncludeSignedError::KeyringNotFile { .. } => "I0145",
            IncludeSignedError::KeyringUnreadable { .. } => "I0146",
        }
    }

    /// One-line human-readable message, safe to render directly in
    /// terminal diagnostics.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            IncludeSignedError::Arity { got } => format!(
                "@include_bytes_signed expects exactly 2 arguments (path, keyring), got {got}"
            ),
            IncludeSignedError::NonStringArg { position } => {
                let which = if *position == 0 { "path" } else { "keyring" };
                format!(
                    "@include_bytes_signed argument #{n} ({which}) must be a string literal",
                    n = position + 1,
                )
            }
            IncludeSignedError::EmptyBlobPath => {
                "@include_bytes_signed: blob path is empty".to_string()
            }
            IncludeSignedError::EmptyKeyringPath => {
                "@include_bytes_signed: keyring path is empty".to_string()
            }
            IncludeSignedError::KeyringMissing { path } => format!(
                "@include_bytes_signed: keyring not found on disk: {}",
                path.display()
            ),
            IncludeSignedError::KeyringNotFile { path } => format!(
                "@include_bytes_signed: keyring path is not a regular file: {}",
                path.display()
            ),
            IncludeSignedError::KeyringUnreadable { path, reason } => format!(
                "@include_bytes_signed: keyring unreadable: {} ({reason})",
                path.display(),
            ),
        }
    }
}

impl std::fmt::Display for IncludeSignedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code(), self.message())
    }
}

impl std::error::Error for IncludeSignedError {}

// -----------------------------------------------------------------------------
// Parser
// -----------------------------------------------------------------------------

/// Validate a `@include_bytes_signed(path, keyring)` invocation.
///
/// Returns an [`IncludeBytesSignedSpec`] the encoder can consume, or
/// an [`IncludeSignedError`] describing the first violation seen.
///
/// # What this function does
///
/// 1. Checks arity — exactly two args (I0140).
/// 2. Checks both args are string literals (I0141, per position).
/// 3. Rejects empty path strings (I0142, I0143).
/// 4. Probes the `keyring` path on disk and rejects it if missing
///    (I0144), non-file (I0145), or unreadable (I0146).
///
/// # What this function deliberately does **not** do
///
/// - It does **not** read the blob at `path`. The blob may be
///   megabytes; the encoder streams it. The parser only ensures the
///   *keyring* is reachable, because the keyring is a config-shaped
///   input (small, per-project, unlikely to change per invocation)
///   whereas the blob is a build-input-shaped input (large,
///   per-artifact, may be generated by a preceding build step).
/// - It does **not** verify the signature. That is the encoder's job
///   (v0.27-M2, `paideia-as-crypto::sig`). A green result from this
///   parser is a syntactic guarantee, not a security guarantee.
/// - It does **not** restrict the blob path to a sandbox root. The
///   encoder applies the `pdx.toml` `include_roots =` policy; putting
///   that check here would force this crate to depend on
///   `paideia-as-config`, and the sandbox is a workspace-level
///   concern, not a parser-level one.
pub fn parse_include_bytes_signed(
    args: &[IncludeSignedArg<'_>],
) -> Result<IncludeBytesSignedSpec, IncludeSignedError> {
    // I0140 — arity.
    if args.len() != 2 {
        return Err(IncludeSignedError::Arity { got: args.len() });
    }

    // I0141 — both arguments must be string literals. Check in
    // positional order so the reported error matches read order.
    let blob_str = match args[0] {
        IncludeSignedArg::Str(s) => s,
        IncludeSignedArg::NonString => {
            return Err(IncludeSignedError::NonStringArg { position: 0 });
        }
    };
    let keyring_str = match args[1] {
        IncludeSignedArg::Str(s) => s,
        IncludeSignedArg::NonString => {
            return Err(IncludeSignedError::NonStringArg { position: 1 });
        }
    };

    // I0142 / I0143 — empty-string paths. Distinct codes so the
    // diagnostic renderer can point at the correct argument even
    // without span information from the surface parser.
    if blob_str.is_empty() {
        return Err(IncludeSignedError::EmptyBlobPath);
    }
    if keyring_str.is_empty() {
        return Err(IncludeSignedError::EmptyKeyringPath);
    }

    let blob_path = PathBuf::from(blob_str);
    let keyring_path = PathBuf::from(keyring_str);

    // I0144 / I0145 / I0146 — keyring must be a readable regular
    // file on disk. See probe_keyring for the fail-fast rationale.
    probe_keyring(&keyring_path)?;

    Ok(IncludeBytesSignedSpec {
        blob_path,
        keyring_path,
        expected_algo: SigAlgo::Hybrid,
    })
}

/// Probe `keyring` and classify the result into I0144 / I0145 / I0146.
///
/// Kept private and standalone so the three failure modes can be
/// exercised in isolation from the full argument-parsing pipeline.
fn probe_keyring(keyring: &Path) -> Result<(), IncludeSignedError> {
    match std::fs::metadata(keyring) {
        Ok(meta) if meta.is_file() => Ok(()),
        Ok(_) => Err(IncludeSignedError::KeyringNotFile {
            path: keyring.to_path_buf(),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(IncludeSignedError::KeyringMissing {
                path: keyring.to_path_buf(),
            })
        }
        Err(e) => Err(IncludeSignedError::KeyringUnreadable {
            path: keyring.to_path_buf(),
            reason: e.to_string(),
        }),
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Materialize a real regular file inside a `tempfile::TempDir`
    /// and return the path. Keeps the TempDir alive through the
    /// returned handle so the file survives until the caller drops
    /// it — tests must bind the TempDir to a local, otherwise the
    /// directory is unlinked before `parse_include_bytes_signed`
    /// probes the keyring path.
    fn touch(dir: &tempfile::TempDir, name: &str, body: &[u8]) -> PathBuf {
        let p = dir.path().join(name);
        let mut f = std::fs::File::create(&p).expect("create keyring fixture");
        f.write_all(body).expect("write keyring fixture");
        p
    }

    #[test]
    fn valid_two_arg_call_yields_hybrid_spec() {
        // Fixture: a real on-disk keyring file the parser can stat.
        let dir = tempfile::tempdir().expect("tempdir");
        let keyring = touch(&dir, "dev.hybrid.keyring", b"stub-keyring-body");
        let keyring_s = keyring.to_str().expect("utf-8 fixture path");

        let args = [
            IncludeSignedArg::Str("firmware/dp83867.bin"),
            IncludeSignedArg::Str(keyring_s),
        ];
        let spec = parse_include_bytes_signed(&args).expect("valid two-arg call must succeed");

        assert_eq!(spec.blob_path, PathBuf::from("firmware/dp83867.bin"));
        assert_eq!(spec.keyring_path, keyring);
        assert_eq!(spec.expected_algo, SigAlgo::Hybrid);
        assert_eq!(SigAlgo::Hybrid.as_str(), "hybrid");
    }

    #[test]
    fn missing_keyring_is_rejected_with_i0144() {
        // Directory that does not contain the named keyring — probing
        // it yields NotFound, which must classify as I0144 (not the
        // generic I0146 unreadable bucket).
        let dir = tempfile::tempdir().expect("tempdir");
        let ghost = dir.path().join("never-created.keyring");
        assert!(!ghost.exists(), "precondition: fixture path must not exist");
        let ghost_s = ghost.to_str().expect("utf-8 fixture path");

        let args = [
            IncludeSignedArg::Str("firmware/blob.bin"),
            IncludeSignedArg::Str(ghost_s),
        ];
        let err = parse_include_bytes_signed(&args)
            .expect_err("missing keyring must be rejected at parse time");

        assert_eq!(err.code(), "I0144");
        match &err {
            IncludeSignedError::KeyringMissing { path } => assert_eq!(path, &ghost),
            other => panic!("expected KeyringMissing, got {other:?}"),
        }
        // Message must name the offending path so the operator can
        // grep their build output and locate the mistyped path.
        assert!(err.message().contains("never-created.keyring"));
    }

    #[test]
    fn non_string_first_arg_is_rejected_with_position_zero() {
        // Even though the keyring arg is invalid (empty non-string),
        // the parser must report the *first* failure in read order —
        // I0141 for position 0. This keeps error output stable when
        // the user is fixing arguments left-to-right.
        let args = [
            IncludeSignedArg::NonString,
            IncludeSignedArg::Str("/does/not/matter"),
        ];
        let err = parse_include_bytes_signed(&args)
            .expect_err("non-string blob arg must be rejected");
        assert_eq!(err.code(), "I0141");
        assert!(matches!(
            err,
            IncludeSignedError::NonStringArg { position: 0 }
        ));
        assert!(err.message().contains("path"));
    }

    #[test]
    fn non_string_second_arg_is_rejected_with_position_one() {
        let args = [
            IncludeSignedArg::Str("firmware/blob.bin"),
            IncludeSignedArg::NonString,
        ];
        let err = parse_include_bytes_signed(&args)
            .expect_err("non-string keyring arg must be rejected");
        assert_eq!(err.code(), "I0141");
        assert!(matches!(
            err,
            IncludeSignedError::NonStringArg { position: 1 }
        ));
        assert!(err.message().contains("keyring"));
    }

    #[test]
    fn wrong_arity_is_rejected_with_i0140() {
        // Zero args.
        let err = parse_include_bytes_signed(&[])
            .expect_err("zero-arg call must be rejected");
        assert_eq!(err.code(), "I0140");
        assert!(matches!(err, IncludeSignedError::Arity { got: 0 }));

        // One arg.
        let err = parse_include_bytes_signed(&[IncludeSignedArg::Str("only.bin")])
            .expect_err("one-arg call must be rejected");
        assert!(matches!(err, IncludeSignedError::Arity { got: 1 }));

        // Three args.
        let err = parse_include_bytes_signed(&[
            IncludeSignedArg::Str("a"),
            IncludeSignedArg::Str("b"),
            IncludeSignedArg::Str("c"),
        ])
        .expect_err("three-arg call must be rejected");
        assert!(matches!(err, IncludeSignedError::Arity { got: 3 }));
    }

    #[test]
    fn empty_paths_have_distinct_codes() {
        // I0142 — empty blob.
        let dir = tempfile::tempdir().expect("tempdir");
        let keyring = touch(&dir, "k.keyring", b"x");
        let keyring_s = keyring.to_str().unwrap();

        let err = parse_include_bytes_signed(&[
            IncludeSignedArg::Str(""),
            IncludeSignedArg::Str(keyring_s),
        ])
        .expect_err("empty blob path must be rejected");
        assert_eq!(err.code(), "I0142");

        // I0143 — empty keyring. Probed *before* the on-disk check
        // because an empty string is a source-code error, not an
        // environment error.
        let err = parse_include_bytes_signed(&[
            IncludeSignedArg::Str("firmware/blob.bin"),
            IncludeSignedArg::Str(""),
        ])
        .expect_err("empty keyring path must be rejected");
        assert_eq!(err.code(), "I0143");
    }

    #[test]
    fn keyring_directory_is_rejected_with_i0145() {
        // A directory exists at the path but is_file() is false —
        // must classify as I0145, not I0144. Distinguishing these
        // matters: I0144 tells the operator "you typoed the path",
        // I0145 tells them "you pointed at a directory, did you
        // mean a file inside it?"
        let dir = tempfile::tempdir().expect("tempdir");
        let dir_as_keyring = dir.path().join("subdir");
        std::fs::create_dir(&dir_as_keyring).expect("mk fixture dir");
        let s = dir_as_keyring.to_str().unwrap();

        let err = parse_include_bytes_signed(&[
            IncludeSignedArg::Str("firmware/blob.bin"),
            IncludeSignedArg::Str(s),
        ])
        .expect_err("directory-as-keyring must be rejected");
        assert_eq!(err.code(), "I0145");
    }

    #[test]
    fn sig_algo_labels_are_stable() {
        // These labels appear in downstream manifest files and in
        // future I0147 diagnostic text; changing them is a breaking
        // change to the intrinsic's ABI, hence the pin.
        assert_eq!(SigAlgo::Ed25519.as_str(), "ed25519");
        assert_eq!(SigAlgo::MlDsa65.as_str(), "mldsa65");
        assert_eq!(SigAlgo::Hybrid.as_str(), "hybrid");
    }

    #[test]
    fn display_impl_prepends_code() {
        // The Display impl is what non-diagnostic-aware callers
        // (bin drivers, `dbg!`) will surface; pin the shape.
        let err = IncludeSignedError::Arity { got: 0 };
        let rendered = format!("{err}");
        assert!(rendered.starts_with("[I0140]"), "got: {rendered}");
    }
}
