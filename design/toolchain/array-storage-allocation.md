# Array storage allocation

**Status:** landed (v0.21.0)
**Issues:** [#1308](https://github.com/paideia-os/paideia-as/issues/1308), [#1309](https://github.com/paideia-os/paideia-as/issues/1309)
**Diagnostics:** `P0211`, `T0576`

## Invariant

> A module-level binding declared `[T; N]` occupies exactly `N * sizeof(T)` bytes
> in the linked image, or the build fails.

There is no third outcome. Before v0.21.0 there was: the compiler would accept a
declaration, report `[T; N]` through the type system, and reserve fewer bytes
than that, with no diagnostic. Both paths that could do so are described below;
both are now hard errors.

This invariant matters more here than in a hosted language. paideia-os is a
kernel with no MMU-enforced separation between one static symbol and the next
inside a section — an under-allocated `.data` symbol does not fault on
overflow, it silently rewrites its neighbour. Two symbols shipped in the kernel
image in exactly that state (`runqueue`, `_loader_seed_empty_sidecar`), and the
absence of an observed failure was a property of which indices the smoke paths
happened to touch, not of the code being correct.

## The two allocation paths

A module-level `let` with an array type reaches storage through
`crates/paideia-as/src/cmd_build.rs`, which dispatches on the RHS IR kind. Two
of those branches size the symbol from the *initialiser* rather than from the
declared type, and that is where both defects lived.

### Path 1 — repeat literal `[v; N]` (#1308)

The parser produces `ExprData::ArrayRepeat { expr, count }` and defers
expansion to the elaborator. Lowering
(`crates/paideia-as-elaborator/src/lower/array_repeat.rs`) turns it into an
`IrKind::ArrayLit` with one structural child per repetition; the data pass then
packs one element per child.

The count is not carried in the AST. paideia-as stores integer literals as bare
`Placeholder` nodes holding only a `Span` — the numeric value is recovered later
by re-reading the source text (`cmd_build`'s `literal_values` pass does this
*after* lowering, which is too late for a pass that has to decide arity *during*
lowering). The original `extract_repeat_count` acknowledged this and returned
`None` unconditionally, so every repeat literal took the "count unknown"
fallback of emitting a single copy.

The fix reads the count out of the `SourceMap` at lowering time, the same
technique `unsafe_walker::immediate::extract_integer_from_span` and
`cmd_build::layout` already use.

**Design choice — failure returns zero children, not one.** When the count
cannot be resolved (a named constant, an expression, a negative value) the
expansion emits `P0211` and returns an *empty* child list. A one-element
fallback is precisely the defect being removed: it produces a symbol that looks
plausible and is wrong. An empty list produces no data entry at all, and the
error diagnostic fails the build regardless.

**Design choice — `MAX_REPEAT_COUNT = 1 << 20`.** Expansion materialises N
structural IR children, so N is an allocation. Without a bound, `[0; 1 << 40]`
is an out-of-memory crash rather than a diagnostic. One million elements covers
every realistic kernel table (`[u64; 1024]` frame metadata, `[u8; 4096]` page
buffers) with three orders of magnitude of headroom.

A future rework could replace expansion with a repeat count recorded in an IR
side table, which would make the bound unnecessary and cut memory for large
arrays. That was not taken here: expansion reuses the entire existing `ArrayLit`
machinery (data encoding, per-element width, section routing, alignment)
unchanged, whereas a side-table representation would need every consumer that
counts children to learn about it. The bound is the cheap half of that trade.

### Path 2 — explicit element list (#1309)

The data pass walks the `ArrayLit` children and packs each one that is an
`IrKind::Literal` with a resolvable value. Anything else is skipped. It then
emitted whatever it had accumulated.

Nothing compared that against the declared arity, which admitted two silent
under-allocations:

1. **Short list.** paideia-os `_frame_meta : [u64; 1024]` was written with 992
   elements — 62 rows of sixteen where the author intended 64. It linked at
   7936 bytes. Indices 992..1023 aliased the next symbol.
2. **Unencodable element.** A named binding, an arithmetic expression, or a
   negative literal (which parses as `ExprPrefix`, not `ExprLiteral`) matches
   neither `if`, contributing no bytes and no diagnostic. `[u64; 4] = [1, 2, k, 4]`
   emitted 24 bytes.

Note that (2) is the more dangerous of the two, because the source *looks*
complete. Counting elements by eye finds the short list; nothing finds the
dropped element.

The fix reconciles the emitted element count against
`declared_array_len_from_type` — a helper that already existed for the
StringLiteral truncate/pad path — or, when there is no type annotation, against
the number of elements actually written. Mismatch emits `T0576` and the entry is
not pushed.

**Scope — the guard requires that this branch actually claimed the array.**
`T0576` fires only when at least one element encoded. An array in which
*nothing* encodes is not a partially-emitted symbol; it is a shape this branch
does not own. paideia-os `_klog_files : [u64; 205] = [(rip + name_file_0), ...]`
is an array of symbol-address relocations: it produces no data entry, therefore
no symbol, and `readelf` finds nothing under that name in the linked image. That
is a real gap ([#1310](https://github.com/paideia-os/paideia-as/issues/1310)),
but a materially different one — a reference to a missing symbol is an
undefined-symbol *link* error, loud and immediate, whereas a short symbol is a
silent runtime overwrite. Reporting an arity mismatch for an unimplemented
element shape would also be a misleading message. The two cases stay separate.

**Design choice — reject rather than zero-pad.** Padding a short initialiser to
the declared length would produce a correctly sized symbol and hide the authoring
error, which is how `_frame_meta` would have shipped silently wrong-but-sized
instead of silently short. An initialiser that does not say what the type says is
a defect in the source, and the compiler says so. Authors who want N zeroed slots
have two exact spellings: `[0; N]` (path 1) or `uninit` (a `.bss` reservation
sized from the type by `compute_bss_size_from_type`).

## Diagnostics

| Code | Fires when | Meaning |
|---|---|---|
| `P0211` | lowering | `[v; N]` where N is not a constant integer literal, is zero, or exceeds `MAX_REPEAT_COUNT` |
| `T0576` | data emission | emitted element count differs from the declared `[T; N]` arity, or an element was not an encodable constant |

`T0576` is the array-literal analogue of `T0558` (declared `[u8; N]` vs actual
`@guid` / `@include_bytes` payload length, see `embed-primitives.md`). Both exist
for the same reason and are worth keeping symmetric: any construct that sizes a
symbol from something other than its declared type needs a reconciliation guard,
or it becomes a silent-truncation path.

## Testing rule

Storage bugs of this class are invisible to source-level tests — the program
type-checks, elaborates, links and often runs. They are only visible in the
linked artifact. Every guard here is therefore pinned by a **byte-exact**
integration test that builds a fixture to ELF and asserts on the symbol's linked
size *and* its bytes
(`crates/paideia-as/tests/build_emit/array_storage_arity.rs`), including one at
N = 512 specifically to catch a small-N special case. Size alone is not enough:
a regression that allocates the right number of bytes with the wrong contents is
still a regression.
