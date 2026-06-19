//! LSP diagnostic adaptation: paideia-as-diagnostics → tower-lsp Diagnostic.

use tower_lsp::lsp_types::{Diagnostic as LspDiagnostic, DiagnosticSeverity, Position, Range, Url};

use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{
    Diagnostic as PaideiaDiagnostic, DiagnosticSink, Severity, SourceMap, VecSink,
};
use paideia_as_lexer::{Lexer, SourceText};
use paideia_as_parser::Parser;

/// Convert a paideia-as Diagnostic to an LSP Diagnostic.
pub fn to_lsp_diagnostic(d: &PaideiaDiagnostic, source_text: &str) -> LspDiagnostic {
    let span = match d.primary_span() {
        Some(s) => s,
        None => {
            // If no primary span, create a zero-width range at the start
            return LspDiagnostic {
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 0,
                    },
                },
                severity: Some(severity_to_lsp(d.code().severity())),
                code: Some(tower_lsp::lsp_types::NumberOrString::String(
                    d.code().to_string(),
                )),
                source: Some("paideia-as".to_string()),
                message: d.message().to_string(),
                ..Default::default()
            };
        }
    };

    let start = byte_offset_to_position(source_text, span.byte_start() as usize);
    let end = byte_offset_to_position(source_text, span.byte_end() as usize);

    LspDiagnostic {
        range: Range { start, end },
        severity: Some(severity_to_lsp(d.code().severity())),
        code: Some(tower_lsp::lsp_types::NumberOrString::String(
            d.code().to_string(),
        )),
        source: Some("paideia-as".to_string()),
        message: d.message().to_string(),
        ..Default::default()
    }
}

fn severity_to_lsp(s: Severity) -> DiagnosticSeverity {
    match s {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Note => DiagnosticSeverity::INFORMATION,
        Severity::Hint => DiagnosticSeverity::HINT,
        Severity::Lint => DiagnosticSeverity::HINT,
        _ => DiagnosticSeverity::INFORMATION,
    }
}

fn byte_offset_to_position(text: &str, byte_offset: usize) -> Position {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in text.char_indices() {
        if i >= byte_offset {
            return Position {
                line,
                character: col,
            };
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    Position {
        line,
        character: col,
    }
}

/// Re-parse and re-elaborate a document, returning the list of LSP diagnostics.
///
/// Phase-2-m8-004: drives paideia-as-parser on the in-memory text;
/// collects diagnostics; converts them.
pub fn diagnose_document(uri: &Url, source: &str) -> Vec<LspDiagnostic> {
    // Create a temporary source map and file ID for this document.
    let mut source_map = SourceMap::new();
    let path = uri
        .to_file_path()
        .unwrap_or_else(|_| std::path::PathBuf::from("unknown"));
    let file = source_map.add_file(path, source.to_string());

    let mut sink = VecSink::new();

    // Lex the source.
    let source_text = match SourceText::from_bytes(file, source.as_bytes()) {
        Ok(s) => s,
        Err(diag) => {
            let _ = sink.emit(*diag);
            return sink
                .into_diagnostics()
                .iter()
                .map(|d| to_lsp_diagnostic(d, source))
                .collect();
        }
    };

    let mut lex_sink = VecSink::new();
    let mut lexer = Lexer::new(file, &source_text);
    let tokens = lexer.collect_tokens(&mut lex_sink);
    for d in lex_sink.into_diagnostics() {
        let _ = sink.emit(d);
    }

    // Parse the tokens.
    let mut arena = AstArena::new();
    {
        let mut parser_sink = VecSink::new();
        let mut p = Parser::new(
            &tokens,
            source_text.content(),
            file,
            &mut arena,
            &mut parser_sink,
        );
        let _ = p.parse_source_file();
        for d in parser_sink.into_diagnostics() {
            let _ = sink.emit(d);
        }
    }

    // Convert all collected diagnostics to LSP form.
    sink.into_diagnostics()
        .iter()
        .map(|d| to_lsp_diagnostic(d, source))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_offset_to_position_handles_first_line() {
        let text = "abc";
        assert_eq!(
            byte_offset_to_position(text, 0),
            Position {
                line: 0,
                character: 0
            }
        );
        assert_eq!(
            byte_offset_to_position(text, 1),
            Position {
                line: 0,
                character: 1
            }
        );
        assert_eq!(
            byte_offset_to_position(text, 2),
            Position {
                line: 0,
                character: 2
            }
        );
        assert_eq!(
            byte_offset_to_position(text, 3),
            Position {
                line: 0,
                character: 3
            }
        );
    }

    #[test]
    fn byte_offset_to_position_handles_multi_line() {
        let text = "abc\ndef\nghi";
        // First line
        assert_eq!(
            byte_offset_to_position(text, 0),
            Position {
                line: 0,
                character: 0
            }
        );
        assert_eq!(
            byte_offset_to_position(text, 3),
            Position {
                line: 0,
                character: 3
            }
        );
        // Second line
        assert_eq!(
            byte_offset_to_position(text, 4),
            Position {
                line: 1,
                character: 0
            }
        );
        assert_eq!(
            byte_offset_to_position(text, 6),
            Position {
                line: 1,
                character: 2
            }
        );
        // Third line
        assert_eq!(
            byte_offset_to_position(text, 8),
            Position {
                line: 2,
                character: 0
            }
        );
        assert_eq!(
            byte_offset_to_position(text, 10),
            Position {
                line: 2,
                character: 2
            }
        );
    }

    #[test]
    fn severity_to_lsp_maps_error_correctly() {
        assert_eq!(severity_to_lsp(Severity::Error), DiagnosticSeverity::ERROR);
    }

    #[test]
    fn severity_to_lsp_maps_warning_correctly() {
        assert_eq!(
            severity_to_lsp(Severity::Warning),
            DiagnosticSeverity::WARNING
        );
    }

    #[test]
    fn diagnose_document_with_parse_error_produces_p_category() {
        // Feed source with unclosed parenthesis to trigger parse error.
        let uri = Url::parse("file:///test.pdx").unwrap();
        let source = "( ) (";
        let diagnostics = diagnose_document(&uri, source);

        // Should have at least one diagnostic with a P-category code.
        assert!(!diagnostics.is_empty(), "expected at least one diagnostic");
        let has_p_category = diagnostics.iter().any(|d| {
            if let Some(tower_lsp::lsp_types::NumberOrString::String(code)) = &d.code {
                code.starts_with('P')
            } else {
                false
            }
        });
        assert!(
            has_p_category,
            "expected at least one P-category diagnostic"
        );
    }

    #[test]
    fn diagnose_document_with_well_formed_input_produces_no_diagnostics() {
        // A valid, simple input.
        let uri = Url::parse("file:///test.pdx").unwrap();
        let source = "module M = structure { }";
        let diagnostics = diagnose_document(&uri, source);

        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for well-formed input, but got: {:?}",
            diagnostics
        );
    }
}
