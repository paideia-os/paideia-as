//! FMT-14 (paideia-as#1578): corpus parser-equivalence harness.
//!
//! Walks every `.pdx` file under the paideia-as workspace and asserts
//!
//! ```ignore
//! parse(src) ≡ parse(format(src))
//! ```
//!
//! where `≡` is structural equivalence in [`paideia_fmt::ast_eq`] —
//! identical [`paideia_as_ast::NodeKind`] at every position, identical
//! non-span payload (atomic ordering, mutability, literal values, …),
//! and identical identifier text at leaf positions. Spans are
//! deliberately ignored because the whole point of the formatter is to
//! shuffle whitespace, which moves every span.
//!
//! Together with FMT-13 (idempotence) this is the second half of the
//! "formatter must not drift semantics" contract: idempotence rules out
//! unbounded churn across saves, and parser-equivalence rules out
//! silent AST reshaping between a source and its re-formatted twin.
//! FMT-15 (round-trip at the token level) closes the triangle.
//!
//! # Running
//!
//! Opt in via `#[ignore]` — a stock `cargo test --package paideia-fmt`
//! stays fast:
//!
//! ```sh
//! cargo test --package paideia-fmt --test corpus_parser_equivalence \
//!     -- --include-ignored --nocapture
//! ```
//!
//! # Corpus discovery
//!
//! Mirrors FMT-13's `tests/corpus_idempotence.rs` deliberately:
//!
//! 1. If `PAIDEIA_FMT_CORPUS` is set, walk *only* that directory.
//! 2. Otherwise, walk `tests/` and `examples/` under the paideia-as
//!    workspace root (located by climbing from `CARGO_MANIFEST_DIR`).
//!
//! # Files not expected to parse are *skipped*
//!
//! The parser-equivalence invariant has no meaning on fixtures the
//! parser rejects, so any file whose *initial* parse fails is
//! classified as [`EquivReport::Skipped`] and ignored. This keeps the
//! ~86-strong reject corpus (`tests/*/corpus/reject/**`, plus named
//! `*_malformed_*.pdx`, `parse_error_*.pdx`, `lex_error.pdx`) out of
//! the gate without an explicit path blocklist — if a reject file ever
//! starts parsing, that's a parser regression surfaced by the dedicated
//! reject-corpus tests, not a formatter concern.
//!
//! A file that parses clean on round #1 but *fails* to parse after
//! formatting is a hard failure (`BrokeParse`) — that's exactly the
//! regression this test exists to catch: the formatter has emitted
//! something the parser no longer accepts.
//!
//! # No formatter-mutable AST fields today
//!
//! The style-spec §7 / §10 passes rewrite whitespace, trailing commas,
//! glyphs, and column alignment — none of which touch any AST field
//! this comparator inspects. The attribute-ordering question (would
//! `#[derive(A, B)]` normalise to `#[derive(B, A)]`?) is explicitly
//! off-limits per style-spec §8.3; FMT-8's `format_attribute.rs`
//! preserves source order. If a future pass ever changes that, this
//! harness will catch it and the pass must either be reverted or
//! documented here as an intentional exception.

use paideia_as_ast::{AstArena, NodeId};
use paideia_as_diagnostics::{DiagnosticSink, FileId, VecSink};
use paideia_as_lexer::SourceText;
use paideia_fmt::ast_eq::ast_structurally_equal;
use paideia_fmt::corpus::walk_pdx;
use paideia_fmt::{format, FormatOptions};
use std::path::{Path, PathBuf};

/// Maximum number of distinct mismatching files whose AST dumps get
/// printed on failure. A single structural regression tends to cascade
/// across the corpus; capping at 10 keeps the terminal readable.
const MAX_DUMP_FILES: usize = 10;

/// Maximum lines of the pre- and post-format AST dumps shown per
/// mismatching file. Matches the ticket's "first 30 lines each" line.
const MAX_DUMP_LINES: usize = 30;

