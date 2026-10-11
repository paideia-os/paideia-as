# paideia-as-shell-hm

**Status:** Shipped — R225.M1 (Hindley–Milner Algorithm W core) + R225.M2 (Rémy-style row types for records) + R225.M3 (Rémy-style row polymorphism for effect rows, disjoint from record rows).

A textbook Damas–Milner type-inference engine over a pure lambda
subset extended with row-polymorphic records. The algorithmic
substrate the later R225 milestones extend (effects,
rank-restricted let-polymorphism, pipeline and Datalog surface).

Lineage: Damas & Milner 1982; Rémy 1989 ("Type Inference for
Records in a Natural Extension of ML"); Leijen 2005 ("Extensible
records with scoped labels").

## Example

```rust
use paideia_as_shell_hm::infer;

let scheme = infer("\\x -> x + 1").unwrap();
// scheme :: forall a. (a) -> a   (specialised to Int at use)
```

## See also

- [`../../design/toolchain/rank-restricted-hm.md`](../../design/toolchain/rank-restricted-hm.md)
  — rank-restricted let-polymorphism spec (R220.M11).
