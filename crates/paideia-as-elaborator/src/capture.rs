//! Closure capture analysis and classification.
//!
//! Analyzes how free variables from an outer scope are captured by a closure,
//! classifying each capture as Reference (no consumption), Value (copy of
//! Unrestricted), or Consume (ownership transfer). See
//! `design/toolchain/custom-assembler.md` §3.2 (closure capture discipline).

use std::collections::HashMap;

use paideia_as_ir::LinClass;

use crate::env::Symbol;
use crate::linearity_ctx::Binding;

/// How a closure captures a free variable from an outer scope.
///
/// The capture kind determines whether the closure can be called multiple times
/// and whether the binding's use-count in the outer scope is affected.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum CaptureKind {
    /// Captured by reference: the closure borrows the binding without consuming it.
    ///
    /// The outer scope's binding remains usable after the closure is created.
    /// No use-count increment in the outer scope.
    Reference,

    /// Captured by value: a copy of an Unrestricted binding is taken.
    ///
    /// Only legal for Unrestricted and Affine bindings (the copy is semantic,
    /// not a runtime operation). No use-count increment in the outer scope
    /// because the copy is "free" for Unrestricted.
    Value,

    /// Captured by consume: the closure takes ownership of the binding.
    ///
    /// The binding is moved into the closure and cannot be used again in the
    /// outer scope. This is only legal if the closure itself is Linear or Affine
    /// (guaranteeing it is called at most once).
    Consume,
}

/// One captured binding inside a closure body.
///
/// Captures are identified by the symbol, the kind of capture, and the
/// original binding metadata.
#[derive(Copy, Clone, Debug)]
pub struct CapturedBinding {
    /// The symbol being captured.
    pub symbol: Symbol,

    /// How the symbol is captured.
    pub kind: CaptureKind,

    /// The binding metadata from the outer scope.
    pub binding: Binding,
}

/// Analyze a closure's body environment: classify each free variable from the
/// outer scope (those NOT introduced inside the closure itself) by how it is
/// captured.
///
/// # Phase-1 heuristic
///
/// A binding is classified as `Consume` if its use-count **increases** during
/// the closure body (post-body use-count > pre-body use-count). Otherwise:
/// - For Linear/Ordered bindings: `Reference` (no ownership transfer without consumption).
/// - For Unrestricted/Affine bindings: `Value` (copy semantics).
///
/// # Arguments
///
/// - `pre_body`: bindings visible **before** the closure body is analyzed.
/// - `post_body`: bindings visible **after** the closure body is analyzed.
///
/// # Returns
///
/// A sorted vector of captured bindings (sorted by symbol for determinism).
#[must_use]
pub fn analyze_captures(
    pre_body: &HashMap<Symbol, Binding>,
    post_body: &HashMap<Symbol, Binding>,
) -> Vec<CapturedBinding> {
    let mut captures = Vec::new();
    for (sym, pre) in pre_body {
        if let Some(post) = post_body.get(sym) {
            let delta = post.uses.saturating_sub(pre.uses);
            let kind = if delta >= 1 {
                CaptureKind::Consume
            } else if matches!(pre.class, LinClass::Linear | LinClass::Ordered) {
                CaptureKind::Reference
            } else {
                CaptureKind::Value
            };
            captures.push(CapturedBinding {
                symbol: *sym,
                kind,
                binding: *pre,
            });
        }
    }
    captures.sort_by_key(|c| c.symbol);
    captures
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn span(start: u32) -> paideia_as_diagnostics::Span {
        paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), start, 1)
    }

    fn binding(class: LinClass, uses: u32) -> Binding {
        Binding {
            class,
            uses,
            bind_span: span(100),
        }
    }

    /// Test 1: consume is detected when use-count grows by 1 or more.
    #[test]
    fn consume_detected_when_use_count_grows() {
        let mut pre = HashMap::new();
        let mut post = HashMap::new();
        let sym = 42;

        pre.insert(sym, binding(LinClass::Linear, 0));
        post.insert(sym, binding(LinClass::Linear, 1));

        let captures = analyze_captures(&pre, &post);
        assert_eq!(captures.len(), 1);
        assert_eq!(captures[0].symbol, sym);
        assert_eq!(captures[0].kind, CaptureKind::Consume);
    }

    /// Test 2: reference is inferred for Linear when no use-count growth.
    #[test]
    fn reference_for_linear_when_no_use_growth() {
        let mut pre = HashMap::new();
        let mut post = HashMap::new();
        let sym = 10;

        pre.insert(sym, binding(LinClass::Linear, 0));
        post.insert(sym, binding(LinClass::Linear, 0));

        let captures = analyze_captures(&pre, &post);
        assert_eq!(captures.len(), 1);
        assert_eq!(captures[0].kind, CaptureKind::Reference);
    }

    /// Test 3: value is inferred for Unrestricted when no use-count growth.
    #[test]
    fn value_for_unrestricted_when_no_use_growth() {
        let mut pre = HashMap::new();
        let mut post = HashMap::new();
        let sym = 20;

        pre.insert(sym, binding(LinClass::Unrestricted, 0));
        post.insert(sym, binding(LinClass::Unrestricted, 0));

        let captures = analyze_captures(&pre, &post);
        assert_eq!(captures.len(), 1);
        assert_eq!(captures[0].kind, CaptureKind::Value);
    }

    /// Test 4: value is inferred for Affine when no use-count growth.
    #[test]
    fn value_for_affine_when_no_use_growth() {
        let mut pre = HashMap::new();
        let mut post = HashMap::new();
        let sym = 30;

        pre.insert(sym, binding(LinClass::Affine, 0));
        post.insert(sym, binding(LinClass::Affine, 0));

        let captures = analyze_captures(&pre, &post);
        assert_eq!(captures.len(), 1);
        assert_eq!(captures[0].kind, CaptureKind::Value);
    }

    /// Test 5: captures are sorted by symbol for determinism.
    #[test]
    fn captures_sorted_by_symbol_for_determinism() {
        let mut pre = HashMap::new();
        let mut post = HashMap::new();

        pre.insert(30, binding(LinClass::Unrestricted, 0));
        pre.insert(10, binding(LinClass::Unrestricted, 0));
        pre.insert(20, binding(LinClass::Unrestricted, 0));

        post.insert(30, binding(LinClass::Unrestricted, 0));
        post.insert(10, binding(LinClass::Unrestricted, 0));
        post.insert(20, binding(LinClass::Unrestricted, 0));

        let captures = analyze_captures(&pre, &post);
        assert_eq!(captures.len(), 3);
        assert_eq!(captures[0].symbol, 10);
        assert_eq!(captures[1].symbol, 20);
        assert_eq!(captures[2].symbol, 30);
    }
}
