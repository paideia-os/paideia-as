//! UNI-8 (paideia-as#1595) — case fold / width / UTF-8 / UCD-script
//! throughput harness.
//!
//! Covers the remaining spec §9.1 rows:
//! * **`fold_default` / `fold(Turkic)`** on ASCII and Latin-heavy mixes
//!   — the CaseFolding.txt driver + Turkic I/i pre-emption branch.
//! * **`str_width`** on ASCII and CJK — the §9.1 ≤2 ns/byte ASCII row
//!   and the ≤15 ns/byte CJK row.
//! * **`Utf8Decoder::next`** on three input shapes: pure ASCII, mixed
//!   multi-byte, and a corpus with embedded invalid bytes (the
//!   `Utf8Error` branch, which §9.1 does not pin but which we track so
//!   a regression in the error path does not go unnoticed).
//! * **UCD `script` batch** — 10 000 deterministic chars drawn from
//!   a repeating multi-script string to amortize per-call overhead.
//!
//! Fixtures are deterministic (seeded repeat or embedded literal) so
//! numbers compare across sessions per spec §9.2.

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use paideia_as_unicode::{
    CaseFoldLocale, Utf8Decoder, fold, fold_default, script, str_width,
};

/// 1 KiB ASCII — fast path for every function here.
fn ascii_1kib() -> String {
    const BASE: &str = "The quick brown fox jumps over the lazy dog. 0123456789 ";
    let mut s = String::with_capacity(1024);
    while s.len() < 1024 {
        s.push_str(BASE);
    }
    s.truncate(1024);
    s
}

/// Mixed Latin + Greek + Cyrillic + CJK, ~1 KiB — exercises the
/// combining-class table and the Turkic I/i pre-emption branch.
fn mixed_1kib() -> String {
    const BASE: &str = "Istanbul İSTANBUL Ελληνικά Русский 日本語 café ";
    let mut s = String::with_capacity(1200);
    while s.len() < 1024 {
        s.push_str(BASE);
    }
    s
}

/// 1 KiB of CJK — worst case for `str_width` (per-cluster width lookup).
fn cjk_1kib() -> String {
    const BASE: &str = "日本語の文字列幅計算の試験的な入力です。";
    let mut s = String::with_capacity(1200);
    while s.len() < 1024 {
        s.push_str(BASE);
    }
    s
}

/// Pure-ASCII byte buffer for the UTF-8 fast-path row.
fn utf8_valid_ascii() -> Vec<u8> {
    ascii_1kib().into_bytes()
}

/// Mixed multi-byte UTF-8 — forces the two-/three-/four-byte branches.
fn utf8_mixed() -> Vec<u8> {
    mixed_1kib().into_bytes()
}

/// 1 KiB where every eighth byte is 0xFF — the invalid-lead branch of
/// `Utf8Decoder::next`. The decoder emits an `Err` and advances one
/// byte, so the ratio of valid-char to error paths stays stable
/// across runs.
fn utf8_invalid() -> Vec<u8> {
    let mut v = Vec::with_capacity(1024);
    for i in 0..1024 {
        v.push(if i % 8 == 7 { 0xFFu8 } else { b'a' });
    }
    v
}

fn bench_fold(c: &mut Criterion) {
    let ascii = ascii_1kib();
    let mixed = mixed_1kib();
    let mut g = c.benchmark_group("fold");
    g.throughput(Throughput::Bytes(ascii.len() as u64));
    g.bench_function("fold_default_ascii", |b| {
        b.iter(|| fold_default(black_box(ascii.as_str())))
    });
    g.throughput(Throughput::Bytes(mixed.len() as u64));
    g.bench_function("fold_default_mixed", |b| {
        b.iter(|| fold_default(black_box(mixed.as_str())))
    });
    g.bench_function("fold_turkic_mixed", |b| {
        b.iter(|| fold(black_box(mixed.as_str()), black_box(CaseFoldLocale::Turkic)))
    });
    g.finish();
}

fn bench_width(c: &mut Criterion) {
    let ascii = ascii_1kib();
    let cjk = cjk_1kib();
    let mut g = c.benchmark_group("width");
    g.throughput(Throughput::Bytes(ascii.len() as u64));
    g.bench_function("str_width_ascii", |b| {
        b.iter(|| str_width(black_box(ascii.as_str())))
    });
    g.throughput(Throughput::Bytes(cjk.len() as u64));
    g.bench_function("str_width_cjk", |b| {
        b.iter(|| str_width(black_box(cjk.as_str())))
    });
    g.finish();
}

/// Drain a decoder; the per-item `black_box` keeps the compiler from
/// eliding the body, and the final `count()` keeps the loop a single
/// expression so each bench stays short.
#[inline(always)]
fn drain_decoder(bytes: &[u8]) -> usize {
    Utf8Decoder::new(bytes)
        .map(|r| {
            black_box(&r);
            r.is_ok() as usize
        })
        .sum()
}

fn bench_utf8(c: &mut Criterion) {
    let ascii = utf8_valid_ascii();
    let mixed = utf8_mixed();
    let invalid = utf8_invalid();
    let mut g = c.benchmark_group("utf8");
    g.throughput(Throughput::Bytes(ascii.len() as u64));
    g.bench_function("utf8_decode_valid_ascii", |b| {
        b.iter(|| drain_decoder(black_box(&ascii)))
    });
    g.throughput(Throughput::Bytes(mixed.len() as u64));
    g.bench_function("utf8_decode_mixed", |b| {
        b.iter(|| drain_decoder(black_box(&mixed)))
    });
    g.throughput(Throughput::Bytes(invalid.len() as u64));
    g.bench_function("utf8_decode_invalid", |b| {
        b.iter(|| drain_decoder(black_box(&invalid)))
    });
    g.finish();
}

/// Deterministic 10 000-char sample for the `script(char)` lookup.
/// Drawn by cycling a fixed multi-script string so every run sees the
/// same char distribution (§9.2 cross-session parity).
fn script_sample_10k() -> Vec<char> {
    const BASE: &str = "aÄαбあ中한عא𐐀";
    let base_chars: Vec<char> = BASE.chars().collect();
    (0..10_000)
        .map(|i| base_chars[i % base_chars.len()])
        .collect()
}

fn bench_script_lookup(c: &mut Criterion) {
    let sample = script_sample_10k();
    let mut g = c.benchmark_group("ucd");
    g.throughput(Throughput::Elements(sample.len() as u64));
    g.bench_function("script_lookup_batch", |b| {
        b.iter(|| {
            for ch in black_box(sample.as_slice()) {
                black_box(script(*ch));
            }
        })
    });
    g.finish();
}

criterion_group!(
    misc_benches,
    bench_fold,
    bench_width,
    bench_utf8,
    bench_script_lookup
);
criterion_main!(misc_benches);
