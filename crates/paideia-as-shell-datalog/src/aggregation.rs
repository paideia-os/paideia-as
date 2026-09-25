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
//! target value to be numeric — either every value is [`Value::Num`]
//! (integer path, result [`AggregateResult::Sum`]/[`AggregateResult::Avg`])
//! or every value is [`Value::Float`] (float path, result
//! [`AggregateResult::SumF`]/[`AggregateResult::AvgF`]). The numeric
//! kind is picked by the first substitution's target value; a later
//! value of the *other* numeric kind aborts with
//! [`AggregationError::NonHomogeneousNumeric`], and any non-numeric
//! value aborts with [`AggregationError::NonNumericTarget`] —
//! partial group results are discarded, mirroring the stratifier's
//! pre-fixpoint rejection discipline from R226.M5 (no half-derived
//! output leaks past a type error). [`Aggregate::Count`],
//! [`Aggregate::Min`], and [`Aggregate::Max`] accept any [`Value`].
//!
//! # Min/max total order
//!
//! [`Value`] derives `PartialEq + Eq + Hash` but not `Ord` (see
//! [`crate::ast`]), so this module carries its own total order:
//! `Float < Num < Str < Ident` across kinds, natural order within a
//! kind. Within `Float` the order is `partial_cmp` with `Equal` as
//! the fallback for NaN (so NaN sorts equal to everything under this
//! comparator — a leftmost-wins tie-break for `min`, a rightmost-wins
//! for `max`; the sentinel behaviour is documented and stable, and a
//! caller who wants IEEE-754-strict NaN handling normalises before
//! wrapping). The across-kind order is arbitrary but stable; it is
//! documented so a REPL user reading the result of a min/max over a
//! mixed-kind column can predict which value wins.
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

/// Per-row aggregation result. `Count`, `Sum`, and `SumF` never
/// appear as `Empty`; `Min`, `Max`, `Avg`, and `AvgF` do — only in
/// the ungrouped zero-input case (see module doc).
///
/// The float variants (`SumF`, `AvgF`) are emitted when the reduced
/// column is uniformly [`Value::Float`]; the integer variants
/// (`Sum`, `Avg`) when it is uniformly [`Value::Num`]. Mixing the
/// two in one group is not silently coerced — the aggregator returns
/// [`AggregationError::NonHomogeneousNumeric`]. Callers pattern-match
/// on the variant to know which numeric kind won, so a downstream
/// formatter (e.g. the shell's result printer) never needs to inspect
/// the underlying `Value` again.
///
/// `PartialEq` (not `Eq`) because `Avg(f64)` / `AvgF(f64)` / `SumF(f64)`
/// carry floats. Callers that need equality on the float variants
/// should compare within an epsilon of their own choosing; the fixture
/// suite uses `to_bits()` on exact-representable operands for the same
/// reason [`Value::Float`] does (see [`crate::ast`] module doc).
#[derive(Clone, Debug, PartialEq)]
pub enum AggregateResult {
    /// `count(?x)` — number of substitutions whose target was bound.
    Count(u64),
    /// `sum(?x)` — integer sum (saturating on i64 overflow). Emitted
    /// only when every observed target value is [`Value::Num`].
    Sum(i64),
    /// `sum(?x)` — float sum. Emitted when every observed target
    /// value is [`Value::Float`]. Overflow follows IEEE 754 (`+inf` /
    /// `-inf`); no saturation, since a float sum has no natural
    /// upper bound to clamp to.
    SumF(f64),
    /// `min(?x)` — least value under the module's total order.
    Min(Value),
    /// `max(?x)` — greatest value under the module's total order.
    Max(Value),
    /// `avg(?x)` — arithmetic mean, `sum as f64 / count as f64`.
    /// Emitted only when every observed target value is [`Value::Num`].
    Avg(f64),
    /// `avg(?x)` — arithmetic mean of a uniformly [`Value::Float`]
    /// column. Distinct variant so a caller can tell "avg of an int
    /// column that happened to land at 25.0" from "avg of a float
    /// column that happened to land at 25.0" without inspecting the
    /// source relation.
    AvgF(f64),
    /// Sentinel for `min`/`max`/`avg` on zero-input ungrouped queries.
    /// Never appears as a value in a grouped result — a group is only
    /// present when at least one substitution lands in it.
    Empty,
}

