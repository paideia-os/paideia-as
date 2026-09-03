//! paideia-as test runner: discovers `#[test]` functions + runs them.
//!
//! # v0.33-M1-007 (paideia-as#1393 / paideia-os#1349)
//!
//! **Before this landing:** `TestRunner::discover` was a plain-text
//! `line.starts_with("#[test]")` substring scan and silently swallowed
//! I/O errors (nonexistent path → `discovered: 0`, exit 0). `cmd_test`
//! then reported every discovered entry as passed without ever invoking
//! the parser. Any downstream `tools/build.sh` fixture-gate that leaned
//! on `paideia-as test` was a false-positive gate.
//!
//! **This landing (scoped fix, Q-A-4 Option B):**
//!
//! 1. `discover` actually invokes the paideia-as lexer on each input
//!    file and walks the token stream for the `#[test]` attribute
//!    pattern (`Hash LBracket Ident("test") RBracket`) followed by a
//!    `let|fn <name>` binding. Substring matches inside comments or
//!    string literals no longer produce phantom entries.
//! 2. Nonexistent paths, encoding failures, and empty files fail with a
//!    real `TestError`, not silent zero.
//! 3. `run` groups entries by source file, invokes the full
//!    parser + elaborator pipeline once per file, and marks entries as
//!    passed only when zero error-severity diagnostics fire. Files that
//!    are explicitly named on the command line but have no `#[test]`
//!    entries are still parsed so garbage files register as failures.
//!
//! **Deferred to v0.34:** per-test parallel execution, an actual runtime
//! fixture-invocation protocol, per-function isolation (a single parse
//! error currently fails every entry in that file), and recursive
//! directory scanning.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{Diagnostic, DiagnosticSink, Severity, SourceMap, VecSink};
use paideia_as_elaborator::{build_enum_registry, build_struct_registry, lower_ast_to_ir};
use paideia_as_lexer::{Lexer, SourceText, Token, TokenKind};
use paideia_as_parser::Parser;
use regex::Regex;

/// A discovered test entry in a source file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestEntry {
    /// Name of the test function.
    pub name: String,
    /// Source file path where the test was found.
    pub source_path: String,
}

/// Summary of test discovery and execution results.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TestSummary {
    /// Total number of `#[test]` entries discovered.
    pub discovered: usize,
    /// Number of entries that passed (their file elaborated with zero errors).
    pub passed: usize,
    /// Number of entries that failed (their file emitted at least one error).
    pub failed: usize,
    /// Number of entries elided by `--filter`.
    pub filtered: usize,
}

/// One file that failed the parse + elaborate pipeline during `run`.
#[derive(Clone, Debug)]
pub struct FailedFile {
    /// The source file that failed.
    pub path: PathBuf,
    /// Human-readable diagnostic lines (one per error).
    pub messages: Vec<String>,
}

/// Result of executing a discovered test suite.
#[derive(Clone, Debug, Default)]
pub struct RunOutcome {
    /// Aggregate counts across every entry.
    pub summary: TestSummary,
    /// Files that failed to lex / parse / elaborate. Even when no
    /// `#[test]` entries live in a file, an explicitly-named garbage
    /// file lands here so the CLI can exit non-zero.
    pub failed_files: Vec<FailedFile>,
}

/// Fatal error surfaced by discovery — the runner cannot continue.
#[derive(Debug)]
pub enum TestError {
    /// The file could not be read (nonexistent, permission denied, …).
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The OS error text.
        message: String,
    },
    /// The file bytes failed pre-lex validation (invalid UTF-8, BOM
    /// oddities, lone CR, empty file — see `SourceText::from_bytes`).
    Encoding {
        /// The path that failed.
        path: PathBuf,
        /// The diagnostic message from the source-text validator.
        message: String,
    },
}

impl std::fmt::Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestError::Io { path, message } => {
                write!(f, "cannot read {}: {}", path.display(), message)
            }
            TestError::Encoding { path, message } => {
                write!(f, "cannot decode {}: {}", path.display(), message)
            }
        }
    }
}

