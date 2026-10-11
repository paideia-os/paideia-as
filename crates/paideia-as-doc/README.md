# paideia-as-doc

**Status:** Shipped — the `paideia-as doc` HTML doc generator.

Walks a parsed `.pdx` file's AST and emits HTML with one section
per top-level item (`let` / `fn` / `struct` / `enum` / `trait` /
`impl` / `effect` / `capability` / `macro`), extracting name,
signature, and leading doc-comment (`///` style preferred; falls
back to the first `//` block). Cross-references match `[Name]`
patterns in doc text to the item's anchor.

## Example

```sh
paideia-as doc examples/15_sum_array.pdx -o /tmp/sum_array.html
```

## See also

- [`../../design/toolchain/tooling-phase4.md`](../../design/toolchain/tooling-phase4.md)
  — Phase 4 m12 closure appendix.
