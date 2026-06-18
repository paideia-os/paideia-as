use std::collections::HashMap;

use crate::row::{EffectRow, RowVarId};

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
}
