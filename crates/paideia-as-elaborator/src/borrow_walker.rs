//! BorrowWalker — tracks active borrows per binding for the m6 borrow checker.
//!
//! Fires S0906 (was spec'd A0700): "cannot borrow X as mutable because it is also borrowed as immutable".
//! Fires S0907 (was spec'd A0701): "cannot borrow X as mutable more than once".

use std::collections::HashMap;

/// Classification of a borrow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BorrowKind {
    /// An immutable borrow, allowing multiple simultaneous borrows of the same binding.
    Immutable,
    /// A mutable borrow, requiring exclusive access (no other borrows allowed).
    Mutable,
}

/// Tracks active borrows per binding for borrow checking.
///
/// The borrow checker enforces that a binding can be borrowed either:
/// - Immutably (any number of times simultaneously)
/// - Mutably (at most once, and no immutable borrows)
///
/// This walker accumulates diagnostics (S0906, S0907) as conflicts are detected.
#[derive(Default, Debug)]
pub struct BorrowWalker {
    /// Map from binding ID to list of active borrows (kind, region ID).
    active: HashMap<u32, Vec<(BorrowKind, u32)>>,
    /// Accumulated diagnostic messages.
    diagnostics: Vec<String>,
}

impl BorrowWalker {
    /// Creates a new, empty BorrowWalker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Attempts to record a new immutable borrow of `binding` in region `region`.
    ///
    /// Returns `Ok(())` if successful, or `Err(S0906 diagnostic)` if there is an active
    /// mutable borrow on this binding.
    ///
    /// Multiple immutable borrows on the same binding are allowed.
    pub fn borrow_immutable(&mut self, binding: u32, region: u32) -> Result<(), String> {
        let actives = self.active.entry(binding).or_default();

        // Check if there is an active mutable borrow.
        if actives
            .iter()
            .any(|(k, _)| matches!(k, BorrowKind::Mutable))
        {
            let msg = s0906_message(binding);
            self.diagnostics.push(msg.clone());
            return Err(msg);
        }

        actives.push((BorrowKind::Immutable, region));
        Ok(())
    }

    /// Attempts to record a new mutable borrow of `binding` in region `region`.
    ///
    /// Returns `Ok(())` if successful, or `Err(...)` if there is any active borrow
    /// on this binding:
    /// - `Err(S0906 diagnostic)` if there are active immutable borrows
    /// - `Err(S0907 diagnostic)` if there is an active mutable borrow
    ///
    /// At most one mutable borrow can be active for a binding at any time.
    pub fn borrow_mutable(&mut self, binding: u32, region: u32) -> Result<(), String> {
        let actives = self.active.entry(binding).or_default();

        if !actives.is_empty() {
            // Determine which conflict message to emit.
            if actives
                .iter()
                .any(|(k, _)| matches!(k, BorrowKind::Mutable))
            {
                let msg = s0907_message(binding);
                self.diagnostics.push(msg.clone());
                return Err(msg);
            } else {
                // Active immutable borrows
                let msg = s0906_message(binding);
                self.diagnostics.push(msg.clone());
                return Err(msg);
            }
        }

        actives.push((BorrowKind::Mutable, region));
        Ok(())
    }

    /// Drops all borrows in the given region.
    ///
    /// Called when a region scope exits, to release borrows that were active
    /// only within that region.
    pub fn drop_region(&mut self, region: u32) {
        for borrows in self.active.values_mut() {
            borrows.retain(|(_, r)| *r != region);
        }
    }

    /// Returns all accumulated diagnostic messages.
    #[must_use]
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    /// Returns the active borrows for a binding (for testing / introspection).
    #[must_use]
    pub fn active_borrows(&self, binding: u32) -> Option<&[(BorrowKind, u32)]> {
        self.active.get(&binding).map(|v| v.as_slice())
    }

    /// Checks whether a binding currently has any active borrows.
    ///
    /// Returns `true` if the binding has at least one active borrow
    /// (either immutable or mutable), `false` otherwise.
    #[must_use]
    pub fn is_borrowed(&self, binding: u32) -> bool {
        self.active.get(&binding).is_some_and(|v| !v.is_empty())
    }
}

/// Constructs the S0906 diagnostic message (immutable + mutable conflict).
fn s0906_message(binding: u32) -> String {
    format!(
        "S0906: Cannot borrow binding {} as mutable because it is also borrowed as immutable",
        binding
    )
}

/// Constructs the S0907 diagnostic message (mutable + mutable conflict).
fn s0907_message(binding: u32) -> String {
    format!(
        "S0907: Cannot borrow binding {} as mutable more than once",
        binding
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrow_walker_starts_empty() {
        let walker = BorrowWalker::new();
        assert_eq!(walker.diagnostics().len(), 0);
        assert_eq!(walker.active_borrows(1), None);
    }

    #[test]
    fn borrow_walker_multiple_immutable_succeed() {
        let mut walker = BorrowWalker::new();
        assert!(walker.borrow_immutable(1, 100).is_ok());
        assert!(walker.borrow_immutable(1, 101).is_ok());
        assert!(walker.borrow_immutable(1, 102).is_ok());
        assert_eq!(walker.diagnostics().len(), 0);

        let active = walker.active_borrows(1).unwrap();
        assert_eq!(active.len(), 3);
        assert!(
            active
                .iter()
                .all(|(k, _)| matches!(k, BorrowKind::Immutable))
        );
    }

    #[test]
    fn borrow_walker_immutable_then_mutable_fires_s0906() {
        let mut walker = BorrowWalker::new();
        assert!(walker.borrow_immutable(1, 100).is_ok());
        let result = walker.borrow_mutable(1, 101);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("S0906"));
        assert_eq!(walker.diagnostics().len(), 1);
    }

    #[test]
    fn borrow_walker_mutable_then_mutable_fires_s0907() {
        let mut walker = BorrowWalker::new();
        assert!(walker.borrow_mutable(1, 100).is_ok());
        let result = walker.borrow_mutable(1, 101);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("S0907"));
        assert_eq!(walker.diagnostics().len(), 1);
    }

    #[test]
    fn borrow_walker_drop_region_releases_borrows() {
        let mut walker = BorrowWalker::new();
        assert!(walker.borrow_immutable(1, 100).is_ok());
        assert!(walker.borrow_immutable(1, 101).is_ok());

        walker.drop_region(100);
        let active = walker.active_borrows(1).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].1, 101);

        walker.drop_region(101);
        assert!(walker.active_borrows(1).unwrap().is_empty());
    }

    #[test]
    fn borrow_walker_mutable_then_immutable_fires_s0906() {
        let mut walker = BorrowWalker::new();
        assert!(walker.borrow_mutable(1, 100).is_ok());
        let result = walker.borrow_immutable(1, 101);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("S0906"));
        assert_eq!(walker.diagnostics().len(), 1);
    }
}
