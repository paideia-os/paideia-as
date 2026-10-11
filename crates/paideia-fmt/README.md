# paideia-fmt

**Status:** **COMPLETE as of paideia-as v0.36.82** (FMT-17 capstone, `paideia-as#1581`); carried unchanged into v0.37.0.

The paideia-as source formatter. All 20 FMT-N wave tickets
(`#1561`–`#1580`) have landed and the three corpus sweeps (FMT-13
idempotence, FMT-14 parser-equivalence, FMT-15 comment-preservation)
are green across the full workspace `.pdx` corpus (`tests/` +
`examples/`). The five follow-up items F1..F5 captured in the
acceptance report are polish, not regression.

Pipeline (lex → parse → attach trivia → AST walk → Doc layout):

```
source
  │
  ▼  paideia-as-lexer
tokens + trivia stream  ─────────────┐
  │                                   │
  ▼  paideia-as-parser                │
AST + CST  ──────────────┐            │
  │                       ▼           ▼
  │               cst.rs + attach.rs (trivia-preserving CST, span-containment attachment)
  │                       │
  ▼  walker.rs            ▼
items/expression/type/pattern/attribute emitters (5 modules, ~4600 LOC combined)
                          │
                          ▼
                 doc.rs (Wadler Doc IR)
                          │
                          ▼
                 layout.rs (classic "best" layout)
                          │
                          ▼
                    formatted output
```

## Example

```rust
use paideia_fmt::{format, FormatConfig};

let src = "module M = structure { let  x :u64=1 }";
let out = format(src, &FormatConfig::default()).unwrap();
// out == "module M = structure {\n  let x : u64 = 1\n}\n"
```

CLI integration lands in `paideia-as fmt`:

```sh
paideia-as fmt 'crates/**/*.pdx' --list-different
```

## See also

- [`../../design/toolchain/paideia-fmt-style-spec.md`](../../design/toolchain/paideia-fmt-style-spec.md)
  — canonical style spec (**FROZEN v1.0**).
- [`../../design/toolchain/paideia-fmt-acceptance.md`](../../design/toolchain/paideia-fmt-acceptance.md)
  — FMT-17 acceptance report (**COMPLETE as of v0.36.82**).
- [`../../CHANGELOG.md`](../../CHANGELOG.md) §v0.36.82 — wave-level
  closure summary.
