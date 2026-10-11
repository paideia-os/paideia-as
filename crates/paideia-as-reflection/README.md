# paideia-as-reflection

**Status:** Shipped — R220.M1 elaborator-reflection surface.

Lands the typed `Syntax` value, the `HygienicId` substrate, the
structural walker API (`WalkAction` / `SyntaxWalker`), and the
`Elab` effect's operation signatures — the substrate hosted DSLs
use to hook themselves into the elaborator, per Christiansen &
Brady 2016 ("Elaborator Reflection: Extending Idris in Idris").

Supersedes the `MP-D5` DEFERRED gate in
`design/toolchain/macros-phase1.md` and gates R220.M2 (hygiene
wiring), R220.M3 (`@dsl_parser`), R220.M9 (LSP embed), R220.M10
(`@fingerprint`) plus the semantic-shell rounds R221..R227.

## Example

Consumed by `@dsl_parser` and the shell-* crates — this crate hosts
the surface rather than a direct user-facing API.

## See also

- [`../../design/toolchain/macros-phase1.md`](../../design/toolchain/macros-phase1.md)
  — supersession context.
- `paideia-as-shell-ast` / `paideia-as-shell-cmd` — downstream
  R220.M9 DSL embed consumers.
