//! R225.M9: HM property-test harness — deterministic random generators
//! for [`Expr`] / [`MonoType`] plus algebraic-property checkers over
//! [`infer`], [`unify`] and the generalise/instantiate roundtrip.
//!
//! Where R225.M1-M8 shipped example-based fixture corpora — each test
//! constructs a fixed input, asserts on a fixed output — R225.M9 gives
//! the same layer a *property* channel: a check that a stated algebraic
//! law holds across a swath of inputs sampled from a seedable generator.
//! The three properties are:
//!
//! * `check_infer_terminates` — [`infer`] never panics on any input the
//!   [`random_expr`] generator emits (Ok and Err both count as
//!   "terminated"; only a panic falsifies).
//! * `check_unify_symmetric` — [`unify`] is symmetric under
//!   alpha-equivalence: `unify(a, b)` and `unify(b, a)` either both fail
//!   or both succeed with substitutions that produce alpha-equivalent
//!   principal types.
//! * `check_generalize_instantiate_roundtrip` — the composition
//!   `instantiate . generalize` is the identity on monotypes up to
//!   alpha-equivalence (the Damas-Milner soundness corollary).
//!
//! # Non-goals at M9
//!
//! * No external property-crate dependency (no `proptest`, no
//!   `quickcheck`). The generator is a hand-rolled LCG so the crate's
//!   pure-std dependency posture stays intact — the same rationale that
//!   kept M1-M8 dependency-free.
//! * No shrinking. A failed property surfaces the (depth, seed) pair
//!   that produced it; the tester debugs from the seed directly. Adding
//!   an integrated shrinker is a separate milestone once real failures
//!   demand it.
//! * No coverage-guided fuzzing. The LCG walk is a uniform random walk
//!   over the constructor space at each level; the tester picks depth
//!   and seed to shape coverage.
//!
//! # Lineage
//!
//! * Claessen & Hughes 2000, *QuickCheck: A Lightweight Tool for Random
//!   Testing of Haskell Programs* — the property-based-testing shape
//!   this module lifts into the pure-std, non-macro Rust idiom.
//! * Knuth, *TAoCP* vol. 2 §3.3.4 — the MMIX 64-bit LCG constants
//!   ([`lcg`]) used to advance the seed between recursive calls.

use std::collections::{BTreeMap, HashMap};
use std::panic;

use crate::effect_row::EffectRow;
use crate::expr::{app, i, lam, let_, s, v, Expr};
use crate::infer::{generalize, infer, instantiate, FreshVarGen, TypeEnv};
use crate::ty::{MonoType, RowType, TypeVar};
use crate::typed_value::TypedValue;
use crate::unify::{unify, unify_with_fresh};

// ---------------------------------------------------------------------
// Deterministic seed advance.
// ---------------------------------------------------------------------

/// Knuth's 64-bit LCG (MMIX constants; TAoCP vol. 2 §3.3.4).
///
/// The multiplier and increment are the standard MMIX pair; every seed
/// advances by one step of this recurrence. No external RNG crate is
/// pulled in — the crate stays pure-std, matching the M1-M8 posture.
fn lcg(seed: u64) -> u64 {
    seed.wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407)
}

/// Pick a short identifier from a small pool by seed.
///
/// The pool is kept intentionally small so a random `Var` reference has
/// a reasonable chance of colliding with a nearby `Lam` / `Let` binder
/// under recursive generation — otherwise infer would return
/// `UnboundVar` on nearly every generated Var, and the harness's
/// coverage over the bound-variable path would be near zero.
fn pick_name(seed: u64) -> String {
    const NAMES: &[&str] = &["x", "y", "z", "u", "w"];
    NAMES[(seed as usize) % NAMES.len()].to_owned()
}

// ---------------------------------------------------------------------
// Random generators.
// ---------------------------------------------------------------------

