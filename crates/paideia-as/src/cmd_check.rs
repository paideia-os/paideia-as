//! `paideia-as check` — phase-1 lex + parse + lower pipeline.
//!
//! The full type checker lands in later PRs. For phase-1, `check`:
//!
//! 1. Reads the input `.pdx` file.
//! 2. Validates UTF-8 / BOM via `SourceText::from_bytes`.
//! 3. Tokenizes via the lexer; diagnostics drain into a collector sink.
//! 4. Parses to AST via `Parser::parse_source_file`.
//! 5. Lowers AST → IR via the elaborator's structural lowering.
//! 6. Renders all diagnostics to stderr via `HumanRenderer`.
//! 7. Exits 0 on no errors, 1 on any error-severity diagnostic.
//!
//! The `--dump-ir` flag also pretty-prints the lowered IR arena to
//! stdout. The `--sarif` flag writes SARIF 2.1.0 diagnostic output
//! to the specified file.

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;

use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{
    Catalog, DiagnosticSink, HumanRenderer, HumanSink, Severity, SourceMap, VecSink,
};
use paideia_as_elaborator::{lower_ast_to_ir, build_struct_registry, build_enum_registry};
use paideia_as_lexer::{Lexer, SourceText};
use paideia_as_parser::Parser;

use crate::cmd_common;

/// Run `paideia-as check <input> [--sarif <PATH>]`.
pub fn run(input: &Path, dump_ir: bool, sarif: Option<&Path>) -> ExitCode {
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

    // VecSink collects every diagnostic emitted from the pipeline so we
    // can render them all (human + SARIF) at the end.
    let mut sink = VecSink::new();
    let catalog = Catalog::embedded();

    let source = match SourceText::from_bytes(file, &bytes) {
        Ok(s) => s,
        Err(diag) => {
            let _ = sink.emit(*diag);
            return finish(&source_map, catalog, sink, sarif);
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
        )
        .with_source_dir(input.parent().map(|p| p.to_path_buf()));
        let _ = p.parse_source_file();
        for d in parser_sink.into_diagnostics() {
            let _ = sink.emit(d);
        }
    }

    // PA-r17-010a (#1070): Build struct registry before lowering.
    let registry = build_struct_registry(&arena, &source_map, &mut sink);

    // Phase 7 m4-003 (#1048/#1049): Build enum registry before lowering.
    let enum_registry = build_enum_registry(&arena, &source_map, &mut sink);

    // For cmd_check, we use an empty payload_map since we don't need the nested pattern lowering.
    let payload_map = std::collections::HashMap::new();

    // Lower (structural, diagnostics emitted for @jump_table validation).
    let lowering = lower_ast_to_ir(&arena, &source_map, &mut sink, &registry, &enum_registry, &payload_map);

    if dump_ir {
        let dump = paideia_as_ir::pretty::dump(&lowering.ir);
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(dump.as_bytes());
    }

    finish(&source_map, catalog, sink, sarif)
}

/// Render human diagnostics to stderr, write SARIF if requested, return exit code.
fn finish(
    source_map: &SourceMap,
    catalog: &Catalog,
    sink: VecSink,
    sarif: Option<&Path>,
) -> ExitCode {
    let diagnostics = sink.into_diagnostics();

    // Render human form to stderr.
    let stderr = std::io::stderr();
    let renderer = HumanRenderer::with_catalog(source_map, /*color*/ true, catalog);
    let mut human = HumanSink::new(stderr.lock(), renderer);
    for d in &diagnostics {
        let _ = human.emit(d.clone());
    }

    // Write SARIF if requested.
    if let Some(path) = sarif {
        let _ = cmd_common::write_sarif(source_map, catalog, &diagnostics, path);
    }

    let has_error = diagnostics.iter().any(|d| d.severity() == Severity::Error);

    if has_error {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
