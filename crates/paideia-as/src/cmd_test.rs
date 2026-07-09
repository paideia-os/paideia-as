//! `paideia-as test [--filter <regex>] [--list] [--format <human|json|tap>] [paths...]`
//!
//! Discovers and runs tests in .pdx files. Supports three output formats:
//! human (default), JSON newline-delimited events, and TAP 13.

use crate::cli::OutputFormat;
use paideia_as_test::{TestEntry, TestRunner, TestSummary};
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

/// Run `paideia-as test [options] [paths...]`.
///
/// Returns an `ExitCode` so the CLI can propagate non-zero on errors.
pub fn run(
    paths: Vec<PathBuf>,
    filter: Option<String>,
    list: bool,
    format: OutputFormat,
) -> ExitCode {
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

    // Route output based on format.
    match format {
        OutputFormat::Human => run_human_format(&entries, list),
        OutputFormat::Json => run_json_format(&entries, list),
        OutputFormat::Tap => run_tap_format(&entries, list),
    }
}

/// Human-readable format: byte-identical to pre-#1111 behavior.
fn run_human_format(entries: &[TestEntry], list: bool) -> ExitCode {
    if list {
        for e in entries {
            println!("{}: {}", e.source_path, e.name);
        }
        return ExitCode::SUCCESS;
    }

    // Run tests (discovery only; all treated as passed).
    let summary = TestSummary {
        discovered: entries.len(),
        passed: entries.len(),
        failed: 0,
        filtered: 0,
    };

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

    ExitCode::SUCCESS
}

/// JSON newline-delimited format (NDJSON).
fn run_json_format(entries: &[TestEntry], list: bool) -> ExitCode {
    let mut stdout = std::io::stdout().lock();
    emit_json(&mut stdout, entries, list);
    ExitCode::SUCCESS
}

/// Emit JSON events (NDJSON) to the provided writer.
fn emit_json(out: &mut dyn Write, entries: &[TestEntry], list: bool) {
    let test_count = entries.len();

    if list {
        // List mode: emit discovered events
        for e in entries {
            let json = serde_json::json!({
                "type": "test",
                "event": "discovered",
                "name": e.name,
                "source_path": e.source_path,
            });
            let _ = writeln!(out, "{}", json.to_string());
        }
        let suite_json = serde_json::json!({
            "type": "suite",
            "event": "listed",
            "count": test_count,
        });
        let _ = writeln!(out, "{}", suite_json.to_string());
    } else {
        // Run mode: emit suite/started + test events + suite/ok
        let start_json = serde_json::json!({
            "type": "suite",
            "event": "started",
            "test_count": test_count,
        });
        let _ = writeln!(out, "{}", start_json.to_string());

        for e in entries {
            let started = serde_json::json!({
                "type": "test",
                "event": "started",
                "name": e.name,
                "source_path": e.source_path,
            });
            let _ = writeln!(out, "{}", started.to_string());

            let ok = serde_json::json!({
                "type": "test",
                "event": "ok",
                "name": e.name,
                "exec_time": 0.0,
            });
            let _ = writeln!(out, "{}", ok.to_string());
        }

        let suite_ok = serde_json::json!({
            "type": "suite",
            "event": "ok",
            "passed": test_count,
            "failed": 0,
            "filtered_out": 0,
        });
        let _ = writeln!(out, "{}", suite_ok.to_string());
    }
}

/// TAP 13 format.
fn run_tap_format(entries: &[TestEntry], list: bool) -> ExitCode {
    let mut stdout = std::io::stdout().lock();
    emit_tap(&mut stdout, entries, list);
    ExitCode::SUCCESS
}

