//! Smoke test for the DDC orchestrator script.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn ddc_run_script_exists_and_is_executable() {
    let script = workspace_root().join("tools/ddc/run.sh");
    assert!(script.exists(), "tools/ddc/run.sh must exist");
    let metadata = std::fs::metadata(&script).unwrap();
    let mode = metadata.permissions().mode();
    assert!(mode & 0o111 != 0, "tools/ddc/run.sh must be executable");
}

#[test]
fn ddc_readme_exists() {
    let readme = workspace_root().join("tools/ddc/README.md");
    assert!(readme.exists(), "tools/ddc/README.md must exist");
}
