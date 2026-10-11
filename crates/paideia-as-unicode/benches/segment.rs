//! UNI-8 (paideia-as#1595) — segmentation throughput harness.
//!
//! Covers UAX #14 line-break opportunities and `wrap_at_width`:
//! * **`line_break_opportunities`** over Lorem ipsum and a CJK run,
//!   exercising both the Latin / ASCII-heavy branch and the per-char
//!   table-walk branch.
//! * **`wrap_at_width`** at a hard-coded 80-column width — the R229
//!   REPL renderer's canonical reflow case. Lorem ipsum lands in the
//!   ASCII fast path (`is_ascii() && no newline`); the CJK input
//!   lands in the slow `wrap_general` path that composes
//!   grapheme-cluster segmentation with per-cluster width lookups.
//!
//! Fixtures are compiled-in `str` literals so cross-session numbers
//! compare apples-to-apples per spec §9.2.

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use paideia_as_unicode::{line_break_opportunities, wrap_at_width};

/// 1 KiB-ish of standard Lorem ipsum — pure ASCII, no mandatory breaks.
/// Hits the UAX #14 crate's Latin branch and the `wrap_at_width` ASCII
/// fast path.
const LOREM: &str = concat!(
    "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do ",
    "eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut ",
    "enim ad minim veniam, quis nostrud exercitation ullamco laboris ",
    "nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in ",
    "reprehenderit in voluptate velit esse cillum dolore eu fugiat ",
    "nulla pariatur. Excepteur sint occaecat cupidatat non proident, ",
    "sunt in culpa qui officia deserunt mollit anim id est laborum. ",
    "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do ",
    "eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut ",
    "enim ad minim veniam, quis nostrud exercitation ullamco laboris."
);

/// Repeated CJK fixture — ~1 KiB, no spaces, forces the per-codepoint
/// table walk in the UAX #14 driver and the grapheme-level fallback in
/// `wrap_at_width`.
const CJK: &str = concat!(
    "日本語の文章における改行機会の計算は、各文字の分類表を参照しつつ",
    "行われます。この入力はCJK専用のベンチマークであり、空白を含まず、",
    "折返し幅に関する最悪ケースの経路を測定するために使用されます。",
    "日本語の文章における改行機会の計算は、各文字の分類表を参照しつつ",
    "行われます。この入力はCJK専用のベンチマークであり、空白を含まず、",
    "折返し幅に関する最悪ケースの経路を測定するために使用されます。"
);

fn bench_line_break_opportunities(c: &mut Criterion) {
    let mut g = c.benchmark_group("line_break_opportunities");
    g.throughput(Throughput::Bytes(LOREM.len() as u64));
    g.bench_function("line_break_opportunities_lorem", |b| {
        b.iter(|| line_break_opportunities(black_box(LOREM)))
    });
    g.throughput(Throughput::Bytes(CJK.len() as u64));
    g.bench_function("line_break_opportunities_cjk", |b| {
        b.iter(|| line_break_opportunities(black_box(CJK)))
    });
    g.finish();
}

fn bench_wrap_at_width(c: &mut Criterion) {
    let mut g = c.benchmark_group("wrap_at_width");
    g.throughput(Throughput::Bytes(LOREM.len() as u64));
    g.bench_function("wrap_at_width_lorem_80", |b| {
        b.iter(|| wrap_at_width(black_box(LOREM), black_box(80)))
    });
    g.throughput(Throughput::Bytes(CJK.len() as u64));
    g.bench_function("wrap_at_width_cjk_80", |b| {
        b.iter(|| wrap_at_width(black_box(CJK), black_box(80)))
    });
    g.finish();
}

criterion_group!(
    segment_benches,
    bench_line_break_opportunities,
    bench_wrap_at_width
);
criterion_main!(segment_benches);