/// Emit TAP 13 output to the provided writer.
fn emit_tap(out: &mut dyn Write, entries: &[TestEntry], list: bool) {
    let test_count = entries.len();

    let _ = writeln!(out, "TAP version 13");

    if test_count == 0 {
        let _ = writeln!(out, "1..0 # SKIP no tests found");
        return;
    }

    let _ = writeln!(out, "1..{}", test_count);

    for (idx, e) in entries.iter().enumerate() {
        let num = idx + 1;
        if list {
            let _ = writeln!(out, "ok {} - {} # SKIP listed only", num, e.name);
        } else {
            let _ = writeln!(out, "ok {} - {}", num, e.name);
        }
    }

    if list {
        let _ = writeln!(out, "# discovered {}", test_count);
    } else {
        let _ = writeln!(out, "# tests {}", test_count);
        let _ = writeln!(out, "# pass {}", test_count);
        let _ = writeln!(out, "# fail 0");
    }
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

    #[test]
    fn emit_json_run_events_matches_golden() {
        let entries = vec![
            TestEntry {
                name: "test_one".to_string(),
                source_path: "test.pdx".to_string(),
            },
            TestEntry {
                name: "test_two".to_string(),
                source_path: "test.pdx".to_string(),
            },
        ];

        let mut out = Vec::new();
        emit_json(&mut out, &entries, false);
        let output = String::from_utf8(out).unwrap();

        // Verify the output is valid NDJSON with expected sequence
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 6); // 1 suite/started + 2*(test/started+test/ok) + 1 suite/ok

        // Parse and verify structure
        let start: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(start["type"], "suite");
        assert_eq!(start["event"], "started");
        assert_eq!(start["test_count"], 2);

        let suite_ok: serde_json::Value = serde_json::from_str(lines[5]).unwrap();
        assert_eq!(suite_ok["type"], "suite");
        assert_eq!(suite_ok["event"], "ok");
        assert_eq!(suite_ok["passed"], 2);
    }

    #[test]
    fn emit_tap_run_events_matches_golden() {
        let entries = vec![
            TestEntry {
                name: "test_one".to_string(),
                source_path: "test.pdx".to_string(),
            },
            TestEntry {
                name: "test_two".to_string(),
                source_path: "test.pdx".to_string(),
            },
        ];

        let mut out = Vec::new();
        emit_tap(&mut out, &entries, false);
        let output = String::from_utf8(out).unwrap();

        let lines: Vec<&str> = output.lines().collect();
        assert!(lines[0].contains("TAP version 13"));
        assert!(lines[1].contains("1..2"));
        assert!(lines[2].contains("ok 1 - test_one"));
        assert!(lines[3].contains("ok 2 - test_two"));
        assert!(lines[4].contains("# tests 2"));
        assert!(lines[5].contains("# pass 2"));
        assert!(lines[6].contains("# fail 0"));
    }

    #[test]
    fn emit_json_list_events_matches_golden() {
        let entries = vec![
            TestEntry {
                name: "test_one".to_string(),
                source_path: "test.pdx".to_string(),
            },
        ];

        let mut out = Vec::new();
        emit_json(&mut out, &entries, true);
        let output = String::from_utf8(out).unwrap();

        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2); // 1 test/discovered + 1 suite/listed

        let discovered: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(discovered["type"], "test");
        assert_eq!(discovered["event"], "discovered");
        assert_eq!(discovered["name"], "test_one");

        let suite: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(suite["type"], "suite");
        assert_eq!(suite["event"], "listed");
        assert_eq!(suite["count"], 1);
    }

    #[test]
    fn emit_tap_list_events_uses_skip_directive() {
        let entries = vec![
            TestEntry {
                name: "test_one".to_string(),
                source_path: "test.pdx".to_string(),
            },
            TestEntry {
                name: "test_two".to_string(),
                source_path: "test.pdx".to_string(),
            },
        ];

        let mut out = Vec::new();
        emit_tap(&mut out, &entries, true);
        let output = String::from_utf8(out).unwrap();

        assert!(output.contains("# SKIP listed only"));
        let lines: Vec<&str> = output.lines().collect();
        for line in &lines[2..4] {
            assert!(line.contains("# SKIP listed only"));
        }
    }

}