impl std::error::Error for TestError {}

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

    /// Whether the runner is in list-only mode.
    #[must_use]
    pub fn is_list_only(&self) -> bool {
        self.list_only
    }

    /// Discover `#[test]` entries in each `.pdx` input file.
    ///
    /// The pipeline invokes the paideia-as lexer on every path, then
    /// walks the token stream for a `#[test]` outer-attribute pattern
    /// (`Hash LBracket Ident("test") RBracket`) followed by a
    /// `let|fn <name>` binding — captures the name.
    ///
    /// # Errors
    ///
    /// * `TestError::Io` — the file could not be read (nonexistent
    ///   path, permission denied, …).
    /// * `TestError::Encoding` — the file bytes failed pre-lex UTF-8 /
    ///   BOM / lone-CR validation, or the file is empty.
    ///
    /// Lexer diagnostics (unterminated strings, stray characters, …)
    /// are collected but do not fail discovery; they surface during
    /// [`TestRunner::run`] as file-level failures.
    pub fn discover(&self, paths: &[PathBuf]) -> Result<Vec<TestEntry>, TestError> {
        let mut entries = Vec::new();

        for path in paths {
            // Skip directories silently: default_scan_paths() feeds us
            // `tests/` and `src/` and we don't yet do recursive walk.
            if path.is_dir() {
                continue;
            }

            let (source, tokens) = tokenize_file(path)?;
            let content = source.content();
            let file_entries = scan_test_entries(content, &tokens, path);
            entries.extend(file_entries);
        }

        // Apply filter if present.
        if let Some(re) = &self.filter {
            entries.retain(|e| re.is_match(&e.name));
        }

        Ok(entries)
    }

    /// Execute the discovered tests: parse + elaborate each unique file
    /// and mark entries as passed only when zero error diagnostics fire.
    ///
    /// `explicit_paths` should be the user-provided path list. Any path
    /// in that list (that is a file) is parsed even if it contained no
    /// `#[test]` entries — that way a garbage file the user asked
    /// about produces a `FailedFile` and non-zero exit, rather than a
    /// silent `0 tests`.
    ///
    /// Grouping semantics: all entries from the same `source_path` share
    /// a single parse + elaborate pass. The scope for v0.33-M1-007 is
    /// coarse — one error diagnostic fails every entry in that file.
    /// Per-function isolation gates on the runtime evaluator (deferred
    /// to v0.34+).
    pub fn run(&self, explicit_paths: &[PathBuf], entries: &[TestEntry]) -> RunOutcome {
        let mut summary = TestSummary::default();
        summary.discovered = entries.len();

        // Union of unique source paths: (a) files the user asked about
        // by name, (b) files we discovered `#[test]` in.
        let mut all_paths: BTreeSet<PathBuf> = BTreeSet::new();
        for p in explicit_paths {
            if !p.is_dir() {
                all_paths.insert(p.clone());
            }
        }
        for e in entries {
            all_paths.insert(PathBuf::from(&e.source_path));
        }

        let mut failed_files: Vec<FailedFile> = Vec::new();

        for path in &all_paths {
            match run_one_file(path) {
                Ok(()) => {
                    // File parsed + elaborated clean; every entry from
                    // this file passes.
                    for e in entries {
                        if PathBuf::from(&e.source_path) == *path {
                            summary.passed += 1;
                        }
                    }
                }
                Err(messages) => {
                    let entries_in_file = entries
                        .iter()
                        .filter(|e| PathBuf::from(&e.source_path) == *path)
                        .count();
                    summary.failed += entries_in_file;
                    failed_files.push(FailedFile {
                        path: path.clone(),
                        messages,
                    });
                }
            }
        }

        RunOutcome {
            summary,
            failed_files,
        }
    }
}

impl Default for TestRunner {
    fn default() -> Self {
        Self::new()
    }
}

// -----------------------------------------------------------------------
// Discovery helpers
// -----------------------------------------------------------------------

