//! `line_wrap` — wrap a Latin + CJK mix at a terminal column budget.
//!
//! Demonstrates UNI-W3's `wrap_at_width`, which combines UAX #14
//! line-break opportunities with UAX #11 East-Asian width so wrap
//! decisions are column-correct against the R229 renderer (not byte-
//! or char-correct). CJK ideographs count as two columns each; a line
//! that mixes "Hello" with "日本語" wraps differently from one that
//! only contains Latin letters.
//!
//! Run with: `cargo run --example line_wrap -p paideia-as-unicode`.

use paideia_as_unicode::{str_width, wrap_at_width};

fn main() {
    let budget: usize = 20;

    let samples = [
        "The quick brown fox jumps over the lazy dog.",
        "Hello, 日本語の世界へようこそ.",
        "paideia-as 日本語 mixed Latin + CJK + αβγ Greek.",
        "A_single_unbreakable_run_longer_than_the_budget_forces_a_grapheme_break.",
    ];

    for s in samples {
        println!("input  (width {:>2}): {:?}", str_width(s), s);
        println!("budget = {} columns:", budget);
        for (i, line) in wrap_at_width(s, budget).iter().enumerate() {
            // The invariant wrap_at_width promises: each line fits the
            // budget EXCEPT when a single grapheme cluster is wider than
            // the budget (the forced-break fallback — kept total so the
            // renderer never deadlocks on a pathological input).
            println!("  [{}] (width {:>2}) {:?}", i, str_width(line), line);
        }
        println!();
    }
}