/// Deterministic random-[`Expr`] generator.
///
/// * `depth == 0` — a bare literal (`Lit::Int` or `Lit::Str`, chosen by
///   `seed & 1`).
/// * `depth > 0` — one of `{Lam, App, Let, Var}` by `seed % 4`. The
///   `Var` branch is a leaf even at positive depth; it stands in for a
///   free-variable reference under the surrounding environment.
///
/// The same `(depth, seed)` pair always returns the same tree; the LCG
/// step in [`lcg`] threads a distinct seed into each recursive call so
/// sibling subtrees do not degenerate into identical copies.
pub fn random_expr(depth: usize, seed: u64) -> Expr {
    if depth == 0 {
        return if seed & 1 == 0 { i() } else { s() };
    }
    let s1 = lcg(seed);
    let s2 = lcg(s1);
    match seed % 4 {
        0 => lam(&pick_name(seed), random_expr(depth - 1, s1)),
        1 => app(random_expr(depth - 1, s1), random_expr(depth - 1, s2)),
        2 => let_(
            &pick_name(seed),
            random_expr(depth - 1, s1),
            random_expr(depth - 1, s2),
        ),
        _ => v(&pick_name(seed)),
    }
}

/// Deterministic random-[`MonoType`] generator.
///
/// * `depth == 0` — a leaf: `Var(TypeVar(seed % 8))` (small pool so
///   different generator instances have a reasonable chance of sharing
///   a variable), `Con("Int")`, or `Con("Str")`.
/// * `depth > 0` — one of `{Var, Con, Arrow, Record}` by `seed % 4`.
///   The `Record` branch synthesises a one-field closed row so a
///   downstream unify property probes the row-unification code path
///   without ballooning tree size.
///
/// Effect rows and typed values are not generated at M9 — the value/
/// effect-row corpora (R225.M2/M3/M5) already exercise those paths, and
/// keeping the generator small keeps its coverage story legible.
pub fn random_mono(depth: usize, seed: u64) -> MonoType {
    if depth == 0 {
        return match seed % 3 {
            0 => MonoType::Var(TypeVar((seed as u32) % 8)),
            1 => MonoType::Con("Int".to_owned()),
            _ => MonoType::Con("Str".to_owned()),
        };
    }
    let s1 = lcg(seed);
    let s2 = lcg(s1);
    match seed % 4 {
        0 => MonoType::Var(TypeVar((seed as u32) % 8)),
        1 => MonoType::Con(if seed & 1 == 0 {
            "Int".to_owned()
        } else {
            "Str".to_owned()
        }),
        2 => MonoType::Arrow(
            Box::new(random_mono(depth - 1, s1)),
            Box::new(random_mono(depth - 1, s2)),
        ),
        _ => {
            let mut fields = HashMap::new();
            fields.insert(pick_name(seed), random_mono(depth - 1, s1));
            MonoType::Record(RowType::from_map(fields, None))
        }
    }
}

// ---------------------------------------------------------------------
// Property checkers.
// ---------------------------------------------------------------------

/// True iff [`infer`] on `expr` (under the empty environment, with a
/// fresh var-generator) returns without panicking — `Ok(_)` and
/// `Err(_)` both count as "terminated".
///
/// A panic is trapped via [`std::panic::catch_unwind`] so a bug in the
/// inference driver surfaces as `false` rather than tearing down the
/// test process. `Expr` implements `Clone`, so the moved copy inside
/// the closure does not violate `UnwindSafe` bounds in practice; the
/// [`std::panic::AssertUnwindSafe`] wrapper documents that we accept
/// the possibility of a poisoned inference state on panic (the whole
/// point of this check is to detect exactly that).
pub fn check_infer_terminates(expr: &Expr) -> bool {
    let cloned = expr.clone();
    let outcome = panic::catch_unwind(panic::AssertUnwindSafe(move || {
        let env = TypeEnv::new();
        let mut fresh = FreshVarGen::new();
        let _ = infer(&env, &cloned, &mut fresh);
    }));
    outcome.is_ok()
}

