// Public field docs are omitted deliberately in this loader — every field is
// one JSON key, self-describing via its `#[serde(rename = "…")]` (or its
// Rust name where they match). Types carry the per-record documentation.
#![allow(missing_docs)]

//! Loader for the vendored NIST ACVP FIPS-204 (ML-DSA) internal-projection vectors.
//!
//! Vector files live under `tests/pq-corpus/vectors/`. See
//! `tests/pq-corpus/vectors/README.md` for provenance and the pinned upstream
//! commit. Files are pre-trimmed to the ML-DSA-65 parameter set only — that is
//! the sole parameter set the paideia signing surface exposes.
//!
//! The struct shape here mirrors the ACVP JSON: a corpus file has a small
//! header plus a `testGroups` array; each group carries per-parameter-set
//! metadata (deterministic flag, group-level pk/sk for sigVer) and its own
//! `tests` array with per-vector hex payloads.
//!
//! Fields we do not exercise (`revision`, `isSample`, `deferred`, unused
//! metadata) are captured with `serde(default)` or skipped so the loader stays
//! forward-compatible with vector-file refreshes.

use serde::Deserialize;
use std::path::Path;

/// One key-generation vector: `pk = KeyGen(seed)`.
#[derive(Debug, Deserialize)]
pub struct KeyGenTest {
    /// ACVP test-case id.
    #[serde(rename = "tcId")]
    pub tc_id: u32,
    /// 32-byte seed, hex-encoded (upper-case in the vector files).
    pub seed: String,
    /// Expected 1952-byte encoded verifying key, hex-encoded.
    pub pk: String,
    /// Expected 4032-byte encoded expanded signing key, hex-encoded.
    /// Not exercised by the paideia wrapper directly (the wrapper stores only
    /// the seed), but kept so a future test can compare-round-trip via the
    /// expanded form.
    pub sk: String,
}

/// One key-generation test group (one per parameter set × testType).
#[derive(Debug, Deserialize)]
pub struct KeyGenGroup {
    #[serde(rename = "tgId")]
    pub tg_id: u32,
    #[serde(rename = "parameterSet")]
    pub parameter_set: String,
    pub tests: Vec<KeyGenTest>,
}

/// Root of `ml_dsa_65_keygen.json`.
#[derive(Debug, Deserialize)]
pub struct KeyGenCorpus {
    #[serde(rename = "vsId")]
    pub vs_id: u32,
    pub algorithm: String,
    pub mode: String,
    #[serde(rename = "testGroups")]
    pub test_groups: Vec<KeyGenGroup>,
}

/// One signature-generation vector.
#[derive(Debug, Deserialize)]
pub struct SigGenTest {
    #[serde(rename = "tcId")]
    pub tc_id: u32,
    /// 4032-byte expanded signing key, hex-encoded.
    pub sk: String,
    /// Message bytes, hex-encoded (length varies per vector).
    pub message: String,
    /// Expected 3309-byte signature, hex-encoded.
    pub signature: String,
    /// Present only in hedged (`deterministic == false`) groups: 32-byte rnd,
    /// hex-encoded. Absent for deterministic vectors — those are signed with
    /// rnd = [0; 32] per FIPS-204.
    #[serde(default)]
    pub rnd: Option<String>,
}

/// One sigGen test group (grouped by parameter set × deterministic flag).
#[derive(Debug, Deserialize)]
pub struct SigGenGroup {
    #[serde(rename = "tgId")]
    pub tg_id: u32,
    #[serde(rename = "parameterSet")]
    pub parameter_set: String,
    pub deterministic: bool,
    pub tests: Vec<SigGenTest>,
}

/// Root of `ml_dsa_65_siggen.json`.
#[derive(Debug, Deserialize)]
pub struct SigGenCorpus {
    #[serde(rename = "vsId")]
    pub vs_id: u32,
    pub algorithm: String,
    pub mode: String,
    #[serde(rename = "testGroups")]
    pub test_groups: Vec<SigGenGroup>,
}

/// One signature-verification vector.
#[derive(Debug, Deserialize)]
pub struct SigVerTest {
    #[serde(rename = "tcId")]
    pub tc_id: u32,
    /// Expected verification outcome (true = signature valid).
    #[serde(rename = "testPassed")]
    pub test_passed: bool,
    /// Message bytes, hex-encoded.
    pub message: String,
    /// 3309-byte signature under test, hex-encoded.
    pub signature: String,
    /// ACVP-supplied failure reason (e.g. "no modification", "message
    /// modified"); useful for diagnosing regressions.
    #[serde(default)]
    pub reason: Option<String>,
}

/// One sigVer test group. The verifying key is fixed at the group level —
/// individual vectors share it and vary only in message/signature/expected.
#[derive(Debug, Deserialize)]
pub struct SigVerGroup {
    #[serde(rename = "tgId")]
    pub tg_id: u32,
    #[serde(rename = "parameterSet")]
    pub parameter_set: String,
    /// 1952-byte encoded verifying key, hex-encoded, shared across the group.
    pub pk: String,
    /// 4032-byte encoded expanded signing key, hex-encoded (unused by the
    /// verify path, kept for completeness).
    #[serde(default)]
    pub sk: Option<String>,
    pub tests: Vec<SigVerTest>,
}

/// Root of `ml_dsa_65_sigver.json`.
#[derive(Debug, Deserialize)]
pub struct SigVerCorpus {
    #[serde(rename = "vsId")]
    pub vs_id: u32,
    pub algorithm: String,
    pub mode: String,
    #[serde(rename = "testGroups")]
    pub test_groups: Vec<SigVerGroup>,
}

/// Absolute path to the vendored ACVP vector directory. Anchored on the
/// pq-corpus crate's own manifest dir so `cargo test` runs from any workspace
/// location (workspace root, crate dir, or an out-of-tree checkout).
pub fn vectors_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("vectors")
}

/// Load and parse `vectors/ml_dsa_65_keygen.json`.
pub fn load_keygen() -> KeyGenCorpus {
    let path = vectors_dir().join("ml_dsa_65_keygen.json");
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// Load and parse `vectors/ml_dsa_65_siggen.json`.
pub fn load_siggen() -> SigGenCorpus {
    let path = vectors_dir().join("ml_dsa_65_siggen.json");
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// Load and parse `vectors/ml_dsa_65_sigver.json`.
pub fn load_sigver() -> SigVerCorpus {
    let path = vectors_dir().join("ml_dsa_65_sigver.json");
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// Decode a hex string into a fixed-size byte array, panicking with a helpful
/// message on any mismatch. Vector files are trusted (we bake them in-tree),
/// so a panic here indicates a corpus-versioning bug, not runtime failure.
pub fn hex_to_array<const N: usize>(hex: &str, field: &str, tc_id: u32) -> [u8; N] {
    let bytes = hex::decode(hex)
        .unwrap_or_else(|e| panic!("tcId={tc_id} field={field}: hex decode: {e}"));
    <[u8; N]>::try_from(bytes.as_slice()).unwrap_or_else(|_| {
        panic!(
            "tcId={tc_id} field={field}: expected {N} bytes, got {}",
            bytes.len()
        )
    })
}

/// Decode a hex string into a `Vec<u8>` (for variable-length fields such as
/// `message`).
pub fn hex_to_vec(hex: &str, field: &str, tc_id: u32) -> Vec<u8> {
    hex::decode(hex).unwrap_or_else(|e| panic!("tcId={tc_id} field={field}: hex decode: {e}"))
}
