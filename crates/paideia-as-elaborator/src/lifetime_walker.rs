//! LifetimeWalker — rejects borrows that outlive their source.
//!
//! For each borrow site, check that the borrow's region outlives the
//! source binding's region. If not, fire S0908 "borrowed value does
//! not live long enough".

use paideia_as_types::{RegionGraph, RegionId};

/// Checks that borrows do not outlive their source bindings.
///
/// The lifetime checker enforces that a borrowed value lives at least
/// as long as the borrow. If the source goes out of scope before the
/// borrow does, we reject with S0908.
#[derive(Debug)]
pub struct LifetimeWalker<'a> {
    graph: &'a RegionGraph,
    diagnostics: Vec<String>,
}

impl<'a> LifetimeWalker<'a> {
    /// Create a new lifetime walker over the given region graph.
    #[must_use]
    pub fn new(graph: &'a RegionGraph) -> Self {
        Self {
            graph,
            diagnostics: Vec::new(),
        }
    }

    /// Check that a borrow's region is outlived by the source's region.
    ///
    /// The source must outlive the borrow: `source_region` ⊇ `borrow_region`.
    /// Returns `Ok(())` or `Err(S0908 message)`.
    pub fn check_borrow(
        &mut self,
        borrow_region: RegionId,
        source_region: RegionId,
    ) -> Result<(), String> {
        // The source must outlive the borrow: source_region ⊇ borrow_region.
        if self.graph.outlives(source_region, borrow_region) {
            Ok(())
        } else {
            let msg = s0908_message(source_region, borrow_region);
            self.diagnostics.push(msg.clone());
            Err(msg)
        }
    }

    /// Returns all accumulated diagnostic messages.
    #[must_use]
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }
}

/// Constructs the S0908 diagnostic message.
fn s0908_message(source: RegionId, borrow: RegionId) -> String {
    format!(
        "S0908: borrowed value does not live long enough (source region {} does not outlive borrow region {})",
        source.0, borrow.0
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifetime_walker_passes_when_source_outlives_borrow() {
        let mut graph = RegionGraph::new();
        let source = RegionId(1);
        let borrow = RegionId(2);
        graph.add_outlives(source, borrow);

        let mut walker = LifetimeWalker::new(&graph);
        assert!(walker.check_borrow(borrow, source).is_ok());
        assert_eq!(walker.diagnostics().len(), 0);
    }

    #[test]
    fn lifetime_walker_fires_s0908_when_source_shorter() {
        let graph = RegionGraph::new();
        let source = RegionId(1);
        let borrow = RegionId(2);
        // Note: no outlives relationship added, so source does not outlive borrow.

        let mut walker = LifetimeWalker::new(&graph);
        let result = walker.check_borrow(borrow, source);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("S0908"));
        assert_eq!(walker.diagnostics().len(), 1);
    }

    #[test]
    fn lifetime_walker_passes_when_same_region() {
        let graph = RegionGraph::new();
        let region = RegionId(1);

        let mut walker = LifetimeWalker::new(&graph);
        // A region always outlives itself.
        assert!(walker.check_borrow(region, region).is_ok());
        assert_eq!(walker.diagnostics().len(), 0);
    }

    #[test]
    fn lifetime_walker_passes_when_source_is_static() {
        let graph = RegionGraph::new();
        let static_region = RegionId::STATIC;
        let borrow = RegionId(1);

        let mut walker = LifetimeWalker::new(&graph);
        // 'static outlives everything.
        assert!(walker.check_borrow(borrow, static_region).is_ok());
        assert_eq!(walker.diagnostics().len(), 0);
    }
}
