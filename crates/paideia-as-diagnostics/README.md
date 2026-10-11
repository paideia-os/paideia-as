# paideia-as-diagnostics

**Status:** Shipped — the shared diagnostic substrate for every paideia-as pass.

Defines stable diagnostic identifiers, source positions, the
`Diagnostic` value type, and three renderers (human CLI, SARIF for
tooling at `<file>.pdx.sarif.json`, LSP `PublishDiagnostics` for
editors). Diagnostic codes are forward-pointer-stable: a fix-it that
worked once will keep working. The catalog rules live inline — this
crate owns both the catalog and the renderer surface.

Code families (non-exhaustive): parser `P0100`–`P0299`, type
`T0500`+, substructural `S0900`+, effects `F1100`+, optimiser
`O1500`–`O1512`, linker `B1700`–`B17FF`, HSM `Q0900`+.

## Example

```rust
use paideia_as_diagnostics::{Diagnostic, Code, Severity, VecSink};

let mut sink = VecSink::default();
sink.emit(Diagnostic::new(Code::T0500, Severity::Error, "type mismatch", span));
let human = sink.render_human();
let sarif = sink.render_sarif();
```

## See also

- `paideia-as-emit`, `paideia-as-elaborator`, `paideia-as-encoder`
  — upstream producers.
- `paideia-lsp` — downstream SARIF/LSP consumer.
