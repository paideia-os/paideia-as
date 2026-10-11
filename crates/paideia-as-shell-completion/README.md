# paideia-as-shell-completion

**Status:** Shipped — R228.M1 tab-completion substrate for the semantic shell.

Given the user's edit buffer plus a byte-offset cursor, tokenises
the pre-cursor slice via `paideia-as-shell-lex` and, based on the
last token's `Context`, emits a `CompletionResponse` naming the
byte range the caller should overwrite and a list of `Candidate`s
ranked solely by ASCII case-sensitive `starts_with`. Smarter ranking
is a follow-up round.

## Example

```rust
use paideia_as_shell_completion::complete;

let resp = complete("ls -", 4);
// resp.range == 3..4, candidates == ["-l", "-a", …]
```

## See also

- `paideia-as-shell-lex` — the pre-cursor tokeniser.
- `paideia-as-shell-cmd` — command registry queried for
  flag/arg candidates.
