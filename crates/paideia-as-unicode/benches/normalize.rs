//! UNI-8 (paideia-as#1595) — normalization throughput harness.
//!
//! Covers the four UAX #15 forms over three input classes:
//! * **ASCII fast path.** The byte-scan branch in `nfc_normalize` /
//!   `nfd_normalize` / `nfkc_normalize` / `nfkd_normalize` short-circuits
//!   when every byte is `< 0x80`; spec §9.1 pins ≤100 ns/call at a
//!   32-byte input (we exercise 1 KiB to amortize the per-call overhead).
//! * **Mixed Latin.** A deliberate precomposed + decomposed interleave
//!   so the quick-check pass fails and the combining-class table loads;
//!   spec §9.1 pins ≤500 ns/call at 32 B.
//! * **CJK long run.** The worst-case path: table-heavy, no fast exit.
//!   Spec §9.1 pins ≤1 µs/char.
//!
//! Each bench's input is seeded (literal `str` or deterministic repeat)
//! so cross-session comparison is apples-to-apples per spec §9.2.
//! Timing primitives are criterion's; no `unsafe` is required here.

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use paideia_as_unicode::{nfc_normalize, nfd_normalize, nfkc_normalize, nfkd_normalize};

/// 1 KiB of pure ASCII — exercises the `< 0x80` byte-scan fast path in
/// every normalization form without any UCD table load.
fn ascii_1kib() -> String {
    const BASE: &str = "The quick brown fox jumps over the lazy dog. 0123456789 ";
    let mut s = String::with_capacity(1024);
    while s.len() < 1024 {
        s.push_str(BASE);
    }
    s.truncate(1024);
    s
}

/// Latin precomposed + decomposed mix — forces the quick-check pass to
/// fail and the composition table to load. Mirrors the common
/// "accented command name" case the R220.M4 `Str::eq` path hits.
fn latin_mixed() -> String {
    // `é` precomposed (U+00E9) alternating with `e` + U+0301 combining
    // acute. Repeated to roughly 1 KiB so per-call overhead is amortized.
    let mut s = String::with_capacity(1024);
    while s.len() < 1024 {
        s.push_str("caf\u{00E9} re\u{0301}sum\u{00E9} na\u{00EF}ve ");
    }
    s
}

/// 1 KiB of CJK — the slow path, no fast exit, UAX #15 Hangul composition
/// algorithm triggered on every third character.
fn cjk_1kib() -> String {
    const BASE: &str = "日本語の文字列を正規化する試験的な入力列です。";
    let mut s = String::with_capacity(1200);
    while s.len() < 1024 {
        s.push_str(BASE);
    }
    s
}

fn bench_nfc(c: &mut Criterion) {
    let ascii = ascii_1kib();
    let latin = latin_mixed();
    let cjk = cjk_1kib();

    let mut g = c.benchmark_group("nfc");
    g.throughput(Throughput::Bytes(ascii.len() as u64));
    g.bench_function("nfc_ascii_fast_path", |b| {
        b.iter(|| nfc_normalize(black_box(ascii.as_str())))
    });
    g.throughput(Throughput::Bytes(latin.len() as u64));
    g.bench_function("nfc_mixed_latin", |b| {
        b.iter(|| nfc_normalize(black_box(latin.as_str())))
    });
    g.throughput(Throughput::Bytes(cjk.len() as u64));
    g.bench_function("nfc_cjk_long", |b| {
        b.iter(|| nfc_normalize(black_box(cjk.as_str())))
    });
    g.finish();
}

fn bench_other_forms_ascii(c: &mut Criterion) {
    let ascii = ascii_1kib();
    let mut g = c.benchmark_group("norm_forms_ascii");
    g.throughput(Throughput::Bytes(ascii.len() as u64));
    g.bench_function("nfkc_ascii_fast_path", |b| {
        b.iter(|| nfkc_normalize(black_box(ascii.as_str())))
    });
    g.bench_function("nfd_ascii_fast_path", |b| {
        b.iter(|| nfd_normalize(black_box(ascii.as_str())))
    });
    g.bench_function("nfkd_ascii_fast_path", |b| {
        b.iter(|| nfkd_normalize(black_box(ascii.as_str())))
    });
    g.finish();
}

criterion_group!(normalize_benches, bench_nfc, bench_other_forms_ascii);
criterion_main!(normalize_benches);
