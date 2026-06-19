use std::collections::HashMap;

use crate::row::{EffectId, EffectRow, RowVarId};

/// Result of unifying two effect rows.
///
/// Contains bindings from row variables to their resolved rows.
#[derive(Default, Debug, Clone, Eq, PartialEq)]
pub struct Substitution {
    /// Bindings from row variables to effect rows.
    pub bindings: HashMap<RowVarId, EffectRow>,
}

impl Substitution {
    /// Create an empty substitution.
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind a row variable to an effect row.
    pub fn bind(&mut self, v: RowVarId, r: EffectRow) {
        self.bindings.insert(v, r);
    }
}

/// Error type for row unification.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum UnifyError {
    /// Two closed rows have different fixed sets.
    Mismatch,
}

/// A structured representation of the differences between two effect rows.
///
/// Used for high-quality row-mismatch diagnostics. The diff shows:
/// - `expected`: the declared row (what should have been provided)
/// - `got`: the inferred/actual row (what was actually inferred)
/// - `name_for`: optional effect name resolver (EffectId -> human-readable name)
///
/// Phase-2-m13: without a name resolver, numeric ids are printed.
/// A real name resolver lands when the elaborator threads the EffectRegistry through.
pub struct RowDiff<'a> {
    /// The declared/expected row.
    pub expected: &'a EffectRow,
    /// The inferred/actual row.
    pub got: &'a EffectRow,
    /// Optional effect name lookup function.
    pub name_for: Option<&'a dyn Fn(EffectId) -> Option<String>>,
}

impl<'a> RowDiff<'a> {
    /// Render the row diff as a multi-line string.
    ///
    /// Format:
    /// ```text
    /// expected: !{Io, Net}
    /// got     : !{Io, Net, Mmio}
    /// diff    : + Mmio
    /// ```
    ///
    /// Effects are sorted lexicographically (by name if resolver provided, else by numeric id).
    /// The diff line lists:
    /// - `+ <name>` for effects in `got` but not in `expected`.
    /// - `- <name>` for effects in `expected` but not in `got`.
    /// - Tail changes (e.g., `- | e1` if `expected` has a tail and `got` doesn't).
    pub fn render(&self) -> String {
        let expected_str = self.format_row(self.expected);
        let got_str = self.format_row(self.got);
        let diff_str = self.format_diff();

        format!(
            "expected: {}\ngot     : {}\ndiff    : {}",
            expected_str, got_str, diff_str
        )
    }

    /// Format a single row for display.
    fn format_row(&self, row: &EffectRow) -> String {
        let mut fixed = row.fixed.clone();
        // Sort by name if resolver available, else by numeric id.
        fixed.sort_by_key(|e| {
            self.name_for
                .and_then(|f| f(*e))
                .unwrap_or_else(|| e.get().to_string())
        });

        let fixed_strs: Vec<String> = fixed
            .iter()
            .map(|e| {
                self.name_for
                    .and_then(|f| f(*e))
                    .unwrap_or_else(|| e.get().to_string())
            })
            .collect();

        let fixed_part = fixed_strs.join(", ");
        match row.tail {
            Some(tail) => format!("!{{{} | r{}}}", fixed_part, tail.get()),
            None => {
                if fixed_part.is_empty() {
                    "!{}".to_string()
                } else {
                    format!("!{{{}}}", fixed_part)
                }
            }
        }
    }

