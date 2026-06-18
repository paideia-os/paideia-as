# end-to-end smoke harness

A self-contained workspace member whose only job is to walk a corpus
of `.pdx` files and assert paideia-as's front end emits the right set
of diagnostic codes (S, F, C, T categories) on each one.

The harness invokes `paideia-as build` via subprocess rather than calling
the elaborator pipeline directly. This ensures the test validates the
actual CLI output that end users see, not a synthetic in-process codepath.

## Layout

```
tests/end-to-end/
├── Cargo.toml
├── README.md
├── src/lib.rs               # the `codes_for(path)` harness
├── tests/runner.rs          # integration test entry point
└── codes/
    ├── s0900_never_used.pdx
    ├── s0900_never_used.expect
    ├── s0901_overused.pdx
    ├── s0901_overused.expect
    ├── s0903_out_of_order.pdx
    ├── s0903_out_of_order.expect
    ├── s0906_branch_mismatch.pdx
    ├── s0906_branch_mismatch.expect
    ├── s0907_illegal_capture.pdx
    ├── s0907_illegal_capture.expect
    ├── f1100_unhandled_effect.pdx
    ├── f1100_unhandled_effect.expect
    ├── f1101_handler_mismatch.pdx
    ├── f1101_handler_mismatch.expect
    ├── f1102_handler_order.pdx
    ├── f1102_handler_order.expect
    ├── f1105_row_mismatch.pdx
    ├── f1105_row_mismatch.expect
    ├── f1106_pure_violation.pdx
    ├── f1106_pure_violation.expect
    ├── c1300_missing_cap.pdx
    ├── c1300_missing_cap.expect
    ├── t0501_type_mismatch.pdx
    └── t0501_type_mismatch.expect
```

## Running

```sh
cargo test -p paideia-end-to-end
```

This harness provides two tests:

1. **`codes_corpus_matches_expect_files`** — currently `#[ignore]`'d (see below).
   When run, walks every `.pdx` in `codes/`, compares emitted codes to its
   `.expect` sidecar, prints a clean diff on mismatch.

2. **`expect_files_cover_every_listed_code`** — NOT ignored. Walks `codes/`,
   parses each `.expect`, and asserts every code in the acceptance criteria
   (S0900, S0901, S0903, S0906, S0907, F1100, F1101, F1102, F1105, F1106,
   C1300, T0501) appears at least once. This catches "fixture missing for
   code X" regressions today, even without the walker plumbing being complete.

## Why the corpus test is `#[ignore]`'d at m1

The IR carries only `IrKind` (no structured payloads). The LineraityWalker,
EffectWalker, CapabilityWalker, and TypeWalker run end-to-end but cannot
fire diagnostics on real source until m2/m5 inject:

- **Linearity classes** (unrestricted, linear, ordered) per binding — m2
- **Effect/capability metadata** — m5
- **Type payloads** — m3

The fixtures define the *intent* that each code *should* trigger once these
payloads are available. Each fixture is minimal, well-formed `.pdx` source
that expresses the semantic violation in a way the grammar accepts.

Run with `--include-ignored` to see the fixtures that will activate once
structured payloads land:

```sh
cargo test -p paideia-end-to-end -- --include-ignored
```

## Adding a fixture

For a **new code**: drop the `.pdx` file into `codes/` plus a sidecar
`<stem>.expect` listing the expected codes — one code per line
(format: `Cxxxx`, `Fxxxx`, `Sxxxx`, or `Txxxx`). Blank lines and
`#`-prefixed comments are allowed.

Example `.expect` file:

```text
# codes/f1100_unhandled_effect.expect
F1100   # perform with no enclosing handler
```

For a fixture to be *complete*, it must:

1. Be syntactically valid `.pdx` source the current parser accepts.
2. Clearly express (in comments) the semantic violation the code should catch.
3. Have a sidecar `.expect` file that lists the code(s) it should emit.
4. Pass `expect_files_cover_every_listed_code` (i.e., the test runs without
   errors).

## Implementation status

- m1: Fixtures present, harness runs, one test (`expect_files_cover_every_listed_code`)
  is active and green. Corpus test is `#[ignore]`'d pending structured IR payloads.

- m2: Linearity-class payloads injected at lowering; S0900/S0901/S0903/S0906/S0907
  codes should fire on real source. Corpus test may be un-`#[ignore]`'d.

- m5: Effect/capability metadata injected; F1100/F1101/F1102/F1105/F1106 and
  C1300 codes should fire. Corpus test continues.

- m3: Type payloads for unification; T0501 codes should fire.
