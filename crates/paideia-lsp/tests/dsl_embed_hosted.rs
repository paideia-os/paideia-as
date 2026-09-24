//! R220.M9 — LSP-embed integration tests for hosted-DSL diagnostics.
//!
//! Four scenarios per issue #1423:
//!   1. Diagnostic round-trip: hosted `E9001` → SARIF JSON → decode
//!      preserves code / message / span.
//!   2. Severity variants: hosted `E9001`, `W9001`, `N9001` all reach
//!      SARIF with the correct SARIF level string.
//!   3. Multi-diagnostic batch: 10 hosted diagnostics land in SARIF in
//!      emission order.
//!   4. Interleaving: native + hosted diagnostics sort deterministically
//!      by primary-span byte-start.
//!
//! Every test uses a **stub** numeric-DSL emitter that pushes payloads
//! directly through the `DslDiagnosticHandle` — R220.M3's real
//! `@dsl_parser` runtime is not yet wired.  The stub proves the pipe
//! works pre-M3.

use paideia_as_diagnostics::{
    Catalog, Category, Diagnostic, DiagnosticCode, FileId, SarifEmitter, Severity, SourceMap, Span,
};
use paideia_as_reflection::{ElabError, ElabWarn};
use paideia_lsp::dsl_embed::{
    DslDiagnosticHandle, hosted_error_code, hosted_note_code, hosted_warn_code,
    interleave_by_span,
};

// ---- stub numeric DSL emitter ----

/// Mimics the future R220.M3 `@dsl_parser` "numeric" DSL body: raises a
/// hosted `E9001` when its input parses to a value outside `[0, 100]`.
fn numeric_dsl_check(input: &str, span: Span, handle: &DslDiagnosticHandle) {
    match input.trim().parse::<i64>() {
        Ok(v) if (0..=100).contains(&v) => {}
        Ok(v) => {
            let err = ElabError {
                message: format!("numeric literal {v} outside [0, 100]"),
                span,
            };
            handle
                .emit_elab_error(&err, hosted_error_code(9001))
                .unwrap();
        }
        Err(_) => {
            let err = ElabError {
                message: format!("`{input}` is not a numeric literal"),
                span,
            };
            handle
                .emit_elab_error(&err, hosted_error_code(9001))
                .unwrap();
        }
    }
}

fn make_source_map(text: &str) -> (SourceMap, FileId) {
    let mut sm = SourceMap::new();
    let file = sm.add_file(std::path::PathBuf::from("dsl_fixture.pdx"), text.into());
    (sm, file)
}

// ---- (1) round-trip ----

#[test]
fn hosted_e9001_round_trips_through_sarif() {
    let text = "999";
    let (sm, file) = make_source_map(text);
    let span = Span::new(file, 0, text.len() as u32);

    let handle = DslDiagnosticHandle::new();
    numeric_dsl_check(text, span, &handle);
    assert_eq!(handle.len(), 1);

    let diags = handle.drain();
    let catalog = Catalog::embedded();
    let emitter = SarifEmitter::new(&sm, catalog);
    let sarif_str = emitter.emit_string(&diags);

    let value: serde_json::Value = serde_json::from_str(&sarif_str).unwrap();
    let result = &value["runs"][0]["results"][0];
    assert_eq!(result["ruleId"], "Z9001");
    assert_eq!(result["level"], "error");
    assert_eq!(result["message"]["text"], "numeric literal 999 outside [0, 100]");
    let region = &result["locations"][0]["physicalLocation"]["region"];
    assert_eq!(region["startLine"], 1);
    assert_eq!(region["startColumn"], 1);
    assert_eq!(region["endColumn"], 4); // "999" is three columns wide (end exclusive).
}

// ---- (2) severity variants ----

