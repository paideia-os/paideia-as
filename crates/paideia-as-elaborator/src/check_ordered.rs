//! Ordered-class rule per `custom-assembler.md` §3.1.
//!
//! An Ordered binding must be consumed in declaration order: when the
//! checker observes a use of a non-first unconsumed Ordered binding,
//! it emits `S0903` ("out-of-order use of ordered binding").
//!
//! Phase-1 model: a per-scope `OrderedLog` records each Ordered
//! declaration's symbol and span in insertion order. On use, the
//! caller invokes [`OrderedLog::record_use`] which finds the first
//! unconsumed binding; if it's not the symbol being used, a diagnostic
//! is produced and the actual symbol is *still* consumed so the rest
//! of the scope can continue without cascading errors.

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};

use crate::env::Symbol;

/// Diagnostic code for out-of-order use of an Ordered binding.
pub const S_OUT_OF_ORDER: u16 = 903;

/// One Ordered binding tracked in a scope.
#[derive(Copy, Clone, Debug)]
pub struct OrderedEntry {
    /// Identifier of the binding.
    pub symbol: Symbol,
    /// Where the binding was declared (for diagnostic spans).
    pub bind_span: Span,
    /// `true` once a use has been recorded.
    pub consumed: bool,
}

/// Per-scope log of Ordered declarations in insertion order.
#[derive(Default, Debug, Clone)]
pub struct OrderedLog {
    entries: Vec<OrderedEntry>,
}

impl OrderedLog {
    /// Construct an empty log.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a new Ordered declaration. Re-declaration of an
    /// already-declared symbol is allowed (shadows the earlier entry).
    pub fn declare(&mut self, symbol: Symbol, bind_span: Span) {
        self.entries.push(OrderedEntry {
            symbol,
            bind_span,
            consumed: false,
        });
    }

    /// Record a use of `symbol`. If it isn't the first unconsumed
    /// Ordered binding, returns one S0903 diagnostic. In either case,
    /// the matching entry's `consumed` flag is set so the rest of the
    /// scope is checked against a coherent state.
    #[must_use]
    pub fn record_use(&mut self, symbol: Symbol, use_span: Span) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        let first_unconsumed = self.entries.iter().position(|e| !e.consumed);
        let target_index = self
            .entries
            .iter()
            .position(|e| e.symbol == symbol && !e.consumed);

        match (first_unconsumed, target_index) {
            (Some(expected), Some(actual)) if expected != actual => {
                let expected_span = self.entries[expected].bind_span;
                diags.push(
                    Diagnostic::error(s_code(S_OUT_OF_ORDER))
                        .message(format!(
                            "out-of-order use of ordered binding: expected first unconsumed at {expected_span:?}, found use of {actual:?}",
                        ))
                        .with_span(use_span)
                        .finish(),
                );
                self.entries[actual].consumed = true;
            }
            (Some(_), Some(actual)) => {
                self.entries[actual].consumed = true;
            }
            _ => {}
        }
        diags
    }

    /// Iterate entries (testing aid).
    #[must_use]
    pub fn entries(&self) -> &[OrderedEntry] {
        &self.entries
    }
}

fn s_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::S, Severity::Error, n).expect("valid S code")
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn span(byte_start: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), byte_start, 1)
    }

    #[test]
    fn in_order_use_passes() {
        let mut log = OrderedLog::new();
        log.declare(1, span(0));
        log.declare(2, span(10));
        assert!(log.record_use(1, span(20)).is_empty());
        assert!(log.record_use(2, span(30)).is_empty());
    }

    #[test]
    fn out_of_order_use_emits_s0903() {
        let mut log = OrderedLog::new();
        log.declare(1, span(0));
        log.declare(2, span(10));
        let diags = log.record_use(2, span(20));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), 903);
    }

    #[test]
    fn out_of_order_marks_entry_consumed_for_subsequent_check() {
        // After out-of-order use of 2, the next use of 1 is now in order.
        let mut log = OrderedLog::new();
        log.declare(1, span(0));
        log.declare(2, span(10));
        let _ = log.record_use(2, span(20));
        let diags = log.record_use(1, span(30));
        assert!(diags.is_empty(), "second use should be in order");
    }

    #[test]
    fn three_bindings_skip_middle_emits_s0903() {
        let mut log = OrderedLog::new();
        log.declare(1, span(0));
        log.declare(2, span(10));
        log.declare(3, span(20));
        assert!(log.record_use(1, span(30)).is_empty());
        let diags = log.record_use(3, span(40));
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn empty_log_use_is_silently_dropped() {
        // No declarations → no constraints to check.
        let mut log = OrderedLog::new();
        assert!(log.record_use(1, span(0)).is_empty());
    }

    #[test]
    fn redeclaration_appends_new_entry() {
        let mut log = OrderedLog::new();
        log.declare(1, span(0));
        log.declare(1, span(10));
        assert_eq!(log.entries().len(), 2);
        assert!(log.record_use(1, span(20)).is_empty());
        assert!(log.record_use(1, span(30)).is_empty());
    }
}