    /// Format the diff line showing what changed.
    fn format_diff(&self) -> String {
        let mut additions: Vec<EffectId> = self
            .got
            .fixed
            .iter()
            .copied()
            .filter(|e| !self.expected.fixed.contains(e))
            .collect();

        let mut removals: Vec<EffectId> = self
            .expected
            .fixed
            .iter()
            .copied()
            .filter(|e| !self.got.fixed.contains(e))
            .collect();

        // Sort by name if resolver available, else by numeric id.
        additions.sort_by_key(|e| {
            self.name_for
                .and_then(|f| f(*e))
                .unwrap_or_else(|| e.get().to_string())
        });

        removals.sort_by_key(|e| {
            self.name_for
                .and_then(|f| f(*e))
                .unwrap_or_else(|| e.get().to_string())
        });

        let mut parts: Vec<String> = Vec::new();

        // Additions: + Effect
        for e in additions {
            let name = self
                .name_for
                .and_then(|f| f(e))
                .unwrap_or_else(|| e.get().to_string());
            parts.push(format!("+ {}", name));
        }

        // Removals: - Effect
        for e in removals {
            let name = self
                .name_for
                .and_then(|f| f(e))
                .unwrap_or_else(|| e.get().to_string());
            parts.push(format!("- {}", name));
        }

        // Tail changes
        match (self.expected.tail, self.got.tail) {
            (Some(e_tail), Some(g_tail)) if e_tail != g_tail => {
                // Both have tails but they differ
                parts.push(format!("~ tail: r{} vs r{}", e_tail.get(), g_tail.get()));
            }
            (Some(e_tail), None) => {
                // Expected has tail, got doesn't
                parts.push(format!("- | r{}", e_tail.get()));
            }
            (None, Some(g_tail)) => {
                // Got has tail, expected doesn't
                parts.push(format!("+ | r{}", g_tail.get()));
            }
            _ => {}
        }

        if parts.is_empty() {
            "(no differences)".to_string()
        } else {
            parts.join(", ")
        }
    }
}