/// Aggregation-time diagnostic. `Count`, `Min`, `Max` never error at
/// aggregation time — every failure below concerns `Sum` / `Avg`
/// over a column that is not uniformly one numeric kind.
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
    /// Sum/Avg saw two different numeric kinds inside the same group
    /// (e.g. one `Value::Num` and one `Value::Float`). The numeric
    /// kind is pinned by the *first* substitution's target value; any
    /// later value of the other kind trips this diagnostic. Reporting
    /// the type names (rather than the raw values) matches how
    /// R226.M9's schema registry will phrase the same violation.
    NonHomogeneousNumeric {
        /// The kind pinned by the first observation (e.g. `"Num"`).
        first_type: String,
        /// The offending later observation's kind (e.g. `"Float"`).
        got: String,
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
        Aggregate::Sum => sum_reduce(values),
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
        Aggregate::Avg => avg_reduce(values),
    }
}

/// Sum reducer with numeric-kind pinning.
///
/// The first target value picks the numeric kind (integer vs float);
/// every later value must match. A wrong-kind numeric value raises
/// [`AggregationError::NonHomogeneousNumeric`]; a non-numeric value
/// raises [`AggregationError::NonNumericTarget`]. On an empty slice
/// the reducer returns [`AggregateResult::Sum(0)`] — the integer
/// additive identity — because the *ungrouped* zero-input path never
/// reaches here (it is short-circuited to `identity_for` upstream),
/// and a grouped call is guaranteed non-empty by the caller. The
/// `Sum(0)` fallback is a defensive default consistent with the
/// module-doc contract if a future refactor changes that.
fn sum_reduce(values: &[Value]) -> Result<AggregateResult, AggregationError> {
    match numeric_kind_of_first(values) {
        NumericKind::Empty => Ok(AggregateResult::Sum(0)),
        NumericKind::Int => {
            let mut acc: i64 = 0;
            for v in values {
                match v {
                    Value::Num(n) => acc = acc.saturating_add(*n),
                    Value::Float(_) => {
                        return Err(AggregationError::NonHomogeneousNumeric {
                            first_type: "Num".to_owned(),
                            got: "Float".to_owned(),
                        });
                    }
                    other => {
                        return Err(AggregationError::NonNumericTarget {
                            got: other.clone(),
                        });
                    }
                }
            }
            Ok(AggregateResult::Sum(acc))
        }
        NumericKind::Float => {
            // IEEE 754 accumulation — no saturation. A NaN anywhere
            // taints the total to NaN (as any float folder would), an
            // overflow yields ±infinity. Documented in the module
            // doc's "Numeric coercion" section.
            let mut acc: f64 = 0.0;
            for v in values {
                match v {
                    Value::Float(x) => acc += *x,
                    Value::Num(_) => {
                        return Err(AggregationError::NonHomogeneousNumeric {
                            first_type: "Float".to_owned(),
                            got: "Num".to_owned(),
                        });
                    }
                    other => {
                        return Err(AggregationError::NonNumericTarget {
                            got: other.clone(),
                        });
                    }
                }
            }
            Ok(AggregateResult::SumF(acc))
        }
    }
}

