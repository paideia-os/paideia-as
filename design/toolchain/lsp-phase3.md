# LSP architecture (Phase 3 m4)

**Status:** Phase 3 m4 closure appendix.
**Scope:** Documents the m8 (Phase 2) + m4 (Phase 3) LSP architecture
after the elaborator-driven-LSP migration.

## 0. Origin

The original LSP spec was section coverage in upstream `paideia-os/paideia-os`'s
`editor-support.md`. This appendix is the local companion mirroring the
m11-005 / m1-013 / m2-006 pattern; the upstream cross-reference lands as a
follow-up PR.

## 1. Why Phase 3 rewires LSP

m8 (Phase 2) shipped 11 LSP handlers (textDocument sync, publishDiagnostics,
hover, definition, references, completion, code actions, formatting, semantic
tokens, inlay hints) via tower-lsp. The handlers were correctness-of-shape
but their semantics relied on **lexical / textual stand-ins**:

- m8-006 hover used a `"linear:"` prefix heuristic on the identifier text.
- m8-007 definition / references walked the document with regex-style
  occurrence matching.
- m8-009 completion matched keywords + identifiers via lexical scope.
- m8-013 inlay hints emitted a static `: <type>` placeholder.

Phase 3 m4 replaces these stand-ins with **elaborator queries**. The
load-bearing piece is the **PositionIndex** side-table (m4-001) that maps
`(FileId, ByteOffset)` → elaborator result. Per-handler issues then port
each m8 handler to consume the index.

## 2. The PositionIndex (m4-001)

`paideia-as-elaborator::position_index::PositionIndex`:

```rust
pub struct PositionEntry {
    pub span_start: ByteOffset,
    pub span_end: ByteOffset,
    pub type_id: Option<TypeId>,
    pub lin_class: Option<LinClass>,
    pub effect_row_id: Option<u32>,
    pub cap_set_id: Option<u32>,
}

pub struct PositionIndex { files: HashMap<FileId, Vec<PositionEntry>>, }
```

API:
- `insert(file, entry)` during walker passes.
- `finish()` sorts each file's entries by `span_start` for `O(log n)`
  binary-search lookup.
- `at(file, pos) -> Option<&PositionEntry>` finds the smallest containing
  span.
- `clear_file(file)` for per-file invalidation (m4-006).

The `Option` fields encode "elaborator hasn't filled this slot yet" —
relevant during partial elaboration (e.g., a function whose effect row
isn't fully resolved because a handler is missing).

## 3. The NameResolutionTable (m4-003)

`paideia-as-elaborator::name_resolution::NameResolutionTable`:

```rust
pub struct NameResolutionTable {
    uses: HashMap<Span, Span>,       // use_span -> def_span
    references: HashMap<Span, Vec<Span>>,
}
```

API: `record(use_site, def_site)` + `definition_of(use_site)` +
`references_of(def_site)`.

Separated from PositionIndex because definitions and references are a
graph relation, not a position-keyed lookup.

## 4. Per-handler architecture (m4-002..005)

### 4.1 Hover (m4-002)

`hover` queries `PositionIndex.at(file, offset)` → formats type / class /
effects / capabilities into markdown. The `"linear:"` prefix heuristic is
retired.

### 4.2 Definition + References (m4-003)

`definition` queries `NameResolutionTable.definition_of(span)` → LSP
location. `references` queries `references_of(def_span)` → list of
locations. Cross-document fixtures exercise the import-aware path
(`tests/lsp-harness/corpus/cross-document/`).

### 4.3 Completion (m4-004)

`CompletionContext` enum:
- `MemberAccess { receiver_type: Option<TypeId> }` — `foo.<cursor>`.
- `TypeAnnotation { in_scope_types: Vec<String> }` — `let x : <cursor>`.
- `Default` — lexical fallback.

The detector queries `PositionIndex.at(file, cursor - 1)` to find the AST
node just before the cursor, then routes to the appropriate branch.

### 4.4 Inlay hints (m4-005)

For each `let` / `val` binding without an explicit type annotation, query
`PositionIndex.at(file, ident_offset)` for the inferred type and render
`: <type>` after the identifier.

## 5. Incremental engine integration (m4-006)

`QueryEngine.invalidate_module(uri)` clears the file's PositionIndex slice
via `clear_file(FileId)` without forcing a workspace re-elaboration. Only
the affected file's dependents are marked dirty.

The m8-014 latency probe is reactivated; it asserts the per-file
invalidation completes in < 100 ms. Today the probe measures near-zero
work because PositionIndex isn't populated yet; the probe gains its
intended meaning when the walker-side population lands.

## 6. Phase-3-m4 honesty

m4 issues #503..508 shipped the **lookup paths** (the LSP-side wiring,
the side-tables, the engine integration). The corresponding **walker-side
population** — where each elaborator pass inserts entries into the
PositionIndex and NameResolutionTable — lands incrementally as a wider
m4 closure activity. This is documented per handler:

- Hover handler wires the `PositionIndex.at` call; today returns "no info
  available" for most positions until walkers populate.
- Definition / references handlers wire the `NameResolutionTable`
  lookups; today return empty until elaborator name resolution records
  the relations.
- Completion's `MemberAccess` branch queries the receiver TypeId; today
  receives `None` and falls back to lexical.
- Inlay hints renders `: ???` placeholder until walker populates the
  type slot.

The lsp-harness tests for each handler assert the gated shape (e.g., "no
info" for hover) so a real-rewrite landing in a future PR breaks the
test and forces an honest update.

## 7. Phase-2 m8 deferrals retired

The m8-006..009 design notes that documented synthetic-class-inference
heuristics are retired as of m4. The retired patterns:

- **"linear: prefix lookup"**: replaced by PositionIndex hover (m4-002).
- **"textual occurrence matching"**: replaced by NameResolutionTable
  (m4-003).
- **"keyword + lexical-scope completion"**: replaced by elaborator-typed
  CompletionContext (m4-004).
- **"static : <type> placeholder"**: replaced by elaborator-driven inlay
  hint (m4-005).
- **"workspace re-elaboration on every file edit"**: replaced by
  per-file QueryEngine.invalidate_module (m4-006).

## 8. Forward links

- Walker-side population: incremental work that activates each m4
  handler's full behaviour. No dedicated milestone — lands as the
  elaborator's existing walkers grow PositionIndex / NameResolutionTable
  inserts.
- Upstream `editor-support.md` cross-reference: follow-up PR on
  `paideia-os/paideia-os`.
- Future LSP coverage: semantic-token elaborator integration (m8-012
  ships syntactic tokens; semantic tokens need elaborator
  consultation), workspace symbols, call hierarchy.