/// Per-file outcome. Only [`EquivReport::Mismatch`] and
/// [`EquivReport::BrokeParse`] fail the sweep.
#[derive(Debug)]
enum EquivReport {
    /// `parse(src) ≡ parse(format(src))` — the formatter preserved the
    /// AST structurally.
    Equal {
        /// File that was checked.
        path: PathBuf,
    },
    /// Source parsed, but `format(src)` ⇒ parse failure. This is the
    /// cardinal regression the harness exists to detect.
    BrokeParse {
        /// File that broke.
        path: PathBuf,
        /// The formatted output the parser rejected.
        formatted: String,
    },
    /// Both parses succeeded but the ASTs differ structurally. The
    /// pretty-printed dumps are retained for the summary printer.
    Mismatch {
        /// File that mismatched.
        path: PathBuf,
        /// Human-readable AST dump of the pre-format parse.
        before: String,
        /// Human-readable AST dump of the post-format parse.
        after: String,
    },
    /// `format` returned an error. Recorded but non-fatal — the FMT-3
    /// contract today is "always `Ok`"; this bucket becomes meaningful
    /// once FMT-9/10 wires real diagnostics through.
    FormatError {
        /// File whose format call returned `Err`.
        path: PathBuf,
        /// `Debug`-rendered error.
        kind: String,
    },
    /// Source did not parse on round #1 — out of scope for this test
    /// (the parser's own reject-corpus tests handle these files).
    Skipped {
        /// File that was skipped.
        path: PathBuf,
    },
}

/// Parse `src` into a fresh arena, returning `None` on any lex or
/// parse failure (or on `SourceText` validation failure — empty input,
/// lone CR, etc.). The returned `(arena, root)` pair is self-contained;
/// callers never need the diagnostic sinks.
fn parse_to_arena(src: &str) -> Option<(AstArena, NodeId)> {
    let file = FileId::new(1)?;
    let source_text = SourceText::from_bytes(file, src.as_bytes()).ok()?;
    let mut lex_sink = VecSink::new();
    let mut lexer = paideia_as_lexer::Lexer::new(file, &source_text);
    let tokens = lexer.collect_tokens(&mut lex_sink);
    if lex_sink.error_count() > 0 {
        return None;
    }
    let mut arena = AstArena::new();
    let mut parse_sink = VecSink::new();
    let root = {
        let mut parser = paideia_as_parser::Parser::new(
            &tokens,
            source_text.content(),
            file,
            &mut arena,
            &mut parse_sink,
        );
        parser.parse_source_file().ok()?
    };
    if parse_sink.error_count() > 0 {
        return None;
    }
    Some((arena, root))
}

/// Check one file. See [`EquivReport`] for the five possible outcomes.
///
/// Returns `Err` only on I/O failure reading the file. Every other
/// classification is folded into the `Ok(EquivReport::*)` variants so
/// a 1,200-file sweep never aborts midway on a single bad file.
fn check_file(path: &Path, opts: &FormatOptions) -> std::io::Result<EquivReport> {
    let src = std::fs::read_to_string(path)?;
    let Some((arena_a, root_a)) = parse_to_arena(&src) else {
        return Ok(EquivReport::Skipped {
            path: path.to_path_buf(),
        });
    };
    let formatted = match format(&src, opts) {
        Ok(s) => s,
        Err(e) => {
            return Ok(EquivReport::FormatError {
                path: path.to_path_buf(),
                kind: format!("{e:?}"),
            });
        }
    };
    let Some((arena_b, root_b)) = parse_to_arena(&formatted) else {
        return Ok(EquivReport::BrokeParse {
            path: path.to_path_buf(),
            formatted,
        });
    };
    if ast_structurally_equal(&src, &arena_a, root_a, &formatted, &arena_b, root_b) {
        Ok(EquivReport::Equal {
            path: path.to_path_buf(),
        })
    } else {
        Ok(EquivReport::Mismatch {
            path: path.to_path_buf(),
            before: dump_ast(&arena_a, root_a),
            after: dump_ast(&arena_b, root_b),
        })
    }
}

/// Pretty-print an AST for the mismatch summary.
///
/// Delegates to [`paideia_as_ast::pretty::print_item`]. For non-item
/// roots (today's parser always returns a [`paideia_as_ast::NodeKind::Structure`]
/// at the top, so this is a defensive path) the pretty-printer falls
/// back to a short "(non-item: n…)" line.
fn dump_ast(arena: &AstArena, root: NodeId) -> String {
    paideia_as_ast::pretty::print_item(arena, root)
}

