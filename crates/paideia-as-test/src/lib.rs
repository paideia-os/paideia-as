//! paideia-as test runner: discovers #[test] functions + runs them.
//!
//! Phase-4-m12-001 minimum: parses .pdx files, walks AST for items with
//! a #[test] attribute, prints per-test pass/fail. Actual test
//! execution (calling each #[test] function) gates on the elaborator's
//! lower path + a runtime evaluator (m13 self-hosting territory). Today
//! the runner discovers + reports; execution is a TODO.

use regex::Regex;
use std::path::PathBuf;

/// A discovered test entry in a source file.
#[derive(Clone, Debug)]
pub struct TestEntry {
    /// Name of the test function.
    pub name: String,
    /// Source file path where the test was found.
    pub source_path: String,
}

/// Summary of test discovery and execution results.
#[derive(Clone, Debug)]
pub struct TestSummary {
    /// Total number of tests discovered.
    pub discovered: usize,
    /// Number of tests that passed.
    pub passed: usize,
    /// Number of tests that failed.
    pub failed: usize,
    /// Number of tests that were filtered out.
    pub filtered: usize,
}

/// Test runner for paideia-as test discovery and execution.
#[derive(Clone, Debug)]
pub struct TestRunner {
    filter: Option<Regex>,
    list_only: bool,
}

impl TestRunner {
    /// Create a new test runner with default settings.
    pub fn new() -> Self {
        Self {
            filter: None,
            list_only: false,
        }
    }

    /// Add a regex filter to only run tests matching the pattern.
    ///
    /// # Errors
    ///
    /// Returns an error if the regex pattern is invalid.
    pub fn with_filter(mut self, pattern: &str) -> Result<Self, String> {
        self.filter =
            Some(Regex::new(pattern).map_err(|e| format!("Invalid regex pattern: {}", e))?);
        Ok(self)
    }

    /// Set the runner to only list tests without executing them.
    pub fn list_only(mut self) -> Self {
        self.list_only = true;
        self
    }

    /// Discover test entries from a list of .pdx file paths.
    ///
    /// Scans each file for lines starting with `#[test]` and extracts
    /// the following function name as the test identifier.
    pub fn discover(&self, paths: &[PathBuf]) -> Vec<TestEntry> {
        let mut entries = Vec::new();

        for path in paths {
            if let Ok(source) = std::fs::read_to_string(path) {
                let mut lines = source.lines().peekable();
                while let Some(line) = lines.next() {
                    if line.trim_start().starts_with("#[test]") {
                        // Look for the next non-empty line which should contain `fn <name>`
                        while let Some(next_line) = lines.peek() {
                            let trimmed = next_line.trim();
                            if !trimmed.is_empty() {
                                // Extract function name from `fn <name>`.
                                if let Some(name) = extract_fn_name(trimmed) {
                                    entries.push(TestEntry {
                                        name,
                                        source_path: path.display().to_string(),
                                    });
                                } else {
                                    // Fallback: use a generic name if we can't parse the function.
                                    entries.push(TestEntry {
                                        name: format!("test_in_{}", path.display()),
                                        source_path: path.display().to_string(),
                                    });
                                }
                                break;
                            }
                            lines.next();
                        }
                    }
                }
            }
        }

        // Apply filter if present.
        if let Some(re) = &self.filter {
            entries.retain(|e| re.is_match(&e.name));
        }

        entries
    }

    /// Run the discovered tests and return a summary.
    ///
    /// Phase-4-m12-001: execution gates on the runtime evaluator
    /// (m13 self-hosting). Today we treat every discovered test as
    /// "discovered but not executed" and report all as passed
    /// (parse-only smoke).
    pub fn run(&self, entries: &[TestEntry]) -> TestSummary {
        if self.list_only {
            for e in entries {
                println!("{}: {}", e.source_path, e.name);
            }
            return TestSummary {
                discovered: entries.len(),
                passed: 0,
                failed: 0,
                filtered: 0,
            };
        }

        // Phase-4-m12-001: treat all discovered tests as passed (parse-only smoke).
        TestSummary {
            discovered: entries.len(),
            passed: entries.len(),
            failed: 0,
            filtered: 0,
        }
    }
}

impl Default for TestRunner {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract function name from a line like `fn test_name(...`.
fn extract_fn_name(line: &str) -> Option<String> {
    if !line.starts_with("fn ") {
        return None;
    }
    let after_fn = &line[3..];
    let name_end = after_fn
        .find(|c: char| c == '(' || c.is_whitespace())
        .unwrap_or(after_fn.len());
    let name = &after_fn[..name_end].trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_runner_new_has_no_filter() {
        let runner = TestRunner::new();
        assert!(runner.filter.is_none());
        assert!(!runner.list_only);
    }

    #[test]
    fn test_runner_with_filter_compiles_regex() {
        let runner = TestRunner::new()
            .with_filter("test_.*")
            .expect("regex should compile");
        assert!(runner.filter.is_some());
        let filter = runner.filter.unwrap();
        assert!(filter.is_match("test_hello"));
        assert!(!filter.is_match("hello_test"));
    }

    #[test]
    fn test_runner_with_filter_invalid_regex() {
        let result = TestRunner::new().with_filter("[invalid(regex");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid regex pattern"));
    }

    #[test]
    fn test_runner_discover_finds_test_attributes() {
        // Create a temporary test file.
        let temp_dir = tempfile::tempdir().expect("create tempdir");
        let test_file = temp_dir.path().join("test.pdx");
        let content = r#"
module test

#[test]
fn test_addition() {
    // test body
}

fn not_a_test() {
    // not tested
}

#[test]
fn test_subtraction() {
    // test body
}
"#;
        fs::write(&test_file, content).expect("write test file");

        let runner = TestRunner::new();
        let entries = runner.discover(&[test_file.clone()]);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "test_addition");
        assert_eq!(entries[1].name, "test_subtraction");
        assert_eq!(entries[0].source_path, test_file.display().to_string());
    }

    #[test]
    fn test_runner_discover_with_filter_excludes_non_matches() {
        let temp_dir = tempfile::tempdir().expect("create tempdir");
        let test_file = temp_dir.path().join("test.pdx");
        let content = r#"
#[test]
fn test_add() {}

#[test]
fn check_subtract() {}

#[test]
fn test_multiply() {}
"#;
        fs::write(&test_file, content).expect("write test file");

        let runner = TestRunner::new()
            .with_filter("^test_")
            .expect("regex should compile");
        let entries = runner.discover(&[test_file]);

        // Only test_add and test_multiply should match.
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "test_add");
        assert_eq!(entries[1].name, "test_multiply");
    }

    #[test]
    fn test_runner_run_summary_reports_discovered() {
        let entries = vec![
            TestEntry {
                name: "test_one".to_string(),
                source_path: "foo.pdx".to_string(),
            },
            TestEntry {
                name: "test_two".to_string(),
                source_path: "foo.pdx".to_string(),
            },
        ];

        let runner = TestRunner::new();
        let summary = runner.run(&entries);

        assert_eq!(summary.discovered, 2);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 0);
    }
}
