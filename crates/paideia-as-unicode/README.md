# paideia-as-unicode

The semantic-shell Unicode substrate: UTF-8 decoding, NFC/NFD/NFKC/NFKD
normalization, case folding, grapheme-cluster and line-break
segmentation, UAX #31 / UTS #39 identifier predicates, UCD property
queries, UAX #11 East-Asian width, and the fingerprint-golden test
harness every UCD upgrade is vetted against. Intended consumers are the
`paideia-as-shell-*` crates and the forthcoming paideia-as LSP; a
general-purpose Unicode library can use it too, but the surface is
scoped to what the shell needs. **Out of scope:** Bidi (UAX #9), IDNA
(UTS #46), and the Unicode Collation Algorithm (UTS #10). If you need
any of those three, pull in a dedicated crate — this one deliberately
does not grow sideways into collation territory.

## UCD pin and upgrade policy

UCD **15.1** across every module. The pin lives in the workspace
dependencies on the `unicode-*` crates family; a UCD bump is a
coordinated workspace update, never a per-crate one. The upgrade
protocol — regeneration of the fingerprint golden files, the diff
review on `tests/ucd-corpus/`, and the `--ignored` conformance sweep
— is set out in
[`design/toolchain/paideia-as-unicode-spec.md`](../../design/toolchain/paideia-as-unicode-spec.md).
Treat that document as the source of truth for every contract this
crate's public API advertises.

## Modules at a glance

Each entry links to the module docs; the full type signatures are in
`cargo doc --open -p paideia-as-unicode`.

### `nfc` — NFC fast path

```rust
use paideia_as_unicode::{is_nfc, nfc_normalize};
assert_eq!(nfc_normalize("cafe\u{0301}"), "café");
assert!(is_nfc("paideia-as"));
```

### `norm` — all four Unicode normalization forms

```rust
use paideia_as_unicode::{Form, normalize};
let nfkc = normalize("ﬃ", Form::NFKC); // ligature → "ffi"
assert_eq!(nfkc, "ffi");
```

### `case` — UCD case folding (default + Turkic)

```rust
use paideia_as_unicode::{CaseFoldLocale, eq_ignore_case_unicode};
assert!(eq_ignore_case_unicode("Straße", "STRASSE", CaseFoldLocale::Default));
```

### `segment` — UAX #14 line breaks + width-aware wrap

```rust
use paideia_as_unicode::wrap_at_width;
let lines = wrap_at_width("Hello, 日本語の世界へ.", 10);
// each `line` fits in 10 rendered columns (CJK counts as two).
```

### `grapheme` — UAX #29 extended grapheme clusters

```rust
use paideia_as_unicode::{grapheme_count, grapheme_advance};
assert_eq!(grapheme_count("é\u{0301}🇯🇵"), 2);
let next = grapheme_advance("🇯🇵abc", 0); // past the flag cluster.
```

### `ident` — UAX #31 + UTS #39 identifier predicates

```rust
use paideia_as_unicode::{IdentProfile, check_ident};
assert!(check_ident("naïve", IdentProfile::Default).is_ok());
assert!(check_ident("paypa\u{0430}l", IdentProfile::AllowedChars).is_err());
```

### `ucd` — General_Category / Script / Block + group predicates

```rust
use paideia_as_unicode::{general_category, is_letter, script};
assert!(is_letter('A'));
let s = script('日');
let gc = general_category('.');
let _ = (s, gc);
```

### `width` — UAX #11 East-Asian width (Latin + East-Asian modes)

```rust
use paideia_as_unicode::{str_width, WidthMode, str_width_in};
assert_eq!(str_width("Hello"), 5);
assert_eq!(str_width("日"), 2);
assert_eq!(str_width_in("α", WidthMode::EastAsian), 2); // Ambiguous → Wide
```

### `fingerprint` — golden harness for UCD-sensitive output

Internal to the test tree. Every UCD-sensitive module pins its
output to `tests/fingerprints/<name>.txt`; a diff on a UCD upgrade is
the review signal. See
[`design/toolchain/paideia-as-unicode-spec.md`](../../design/toolchain/paideia-as-unicode-spec.md)
§ "Fingerprint-golden protocol".

## Running the examples

The `examples/` tree holds four runnable demos:

```shell
cargo run --example nfc_demo       -p paideia-as-unicode
cargo run --example line_wrap      -p paideia-as-unicode
cargo run --example case_fold      -p paideia-as-unicode
cargo run --example script_detect  -p paideia-as-unicode
```

Each example is self-contained (no network, no file I/O beyond stdout),
asserts its own invariants, and prints a short summary. Read them for
the smallest working surface of each module.

## Testing

The crate ships three tiers of test discipline — ordered from fastest
to most exhaustive:

```shell
# Fast suite: unit + property tests, ~a few seconds.
cargo test -p paideia-as-unicode

# Conformance sweeps against the vendored UCD corpus. Marked `#[ignore]`
# so a `cargo test` run stays fast; opt in explicitly.
cargo test -p paideia-as-unicode -- --ignored

# Criterion throughput + p99 latency harness. Reports land under
# `target/criterion/`; see `benches/README.md`.
cargo bench -p paideia-as-unicode

# UTF-8 decoder + round-trip fuzzers.  Requires `cargo install
# cargo-fuzz` and a nightly toolchain.
cd fuzz && cargo fuzz run utf8_decoder
cd fuzz && cargo fuzz run utf8_roundtrip
```

The fingerprint goldens under `tests/fingerprints/` are the signal on a
UCD upgrade — a diff there is a deliberate human review, not a flaky
test, and must be committed in the same PR as the dependency bump.

## Design

* [`design/toolchain/paideia-as-unicode-spec.md`](../../design/toolchain/paideia-as-unicode-spec.md)
  — authoritative specification. The surface, invariants, conformance
  posture, and testing discipline of every module in this crate live
  there; subsequent `UNI-N` tickets cite it.
* Fingerprint-golden protocol — § "Fingerprint-golden protocol" of the
  spec above. Any UCD-sensitive output is pinned to a text file under
  `tests/fingerprints/`; UCD upgrades land as a visible diff against
  those files.
* Dependency choice rationale — the module headers in `src/` cite the
  individual UCD-vendor crate the module leans on. The combined UCD
  15.1 tables are ~300 KiB of static data; hand-maintaining them would
  dwarf every other module in this crate.
