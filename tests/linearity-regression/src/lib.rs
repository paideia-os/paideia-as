//! Linearity-regression harness for paideia-as.
//!
//! See `tests/harness.rs` for the test entry point. The harness walks
//! `accept/` and `reject/` subdirectories of this crate and asserts:
//!
//! - Each accept file produces zero `S`-category diagnostics.
//! - Each reject file produces exactly the set of `S`-codes listed in
//!   the companion `<file>.expect` sidecar (one `Sxxxx` code per line).

#![warn(missing_docs)]
#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::path::Path;

use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{Category, DiagnosticSink, SourceMap, VecSink};
use paideia_as_elaborator::lower_ast_to_ir;
use paideia_as_lexer::{Lexer, SourceText};
use paideia_as_parser::Parser;

/// Run the front-end pipeline (lex → parse → lower) on `path` and
/// return the sorted set of `S`-category diagnostic codes it emitted.
///
/// Reading or UTF-8-decoding errors are surfaced as a single
/// pseudo-code `"<read-error>"` in the result so the harness reports
/// them as a failure.
pub fn s_codes_for(path: &Path) -> Result<BTreeSet<String>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut source_map = SourceMap::new();
    let content_string = String::from_utf8_lossy(&bytes).into_owned();
    let file = source_map.add_file(path.to_path_buf(), content_string);

    let mut sink = VecSink::new();
    let source = match SourceText::from_bytes(file, &bytes) {
        Ok(s) => s,
        Err(diag) => {
            let _ = sink.emit(*diag);
            return Ok(collect_s_codes(sink));
        }
    };

    let mut lex_sink = VecSink::new();
    let mut lexer = Lexer::new(file, &source);
    let tokens = lexer.collect_tokens(&mut lex_sink);
    for d in lex_sink.into_diagnostics() {
        let _ = sink.emit(d);
    }

    let mut arena = AstArena::new();
    {
        let mut parser_sink = VecSink::new();
        let mut p = Parser::new(
            &tokens,
            source.content(),
            file,
            &mut arena,
            &mut parser_sink,
        );
        let _ = p.parse_source_file();
        for d in parser_sink.into_diagnostics() {
            let _ = sink.emit(d);
        }
    }
    let _ = lower_ast_to_ir(&arena);

    Ok(collect_s_codes(sink))
}

fn collect_s_codes(sink: VecSink) -> BTreeSet<String> {
    sink.into_diagnostics()
        .into_iter()
        .filter(|d| d.code().category() == Category::S)
        .map(|d| format!("S{:04}", d.code().number()))
        .collect()
}

/// Parse a `.expect` sidecar file: one `Sxxxx` code per line; `#`
/// starts a comment; blank lines are skipped.
pub fn parse_expect_file(content: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in content.lines() {
        let trimmed = match line.split('#').next() {
            Some(s) => s.trim(),
            None => "",
        };
        if !trimmed.is_empty() {
            out.insert(trimmed.to_string());
        }
    }
    out
}
