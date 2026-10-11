# paideia-as-unicode — criterion benches (UNI-8)

Lands the throughput + p99 harness `design/toolchain/paideia-as-unicode-spec.md` §9 commits the crate to. Benches are **reported**, not enforced, at this wave; a regression gate (fail-build on >20% drop) is deferred per spec §9.1.

## Layout

| File | Covers | Spec rows (§9.1) |
|---|---|---|
| `normalize.rs` | `nfc_normalize`, `nfd_normalize`, `nfkc_normalize`, `nfkd_normalize` | NFC rows + the three other-form ASCII fast paths |
| `segment.rs` | `line_break_opportunities`, `wrap_at_width` | UAX #14 reflow (not pinned in §9.1 — reported alongside) |
| `misc.rs` | `fold`, `fold_default`, `str_width`, `Utf8Decoder::next`, `script` | fold / width / UTF-8 decoder / UCD rows |

Each file registers one `criterion_main!`; `[[bench]]` entries in `Cargo.toml` carry `harness = false` so criterion drives the harness directly.

## Running

```bash
# Everything in this crate (fast path for a wave-N regression check):
cargo bench -p paideia-as-unicode

# One file only:
cargo bench -p paideia-as-unicode --bench normalize
cargo bench -p paideia-as-unicode --bench segment
cargo bench -p paideia-as-unicode --bench misc

# One function (criterion matches by substring of the id):
cargo bench -p paideia-as-unicode --bench normalize -- nfc_ascii_fast_path
```

The usual criterion flags apply: `--save-baseline <name>` and `--baseline <name>` make before/after comparison across a change a one-command operation.

## Expected thresholds (spec §9.1)

| Bench id | Target | Rationale |
|---|---|---|
| `nfc_ascii_fast_path` | ≤100 ns/call at 32 B (we exercise 1 KiB; divide to compare) | R222.M5 ≤50 µs command-lookup budget |
| `nfc_mixed_latin` | ≤500 ns/call at 32 B | common Latin command name path |
| `nfc_cjk_long` | ≤1 µs/char | NFC worst case w/ `unicode-normalization` |
| `nfkc_ascii_fast_path` / `nfd_ascii_fast_path` / `nfkd_ascii_fast_path` | ≤2 µs/char on worst case; ASCII fast path is byte-scan only | decomposition ~2× composition cost |
| `line_break_opportunities_lorem` / `_cjk` | not pinned in §9.1 — report nanoseconds and bytes/sec | UAX #14 driver cost |
| `wrap_at_width_lorem_80` / `_cjk_80` | not pinned — report; R229 renderer reflow case | ASCII fast path vs. grapheme/width composition |
| `fold_default_ascii` / `_mixed` / `fold_turkic_mixed` | not pinned — report | CaseFolding.txt driver + Turkic I/i branch |
| `str_width_ascii` | ≤2 ns/byte | byte-scan + grapheme-count path |
| `str_width_cjk` | ≤15 ns/byte | per-cluster width lookup |
| `utf8_decode_valid_ascii` | ≤3 ns/call per codepoint | byte-dispatch, no table |
| `utf8_decode_mixed` | ≤8 ns/call for 4-byte emoji | four-byte decode + validate |
| `utf8_decode_invalid` | not pinned — report | error-path throughput tracker |
| `script_lookup_batch` | not pinned — report ns/elem over 10 000 chars | multi-script dispatch cost |

Thresholds marked "not pinned" are tracked so a regression surfaces at the next wave's report, per spec §9.2 "comparable to another number taken on the same silicon".

## Interpreting the HTML report

Criterion writes an HTML report to `target/criterion/` whenever a bench runs with `html_reports` on (which this crate pulls in by `features`). Entry point: `target/criterion/report/index.html`. Each bench id gets its own subdirectory with:

* **`index.html`** — mean, median, user-visible estimate (point + 95% CI) and a kernel-density plot of the per-iteration samples.
* **`iteration_times.svg`** — the raw per-iteration cost, useful for spotting a bimodal distribution (fast / slow page faults, GC-like spikes, thermal throttling).
* **`pdf.svg`** — probability density of the per-iteration estimates; a narrow peak with no second mode is the healthy shape.
* **`regression.svg`** — plot of total time vs. iterations; slope is the mean. A nonzero intercept exposes per-call setup leakage.

For a wave-N regression check, open the group index (e.g. `target/criterion/nfc/report/index.html`) and compare the point estimates to the previous wave's numbers in `.plans/scratch/bench-UNI-N.md` (spec §9.2). A >20% drop is a flag for a follow-on hardening ticket; a <20% drop is noted but not actioned at this wave.

## Fixture discipline (spec §9.2)

* Inputs are literal `&str`s or deterministic `String::push_str` loops — never timestamp-seeded. Cross-session comparison is apples-to-apples.
* Each bench input is wrapped in `black_box` at the call site so the compiler cannot constant-fold the body.
* `criterion` handles timing primitives; no `unsafe` lives under `benches/`.
* Numbers are only comparable to numbers taken on the same silicon. Attach a `uname -m` + `cat /proc/cpuinfo | grep model.name` header to any `.plans/scratch/bench-UNI-N.md` report so cross-kernel reads stay honest.
