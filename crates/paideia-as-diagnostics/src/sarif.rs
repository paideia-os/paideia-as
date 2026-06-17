//! SARIF 2.1.0 emitter for diagnostic output.
//!
//! Emits static analysis results in SARIF (Static Analysis Results Format) v2.1.0.
//! See https://sarifweb.azurewebsites.net/ for the specification.

use crate::{Catalog, Diagnostic, Severity, SourceMap, Span};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

/// Emits diagnostics in SARIF 2.1.0 format.
///
/// Combines diagnostic information with the source map and catalog metadata
/// to produce a complete SARIF output suitable for integration with
/// static analysis workflows.
pub struct SarifEmitter<'a> {
    source_map: &'a SourceMap,
    catalog: &'a Catalog,
}

impl<'a> SarifEmitter<'a> {
    /// Creates a new SARIF emitter.
    ///
    /// # Arguments
    ///
    /// * `source_map` - The source map used to look up file paths and locations.
    /// * `catalog` - The diagnostic catalog for rule metadata.
    pub fn new(source_map: &'a SourceMap, catalog: &'a Catalog) -> Self {
        Self {
            source_map,
            catalog,
        }
    }

    /// Emits diagnostics as a JSON value.
    ///
    /// Returns a `serde_json::Value` in SARIF 2.1.0 format. This is the canonical
    /// JSON representation, which can be serialized or further processed.
    pub fn emit(&self, diagnostics: &[Diagnostic]) -> Value {
        let rules = self.build_rules();
        let results = self.build_results(diagnostics);
        let artifacts = self.build_artifacts(diagnostics);

        json!({
            "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
            "version": "2.1.0",
            "runs": [{
                "tool": {
                    "driver": {
                        "name": "paideia-as",
                        "version": env!("CARGO_PKG_VERSION"),
                        "semanticVersion": env!("CARGO_PKG_VERSION"),
                        "informationUri": "https://paideia-os.org/",
                        "rules": rules,
                    }
                },
                "results": results,
                "artifacts": artifacts,
            }]
        })
    }

    /// Emits diagnostics as a formatted JSON string.
    ///
    /// Produces pretty-printed JSON suitable for human inspection or output.
    pub fn emit_string(&self, diagnostics: &[Diagnostic]) -> String {
        serde_json::to_string_pretty(&self.emit(diagnostics)).unwrap_or_else(|_| "{}".to_string())
    }

