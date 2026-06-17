use paideia_as_diagnostics::*;
use std::path::PathBuf;

mod common {
    use jsonschema::JSONSchema;
    use serde_json::Value;

    const SCHEMA_TEXT: &str = include_str!("data/sarif_schema_2_1_0.json");

    /// Validates a SARIF output value against the official schema.
    pub fn validate(value: &Value) -> Result<(), String> {
        let schema: Value = serde_json::from_str(SCHEMA_TEXT).expect("parse vendored schema");
        let compiled = JSONSchema::compile(&schema).expect("compile schema");
        let result = compiled.validate(value);
        if let Err(errors) = result {
            let mut messages = Vec::new();
            for e in errors {
                messages.push(format!("  {} at {}", e, e.instance_path));
            }
            return Err(format!(
                "Schema validation failed:\n{}",
                messages.join("\n")
            ));
        }
        Ok(())
    }
}

#[test]
fn empty_diagnostics_produces_valid_sarif() {
    let source_map = SourceMap::new();
    let catalog = Catalog::embedded();
    let emitter = SarifEmitter::new(&source_map, catalog);

    let sarif = emitter.emit(&[]);

    common::validate(&sarif).expect("SARIF output should be valid");

    let runs = sarif.get("runs").expect("should have runs");
    assert_eq!(runs.as_array().unwrap().len(), 1);

    let run = &runs.as_array().unwrap()[0];
    assert_eq!(
        run.get("results").unwrap().as_array().unwrap().len(),
        0,
        "empty input should produce empty results"
    );
    assert_eq!(
        run.get("artifacts").unwrap().as_array().unwrap().len(),
        0,
        "empty input should produce empty artifacts"
    );

    let rules = run
        .get("tool")
        .unwrap()
        .get("driver")
        .unwrap()
        .get("rules")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(
        rules.len(),
        catalog.len(),
        "rules array should have one entry per catalog entry"
    );
}

#[test]
fn single_e0001_diagnostic() {
    let mut source_map = SourceMap::new();
    let file_id = source_map.add_file(PathBuf::from("test.pdx"), "hello world\n".to_string());

    let catalog = Catalog::embedded();
    let emitter = SarifEmitter::new(&source_map, catalog);

    let code = "E0001".parse::<DiagnosticCode>().unwrap();
    let span = Span::new(file_id, 0, 5); // "hello"
    let diag = Diagnostic::error(code)
        .with_span(span)
        .message("test error".to_string())
        .finish();

    let sarif = emitter.emit(&[diag]);

    common::validate(&sarif).expect("SARIF output should be valid");

    let run = &sarif.get("runs").unwrap().as_array().unwrap()[0];
    let results = run.get("results").unwrap().as_array().unwrap();

    assert_eq!(results.len(), 1);

    let result = &results[0];
    assert_eq!(result.get("ruleId").unwrap().as_str().unwrap(), "E0001");
    assert_eq!(result.get("level").unwrap().as_str().unwrap(), "error");
    assert_eq!(
        result
            .get("message")
            .unwrap()
            .get("text")
            .unwrap()
            .as_str()
            .unwrap(),
        "test error"
    );

    let locations = result.get("locations").unwrap().as_array().unwrap();
    assert_eq!(locations.len(), 1);

    let region = locations[0]
        .get("physicalLocation")
        .unwrap()
        .get("region")
        .unwrap();
    assert_eq!(region.get("startLine").unwrap().as_u64().unwrap(), 1);
    assert_eq!(region.get("startColumn").unwrap().as_u64().unwrap(), 1);
}

#[test]
fn multi_diagnostic_single_run() {
    let mut source_map = SourceMap::new();
    let file1 = source_map.add_file(PathBuf::from("test1.pdx"), "file1 content".to_string());
    let file2 = source_map.add_file(PathBuf::from("test2.pdx"), "file2 content".to_string());
    let file3 = source_map.add_file(PathBuf::from("test3.pdx"), "file3 content".to_string());

    let catalog = Catalog::embedded();
    let emitter = SarifEmitter::new(&source_map, catalog);

    let diag1 = Diagnostic::error("E0001".parse().unwrap())
        .with_span(Span::new(file1, 0, 5))
        .message("error 1".to_string())
        .finish();

    let diag2 = Diagnostic::warning("E0002".parse().unwrap())
        .with_span(Span::new(file2, 0, 5))
        .message("error 2".to_string())
        .finish();

    let diag3 = Diagnostic::note("E0003".parse().unwrap())
        .with_span(Span::new(file3, 0, 5))
        .message("error 3".to_string())
        .finish();

    let sarif = emitter.emit(&[diag1, diag2, diag3]);

    common::validate(&sarif).expect("SARIF output should be valid");

    let run = &sarif.get("runs").unwrap().as_array().unwrap()[0];
    assert_eq!(run.get("results").unwrap().as_array().unwrap().len(), 3);
    assert_eq!(run.get("artifacts").unwrap().as_array().unwrap().len(), 3);

    let rules = run
        .get("tool")
        .unwrap()
        .get("driver")
        .unwrap()
        .get("rules")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(
        rules.len(),
        catalog.len(),
        "should have all catalog codes in rules"
    );
}