#[test]
#[ignore = "sweeps ~1,200 .pdx files; opt in with `--include-ignored`"]
fn fmt_corpus_preserves_parse_tree() {
    let dirs = resolve_corpus_dirs();
    assert!(
        !dirs.is_empty(),
        "FMT-14: no corpus directories resolved — check PAIDEIA_FMT_CORPUS \
         or that CARGO_MANIFEST_DIR points inside the paideia-as workspace"
    );

    let mut paths: Vec<PathBuf> = Vec::new();
    for d in &dirs {
        paths.extend(walk_pdx(d));
    }
    paths.sort();
    paths.dedup();

    let opts = FormatOptions::default();

    let mut scanned = 0usize;
    let mut equal = 0usize;
    let mut skipped = 0usize;
    let mut format_errors: Vec<EquivReport> = Vec::new();
    let mut broke_parse: Vec<EquivReport> = Vec::new();
    let mut mismatches: Vec<EquivReport> = Vec::new();

    for path in &paths {
        scanned += 1;
        match check_file(path, &opts) {
            Ok(EquivReport::Equal { .. }) => equal += 1,
            Ok(EquivReport::Skipped { .. }) => skipped += 1,
            Ok(r @ EquivReport::FormatError { .. }) => format_errors.push(r),
            Ok(r @ EquivReport::BrokeParse { .. }) => broke_parse.push(r),
            Ok(r @ EquivReport::Mismatch { .. }) => mismatches.push(r),
            Err(e) => {
                eprintln!("FMT-14: i/o error on {}: {e}", path.display());
            }
        }
    }

    eprintln!("FMT-14 corpus parser-equivalence sweep");
    eprintln!("  roots:         {:?}", dirs);
    eprintln!("  scanned:       {scanned}");
    eprintln!("  equal:         {equal}");
    eprintln!("  skipped:       {skipped} (did not parse pre-format — out of scope)");
    eprintln!("  mismatches:    {}", mismatches.len());
    eprintln!("  broke-parse:   {}", broke_parse.len());
    eprintln!("  format-error:  {}", format_errors.len());

    if !format_errors.is_empty() {
        for r in format_errors.iter().take(5) {
            if let EquivReport::FormatError { path, kind } = r {
                eprintln!("  format-error on {}: {kind}", path.display());
            }
        }
        if format_errors.len() > 5 {
            eprintln!(
                "  … and {} more format-error entries",
                format_errors.len() - 5
            );
        }
    }

    if !broke_parse.is_empty() {
        for r in broke_parse.iter().take(MAX_DUMP_FILES) {
            if let EquivReport::BrokeParse { path, formatted } = r {
                eprintln!("--- formatted output no longer parses: {} ---", path.display());
                for line in formatted.lines().take(MAX_DUMP_LINES) {
                    eprintln!("    {line}");
                }
                if formatted.lines().count() > MAX_DUMP_LINES {
                    eprintln!("    (output truncated at {MAX_DUMP_LINES} lines)");
                }
            }
        }
        if broke_parse.len() > MAX_DUMP_FILES {
            eprintln!(
                "… and {} more broke-parse files (output capped at {})",
                broke_parse.len() - MAX_DUMP_FILES,
                MAX_DUMP_FILES
            );
        }
    }

    if !mismatches.is_empty() {
        for r in mismatches.iter().take(MAX_DUMP_FILES) {
            if let EquivReport::Mismatch {
                path,
                before,
                after,
            } = r
            {
                eprintln!("--- AST mismatch: {} ---", path.display());
                eprintln!("  --- pre-format AST (first {MAX_DUMP_LINES} lines) ---");
                for line in before.lines().take(MAX_DUMP_LINES) {
                    eprintln!("    {line}");
                }
                eprintln!("  --- post-format AST (first {MAX_DUMP_LINES} lines) ---");
                for line in after.lines().take(MAX_DUMP_LINES) {
                    eprintln!("    {line}");
                }
            }
        }
        if mismatches.len() > MAX_DUMP_FILES {
            eprintln!(
                "… and {} more AST mismatches (output capped at {})",
                mismatches.len() - MAX_DUMP_FILES,
                MAX_DUMP_FILES
            );
        }
    }

    let fails = mismatches.len() + broke_parse.len();
    if fails > 0 {
        panic!(
            "FMT-14: {} / {} files failed `parse(src) ≡ parse(format(src))` \
             ({} AST mismatches, {} formatter broke parse)",
            fails,
            scanned,
            mismatches.len(),
            broke_parse.len()
        );
    }
}

