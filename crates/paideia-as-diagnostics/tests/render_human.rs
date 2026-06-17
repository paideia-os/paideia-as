use paideia_as_diagnostics::{
    Category, Diagnostic, DiagnosticCode, HumanRenderer, Severity, SourceMap, Span,
};
use std::path::PathBuf;

/// Helper to construct a simple error diagnostic with code E0001.
fn diag_e0001(span: Span, message: &str) -> Diagnostic {
    let code = DiagnosticCode::new(Category::E, Severity::Error, 1).unwrap();
    Diagnostic::error(code)
        .message(message)
        .with_span(span)
        .finish()
}

#[test]
fn synthetic_e0001_color_off() {
    let mut source_map = SourceMap::new();
    let content = "abc xyz\n".to_string();
    let file = source_map.add_file(PathBuf::from("test.pdx"), content);

    // Span covers bytes 4..7, which is "xyz" (the second word).
    let span = Span::new(file, 4, 3);
    let diag = diag_e0001(span, "unexpected token");

    let renderer = HumanRenderer::new(&source_map, false);
    let rendered = renderer.render(&diag);

    insta::assert_snapshot!(rendered);
}

#[test]
fn synthetic_e0001_color_on() {
    let mut source_map = SourceMap::new();
    let content = "abc xyz\n".to_string();
    let file = source_map.add_file(PathBuf::from("test.pdx"), content);

    // Span covers bytes 4..7, which is "xyz".
    let span = Span::new(file, 4, 3);
    let diag = diag_e0001(span, "unexpected token");

    let renderer = HumanRenderer::new(&source_map, true);
    let rendered = renderer.render(&diag);

    insta::assert_snapshot!(rendered);
}

#[test]
fn multibyte_caret_alignment() {
    let mut source_map = SourceMap::new();
    // Content: "a家b cde\n" (9 bytes)
    // 'a' is 1 byte at 0
    // '家' is 3 bytes at 1..4
    // 'b' is 1 byte at 4
    // ' ' is 1 byte at 5
    // 'cde' is at 6..9
    let content = "a家b cde\n".to_string();
    let file = source_map.add_file(PathBuf::from("test.pdx"), content);

    // Span covers bytes 6..9, which is "cde".
    let span = Span::new(file, 6, 3);
    let diag = diag_e0001(span, "bad input");

    let renderer = HumanRenderer::new(&source_map, false);
    let rendered = renderer.render(&diag);

    insta::assert_snapshot!(rendered);
}

#[test]
fn secondary_span_different_line() {
    let mut source_map = SourceMap::new();
    // Content: "alpha\nbeta\ngamma\n" (17 bytes)
    // Line 1: "alpha" (bytes 0..5)
    // '\n' at byte 5
    // Line 2: "beta" (bytes 6..10)
    // '\n' at byte 10
    // Line 3: "gamma" (bytes 11..16)
    // '\n' at byte 16
    let content = "alpha\nbeta\ngamma\n".to_string();
    let file = source_map.add_file(PathBuf::from("test.pdx"), content);

    // Primary span: "beta" (bytes 6..10)
    let primary = Span::new(file, 6, 4);
    // Secondary span: "alpha" (bytes 0..5) with label "defined here"
    let secondary = Span::new(file, 0, 5);

    let code = DiagnosticCode::new(Category::E, Severity::Error, 1).unwrap();
    let diag = Diagnostic::error(code)
        .message("redefinition error")
        .with_span(primary)
        .with_label(secondary, "defined here")
        .finish();

    let renderer = HumanRenderer::new(&source_map, false);
    let rendered = renderer.render(&diag);

    insta::assert_snapshot!(rendered);
}

#[test]
fn no_primary_span_falls_back_to_summary() {
    let source_map = SourceMap::new();

    let code = DiagnosticCode::new(Category::E, Severity::Error, 1).unwrap();
    let diag = Diagnostic::error(code).message("missing primary").finish();

    let renderer = HumanRenderer::new(&source_map, false);
    let rendered = renderer.render(&diag);

    insta::assert_snapshot!(rendered);
}