#[test]
fn secondary_span_yields_related_location() {
    let mut source_map = SourceMap::new();
    let file_id = source_map.add_file(PathBuf::from("test.pdx"), "abcdefghij".to_string());

    let catalog = Catalog::embedded();
    let emitter = SarifEmitter::new(&source_map, catalog);

    let diag = Diagnostic::error("E0001".parse().unwrap())
        .with_span(Span::new(file_id, 0, 5))
        .message("primary error".to_string())
        .with_label(Span::new(file_id, 5, 2), "related 1".to_string())
        .with_label(Span::new(file_id, 7, 3), "related 2".to_string())
        .finish();

    let sarif = emitter.emit(&[diag]);

    common::validate(&sarif).expect("SARIF output should be valid");

    let run = &sarif.get("runs").unwrap().as_array().unwrap()[0];
    let result = &run.get("results").unwrap().as_array().unwrap()[0];

    let related_locs = result
        .get("relatedLocations")
        .expect("should have relatedLocations")
        .as_array()
        .unwrap();
    assert_eq!(related_locs.len(), 2);

    assert_eq!(
        related_locs[0]
            .get("message")
            .unwrap()
            .get("text")
            .unwrap()
            .as_str()
            .unwrap(),
        "related 1"
    );
    assert_eq!(
        related_locs[1]
            .get("message")
            .unwrap()
            .get("text")
            .unwrap()
            .as_str()
            .unwrap(),
        "related 2"
    );
}

#[test]
fn suggested_fix_serialized() {
    let mut source_map = SourceMap::new();
    let file_id = source_map.add_file(PathBuf::from("test.pdx"), "wrong code here".to_string());

    let catalog = Catalog::embedded();
    let emitter = SarifEmitter::new(&source_map, catalog);

    let diag = Diagnostic::error("E0001".parse().unwrap())
        .with_span(Span::new(file_id, 0, 5))
        .message("bad syntax".to_string())
        .with_suggestion(SuggestedFix {
            span: Span::new(file_id, 0, 5),
            replacement: "correct".to_string(),
            description: "fix the code".to_string(),
        })
        .finish();

    let sarif = emitter.emit(&[diag]);

    common::validate(&sarif).expect("SARIF output should be valid");

    let run = &sarif.get("runs").unwrap().as_array().unwrap()[0];
    let result = &run.get("results").unwrap().as_array().unwrap()[0];

    let fixes = result
        .get("fixes")
        .expect("should have fixes")
        .as_array()
        .unwrap();
    assert_eq!(fixes.len(), 1);

    let fix = &fixes[0];
    assert_eq!(
        fix.get("description")
            .unwrap()
            .get("text")
            .unwrap()
            .as_str()
            .unwrap(),
        "fix the code"
    );

    let artifact_changes = fix.get("artifactChanges").unwrap().as_array().unwrap();
    assert_eq!(artifact_changes.len(), 1);

    let replacements = artifact_changes[0]
        .get("replacements")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(replacements.len(), 1);

    let replacement = &replacements[0];
    let inserted = replacement
        .get("insertedContent")
        .unwrap()
        .get("text")
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(inserted, "correct");
}

#[test]
fn severity_mapping_collapses_hint_and_lint_to_note() {
    let mut source_map = SourceMap::new();
    let file_id = source_map.add_file(PathBuf::from("test.pdx"), "test content".to_string());

    let catalog = Catalog::embedded();
    let emitter = SarifEmitter::new(&source_map, catalog);

    let diag_error = Diagnostic::error("E0001".parse().unwrap())
        .with_span(Span::new(file_id, 0, 4))
        .message("error".to_string())
        .finish();

    let diag_hint = Diagnostic::hint("E0001".parse().unwrap())
        .with_span(Span::new(file_id, 0, 4))
        .message("hint".to_string())
        .finish();

    let diag_lint = Diagnostic::lint("E0001".parse().unwrap())
        .with_span(Span::new(file_id, 0, 4))
        .message("lint".to_string())
        .finish();

    let sarif = emitter.emit(&[diag_error, diag_hint, diag_lint]);

    common::validate(&sarif).expect("SARIF output should be valid");

    let run = &sarif.get("runs").unwrap().as_array().unwrap()[0];
    let results = run.get("results").unwrap().as_array().unwrap();

    assert_eq!(results[0].get("level").unwrap().as_str().unwrap(), "error");
    assert_eq!(results[1].get("level").unwrap().as_str().unwrap(), "note");
    assert_eq!(results[2].get("level").unwrap().as_str().unwrap(), "note");
}

#[test]
fn snapshot_multi_diagnostic() {
    let mut source_map = SourceMap::new();
    let file1 = source_map.add_file(
        PathBuf::from("module.pdx"),
        "def main():\n    x = 42\n".to_string(),
    );
    let file2 = source_map.add_file(
        PathBuf::from("lib.pdx"),
        "def helper():\n    pass\n".to_string(),
    );

    let catalog = Catalog::embedded();
    let emitter = SarifEmitter::new(&source_map, catalog);

    let diag1 = Diagnostic::error("E0001".parse().unwrap())
        .with_span(Span::new(file1, 15, 2))
        .message("invalid UTF-8 sequence".to_string())
        .with_label(Span::new(file1, 15, 2), "bad bytes here".to_string())
        .finish();

    let diag2 = Diagnostic::warning("E0002".parse().unwrap())
        .with_span(Span::new(file2, 5, 4))
        .message("unused variable".to_string())
        .with_suggestion(SuggestedFix {
            span: Span::new(file2, 5, 4),
            replacement: "".to_string(),
            description: "remove unused variable".to_string(),
        })
        .finish();

    let diag3 = Diagnostic::note("E0003".parse().unwrap())
        .with_span(Span::new(file1, 0, 11))
        .message("informational note".to_string())
        .finish();

    let sarif = emitter.emit(&[diag1, diag2, diag3]);

    common::validate(&sarif).expect("SARIF output should be valid");

    insta::assert_snapshot!(serde_json::to_string_pretty(&sarif).unwrap());
}
