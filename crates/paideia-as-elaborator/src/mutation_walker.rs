//! MutationWalker — rejects assignment to a binding while it's borrowed.
//!
//! Fires S0909 (was spec'd A0703): "cannot assign to value while borrowed".

use crate::borrow_walker::BorrowWalker;

/// Checks that assignments do not occur while a binding is borrowed.
///
/// The mutation checker enforces that a binding cannot be assigned a new value
/// while there are active borrows (immutable or mutable) on that binding.
/// This prevents mutation-through-borrow races.
#[derive(Debug)]
pub struct MutationWalker<'a> {
    /// Reference to the borrow walker to check borrow state.
    borrows: &'a BorrowWalker,
    /// Accumulated diagnostic messages.
    diagnostics: Vec<String>,
}

impl<'a> MutationWalker<'a> {
    /// Creates a new mutation walker over the given borrow walker.
    #[must_use]
    pub fn new(borrows: &'a BorrowWalker) -> Self {
        Self {
            borrows,
            diagnostics: Vec::new(),
        }
    }

    /// Check that the binding is not borrowed before assigning.
    ///
    /// Returns `Ok(())` if the binding has no active borrows, or
    /// `Err(S0909 diagnostic)` if there is at least one active borrow.
    pub fn check_assignment(&mut self, binding: u32) -> Result<(), String> {
        if self.borrows.is_borrowed(binding) {
            let msg = s0909_message(binding);
            self.diagnostics.push(msg.clone());
            Err(msg)
        } else {
            Ok(())
        }
    }

    /// Returns all accumulated diagnostic messages.
    #[must_use]
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }
}

/// Constructs the S0909 diagnostic message.
fn s0909_message(binding: u32) -> String {
    format!(
        "S0909: Cannot assign to value {} while it is borrowed",
        binding
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::borrow_walker::BorrowWalker;

    #[test]
    fn mutation_walker_assigns_freely_when_no_borrow() {
        let borrows = BorrowWalker::new();
        let mut walker = MutationWalker::new(&borrows);
        assert!(walker.check_assignment(1).is_ok());
        assert_eq!(walker.diagnostics().len(), 0);
    }

    #[test]
    fn mutation_walker_fires_s0909_when_borrowed() {
        let mut borrows = BorrowWalker::new();
        assert!(borrows.borrow_immutable(1, 100).is_ok());

        let mut walker = MutationWalker::new(&borrows);
        let result = walker.check_assignment(1);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("S0909"));
        assert_eq!(walker.diagnostics().len(), 1);
    }

    #[test]
    fn mutation_walker_allows_assign_after_borrow_dropped() {
        let mut borrows = BorrowWalker::new();
        assert!(borrows.borrow_immutable(1, 100).is_ok());

        // Drop the region holding the borrow.
        borrows.drop_region(100);

        let mut walker = MutationWalker::new(&borrows);
        assert!(walker.check_assignment(1).is_ok());
        assert_eq!(walker.diagnostics().len(), 0);
    }
}
