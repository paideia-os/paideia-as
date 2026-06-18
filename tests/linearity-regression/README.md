# linearity-regression harness

A self-contained workspace member whose only job is to walk a corpus
of `.pdx` files and assert paideia-as's front end emits the right set
of `S`-category (substructural) diagnostics on each one.

The harness invokes `paideia-as build` via subprocess rather than calling
the elaborator pipeline directly. This ensures the test validates the
actual CLI output that end users see, not a synthetic in-process codepath.

## Layout

```
tests/linearity-regression/
├── Cargo.toml
├── README.md
├── src/lib.rs            # the `s_codes_for(path)` harness
├── tests/harness.rs      # integration test entry point
├── accept/               # files that must emit zero S-codes
│   └── *.pdx
└── reject/
    ├── *.pdx             # files that must emit a specific S-code set
    └── *.expect          # one Sxxxx per line; `#` starts a comment
```

## Running

```sh
cargo test -p paideia-linearity-regression
```

The accept corpus runs ~0.5-2s per file (40 files total). Total wall-clock
is typically 20-40s on a modern machine. Performance is dominated by subprocess
spawn and binary startup; once the binary is warm, each `paideia-as build`
invocation is fast.

The `reject_corpus_emits_expected_s_codes` test is currently `#[ignore]`'d
because the IR carries only `IrKind` (no structured payloads). The LineraityWalker
(and other walkers) run end-to-end but cannot fire diagnostics on real source
until m2/m5 inject binding-level linearity classes and effect/capability metadata.
Run with `--include-ignored` to see the fixtures that will activate once
structured payloads land.

## Adding a fixture

For an **accept** case: drop a valid `.pdx` file into `accept/`. The
harness fails if any `S0xxx` is emitted.

For a **reject** case: drop the `.pdx` file under `reject/` plus a
sidecar `<stem>.expect` listing the expected codes — one `Sxxxx` per
line. Blank lines and `#`-prefixed comments are allowed.

```text
# reject/use_after_consume.expect
S0901   # used after consume
```

The harness fails if the emitted code set doesn't match exactly.
