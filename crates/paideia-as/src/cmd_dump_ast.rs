//! `paideia-as dump-ast <file>` — lex + parse + pretty-print the AST.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{
    DiagnosticSink, HumanRenderer, HumanSink, SourceMap, VecSink,
};
use paideia_as_lexer::{Lexer, SourceText};
use paideia_as_parser::Parser;

/// Run `paideia-as dump-ast <input>`.
///
/// Returns an `ExitCode` so the CLI can propagate non-zero on errors.
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

    let source = match SourceText::from_bytes(file, &bytes) {
        Ok(s) => s,
        Err(diag) => {
            let renderer = HumanRenderer::new(&source_map, crate::color::should_use_color());
            eprint!("{}", renderer.render(&diag));
            return ExitCode::from(1);
        }
    };

    let mut sink = HumanSink::new(std::io::stderr(), HumanRenderer::new(&source_map, crate::color::should_use_color()));

    // Lex.
    let mut lex_sink = VecSink::new();
    let mut lexer = Lexer::new(file, &source);
    let tokens = lexer.collect_tokens(&mut lex_sink);
    for d in lex_sink.into_diagnostics() {
        let _ = sink.emit(d);
    }

    // Parse (real parser, mirrors cmd_check.rs).
    let mut arena = AstArena::new();
    {
        let mut parser_sink = VecSink::new();
        let mut p = Parser::new(
            &tokens,
            source.content(),
            file,
            &mut arena,
            &mut parser_sink,
        )
        .with_source_dir(input.parent().map(|p| p.to_path_buf()));
        let _ = p.parse_source_file();
        for d in parser_sink.into_diagnostics() {
            let _ = sink.emit(d);
        }
    }

    let dump = paideia_as_ast::pretty::dump_arena(&arena);
    println!("{dump}");

    ExitCode::SUCCESS
}
