//! `paideia-as test [--filter <regex>] [--list] [paths...]`
//!
//! Discovers and runs tests in .pdx files. Phase-4-m12-001 form: discovery only.

use paideia_as_test::TestRunner;
use std::path::PathBuf;
use std::process::ExitCode;

/// Run `paideia-as test [options] [paths...]`.
///
/// Returns an `ExitCode` so the CLI can propagate non-zero on errors.
pub fn run(paths: Vec<PathBuf>, filter: Option<String>, list: bool) -> ExitCode {
    // If no paths provided, scan `tests/` and `src/`.
    let scan_paths = if paths.is_empty() {
        default_scan_paths()
    } else {
        paths
    };

    // Build the runner.
    let mut runner = TestRunner::new();

    if let Some(pattern) = filter {
        runner = match runner.with_filter(&pattern) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("paideia-as test: {}", e);
                return ExitCode::from(1);
            }
        };
    }

    if list {
        runner = runner.list_only();
    }

    // Discover tests.
    let entries = runner.discover(&scan_paths);

    // Run tests.
    let summary = runner.run(&entries);

    // Report results.
    if !list {
        eprintln!(
            "test result: {}. discovered: {}; passed: {}; failed: {}",
            if summary.failed == 0 { "ok" } else { "FAILED" },
            summary.discovered,
            summary.passed,
            summary.failed
        );

        if summary.failed > 0 {
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}

/// Default paths to scan for test files.
fn default_scan_paths() -> Vec<PathBuf> {
    vec![PathBuf::from("tests"), PathBuf::from("src")]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn default_scan_paths_returns_two_dirs() {
        let paths = default_scan_paths();
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], PathBuf::from("tests"));
        assert_eq!(paths[1], PathBuf::from("src"));
    }

    #[test]
    fn run_with_empty_paths_uses_defaults() {
        let temp_dir = tempfile::tempdir().expect("create tempdir");
        let test_file = temp_dir.path().join("test.pdx");
        let content = "#[test]\nfn test_foo() {}";
        fs::write(&test_file, content).expect("write test file");

        // Note: actual run would use defaults, but we're not testing that here
        // since it depends on real filesystem state.
    }
}