/// Resolve corpus roots. See the module header for the resolution rules.
fn resolve_corpus_dirs() -> Vec<PathBuf> {
    resolve_corpus_dirs_with(std::env::var("PAIDEIA_FMT_CORPUS").ok())
}

/// Testable core (explicit env-var arg, no `set_var` races). Mirrors
/// FMT-13's helper to a T so the two sweeps always see the same roots.
fn resolve_corpus_dirs_with(override_dir: Option<String>) -> Vec<PathBuf> {
    if let Some(v) = override_dir {
        return vec![PathBuf::from(v)];
    }
    let root = workspace_root();
    let mut dirs = Vec::new();
    for sub in ["tests", "examples"] {
        let d = root.join(sub);
        if d.is_dir() {
            dirs.push(d);
        }
    }
    dirs
}

/// Climb from `CARGO_MANIFEST_DIR` to the paideia-as workspace root
/// (the directory whose signature is `Cargo.toml` + `crates/`).
/// Bounded at 6 steps so a malformed layout fails fast.
fn workspace_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for _ in 0..6 {
        if dir.join("Cargo.toml").is_file() && dir.join("crates").is_dir() {
            return dir;
        }
        if !dir.pop() {
            break;
        }
    }
    panic!(
        "FMT-14: failed to locate paideia-as workspace root from \
         CARGO_MANIFEST_DIR = {}",
        env!("CARGO_MANIFEST_DIR")
    );
}

// --- Sanity tests that run by default --------------------------------
//
// These exercise the harness itself without taking the `#[ignore]`'d
// full-corpus hit, so a regression in parse_to_arena / check_file
// surfaces at the normal `cargo test` tempo.

#[test]
fn parse_to_arena_round_trips_simple_let() {
    let src = "pub let x : u64 = 1\n";
    let (_arena, _root) = parse_to_arena(src).expect("parse clean");
}

#[test]
fn parse_to_arena_rejects_broken_input() {
    assert!(parse_to_arena("let 123\n").is_none());
}

#[test]
fn check_file_classifies_simple_let_as_equal() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("ok.pdx");
    std::fs::write(&p, "pub let x : u64 = 1\n").unwrap();
    let r = check_file(&p, &FormatOptions::default()).unwrap();
    assert!(
        matches!(r, EquivReport::Equal { .. }),
        "expected Equal, got {r:?}"
    );
}

#[test]
fn check_file_skips_reject_fixture() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("broken.pdx");
    std::fs::write(&p, "let 123\n").unwrap();
    let r = check_file(&p, &FormatOptions::default()).unwrap();
    assert!(
        matches!(r, EquivReport::Skipped { .. }),
        "expected Skipped, got {r:?}"
    );
}

#[test]
fn check_file_handles_multi_item_module() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("mod.pdx");
    std::fs::write(&p, "module M = structure { let x = 1 let y = 2 }\n").unwrap();
    let r = check_file(&p, &FormatOptions::default()).unwrap();
    assert!(
        matches!(r, EquivReport::Equal { .. }),
        "expected Equal, got {r:?}"
    );
}

#[test]
fn resolve_corpus_dirs_defaults_cover_tests_and_examples() {
    let dirs = resolve_corpus_dirs_with(None);
    assert!(
        dirs.iter().any(|d| d.ends_with("tests")),
        "expected tests/ dir in {dirs:?}"
    );
}

#[test]
fn resolve_corpus_dirs_honours_env_override() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = resolve_corpus_dirs_with(Some(tmp.path().display().to_string()));
    assert_eq!(dirs.len(), 1);
    assert_eq!(dirs[0], tmp.path());
}

#[test]
fn workspace_root_lands_on_paideia_as() {
    let root = workspace_root();
    assert!(root.join("Cargo.toml").is_file());
    assert!(root.join("crates").is_dir());
    assert!(root.join("crates/paideia-fmt").is_dir());
}
