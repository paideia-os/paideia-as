//! Fixture-path resolution for `paideia-as` integration tests.
//!
//! Every fixture lives at a workspace- or crate-relative path, resolved
//! against `CARGO_MANIFEST_DIR` (which Cargo sets to
//! `<workspace>/crates/paideia-as` when the integration tests compile).
//!
//! Scratch outputs go through [`ScratchDir`], a thin wrapper around
//! `tempfile::TempDir` that gives named artifact paths (`.o`, `.elf`, ...) and
//! auto-cleans on drop.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

/// Directory containing the crate's `Cargo.toml`, i.e. `crates/paideia-as/`.
fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Two directories above the crate manifest — the workspace root
/// (`tools/paideia-as/`).
fn workspace_root() -> PathBuf {
    let mut p = manifest_dir();
    p.pop(); // -> crates/
    p.pop(); // -> tools/paideia-as/
    p
}

/// Resolve a fixture living under the workspace-root `tests/build-emit/` tree.
///
/// Accepts either a bare basename (`"foo.pdx"`) or a subdirectory-qualified
/// path (`"control_flow/cmp_reg_reg.pdx"`). Returns the absolute path.
pub fn build_emit(name: &str) -> PathBuf {
    workspace_root().join("tests").join("build-emit").join(name)
}

/// Resolve a fixture living directly under the workspace-root `tests/` tree
/// — used by ticket-scoped bundles like `pa_r17_1074_t0535_record_field/`
/// and `pa_r17_003b_t0535/`.
pub fn workspace_tests(rel: &str) -> PathBuf {
    workspace_root().join("tests").join(rel)
}

/// Resolve a path relative to the workspace root
/// (e.g. `"examples/02_functions.pdx"`).
pub fn workspace(rel: &str) -> PathBuf {
    workspace_root().join(rel)
}

/// Resolve a fixture under the crate's own `tests/` tree — used by
/// `tests/data/*.pdx`, `tests/cross_file/*.pdx`, `tests/fixtures/*` and
/// legacy `tests/*.pdx` files.
pub fn crate_tests(rel: &str) -> PathBuf {
    manifest_dir().join("tests").join(rel)
}

/// Scratch temp directory that owns a `tempfile::TempDir` and hands out named
/// artifact paths. Drop removes the directory tree.
pub struct ScratchDir {
    dir: TempDir,
}

impl ScratchDir {
    /// Create a fresh scratch dir with a human-readable prefix (helps when
    /// inspecting `/tmp` during a test hang).
    pub fn new(tag: &str) -> Self {
        let dir = tempfile::Builder::new()
            .prefix(&format!("paideia-as-{tag}-"))
            .tempdir()
            .expect("create scratch tempdir");
        Self { dir }
    }

    /// Root path of the scratch dir.
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Path for a named artifact inside the scratch dir (e.g. `"out.o"`).
    pub fn artifact(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
}
