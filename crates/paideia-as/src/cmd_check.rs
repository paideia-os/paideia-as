//! `paideia-as check` — phase-1 lex + parse + lower pipeline.
//!
//! The full type checker lands in later PRs. For phase-1, `check`:
//!
//! 1. Reads the input `.pdx` file.
//! 2. Validates UTF-8 / BOM via `SourceText::from_bytes`.
//! 3. Tokenizes via the lexer; diagnostics drain into a collector sink.
//! 4. Parses to AST via `Parser::parse_source_file`.
//! 5. Lowers AST → IR via the elaborator's structural lowering.
//! 6. Writes a SARIF sidecar at `<input>.sarif.json`.
//! 7. Renders all diagnostics to stderr via `HumanRenderer`.
//! 8. Exits 0 on no errors, 1 on any error-severity diagnostic.
//!
//! The `--dump-ir` flag also pretty-prints the lowered IR arena to
//! stdout.

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;

use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{
    Catalog, DiagnosticSink, HumanRenderer, HumanSink, SarifEmitter, Severity, SourceMap, VecSink,
};
use paideia_as_elaborator::lower_ast_to_ir;
use paideia_as_lexer::{Lexer, SourceText};
use paideia_as_parser::Parser;

/// Run `paideia-as check <input>`.
pub fn run(input: &Path, dump_ir: bool) -> ExitCode {
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
            return finish(&source_map, catalog, sink, input, false, false);
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

    // Lower (structural, no diagnostics emitted by the lowerer in phase-1).
    let lowering = lower_ast_to_ir(&arena);

    if dump_ir {
        let dump = paideia_as_ir::pretty::dump(&lowering.ir);
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(dump.as_bytes());
    }

    finish(&source_map, catalog, sink, input, true, true)
}

/// Render human diagnostics to stderr, write SARIF sidecar, return exit code.
fn finish(
    source_map: &SourceMap,
    catalog: &Catalog,
    sink: VecSink,
    input: &Path,
    write_sarif: bool,
    _phase_complete: bool,
) -> ExitCode {
    let diagnostics = sink.into_diagnostics();

    // Render human form to stderr.
    let stderr = std::io::stderr();
    let renderer = HumanRenderer::with_catalog(source_map, /*color*/ true, catalog);
    let mut human = HumanSink::new(stderr.lock(), renderer);
    for d in &diagnostics {
        let _ = human.emit(d.clone());
    }

    // Write SARIF sidecar.
    if write_sarif {
        let sarif_path = sarif_path_for(input);
        if let Ok(file) = fs::File::create(&sarif_path) {
            let emitter = SarifEmitter::new(source_map, catalog);
            let json = emitter.emit_string(&diagnostics);
            let mut writer = std::io::BufWriter::new(file);
            let _ = writer.write_all(json.as_bytes());
        }
    }

    let has_error = diagnostics.iter().any(|d| d.severity() == Severity::Error);

    if has_error {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Compute `<input>.sarif.json` next to the input file.
fn sarif_path_for(input: &Path) -> std::path::PathBuf {
    let mut p = input.to_path_buf();
    let mut name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "input".to_string());
    name.push_str(".sarif.json");
    p.set_file_name(name);
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sarif_path_adds_suffix() {
        let p = Path::new("example.pdx");
        assert_eq!(sarif_path_for(p), Path::new("example.pdx.sarif.json"));
    }

    #[test]
    fn sarif_path_with_directory() {
        let p = Path::new("/tmp/foo/example.pdx");
        assert_eq!(
            sarif_path_for(p),
            Path::new("/tmp/foo/example.pdx.sarif.json")
        );
    }
}
