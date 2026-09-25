//! Shared test helpers for the R226.M1 (parser) and R226.M2
//! (seminaïve evaluator) fixture corpora.
//!
//! Each fixture tags itself with a `r226-mN-NN` fingerprint so the
//! debugger's @fingerprint correlator (R220.M10) can attribute
//! pass/fail without re-parsing test names.

#![allow(dead_code)]

use paideia_as_shell_datalog::{
    parse_block, parse_query, Database, Program, Query, Value,
};
use paideia_as_shell_lex::{tokenize_ok, Context, Token};

/// Tokenize `src` (a bare Datalog string, no `datalog { … }` wrapper —
/// this helper adds the wrapper and strips it back off so fixtures
/// stay compact) and return the `Context::Datalog` slice.
pub fn dlg_tokens(src: &str) -> Vec<Token> {
    let wrapped = format!("datalog {{ {} }}", src);
    let all = tokenize_ok(&wrapped);
    // Drop the outer `datalog`, `{`, and trailing `}` — everything
    // between is `Context::Datalog`.
    all.into_iter()
        .filter(|t| t.context == Context::Datalog)
        .filter(|t| !matches!(
            t.kind,
            paideia_as_shell_lex::TokenKind::LBrace
                | paideia_as_shell_lex::TokenKind::RBrace
        ))
        .collect()
}

/// Parse `src` and materialize its fixpoint. Panics on any error —
/// fixtures use this only when they expect the parse+eval to succeed.
pub fn build_db(fingerprint: &str, src: &str) -> Database {
    let tokens = dlg_tokens(src);
    let program = parse_block(&tokens)
        .unwrap_or_else(|e| panic!("{fingerprint}: parse failed: {e:?}"));
    Database::from_program(&program)
        .unwrap_or_else(|e| panic!("{fingerprint}: eval failed: {e:?}"))
}

/// Parse a query and evaluate it against `db`, returning the set of
/// values a single free variable takes.
pub fn query_var(fingerprint: &str, db: &Database, query_src: &str, var: &str) -> Vec<Value> {
    let tokens = dlg_tokens(query_src);
    let q: Query = parse_query(&tokens)
        .unwrap_or_else(|e| panic!("{fingerprint}: query parse failed: {e:?}"));
    let results = paideia_as_shell_datalog::query(db, &q)
        .unwrap_or_else(|e| panic!("{fingerprint}: query eval failed: {e:?}"));
    let mut vs: Vec<Value> = results
        .into_iter()
        .filter_map(|b| b.get(var).cloned())
        .collect();
    // Sort for stable comparison — HashMap iteration order is
    // non-deterministic across runs.
    vs.sort_by(|a, b| format!("{a}").cmp(&format!("{b}")));
    vs
}

/// Assert two `Vec<Value>` lists have the same elements after sorting.
pub fn assert_values_eq(fingerprint: &str, got: Vec<Value>, want: &[Value]) {
    let mut w: Vec<Value> = want.to_vec();
    w.sort_by(|a, b| format!("{a}").cmp(&format!("{b}")));
    if got != w {
        panic!(
            "{fingerprint}: expected {want:#?}, got {got:#?}",
            want = w,
            got = got
        );
    }
    assert!(!fingerprint.is_empty());
}

/// Convenience: `Value::Ident("alice")`.
pub fn ident(s: &str) -> Value {
    Value::Ident(s.to_owned())
}