/// True iff [`unify`] agrees on `(a, b)` and `(b, a)` up to
/// alpha-equivalence of the resulting principal types.
///
/// The check has three arms:
///
/// * Both directions fail → the pair is not unifiable from either side;
///   symmetric-in-failure holds. Errors' internal wording differs (the
///   `a`/`b` fields swap), so we compare `Result::is_err` rather than
///   the variants themselves.
/// * Both directions succeed → apply each substitution back to `a` and
///   check the two refined monotypes are [`alpha_eq`]. Unification is a
///   most-general unifier: `s_ab(a)` and `s_ba(a)` must coincide under
///   variable renaming.
/// * One side succeeds and the other fails → symmetry is broken; return
///   `false`. This is the property's actual falsifier, the case a
///   regression would surface.
///
/// Uses [`unify_with_fresh`] with a shared [`FreshVarGen`] advanced past
/// every variable free in `a` or `b`, mirroring
/// [`check_generalize_instantiate_roundtrip`]. The bare [`unify`] wrapper
/// would allocate its own `FreshVarGen::new()` starting at id 0 per
/// call, so a row-tail witness minted for open records could silently
/// collide with a hand-authored `TypeVar(0)` in the input — a genuine
/// aliasing hazard on Rémy-style `(open, open)` record pairs. `random_
/// mono` only emits closed rows today, so the shipped fixtures never
/// hit that branch; the guard is here so the public API is honest for
/// any caller (including a future generator that does emit open rows).
pub fn check_unify_symmetric(a: &MonoType, b: &MonoType) -> bool {
    let mut fresh = FreshVarGen::new();
    let mut max_id: u32 = 0;
    let mut have_any = false;
    for tv in a.free_vars() {
        max_id = max_id.max(tv.0);
        have_any = true;
    }
    for tv in b.free_vars() {
        max_id = max_id.max(tv.0);
        have_any = true;
    }
    if have_any {
        let target = max_id.saturating_add(1);
        for _ in 0..target {
            let _ = fresh.fresh();
        }
    }
    match (
        unify_with_fresh(a, b, &mut fresh),
        unify_with_fresh(b, a, &mut fresh),
    ) {
        (Err(_), Err(_)) => true,
        (Ok(sab), Ok(sba)) => alpha_eq(&sab.apply(a), &sba.apply(a)),
        _ => false,
    }
}

/// True iff `instantiate(generalize(env, mono))` is alpha-equivalent to
/// `mono`.
///
/// [`generalize`] quantifies exactly the variables free in `mono` that
/// are not free in `env`; [`instantiate`] replaces each of those
/// quantified variables with a fresh monotype variable. The composed
/// operation returns a monotype identical to `mono` up to variable
/// renaming — the Damas-Milner soundness corollary.
///
/// To keep the alpha-equivalence check honest, we advance the
/// [`FreshVarGen`] past every variable free in `mono` *or* `env`
/// beforehand: otherwise a freshly minted id could collide with an
/// env-free var already present in `mono`, silently identifying two
/// variables that were semantically distinct in the original. The
/// advance loop calls `fresh.fresh()` at most `max_id + 1` times, so
/// callers should keep their manual [`TypeVar`] ids small.
pub fn check_generalize_instantiate_roundtrip(env: &TypeEnv, mono: &MonoType) -> bool {
    let scheme = generalize(env, mono);
    let mut fresh = FreshVarGen::new();

    // Determine the highest TypeVar id present in either `mono` or the
    // environment's schemes, so the fresh var-generator's next id
    // strictly exceeds it. Prevents an accidental id reuse from
    // aliasing a formerly-distinct free variable with a formerly-
    // quantified one.
    let mut max_id: u32 = 0;
    let mut have_any = false;
    for tv in mono.free_vars() {
        max_id = max_id.max(tv.0);
        have_any = true;
    }
    for tv in env.free_vars() {
        max_id = max_id.max(tv.0);
        have_any = true;
    }
    if have_any {
        // Advance past `max_id`. Guarded add so we never overflow if
        // `max_id == u32::MAX` (a caller-supplied nonsense input, but
        // safety first).
        let target = max_id.saturating_add(1);
        for _ in 0..target {
            let _ = fresh.fresh();
        }
    }

    let mono2 = instantiate(&scheme, &mut fresh);
    alpha_eq(mono, &mono2)
}

