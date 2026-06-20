#![forbid(unsafe_code)]
//! Extended diagnostic format with multi-span reasoning.
//!
//! Provides `ExtendedBorrowDiagnostic` for borrow-check diagnostics that include
//! structured origin and end span information, enabling richer diagnostic UX
//! in both human-readable and SARIF formats.

use serde_json::json;

/// Extended diagnostic for borrow-related issues with origin and end spans.
///
/// Captures borrow-check diagnostic information including the site where the borrow
/// originates (e.g., `&x` or `&mut x`) and where it ends (lexical scope-end or NLL
/// last-use). IR node IDs serve as stable references for correlating diagnostic
/// information with compiler IR.
#[derive(Clone, Debug)]
pub struct ExtendedBorrowDiagnostic {
    /// Diagnostic code (e.g., "S0906", "S0907", "S0908", "S0909").
    pub code: String,
    /// Primary diagnostic message.
    pub primary_message: String,
    /// IR node ID where the borrow originates (the `&x` or `&mut x` site).
    pub borrow_originates_at: Option<u32>,
    /// IR node ID where the borrow ends (lexical scope-end or NLL last-use).
    pub borrow_ends_at: Option<u32>,
}

impl ExtendedBorrowDiagnostic {
    /// Creates a new extended borrow diagnostic.
    ///
    /// # Arguments
    ///
    /// * `code` - The diagnostic code (e.g., "S0906").
    /// * `primary_message` - The main diagnostic message.
    /// * `borrow_originates_at` - Optional IR node ID of the borrow origin.
    /// * `borrow_ends_at` - Optional IR node ID of the borrow end.
    #[must_use]
    pub fn new(
        code: String,
        primary_message: String,
        borrow_originates_at: Option<u32>,
        borrow_ends_at: Option<u32>,
    ) -> Self {
        Self {
            code,
            primary_message,
            borrow_originates_at,
            borrow_ends_at,
        }
    }

    /// Renders the diagnostic in human-readable form with origin and end information.
    ///
    /// Produces a formatted string containing the error code, message, and
    /// IR node references for origin and end spans.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = format!("error[{}]: {}\n", self.code, self.primary_message);
        if let Some(o) = self.borrow_originates_at {
            out.push_str(&format!("  borrow originates at IR node {}\n", o));
        }
        if let Some(e) = self.borrow_ends_at {
            out.push_str(&format!("  borrow ends at IR node {}\n", e));
        }
        out
    }

    /// Renders the diagnostic as a SARIF JSON value.
    ///
    /// Produces a SARIF result object with `relatedLocations` array containing
    /// the origin and end span information, enabling tooling integration.
    #[must_use]
    pub fn render_sarif(&self) -> serde_json::Value {
        let mut related_locations = Vec::new();

        if let Some(origin) = self.borrow_originates_at {
            related_locations.push(json!({
                "physicalLocation": {
                    "artifactLocation": {
                        "uri": "ir-node"
                    },
                    "region": {
                        "startLine": origin
                    }
                },
                "message": {
                    "text": "borrow originates here"
                }
            }));
        }

        if let Some(end) = self.borrow_ends_at {
            related_locations.push(json!({
                "physicalLocation": {
                    "artifactLocation": {
                        "uri": "ir-node"
                    },
                    "region": {
                        "startLine": end
                    }
                },
                "message": {
                    "text": "borrow ends here"
                }
            }));
        }

        json!({
            "ruleId": self.code,
            "message": {
                "text": self.primary_message
            },
            "relatedLocations": related_locations
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_borrow_diagnostic_render_includes_origin_and_end() {
        let diag = ExtendedBorrowDiagnostic::new(
            "S0906".to_string(),
            "cannot borrow as mutable".to_string(),
            Some(42),
            Some(99),
        );

        let rendered = diag.render();
        assert!(rendered.contains("error[S0906]"));
        assert!(rendered.contains("cannot borrow as mutable"));
        assert!(rendered.contains("borrow originates at IR node 42"));
        assert!(rendered.contains("borrow ends at IR node 99"));
    }

    #[test]
    fn extended_borrow_diagnostic_sarif_includes_related_locations() {
        let diag = ExtendedBorrowDiagnostic::new(
            "S0907".to_string(),
            "use of moved value".to_string(),
            Some(10),
            Some(20),
        );

        let sarif = diag.render_sarif();
        assert_eq!(sarif["ruleId"], "S0907");
        assert_eq!(sarif["message"]["text"], "use of moved value");

        let related = &sarif["relatedLocations"];
        assert_eq!(related.as_array().unwrap().len(), 2);

        // Check origin location
        assert_eq!(related[0]["message"]["text"], "borrow originates here");
        assert_eq!(related[0]["physicalLocation"]["region"]["startLine"], 10);

        // Check end location
        assert_eq!(related[1]["message"]["text"], "borrow ends here");
        assert_eq!(related[1]["physicalLocation"]["region"]["startLine"], 20);
    }

    #[test]
    fn extended_borrow_diagnostic_render_handles_missing_spans() {
        let diag = ExtendedBorrowDiagnostic::new(
            "S0908".to_string(),
            "borrow checker error".to_string(),
            None,
            None,
        );

        let rendered = diag.render();
        assert!(rendered.contains("error[S0908]"));
        assert!(rendered.contains("borrow checker error"));
        assert!(!rendered.contains("borrow originates"));
        assert!(!rendered.contains("borrow ends"));

        let sarif = diag.render_sarif();
        assert_eq!(sarif["relatedLocations"].as_array().unwrap().len(), 0);
    }
}