#[test]
fn hosted_severity_variants_reach_sarif_correctly() {
    let text = "abc";
    let (sm, file) = make_source_map(text);
    let span = Span::new(file, 0, text.len() as u32);

    let handle = DslDiagnosticHandle::new();
    handle
        .emit_elab_error(
            &ElabError {
                message: "error variant".into(),
                span,
            },
            hosted_error_code(9001),
        )
        .unwrap();
    handle
        .emit_elab_warn(
            &ElabWarn {
                message: "warn variant".into(),
                span,
            },
            hosted_warn_code(9001),
        )
        .unwrap();
    handle
        .emit_note("note variant", span, hosted_note_code(9001))
        .unwrap();

    let diags = handle.drain();
    assert_eq!(diags.len(), 3);

    let catalog = Catalog::embedded();
    let emitter = SarifEmitter::new(&sm, catalog);
    let value = emitter.emit(&diags);
    let results = value["runs"][0]["results"].as_array().unwrap();
    assert_eq!(results.len(), 3);
    // Emission order preserved.
    assert_eq!(results[0]["level"], "error");
    assert_eq!(results[1]["level"], "warning");
    assert_eq!(results[2]["level"], "note");
    // Every rule id is the same Z-category wire form; severity lives outside the wire code.
    for r in results {
        assert_eq!(r["ruleId"], "Z9001");
    }
}

// ---- (3) multi-diagnostic batch ----

#[test]
fn ten_hosted_diagnostics_land_in_sarif_in_order() {
    // Ten distinct spans / values so each result carries its own message.
    let text: String = (0..10)
        .map(|i| format!("{}\n", 200 + i))
        .collect::<String>();
    let (sm, file) = make_source_map(&text);

    let handle = DslDiagnosticHandle::new();
    let mut expected = Vec::new();
    let mut cursor = 0u32;
    for i in 0..10 {
        let val = 200 + i;
        let literal = format!("{val}");
        let span = Span::new(file, cursor, literal.len() as u32);
        cursor += literal.len() as u32 + 1; // account for the '\n' separator.
        numeric_dsl_check(&literal, span, &handle);
        expected.push(format!("numeric literal {val} outside [0, 100]"));
    }
    assert_eq!(handle.len(), 10);

    let diags = handle.drain();
    let catalog = Catalog::embedded();
    let emitter = SarifEmitter::new(&sm, catalog);
    let value = emitter.emit(&diags);
    let results = value["runs"][0]["results"].as_array().unwrap();
    assert_eq!(results.len(), 10);
    for (i, r) in results.iter().enumerate() {
        assert_eq!(r["ruleId"], "Z9001");
        assert_eq!(r["level"], "error");
        assert_eq!(r["message"]["text"], expected[i]);
    }
}

// ---- (4) interleaving ----

#[test]
fn native_and_hosted_diagnostics_interleave_sorted_by_span() {
    let text = "aaaa bbbb cccc dddd";
    let (sm, file) = make_source_map(text);

    // Two native diagnostics at bytes 5 and 15.
    let native = vec![
        Diagnostic::error(DiagnosticCode::new(Category::P, Severity::Error, 100).unwrap())
            .message("native @ 5")
            .with_span(Span::new(file, 5, 4))
            .finish(),
        Diagnostic::error(DiagnosticCode::new(Category::T, Severity::Error, 501).unwrap())
            .message("native @ 15")
            .with_span(Span::new(file, 15, 4))
            .finish(),
    ];

    // Two hosted diagnostics at bytes 0 and 10.
    let handle = DslDiagnosticHandle::new();
    numeric_dsl_check("aaaa", Span::new(file, 0, 4), &handle);
    numeric_dsl_check("cccc", Span::new(file, 10, 4), &handle);
    let hosted = handle.drain();

    let merged = interleave_by_span(&native, &hosted);
    let starts: Vec<u32> = merged
        .iter()
        .map(|d| d.primary_span().unwrap().byte_start())
        .collect();
    assert_eq!(starts, vec![0, 5, 10, 15]);
    // Rule-id shape reflects the source: Z for hosted, P/T for native.
    let cats: Vec<char> = merged
        .iter()
        .map(|d| d.code().category().letter())
        .collect();
    assert_eq!(cats, vec!['Z', 'P', 'Z', 'T']);

    // And the whole merged vector still SARIF-encodes cleanly.
    let catalog = Catalog::embedded();
    let emitter = SarifEmitter::new(&sm, catalog);
    let value = emitter.emit(&merged);
    let results = value["runs"][0]["results"].as_array().unwrap();
    assert_eq!(results.len(), 4);
    assert_eq!(results[0]["ruleId"], "Z9001");
    assert_eq!(results[1]["ruleId"], "P0100");
    assert_eq!(results[2]["ruleId"], "Z9001");
    assert_eq!(results[3]["ruleId"], "T0501");
}
