//! `paideia-as build` — phase-1 placeholder backend.
//!
//! Closes deliverable 4 ("smoke-test elaboration"): the pipeline runs
//! lex → parse → lower → placeholder. The real ELF/PAX/PE emitters
//! arrive at deliverable 8. For now we write a tiny
//! `<input>.placeholder` artifact containing a BLAKE3 hash of the
//! lowered IR's pretty-printed form so the smoke test can verify the
//! pipeline produced something deterministic.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{
    Catalog, DiagnosticSink, HumanRenderer, HumanSink, Severity, SourceMap, VecSink,
};
use paideia_as_elaborator::{lower_ast_to_ir, placeholder_for};
use paideia_as_lexer::{Lexer, SourceText};
use paideia_as_parser::Parser;

/// Run `paideia-as build <input>`.
pub fn run(input: &Path) -> ExitCode {
    let bytes = match fs::read(input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("paideia-as: cannot read {}: {e}", input.display());
            return ExitCode::from(2);
        }
    };

    let mut source_map = SourceMap::new();
    let content_string = String::from_utf8_lossy(&bytes).into_owned();
    let file = source_map.add_file(input.to_path_buf(), content_string);

    let mut sink = VecSink::new();
    let catalog = Catalog::embedded();

    let source = match SourceText::from_bytes(file, &bytes) {
        Ok(s) => s,
        Err(diag) => {
            let _ = sink.emit(*diag);
            return finish(&source_map, catalog, sink, None, input);
        }
    };

    // Lex.
    let mut lex_sink = VecSink::new();
    let mut lexer = Lexer::new(file, &source);
    let tokens = lexer.collect_tokens(&mut lex_sink);
    for d in lex_sink.into_diagnostics() {
        let _ = sink.emit(d);
    }

    // Parse.
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

    // If there are any errors so far, do not emit the placeholder.
    let lowering = lower_ast_to_ir(&arena);

    let preview = sink
        .diagnostics()
        .iter()
        .any(|d| d.severity() == Severity::Error);
    let placeholder_to_write = if preview {
        None
    } else {
        Some(placeholder_for(&lowering.ir))
    };

    finish(&source_map, catalog, sink, placeholder_to_write, input)
}

fn finish(
    source_map: &SourceMap,
    catalog: &Catalog,
    sink: VecSink,
    placeholder: Option<String>,
    input: &Path,
) -> ExitCode {
    let diagnostics = sink.into_diagnostics();

    // Human render to stderr.
    let stderr = std::io::stderr();
    let renderer = HumanRenderer::with_catalog(source_map, /*color*/ true, catalog);
    let mut human = HumanSink::new(stderr.lock(), renderer);
    for d in &diagnostics {
        let _ = human.emit(d.clone());
    }

    let has_error = diagnostics.iter().any(|d| d.severity() == Severity::Error);

    if let Some(text) = placeholder
        && !has_error
    {
        let path = placeholder_path_for(input);
        match fs::File::create(&path) {
            Ok(file) => {
                let mut w = std::io::BufWriter::new(file);
                let _ = w.write_all(text.as_bytes());
            }
            Err(e) => {
                eprintln!(
                    "paideia-as: cannot write placeholder at {}: {e}",
                    path.display()
                );
                return ExitCode::from(2);
            }
        }
    }

    if has_error {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// `<dir>/<basename>.placeholder` next to the input file.
fn placeholder_path_for(input: &Path) -> PathBuf {
    let mut p = input.to_path_buf();
    let stem = p
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "input".to_string());
    p.set_file_name(format!("{stem}.placeholder"));
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_path_replaces_extension() {
        let p = Path::new("example.pdx");
        assert_eq!(placeholder_path_for(p), Path::new("example.placeholder"));
    }

    #[test]
    fn placeholder_path_preserves_directory() {
        let p = Path::new("/tmp/foo/example.pdx");
        assert_eq!(
            placeholder_path_for(p),
            Path::new("/tmp/foo/example.placeholder")
        );
    }
}