/// Unify two effect rows.
///
/// Phase-1 unifier:
/// - If both rows are closed (no tail), they unify iff fixed sets are
///   equal (sorted-and-deduped vecs equal).
/// - If `a` is `{fixed_a | tail_a}` and `b` is `{fixed_b | tail_b}`,
///   compute `extras_a = fixed_b \ fixed_a` and `extras_b = fixed_a \
///   fixed_b`. The substitution binds `tail_a |-> { extras_a | tail_b }`
///   if `tail_a.is_some()`; symmetrically for `tail_b`. Both extras must
///   end up bound to some tail OR be empty.
/// - If extras exist but the corresponding side has no tail, return
///   Mismatch.
pub fn unify(a: &EffectRow, b: &EffectRow) -> Result<Substitution, UnifyError> {
    let extras_a_only: Vec<_> = a
        .fixed
        .iter()
        .copied()
        .filter(|e| !b.fixed.contains(e))
        .collect();
    let extras_b_only: Vec<_> = b
        .fixed
        .iter()
        .copied()
        .filter(|e| !a.fixed.contains(e))
        .collect();

    let mut subst = Substitution::new();

    // `b` has effects `a` doesn't → bind a.tail to {extras_b_only | b.tail}.
    if !extras_b_only.is_empty() {
        match a.tail {
            Some(v) => subst.bind(v, EffectRow::from_ids(extras_b_only, b.tail)),
            None => return Err(UnifyError::Mismatch),
        }
    }
    // `a` has effects `b` doesn't → bind b.tail to {extras_a_only | a.tail}.
    if !extras_a_only.is_empty() {
        match b.tail {
            Some(v) => subst.bind(v, EffectRow::from_ids(extras_a_only, a.tail)),
            None => return Err(UnifyError::Mismatch),
        }
    }
    Ok(subst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::row::EffectId;

    #[test]
    fn unify_empty_with_empty() {
        let row_a = EffectRow::empty();
        let row_b = EffectRow::empty();

        let result = unify(&row_a, &row_b).unwrap();

        assert!(result.bindings.is_empty());
    }

    #[test]
    fn unify_closed_equal_rows() {
        let e1 = EffectId::new(1).unwrap();
        let e2 = EffectId::new(2).unwrap();

        let row_a = EffectRow::from_ids(vec![e1, e2], None);
        let row_b = EffectRow::from_ids(vec![e1, e2], None);

        let result = unify(&row_a, &row_b).unwrap();

        assert!(result.bindings.is_empty());
    }

    #[test]
    fn unify_closed_mismatch_rows() {
        let e1 = EffectId::new(1).unwrap();
        let e2 = EffectId::new(2).unwrap();

        let row_a = EffectRow::from_ids(vec![e1], None);
        let row_b = EffectRow::from_ids(vec![e2], None);

        let result = unify(&row_a, &row_b);

        assert_eq!(result, Err(UnifyError::Mismatch));
    }

    #[test]
    fn unify_row_variable_e1_with_concrete_extra() {
        let io = EffectId::new(1).unwrap();
        let ipc = EffectId::new(2).unwrap();
        let e1 = RowVarId::new(1).unwrap();
        let e2 = RowVarId::new(2).unwrap();

        // !{io | e1}
        let row_a = EffectRow::from_ids(vec![io], Some(e1));
        // !{io, ipc | e2}
        let row_b = EffectRow::from_ids(vec![io, ipc], Some(e2));

        let result = unify(&row_a, &row_b).unwrap();

        // e1 should bind to {ipc | e2}
        assert_eq!(result.bindings.len(), 1);
        let e1_binding = result.bindings.get(&e1).unwrap();
        assert_eq!(e1_binding.fixed.len(), 1);
        assert_eq!(e1_binding.fixed[0].get(), ipc.get());
        assert_eq!(e1_binding.tail, Some(e2));
    }

    // Five row-unification scenarios per issue #212

    #[test]
    fn closed_unifies_with_closed_when_same() {
        let io = EffectId::new(1).unwrap();

        let row_a = EffectRow::from_ids(vec![io], None);
        let row_b = EffectRow::from_ids(vec![io], None);

        let result = unify(&row_a, &row_b).unwrap();

        // Both rows are identical; no bindings needed.
        assert!(result.bindings.is_empty());
    }

    #[test]
    fn closed_disagrees_emits_mismatch() {
        let io = EffectId::new(1).unwrap();
        let net = EffectId::new(2).unwrap();

        let row_a = EffectRow::from_ids(vec![io], None);
        let row_b = EffectRow::from_ids(vec![net], None);

        let result = unify(&row_a, &row_b);

        // Closed rows with different fixed sets fail to unify.
        assert_eq!(result, Err(UnifyError::Mismatch));
    }

    #[test]
    fn open_unifies_with_closed_binds_tail() {
        let io = EffectId::new(1).unwrap();
        let net = EffectId::new(2).unwrap();
        let r1 = RowVarId::new(1).unwrap();

        // !{io | r1}
        let row_a = EffectRow::from_ids(vec![io], Some(r1));
        // !{io, net}
        let row_b = EffectRow::from_ids(vec![io, net], None);

        let result = unify(&row_a, &row_b).unwrap();

        // r1 should bind to {net}
        assert_eq!(result.bindings.len(), 1);
        let r1_binding = result.bindings.get(&r1).unwrap();
        assert_eq!(r1_binding.fixed.len(), 1);
        assert_eq!(r1_binding.fixed[0].get(), net.get());
        assert!(r1_binding.is_closed());
    }

    #[test]
    fn open_unifies_with_open_substitutes_one_tail() {
        let io = EffectId::new(1).unwrap();
        let net = EffectId::new(2).unwrap();
        let r1 = RowVarId::new(1).unwrap();
        let r2 = RowVarId::new(2).unwrap();

        // !{io | r1}
        let row_a = EffectRow::from_ids(vec![io], Some(r1));
        // !{net | r2}
        let row_b = EffectRow::from_ids(vec![net], Some(r2));

        let result = unify(&row_a, &row_b).unwrap();

        // r1 should bind to {net | r2}, r2 should bind to {io | r1}
        assert_eq!(result.bindings.len(), 2);

        let r1_binding = result.bindings.get(&r1).unwrap();
        assert_eq!(r1_binding.fixed.len(), 1);
        assert_eq!(r1_binding.fixed[0].get(), net.get());
        assert_eq!(r1_binding.tail, Some(r2));

        let r2_binding = result.bindings.get(&r2).unwrap();
        assert_eq!(r2_binding.fixed.len(), 1);
        assert_eq!(r2_binding.fixed[0].get(), io.get());
        assert_eq!(r2_binding.tail, Some(r1));
    }

    #[test]
    fn row_unification_idempotent_under_self() {
        let io = EffectId::new(1).unwrap();
        let r1 = RowVarId::new(1).unwrap();

        // !{io | r1}
        let row_a = EffectRow::from_ids(vec![io], Some(r1));

        let result = unify(&row_a, &row_a).unwrap();

        // Unifying a row with itself produces an identity substitution.
        assert!(result.bindings.is_empty());
    }

    // ── RowDiff rendering tests ────────────────────────────────────────

    #[test]
    fn row_diff_renders_addition() {
        // expected {Io}, got {Io, Net} → diff line is + Net
        let io = EffectId::new(1).unwrap();
        let net = EffectId::new(2).unwrap();

        let expected = EffectRow::from_ids(vec![io], None);
        let got = EffectRow::from_ids(vec![io, net], None);

        let diff = RowDiff {
            expected: &expected,
            got: &got,
            name_for: None,
        };

        let output = diff.render();
        assert!(output.contains("+ 2")); // net = 2
        assert!(output.contains("expected:"));
        assert!(output.contains("got     :"));
        assert!(output.contains("diff    :"));
    }

    #[test]
    fn row_diff_renders_removal() {
        // expected {Io, Net}, got {Io} → diff line is - Net
        let io = EffectId::new(1).unwrap();
        let net = EffectId::new(2).unwrap();

        let expected = EffectRow::from_ids(vec![io, net], None);
        let got = EffectRow::from_ids(vec![io], None);

        let diff = RowDiff {
            expected: &expected,
            got: &got,
            name_for: None,
        };

        let output = diff.render();
        assert!(output.contains("- 2")); // net = 2
    }

    #[test]
    fn row_diff_renders_both() {
        // expected {Io, Net}, got {Io, Mmio} → both + Mmio and - Net
        let io = EffectId::new(1).unwrap();
        let net = EffectId::new(2).unwrap();
        let mmio = EffectId::new(3).unwrap();

        let expected = EffectRow::from_ids(vec![io, net], None);
        let got = EffectRow::from_ids(vec![io, mmio], None);

        let diff = RowDiff {
            expected: &expected,
            got: &got,
            name_for: None,
        };

        let output = diff.render();
        assert!(output.contains("+ 3")); // mmio = 3
        assert!(output.contains("- 2")); // net = 2
    }

    #[test]
    fn row_diff_renders_with_tail_change() {
        // expected !{Io | e}, got !{Io} → diff mentions tail removal
        let io = EffectId::new(1).unwrap();
        let r_expected = RowVarId::new(1).unwrap();

        let expected = EffectRow::from_ids(vec![io], Some(r_expected));
        let got = EffectRow::from_ids(vec![io], None);

        let diff = RowDiff {
            expected: &expected,
            got: &got,
            name_for: None,
        };

        let output = diff.render();
        // Should show removal of the tail variable
        assert!(output.contains("- | r1"));
    }

    #[test]
    fn row_diff_renders_with_tail_addition() {
        // expected !{Io}, got !{Io | e} → diff mentions tail addition
        let io = EffectId::new(1).unwrap();
        let r_got = RowVarId::new(2).unwrap();

        let expected = EffectRow::from_ids(vec![io], None);
        let got = EffectRow::from_ids(vec![io], Some(r_got));

        let diff = RowDiff {
            expected: &expected,
            got: &got,
            name_for: None,
        };

        let output = diff.render();
        // Should show addition of the tail variable
        assert!(output.contains("+ | r2"));
    }

    #[test]
    fn row_diff_lex_sorted() {
        // expected {Acl, Ipc, Net}, got {Net, Ipc, Acl} (same set, different order)
        // → diff should be empty (sets equal after sort)
        let acl = EffectId::new(1).unwrap();
        let ipc = EffectId::new(2).unwrap();
        let net = EffectId::new(3).unwrap();

        let expected = EffectRow::from_ids(vec![acl, ipc, net], None);
        let got = EffectRow::from_ids(vec![net, ipc, acl], None);

        let diff = RowDiff {
            expected: &expected,
            got: &got,
            name_for: None,
        };

        let output = diff.render();
        // Diff section should be empty or show no differences
        assert!(output.contains("(no differences)"));
    }

    #[test]
    fn row_diff_with_name_resolver() {
        // Test with a name resolver to display readable effect names
        let io = EffectId::new(1).unwrap();
        let net = EffectId::new(2).unwrap();
        let mmio = EffectId::new(3).unwrap();

        let expected = EffectRow::from_ids(vec![io, net], None);
        let got = EffectRow::from_ids(vec![io, mmio], None);

        let name_map = |e: EffectId| -> Option<String> {
            match e.get() {
                1 => Some("Io".to_string()),
                2 => Some("Net".to_string()),
                3 => Some("Mmio".to_string()),
                _ => None,
            }
        };

        let diff = RowDiff {
            expected: &expected,
            got: &got,
            name_for: Some(&name_map),
        };

        let output = diff.render();
        assert!(output.contains("Io"));
        assert!(output.contains("+ Mmio"));
        assert!(output.contains("- Net"));
    }
}
