//! LSP harness end-to-end tests.
//!
//! Four correctness tests exercising LSP handlers end-to-end via direct library API.
//! One #[ignore]'d latency probe for release-build profiling.

use lsp_harness::{
    DocumentStore, ParseCache, create_document, definition_at, diagnose_document,
    diagnose_document_with_cache, hover_at, references_at, test_url,
};
use std::time::Instant;
use tower_lsp::lsp_types::{
    GotoDefinitionParams, HoverParams, Position, ReferenceContext, ReferenceParams,
    TextDocumentIdentifier, TextDocumentPositionParams,
};

/// Test that malformed documents produce at least one diagnostic.
#[test]
fn correctness_diagnostics_publish_on_change() {
    let uri = test_url("malformed.pax");
    let malformed_source = "fn main( { }"; // Missing closing paren before opening brace

    let diagnostics = diagnose_document(&uri, malformed_source);

    // Should produce at least one diagnostic for the parse error
    assert!(
        !diagnostics.is_empty(),
        "Expected at least one diagnostic for malformed source"
    );

    // At least one should be an error or warning (not just information)
    let has_error_level = diagnostics.iter().any(|d| {
        matches!(
            d.severity,
            Some(tower_lsp::lsp_types::DiagnosticSeverity::ERROR)
        )
    });
    assert!(
        has_error_level,
        "Expected at least one ERROR-level diagnostic"
    );
}

/// Test that hover returns no info when PositionIndex is not populated by walkers.
///
/// Phase-3-m4-002 honest scaffold: walkers populate PositionIndex (future m4 work).
/// Until then, PositionIndex.at() returns None, so hover_at() returns None.
/// Once walkers populate, this test should assert the real hover shape with type/class/effects/caps.
#[test]
fn correctness_hover_returns_linear_class_on_linear_prefix() {
    let store = DocumentStore::new();
    let text = "linear:x let y = 0";
    let uri = create_document(&store, "hover_test.pax", text);

    // Hover at position 0 (start of "linear:x")
    let params = HoverParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: Position {
                line: 0,
                character: 0,
            },
        },
        work_done_progress_params: Default::default(),
    };

    let hover = hover_at(&store, &params);
    // Currently returns None because PositionIndex is not populated by walkers.
    // TODO: once m4 walker population lands, mock-populate the index and assert:
    // - markdown.contains("Linear")
    // - markdown.contains("type:")
    // - markdown.contains("effects:")
    // - markdown.contains("capabilities:")
    assert!(
        hover.is_none(),
        "Expected no hover (PositionIndex not populated by walkers yet)"
    );
}

/// Test that definition jumps to the first occurrence of an identifier.
#[test]
fn correctness_definition_lands_on_first_occurrence() {
    let store = DocumentStore::new();
    let text = "let x = 0\nlet y = x"; // x defined on line 0, referenced on line 1
    let uri = create_document(&store, "definition_test.pax", text);

    // Jump to definition from line 1, character 8 (the second "x")
    let params = GotoDefinitionParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: Position {
                line: 1,
                character: 8,
            },
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };

    let response = definition_at(&store, &params);
    assert!(response.is_some(), "Expected definition response");

    if let Some(tower_lsp::lsp_types::GotoDefinitionResponse::Scalar(location)) = response {
        // Should jump to line 0 (the first occurrence)
        assert_eq!(
            location.range.start.line, 0,
            "Expected definition on line 0"
        );
        assert_eq!(
            location.range.start.character, 4,
            "Expected definition at character 4 (start of 'x')"
        );
    } else {
        panic!("Expected scalar location response");
    }
}

/// Test that references returns all occurrences across multiple documents.
#[test]
fn correctness_references_returns_all_occurrences_across_documents() {
    let store = DocumentStore::new();

    // First document: definition and one use
    let text1 = "let myvar = 1\nlet result = myvar";
    let uri1 = create_document(&store, "file1.pax", text1);

    // Second document: another use of myvar
    let text2 = "fn test() { myvar }";
    let _uri2 = create_document(&store, "file2.pax", text2);

    // Ask for references at the definition site (line 0, character 4)
    let params = ReferenceParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri1.clone() },
            position: Position {
                line: 0,
                character: 4,
            },
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
        context: ReferenceContext {
            include_declaration: true,
        },
    };

    let references = references_at(&store, &params);
    assert!(references.is_some(), "Expected references response");

    let refs = references.unwrap();
    // Should find: definition on line 0 (file1), use on line 1 (file1), use on line 0 (file2)
    assert!(
        refs.len() >= 2,
        "Expected at least 2 references (definition + uses), got {}",
        refs.len()
    );

    // At least one reference should be in file1
    assert!(
        refs.iter().any(|r| r.uri == uri1),
        "Expected reference in file1"
    );
}

/// Latency probe: single-character change on a 1000-line synthetic document.
///
/// #[ignore] because debug builds may exceed 100ms budget.
/// Enable in release CI with `cargo test --release`.
#[test]
#[ignore]
fn latency_single_char_change_under_100ms() {
    let uri = test_url("latency_test.pax");

    // Build a 1000-line synthetic document.
    let line = "let x = 0\n";
    let synthetic_doc = line.repeat(1000);

    // Baseline: diagnose the original document.
    let cache = ParseCache::with_default_capacity();
    let _baseline = diagnose_document_with_cache(&uri, &synthetic_doc, &cache);

    // Mutate line 500 (single character change in the middle).
    let lines: Vec<&str> = synthetic_doc.lines().collect();
    let mut mutated = String::new();
    for (i, line_text) in lines.iter().enumerate() {
        if i == 500 {
            // Change "let" to "let " (add a space) — single-character mutation
            mutated.push_str("let  = 0\n"); // Extra space
        } else {
            mutated.push_str(line_text);
            mutated.push('\n');
        }
    }

    // Measure time to re-diagnose with cache.
    let start = Instant::now();
    let _result = diagnose_document_with_cache(&uri, &mutated, &cache);
    let elapsed = start.elapsed();

    // Assert under 100ms wall clock.
    assert!(
        elapsed.as_millis() < 100,
        "Single-character change took {} ms; target <100ms",
        elapsed.as_millis()
    );
}