    /// Maps a diagnostic severity level to a SARIF level string.
    fn severity_to_sarif_level(severity: Severity) -> &'static str {
        match severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
            Severity::Hint => "note",
            Severity::Lint => "note",
        }
    }

    /// Builds the rules array from the catalog.
    fn build_rules(&self) -> Vec<Value> {
        self.catalog
            .iter()
            .map(|(code, entry)| {
                let mut rule = Map::new();
                rule.insert("id".to_string(), json!(code.to_string()));
                rule.insert("name".to_string(), json!(entry.title));
                rule.insert(
                    "shortDescription".to_string(),
                    json!({ "text": entry.brief }),
                );

                if !entry.description.is_empty() {
                    rule.insert(
                        "fullDescription".to_string(),
                        json!({ "text": entry.description }),
                    );
                }

                rule.insert(
                    "defaultConfiguration".to_string(),
                    json!({
                        "level": Self::severity_to_sarif_level(entry.severity.into())
                    }),
                );

                rule.insert(
                    "helpUri".to_string(),
                    json!(format!("https://paideia-os.org/diagnostics/{}", code)),
                );

                Value::Object(rule)
            })
            .collect()
    }

    /// Builds the results array from diagnostics.
    fn build_results(&self, diagnostics: &[Diagnostic]) -> Vec<Value> {
        diagnostics
            .iter()
            .map(|diag| self.build_result(diag))
            .collect()
    }

    /// Builds a single result object for a diagnostic.
    fn build_result(&self, diag: &Diagnostic) -> Value {
        let mut result = Map::new();

        result.insert("ruleId".to_string(), json!(diag.code().to_string()));
        result.insert(
            "level".to_string(),
            json!(Self::severity_to_sarif_level(diag.severity())),
        );
        result.insert("message".to_string(), json!({ "text": diag.message() }));

        if let Some(primary_span) = diag.primary_span() {
            result.insert(
                "locations".to_string(),
                json!(vec![self.build_location(primary_span, None)]),
            );
        }

        if !diag.secondary_spans().is_empty() {
            result.insert(
                "relatedLocations".to_string(),
                json!(
                    diag.secondary_spans()
                        .iter()
                        .map(|sec| self.build_location(sec.span, Some(&sec.label)))
                        .collect::<Vec<_>>()
                ),
            );
        }

        if !diag.suggestions().is_empty() {
            result.insert(
                "fixes".to_string(),
                json!(
                    diag.suggestions()
                        .iter()
                        .map(|fix| self.build_fix(fix))
                        .collect::<Vec<_>>()
                ),
            );
        }

        Value::Object(result)
    }

    /// Builds a location object for a span.
    fn build_location(&self, span: Span, label: Option<&str>) -> Value {
        let file_id = span.file();
        let path = self.source_map.path(file_id);

        let start_line_col = self
            .source_map
            .byte_to_line_col(file_id, span.byte_start())
            .unwrap_or_else(|| crate::LineCol::new(1, 1));

        let end_line_col = self
            .source_map
            .byte_to_line_col(file_id, span.byte_start() + span.byte_len())
            .unwrap_or(start_line_col);

        let mut location = Map::new();
        location.insert(
            "physicalLocation".to_string(),
            json!({
                "artifactLocation": { "uri": path.to_string_lossy() },
                "region": {
                    "startLine": start_line_col.line,
                    "startColumn": start_line_col.col,
                    "endLine": end_line_col.line,
                    "endColumn": end_line_col.col,
                }
            }),
        );

        if let Some(lbl) = label {
            location.insert("message".to_string(), json!({ "text": lbl }));
        }

        Value::Object(location)
    }

    /// Builds a fix object from a suggested fix.
    fn build_fix(&self, fix: &crate::SuggestedFix) -> Value {
        let span = fix.span;
        let file_id = span.file();

        let start_line_col = self
            .source_map
            .byte_to_line_col(file_id, span.byte_start())
            .unwrap_or_else(|| crate::LineCol::new(1, 1));

        let end_line_col = self
            .source_map
            .byte_to_line_col(file_id, span.byte_start() + span.byte_len())
            .unwrap_or(start_line_col);

        let path = self.source_map.path(file_id);

        json!({
            "description": { "text": fix.description },
            "artifactChanges": [{
                "artifactLocation": { "uri": path.to_string_lossy() },
                "replacements": [{
                    "deletedRegion": {
                        "startLine": start_line_col.line,
                        "startColumn": start_line_col.col,
                        "endLine": end_line_col.line,
                        "endColumn": end_line_col.col,
                    },
                    "insertedContent": { "text": fix.replacement },
                }]
            }]
        })
    }

    /// Builds the artifacts array from all spans in diagnostics.
    fn build_artifacts(&self, diagnostics: &[Diagnostic]) -> Vec<Value> {
        let mut files = BTreeSet::new();

        for diag in diagnostics {
            if let Some(span) = diag.primary_span() {
                files.insert(span.file());
            }
            for sec in diag.secondary_spans() {
                files.insert(sec.span.file());
            }
            for fix in diag.suggestions() {
                files.insert(fix.span.file());
            }
        }

        files
            .iter()
            .map(|file_id| {
                let path = self.source_map.path(*file_id);
                let content = self.source_map.content(*file_id);
                json!({
                    "location": { "uri": path.to_string_lossy() },
                    "encoding": "UTF-8",
                    "length": content.len(),
                })
            })
            .collect()
    }
}
