# UCD 15.1.0 conformance corpus

This directory holds the vendored Unicode Character Database (UCD) 15.1
conformance test files consumed by `tests/ucd_conformance.rs`
(paideia-as#1594, UNI-7).

The harnesses are gated behind `#[ignore]` and do **not** download at
test time; checkout the files manually, then run the sweep with:

```
cargo test -p paideia-as-unicode -- --ignored --nocapture
```

## Files

| File                      | Spec                   | Harness                                      |
| ------------------------- | ---------------------- | -------------------------------------------- |
| `GraphemeBreakTest.txt`   | UAX #29 §3             | `ucd_conformance_grapheme_break_test`        |
| `NormalizationTest.txt`   | UAX #15 §5             | `ucd_conformance_normalization_test`         |
| `LineBreakTest.txt`       | UAX #14 §7             | `ucd_conformance_line_break_test`            |
| `IdentifierStatus.txt`    | UTS #39 §3.1 / Table 1 | `ucd_conformance_identifier_status_test`     |

## Vendoring policy

* Only files that are **cited by `ucd_conformance.rs`** live here. Full
  UCD 15.1 is ~15 MiB; vendoring just the conformance corpora keeps the
  repo under one MB per file and well under the GitHub recommended
  package size.
* The four files are refreshed when the UCD version pin changes
  (currently `unicode-normalization` tracks UCD 15.1). A UCD version
  bump creates a sibling `tests/ucd-corpus/<version>/` directory; the
  harness's `CORPUS_DIR` constant moves at the same commit.
* Each file ships as **a placeholder** with a `# VENDOR: fetch from
  unicode.org on first --include-ignored run` comment plus 2–3
  hand-crafted real rows. The placeholder is enough to smoke-test the
  parser on a fresh checkout; the full sweep requires the real
  download below.

## Manual vendor step

Fetch the four files from unicode.org directly (no mirrors, so the
checksum trail matches the UCD release):

```sh
cd tests/ucd-corpus/15.1.0

curl -o GraphemeBreakTest.txt \
  https://www.unicode.org/Public/15.1.0/ucd/auxiliary/GraphemeBreakTest.txt

curl -o NormalizationTest.txt \
  https://www.unicode.org/Public/15.1.0/ucd/NormalizationTest.txt

curl -o LineBreakTest.txt \
  https://www.unicode.org/Public/15.1.0/ucd/auxiliary/LineBreakTest.txt

curl -o IdentifierStatus.txt \
  https://www.unicode.org/Public/security/15.1.0/IdentifierStatus.txt
```

The three `ucd/` files sit under the main UCD tree; `IdentifierStatus.txt`
lives under the `security/` subtree per UTS #39 §3.1.

## Harness behaviour on missing / placeholder data

* **File missing on disk** → the test logs a `SKIP — <reason>` line to
  stderr and returns `Ok(())` so a fresh checkout still green-lights
  the `#[ignore]`-gated sweep.
* **File present but comments only** → the parser yields zero data
  rows; the sweep reports `0 / 0 rows OK` and passes.
* **File present with 2–3 placeholder rows** (default checkout state)
  → the parser runs over those rows as a smoke test. The log line
  reads `(placeholder — smoke only)` instead of `(full corpus)`.
* **File present and freshly curled** → the full sweep runs. Expect
  the `uni7-ident-sweep` test to fail until the full UTS #39 allowed
  set is wired into `check_default`; see the test comment for the
  known gap.

## Smoke-row sources

The hand-crafted smoke rows cite UAX examples:

* `GraphemeBreakTest.txt` → UAX #29 §3.1 GB1/GB2 (empty), GB3 (CR×LF),
  GB4/GB5 (control break).
* `NormalizationTest.txt` → UAX #15 §5 Part 1 single-character canonical
  identity rows.
* `LineBreakTest.txt` → UAX #14 §7 mandatory break (LF) + ASCII space
  break opportunity.
* `IdentifierStatus.txt` → UTS #39 Table 1 rows for an ASCII Allowed
  letter and a Restricted historical-script letter.
