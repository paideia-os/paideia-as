# paideia-as-shell-pds

**Status:** Shipped — R227.M1 `.pds` script header pragma parser.

Consumes a `.pds` source string, walks the leading `#…` pragma
lines (after an optional `#!` shebang), and returns a typed
`PdsHeader` plus the byte offset of the body — the point the
shell-lex / shell-ast layer takes over.

```text
.pds source
    │
    ▼  paideia-as-shell-pds::parse_header
PdsHeader { capabilities, requires_paideia, imports,
            schemas, ascii, body_offset }
    │
    ▼  src[header.body_offset ..]
paideia-as-shell-lex → paideia-as-shell-ast → paideia-as-shell-eval
```

The header parser is deliberately self-contained: it does not
import the shell tokenizer. Header pragmas are ASCII lines, so no
NFC pass is paid for them.

## Example

```rust
use paideia_as_shell_pds::parse_header;

let (header, body_offset) = parse_header("#!/usr/bin/env pds\n#requires paideia 0.36\n").unwrap();
```

## See also

- `paideia-as-shell-lex`, `paideia-as-shell-ast` — downstream body
  parsers.