/// Average reducer — same numeric-kind pinning as [`sum_reduce`]; the
/// result variant is [`AggregateResult::Avg`] (integer path) or
/// [`AggregateResult::AvgF`] (float path). An empty slice yields
/// [`AggregateResult::Empty`]: `0/0` has no meaningful value.
fn avg_reduce(values: &[Value]) -> Result<AggregateResult, AggregationError> {
    if values.is_empty() {
        return Ok(AggregateResult::Empty);
    }
    match numeric_kind_of_first(values) {
        // Unreachable in practice (empty short-circuit above) — kept
        // for total-match hygiene; if the empty case ever leaks
        // through, `Empty` is still the right answer.
        NumericKind::Empty => Ok(AggregateResult::Empty),
        NumericKind::Int => {
            let mut sum: i64 = 0;
            for v in values {
                match v {
                    Value::Num(n) => sum = sum.saturating_add(*n),
                    Value::Float(_) => {
                        return Err(AggregationError::NonHomogeneousNumeric {
                            first_type: "Num".to_owned(),
                            got: "Float".to_owned(),
                        });
                    }
                    other => {
                        return Err(AggregationError::NonNumericTarget {
                            got: other.clone(),
                        });
                    }
                }
            }
            let avg = (sum as f64) / (values.len() as f64);
            Ok(AggregateResult::Avg(avg))
        }
        NumericKind::Float => {
            let mut sum: f64 = 0.0;
            for v in values {
                match v {
                    Value::Float(x) => sum += *x,
                    Value::Num(_) => {
                        return Err(AggregationError::NonHomogeneousNumeric {
                            first_type: "Float".to_owned(),
                            got: "Num".to_owned(),
                        });
                    }
                    other => {
                        return Err(AggregationError::NonNumericTarget {
                            got: other.clone(),
                        });
                    }
                }
            }
            Ok(AggregateResult::AvgF(sum / (values.len() as f64)))
        }
    }
}

/// The numeric kind pinned by the first target value in `values`.
///
/// A non-numeric first value is *not* rejected here — the caller
/// (`sum_reduce` / `avg_reduce`) walks the whole slice and surfaces
/// [`AggregationError::NonNumericTarget`] on the actual offending
/// value with its span-agnostic position preserved. Returning the
/// kind here would require an extra allocation to carry the value
/// forward; deferring keeps the fast path allocation-free.
enum NumericKind {
    Empty,
    Int,
    Float,
}

fn numeric_kind_of_first(values: &[Value]) -> NumericKind {
    match values.first() {
        None => NumericKind::Empty,
        Some(Value::Float(_)) => NumericKind::Float,
        // Default to integer for `Num` *and* for any non-numeric
        // first value — the integer path is where we already have
        // a `NonNumericTarget` diagnostic, so any wrong-kind
        // report reads consistently regardless of which value came
        // first.
        Some(_) => NumericKind::Int,
    }
}

/// Total order on [`Value`] used by `min` / `max`.
///
/// Across kinds: `Float < Num < Str < Ident`. Within a kind:
///
/// * `Float` — [`f64::partial_cmp`], with [`std::cmp::Ordering::Equal`]
///   as the fallback for NaN. NaN therefore sorts equal to every
///   other float under this comparator, giving a leftmost-wins
///   tie-break for `min` and a rightmost-wins tie-break for `max`.
///   The choice keeps the reducer total without picking a "NaN
///   propagates" or "NaN is smallest/largest" convention that would
///   surprise a downstream reader (a saturating float folder would
///   have to encode either, and the schema-driven column semantics
///   R226.M9 targets do not yet exist to constrain the choice).
/// * `Num` — [`i64::cmp`].
/// * `Str`/`Ident` — [`String::cmp`].
///
/// The across-kind order is arbitrary but stable; it is documented
/// so a REPL user reading the result of a min/max over a mixed-kind
/// column can predict which value wins. Float sits below `Num`
/// because a homogeneous float column reads more naturally as
/// "smaller than the integer column it might be widened into" than
/// the reverse.
fn compare_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => x.partial_cmp(y).unwrap_or(Equal),
        (Value::Num(x), Value::Num(y)) => x.cmp(y),
        (Value::Str(x), Value::Str(y)) => x.cmp(y),
        (Value::Ident(x), Value::Ident(y)) => x.cmp(y),
        // Across-kind fall-through, in the documented order
        // (Float < Num < Str < Ident). Order the arms so the earlier
        // kind on the left wins `Less`; every wrap-around case is
        // covered by pairing the intra-kind arms above.
        (Value::Float(_), _) => Less,
        (_, Value::Float(_)) => Greater,
        (Value::Num(_), _) => Less,
        (_, Value::Num(_)) => Greater,
        (Value::Str(_), _) => Less,
        (_, Value::Str(_)) => Greater,
    }
}
