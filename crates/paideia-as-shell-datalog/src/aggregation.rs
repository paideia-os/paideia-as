//! R226.M6 aggregation evaluator.
//!
//! Consumes the substitution set produced by the body of an
//! [`crate::ast::AggregateQuery`] and reduces it to one output row per
//! distinct group-by tuple. The AST piece lives in
//! [`crate::ast::Aggregate`] + [`crate::ast::AggregateQuery`]; the
//! evaluator entry point that materialises the fixpoint, enumerates
//! the body, and calls into this module is
//! [`crate::eval::Evaluator::run_aggregate_query`].
//!
//! # Result shape
//!
//! ```text
//!   HashMap<Vec<Value>, AggregateResult>
//! ```
//!
//! The key is the group-by tuple, in the same positional order the
//! [`crate::ast::AggregateQuery::group_by`] vector holds. An
//! *ungrouped* query keys the (only) output row under the zero-length
//! vector, so a caller destructures with `out.get(&Vec::new())` for
//! the singleton value.
//!
//! # Empty-input behaviour
//!
//! For the **ungrouped** case (`group_by.is_empty()`) the aggregator
//! always emits exactly one row, even when no substitution was
//! observed:
//!
//! * [`Aggregate::Count`] → [`AggregateResult::Count(0)`](AggregateResult::Count).
//! * [`Aggregate::Sum`]   → [`AggregateResult::Sum(0)`](AggregateResult::Sum) — the
//!   additive identity.
//! * [`Aggregate::Min`] / [`Aggregate::Max`] / [`Aggregate::Avg`] →
//!   [`AggregateResult::Empty`]. There is no meaningful identity for
//!   min/max (no witness in an empty set) or avg (`0/0`); the
//!   sentinel lets a caller distinguish "no rows matched" from
//!   "aggregate produced a zero-valued result".
//!
//! For the **grouped** case an empty substitution set means an empty
//! output map (no keys) — a group is only present when at least one
//! substitution lands in it. The `Empty` sentinel therefore never
//! appears as a *value* in a grouped result; it only appears as the
//! singleton value of an ungrouped query with zero input.
//!
//! # Numeric coercion
//!
//! [`Aggregate::Sum`] and [`Aggregate::Avg`] require every observed
//! target value to be [`Value::Num`]. Any non-numeric value aborts
//! the whole aggregation with
//! [`AggregationError::NonNumericTarget`] — partial group results are
//! discarded, mirroring the stratifier's pre-fixpoint rejection
//! discipline from R226.M5 (no half-derived output leaks past a type
//! error). [`Aggregate::Count`], [`Aggregate::Min`], and
//! [`Aggregate::Max`] accept any [`Value`].
//!
//! # Min/max total order
//!
//! [`Value`] derives `PartialEq + Eq + Hash` but not `Ord` (see
//! [`crate::ast`]), so this module carries its own total order:
//! `Num < Str < Ident` across kinds, natural order within a kind.
//! The choice is stable and documented so a REPL user can read the
//! result of `min(?x)` on a mixed-kind column without having to
//! reason about hash iteration order.
//!
//! # Overflow policy
//!
//! Integer sums use `i64::saturating_add`. The shell's aggregation
//! target is analytic, not arithmetic-heavy; a saturating fold keeps
//! a runaway sum from panicking a REPL that otherwise treats
//! aggregation as a read-only diagnostic. R226.M9 will attach an
//! overflow diagnostic once the schema registry pins numeric column
//! types.

use crate::ast::{Aggregate, Value};
use std::collections::HashMap;

/// Per-row aggregation result. `Count` and `Sum` never appear as
/// `Empty`; `Min`, `Max`, and `Avg` do — only in the ungrouped
/// zero-input case (see module doc).
///
/// `PartialEq` (not `Eq`) because `Avg(f64)` carries a float. Callers
/// that need equality on `Avg` results should compare within an
/// epsilon of their own choosing.
#[derive(Clone, Debug, PartialEq)]
pub enum AggregateResult {
    /// `count(?x)` — number of substitutions whose target was bound.
    Count(u64),
    /// `sum(?x)` — integer sum (saturating on i64 overflow).
    Sum(i64),
    /// `min(?x)` — least value under the module's total order.
    Min(Value),
    /// `max(?x)` — greatest value under the module's total order.
    Max(Value),
    /// `avg(?x)` — arithmetic mean, `sum as f64 / count as f64`.
    Avg(f64),
    /// Sentinel for `min`/`max`/`avg` on zero-input ungrouped queries.
    /// Never appears as a value in a grouped result — a group is only
    /// present when at least one substitution lands in it.
    Empty,
}

/// Aggregation-time diagnostic. The only failure mode at M6 is a
/// non-numeric target for `Sum` or `Avg`; `Count`, `Min`, `Max` never
/// error at aggregation time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AggregationError {
    /// Target variable resolved to a non-numeric value while running
    /// [`Aggregate::Sum`] or [`Aggregate::Avg`]. The first offending
    /// value is captured; the aggregation aborts before any group's
    /// result is emitted, so the caller sees an all-or-nothing failure.
    NonNumericTarget {
        /// The offending value.
        got: Value,
    },
}