/// Read + validate + tokenize `path`. Never touches the parser.
///
/// Returns the validated `SourceText` (so callers can reach into the
/// content by span) and the collected token vector.
fn tokenize_file(path: &Path) -> Result<(SourceText, Vec<Token>), TestError> {
    let bytes = std::fs::read(path).map_err(|e| TestError::Io {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    // FileId 1 is fine here — we don't render diagnostics from this
    // pass, so cross-file collisions don't matter.
    let file_id = paideia_as_diagnostics::FileId::new(1).expect("FileId(1) valid");

    let source = SourceText::from_bytes(file_id, &bytes).map_err(|diag| TestError::Encoding {
        path: path.to_path_buf(),
        message: diag.message().to_string(),
    })?;

    let mut sink = VecSink::new();
    let mut lexer = Lexer::new(file_id, &source);
    let tokens = lexer.collect_tokens(&mut sink);

    Ok((source, tokens))
}

/// Walk the token stream for `#[test]` attribute patterns.
///
/// A match is: `Hash LBracket Ident("test") RBracket` followed by
/// (optional `KwPub`) then `KwLet` (optional `KwMut`) `Ident(name)`
/// OR `KwFn Ident(name)`. Trivia tokens are already elided by the
/// lexer output for our purposes — the stream contains only "hard"
/// tokens.
fn scan_test_entries(content: &str, tokens: &[Token], path: &Path) -> Vec<TestEntry> {
    let mut out = Vec::new();
    let mut i = 0;

    while i + 3 < tokens.len() {
        // Look for #[test]
        if tokens[i].kind == TokenKind::Hash
            && tokens[i + 1].kind == TokenKind::LBracket
            && tokens[i + 2].kind == TokenKind::Ident
            && span_text(content, &tokens[i + 2]) == "test"
            && tokens[i + 3].kind == TokenKind::RBracket
        {
            // Advance past the attribute and look for the binding.
            let mut j = i + 4;

            // Optional `pub`.
            if j < tokens.len() && tokens[j].kind == TokenKind::KwPub {
                j += 1;
            }

            let name = if j < tokens.len() && tokens[j].kind == TokenKind::KwLet {
                j += 1;
                if j < tokens.len() && tokens[j].kind == TokenKind::KwMut {
                    j += 1;
                }
                if j < tokens.len() && tokens[j].kind == TokenKind::Ident {
                    Some(span_text(content, &tokens[j]).to_string())
                } else {
                    None
                }
            } else if j < tokens.len() && tokens[j].kind == TokenKind::KwFn {
                j += 1;
                if j < tokens.len() && tokens[j].kind == TokenKind::Ident {
                    Some(span_text(content, &tokens[j]).to_string())
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(name) = name {
                out.push(TestEntry {
                    name,
                    source_path: path.display().to_string(),
                });
            }
            // Advance past this attribute regardless of success to avoid
            // re-matching the same `#[test]` on the next iteration.
            i = j.max(i + 4);
            continue;
        }

        i += 1;
    }

    out
}

fn span_text<'s>(content: &'s str, tok: &Token) -> &'s str {
    let start = tok.span.byte_start() as usize;
    let end = tok.span.byte_end() as usize;
    if start <= content.len() && end <= content.len() && start <= end {
        &content[start..end]
    } else {
        ""
    }
}

// -----------------------------------------------------------------------
// Execution helpers
// -----------------------------------------------------------------------

/// Full pipeline for one file: read → validate → lex → parse → elaborate.
///
/// Returns `Ok(())` iff zero error-severity diagnostics fire. On
/// failure, returns human-readable diagnostic lines for the CLI to
/// print. This is the "call the paideia-as parser + elaborator + fail
/// on any diagnostic" contract from the v0.33-M1-007 task.
fn run_one_file(path: &Path) -> Result<(), Vec<String>> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return Err(vec![format!("cannot read {}: {}", path.display(), e)]),
    };

    let mut source_map = SourceMap::new();
    let content_string = String::from_utf8_lossy(&bytes).into_owned();
    let file_id = source_map.add_file(path.to_path_buf(), content_string);

    let mut sink = VecSink::new();

    let source = match SourceText::from_bytes(file_id, &bytes) {
        Ok(s) => s,
        Err(diag) => {
            return Err(vec![format!(
                "encoding error in {}: {}",
                path.display(),
                diag.message()
            )]);
        }
    };

    // Lex. Diagnostics drain into a dedicated sink so we can hand
    // ownership of the token vector to the parser without holding a
    // borrow on the master sink.
    let mut lex_sink = VecSink::new();
    let mut lexer = Lexer::new(file_id, &source);
    let tokens = lexer.collect_tokens(&mut lex_sink);
    for d in lex_sink.into_diagnostics() {
        let _ = sink.emit(d);
    }

    // Parse.
    let mut arena = AstArena::new();
    {
        let mut parser_sink = VecSink::new();
        let mut p = Parser::new(&tokens, source.content(), file_id, &mut arena, &mut parser_sink)
            .with_source_dir(path.parent().map(|p| p.to_path_buf()));
        let _ = p.parse_source_file();
        for d in parser_sink.into_diagnostics() {
            let _ = sink.emit(d);
        }
    }

    // Elaborate (mirror cmd_check: struct + enum registries then lower).
    let registry = build_struct_registry(&arena, &source_map, &mut sink);
    let enum_registry = build_enum_registry(&arena, &source_map, &mut sink);
    let payload_map = std::collections::HashMap::new();
    let _lowering = lower_ast_to_ir(
        &arena,
        &source_map,
        &mut sink,
        &registry,
        &enum_registry,
        &payload_map,
    );

    let diagnostics = sink.into_diagnostics();
    let errors: Vec<&Diagnostic> = diagnostics
        .iter()
        .filter(|d| d.severity() == Severity::Error)
        .collect();

    if errors.is_empty() {
        Ok(())
    } else {
        let messages: Vec<String> = errors
            .iter()
            .map(|d| format!("{}: {}", d.code(), d.message()))
            .collect();
        Err(messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_fixture(dir: &Path, name: &str, contents: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, contents).expect("write fixture");
        path
    }

    // -----------------------------------------------------------------
    // Legacy shape checks (kept for API smoke).
    // -----------------------------------------------------------------

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

    // -----------------------------------------------------------------
    // v0.33-M1-007 acceptance coverage (paideia-as#1393 / paideia-os#1349).
    // -----------------------------------------------------------------

    /// Acceptance 1: nonexistent path fails with a real error (was
    /// silent `discovered: 0`, exit 0).
    #[test]
    fn nonexistent_path_fails_with_diagnostic() {
        let runner = TestRunner::new();
        let missing = PathBuf::from("/tmp/paideia_as_test_does_not_exist_v033.pdx");
        let result = runner.discover(&[missing.clone()]);
        assert!(result.is_err(), "expected Err for nonexistent path");
        match result.unwrap_err() {
            TestError::Io { path, message } => {
                assert_eq!(path, missing);
                assert!(!message.is_empty(), "message should carry OS error text");
            }
            other => panic!("expected TestError::Io, got {:?}", other),
        }
    }

    /// Acceptance 2: a file that fails the parse + elaborate pipeline
    /// surfaces as a `FailedFile` in `RunOutcome` (was silent
    /// `discovered: 0`, exit 0).
    ///
    /// Uses `run` because parser-level garbage still lexes cleanly and
    /// leaves discover Ok — the elaborate step is what catches it.
    #[test]
    fn garbage_file_fails_run_with_diagnostic() {
        let temp_dir = tempfile::tempdir().expect("create tempdir");
        // "let let let" lexes fine (three KwLet tokens) but the parser
        // rejects it — parse_let_decl expects an Ident after `let`.
        let garbage = write_fixture(temp_dir.path(), "garbage.pdx", "let let let\n");

        let runner = TestRunner::new();
        let entries = runner.discover(&[garbage.clone()]).expect("discover ok");
        assert!(entries.is_empty(), "no #[test] in garbage");

        let outcome = runner.run(&[garbage.clone()], &entries);
        assert!(
            !outcome.failed_files.is_empty(),
            "garbage file should show up in failed_files"
        );
        assert_eq!(outcome.failed_files[0].path, garbage);
        assert!(
            !outcome.failed_files[0].messages.is_empty(),
            "failed file should carry at least one diagnostic message"
        );
    }

    /// Acceptance 3: a file with a proper `#[test]` attribute in front
    /// of a function-shaped binding is discovered by the token walk
    /// (not by a substring scan of comments or string literals).
    #[test]
    fn proper_test_attribute_is_discovered_and_counted() {
        let temp_dir = tempfile::tempdir().expect("create tempdir");
        // Two real `#[test]` attributes and a decoy inside a line
        // comment. The substring scanner would count three; the token
        // walk counts two.
        let content = r#"
// This comment mentions #[test] but should not count.
#[test]
let test_addition = 1

#[test]
let test_subtraction = 2

let not_a_test = 3
"#;
        let path = write_fixture(temp_dir.path(), "has_tests.pdx", content);

        let runner = TestRunner::new();
        let entries = runner.discover(&[path.clone()]).expect("discover ok");
        assert_eq!(entries.len(), 2, "expected two real #[test] entries");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"test_addition"), "names={:?}", names);
        assert!(names.contains(&"test_subtraction"), "names={:?}", names);
        for e in &entries {
            assert_eq!(e.source_path, path.display().to_string());
        }
    }

    /// Acceptance 4: a file with no `#[test]` attribute and a clean
    /// parse reports zero entries and produces no failed files (the
    /// baseline "everything OK" flow — should not regress).
    #[test]
    fn clean_file_without_tests_reports_zero_and_ok() {
        let temp_dir = tempfile::tempdir().expect("create tempdir");
        // Minimal well-formed module (mirrors tests/data/hello.pdx).
        let content = "module Hello = structure {\n  let add_one = 1\n}\n";
        let path = write_fixture(temp_dir.path(), "clean.pdx", content);

        let runner = TestRunner::new();
        let entries = runner.discover(&[path.clone()]).expect("discover ok");
        assert!(entries.is_empty(), "expected zero entries: got {:?}", entries);

        let outcome = runner.run(&[path.clone()], &entries);
        assert!(
            outcome.failed_files.is_empty(),
            "clean file should not appear in failed_files: {:?}",
            outcome.failed_files
        );
        assert_eq!(outcome.summary.discovered, 0);
        assert_eq!(outcome.summary.passed, 0);
        assert_eq!(outcome.summary.failed, 0);
    }

    /// The token walk must ignore `#[test]` sequences that live inside
    /// string literals — the pre-v0.33-M1-007 substring scan counted
    /// them.
    #[test]
    fn test_attribute_inside_string_literal_is_not_discovered() {
        let temp_dir = tempfile::tempdir().expect("create tempdir");
        let content = "module M = structure {\n  let s = \"#[test]\"\n}\n";
        let path = write_fixture(temp_dir.path(), "string.pdx", content);

        let runner = TestRunner::new();
        let entries = runner.discover(&[path.clone()]).expect("discover ok");
        assert!(
            entries.is_empty(),
            "#[test] inside a string literal should not be discovered: {:?}",
            entries
        );
    }

    /// Filter still works after the discovery rewrite.
    #[test]
    fn filter_narrows_discovered_entries() {
        let temp_dir = tempfile::tempdir().expect("create tempdir");
        let content = r#"
#[test]
let test_add = 1

#[test]
let check_subtract = 2

#[test]
let test_multiply = 3
"#;
        let path = write_fixture(temp_dir.path(), "filter.pdx", content);

        let runner = TestRunner::new()
            .with_filter("^test_")
            .expect("regex compiles");
        let entries = runner.discover(&[path]).expect("discover ok");
        assert_eq!(entries.len(), 2);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"test_add"));
        assert!(names.contains(&"test_multiply"));
        assert!(!names.contains(&"check_subtract"));
    }

    /// Empty inputs are OK — no files, no entries, no failures.
    #[test]
    fn run_with_no_paths_is_a_no_op() {
        let runner = TestRunner::new();
        let outcome = runner.run(&[], &[]);
        assert!(outcome.failed_files.is_empty());
        assert_eq!(outcome.summary, TestSummary::default());
    }
}