// ---------------------------------------------------------------------
// Alpha-equivalence on monotypes.
// ---------------------------------------------------------------------

/// Two monotypes are alpha-equivalent iff they have the same structure
/// up to a consistent renaming of type variables.
///
/// Implemented by [`normalize`]-then-`==`: each side is walked and its
/// type variables are re-numbered in first-appearance order into a
/// canonical `0, 1, 2, ...` sequence, then the two canonical forms are
/// compared structurally.
fn alpha_eq(a: &MonoType, b: &MonoType) -> bool {
    normalize(a) == normalize(b)
}

/// Return a copy of `ty` in which every [`TypeVar`] is renamed to a
/// canonical `0, 1, 2, ...` id in first-appearance order.
///
/// Order of appearance follows the same left-to-right walk the M5/M6
/// `generalize` walker uses (arrow: left then right; row: sorted by
/// field name; effect row: sorted-map key order; typed value: value
/// row then effect row). Two alpha-equivalent monotypes normalise to
/// identical trees.
fn normalize(ty: &MonoType) -> MonoType {
    let mut renaming: HashMap<TypeVar, TypeVar> = HashMap::new();
    let mut next: u32 = 0;
    normalize_walk(ty, &mut renaming, &mut next)
}

fn rename(v: TypeVar, renaming: &mut HashMap<TypeVar, TypeVar>, next: &mut u32) -> TypeVar {
    *renaming.entry(v).or_insert_with(|| {
        let id = *next;
        *next = next
            .checked_add(1)
            .expect("paideia-as-shell-hm::property::normalize: TypeVar counter overflow");
        TypeVar(id)
    })
}

fn normalize_walk(
    ty: &MonoType,
    renaming: &mut HashMap<TypeVar, TypeVar>,
    next: &mut u32,
) -> MonoType {
    match ty {
        MonoType::Var(v) => MonoType::Var(rename(*v, renaming, next)),
        MonoType::Con(name) => MonoType::Con(name.clone()),
        MonoType::Arrow(x, y) => MonoType::Arrow(
            Box::new(normalize_walk(x, renaming, next)),
            Box::new(normalize_walk(y, renaming, next)),
        ),
        MonoType::Record(row) => MonoType::Record(normalize_row(row, renaming, next)),
        MonoType::EffectRow(row) => {
            MonoType::EffectRow(normalize_effect_row(row, renaming, next))
        }
        MonoType::Typed(tv) => MonoType::Typed(Box::new(TypedValue {
            value_row: normalize_row(&tv.value_row, renaming, next),
            effect_row: normalize_effect_row(&tv.effect_row, renaming, next),
        })),
    }
}

fn normalize_row(
    row: &RowType,
    renaming: &mut HashMap<TypeVar, TypeVar>,
    next: &mut u32,
) -> RowType {
    // Flatten into (fields, tail); iterate fields in sorted order for
    // deterministic first-appearance numbering.
    let (fields, tail) = row.to_map();
    let mut sorted: Vec<(String, MonoType)> = fields.into_iter().collect();
    sorted.sort_by(|x, y| x.0.cmp(&y.0));
    let new_fields: HashMap<String, MonoType> = sorted
        .into_iter()
        .map(|(k, ty)| (k, normalize_walk(&ty, renaming, next)))
        .collect();
    let new_tail = tail.map(|v| rename(v, renaming, next));
    RowType::from_map(new_fields, new_tail)
}

fn normalize_effect_row(
    row: &EffectRow,
    renaming: &mut HashMap<TypeVar, TypeVar>,
    next: &mut u32,
) -> EffectRow {
    // BTreeMap iteration is already sorted; walk payloads first, then
    // the tail row-var — matches the generalize walker's order.
    let present: BTreeMap<String, MonoType> = row
        .present
        .iter()
        .map(|(k, ty)| (k.clone(), normalize_walk(ty, renaming, next)))
        .collect();
    let tail = row.tail.map(|v| rename(v, renaming, next));
    EffectRow { present, tail }
}