/// Reduce `substitutions` under `agg` on `target`, partitioned by
/// `group_by`.
///
/// See the module doc for empty-input, grouping, and numeric-coercion
/// rules. This function is deterministic and side-effect-free — the
/// output `HashMap` iteration order is not defined, but the *contents*
/// (key set + per-key `AggregateResult`) are a pure function of the
/// inputs.
pub fn evaluate(
    agg: Aggregate,
    target: &str,
    group_by: &[String],
    substitutions: &[HashMap<String, Value>],
) -> Result<HashMap<Vec<Value>, AggregateResult>, AggregationError> {
    // Partition substitutions by their group-by tuple; collect the
    // target values that land in each group. A substitution whose
    // group-by tuple is not fully bound is silently skipped
    // (range-restriction violation; see module doc) — mirrors the
    // eval.rs `ground_atom` skip so the two pieces stay in lockstep.
    let mut groups: HashMap<Vec<Value>, Vec<Value>> = HashMap::new();
    for sub in substitutions {
        let mut key: Vec<Value> = Vec::with_capacity(group_by.len());
        let mut key_ok = true;
        for v in group_by {
            match sub.get(v) {
                Some(val) => key.push(val.clone()),
                None => {
                    key_ok = false;
                    break;
                }
            }
        }
        if !key_ok {
            continue;
        }
        // For `count` we still need to collect *something* per matched
        // substitution — pushing the target value gives a natural
        // arity-agnostic count; a substitution missing the target
        // variable is treated as "no observation for this row" and
        // does NOT count (this matches SQL's `COUNT(col)` semantics,
        // which skips NULLs — the closest analogue we have to an
        // unbound target). For the ungrouped case the "no rows
        // matched" path is handled uniformly below via
        // `identity_for(agg)`, so no special-case branch is needed
        // here.
        if let Some(v) = sub.get(target) {
            groups.entry(key).or_default().push(v.clone());
        }
    }

    let mut out: HashMap<Vec<Value>, AggregateResult> = HashMap::new();

    // Ungrouped queries always emit one row — the identity when no
    // observations landed, or the reduction when they did.
    if group_by.is_empty() {
        let values = groups.remove(&Vec::new()).unwrap_or_default();
        let row = if values.is_empty() {
            identity_for(agg)
        } else {
            reduce_group(agg, &values)?
        };
        out.insert(Vec::new(), row);
        return Ok(out);
    }

    // Grouped queries: one row per group actually observed.
    for (key, values) in groups {
        let row = reduce_group(agg, &values)?;
        out.insert(key, row);
    }
    Ok(out)
}

/// The identity row for an ungrouped zero-input aggregation. See the
/// module doc's "Empty-input behaviour" section for the rationale.
fn identity_for(agg: Aggregate) -> AggregateResult {
    match agg {
        Aggregate::Count => AggregateResult::Count(0),
        Aggregate::Sum => AggregateResult::Sum(0),
        Aggregate::Min | Aggregate::Max | Aggregate::Avg => AggregateResult::Empty,
    }
}

/// Fold a non-empty slice of target values under `agg`. `values` is
/// always non-empty on entry (the caller filters empty groups) but the
/// function still returns `Empty` for min/max/avg on `values.is_empty()`
/// as a defensive default — a future refactor that starts calling this
/// with an empty slice would then read the same shape ungrouped
/// callers expect.
fn reduce_group(
    agg: Aggregate,
    values: &[Value],
) -> Result<AggregateResult, AggregationError> {
    match agg {
        Aggregate::Count => Ok(AggregateResult::Count(values.len() as u64)),
        Aggregate::Sum => {
            let mut acc: i64 = 0;
            for v in values {
                match v {
                    Value::Num(n) => acc = acc.saturating_add(*n),
                    other => {
                        return Err(AggregationError::NonNumericTarget {
                            got: other.clone(),
                        })
                    }
                }
            }
            Ok(AggregateResult::Sum(acc))
        }
        Aggregate::Min => {
            let mut best: Option<&Value> = None;
            for v in values {
                let take = match best {
                    None => true,
                    Some(b) => compare_values(v, b) == std::cmp::Ordering::Less,
                };
                if take {
                    best = Some(v);
                }
            }
            Ok(match best {
                Some(v) => AggregateResult::Min(v.clone()),
                None => AggregateResult::Empty,
            })
        }
        Aggregate::Max => {
            let mut best: Option<&Value> = None;
            for v in values {
                let take = match best {
                    None => true,
                    Some(b) => compare_values(v, b) == std::cmp::Ordering::Greater,
                };
                if take {
                    best = Some(v);
                }
            }
            Ok(match best {
                Some(v) => AggregateResult::Max(v.clone()),
                None => AggregateResult::Empty,
            })
        }
        Aggregate::Avg => {
            if values.is_empty() {
                return Ok(AggregateResult::Empty);
            }
            let mut sum: i64 = 0;
            for v in values {
                match v {
                    Value::Num(n) => sum = sum.saturating_add(*n),
                    other => {
                        return Err(AggregationError::NonNumericTarget {
                            got: other.clone(),
                        })
                    }
                }
            }
            let avg = (sum as f64) / (values.len() as f64);
            Ok(AggregateResult::Avg(avg))
        }
    }
}

/// Total order on [`Value`] used by `min` / `max`.
///
/// Across kinds: `Num < Str < Ident`. Within a kind: the natural
/// order (`i64::cmp` for `Num`, `String::cmp` for `Str` / `Ident`).
/// The across-kind order is arbitrary but stable; it is documented so
/// a REPL user reading the result of a min/max over a mixed-kind
/// column can predict which value wins.
fn compare_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    match (a, b) {
        (Value::Num(x), Value::Num(y)) => x.cmp(y),
        (Value::Str(x), Value::Str(y)) => x.cmp(y),
        (Value::Ident(x), Value::Ident(y)) => x.cmp(y),
        (Value::Num(_), _) => Less,
        (_, Value::Num(_)) => Greater,
        (Value::Str(_), _) => Less,
        (_, Value::Str(_)) => Greater,
    }
}
