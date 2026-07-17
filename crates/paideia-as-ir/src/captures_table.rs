//! Lambda capture analysis side-table — Issue #994.
//!
//! Tracks the analyzed captures for each lambda node during elaboration.
//! The captures are populated by check_linearity.rs after analyze_captures runs,
//! and are consumed by the elaborator to decide whether a lambda should be emitted
//! as a ClosureCons (if captures exist) or as a bare Lambda.

use crate::node::IrNodeId;
use std::collections::HashMap;

/// Describes a single captured binding analyzed for a lambda.
///
/// Mirrors the structure from capture.rs but suitable for IR-level storage.
#[derive(Debug, Clone)]
pub struct AnalyzedCapture {
    /// Binding symbol (from the enclosing scope).
    pub symbol: u32,
    /// How the capture is used: 0=Reference, 1=Value, 2=Consume.
    pub kind: u8,
}

/// Side-table mapping lambda IDs to their analyzed captures.
/// Sparse: only entries for Lambdas that have captures.
#[derive(Debug, Default)]
pub struct CapturesTable {
    entries: HashMap<IrNodeId, Vec<AnalyzedCapture>>,
}

impl CapturesTable {
    /// Construct an empty captures table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Insert captures for a lambda.
    pub fn insert(&mut self, lambda_id: IrNodeId, captures: Vec<AnalyzedCapture>) {
        self.entries.insert(lambda_id, captures);
    }

    /// Retrieve captures for a lambda by its ID.
    #[must_use]
    pub fn get(&self, lambda_id: IrNodeId) -> Option<&[AnalyzedCapture]> {
        self.entries.get(&lambda_id).map(|v| v.as_slice())
    }

    /// Check if a lambda has captures.
    #[must_use]
    pub fn has_captures(&self, lambda_id: IrNodeId) -> bool {
        self.entries.contains_key(&lambda_id) && !self.entries[&lambda_id].is_empty()
    }
}
