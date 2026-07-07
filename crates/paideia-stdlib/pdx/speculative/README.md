# Speculative Phase-4 Stdlib Modules

This directory contains `.pdx` files that represent aspirational Phase-4 (v0.16+) stdlib implementations. They **do not parse** under the current (Phase-3) grammar because they use features planned for later milestones:

- Top-level `fn` declarations (planned Phase-4)
- Nominal `record Name { ... }` syntax (planned Phase-4)
- `linear self` parameter discipline (planned Phase-4)
- `use paideia.raw_mem;` imports (planned Phase-4)

## Contents

### Collections

- **`vec.pdx`** and `vec_*.pdx` — Vector implementation (21 files)
- **`hashmap.pdx`** and `hashmap_*.pdx` — Hash map / open-addressing table (12 files)
- **`iterator.pdx`** and `iterator_*.pdx` — Iterator trait and adapters (12 files)

### I/O

- **`file.pdx`** and `file_*.pdx` — File handle operations (7 files)

### System Memory

- **`system_alloc.pdx`**, `system_alloc_decl.pdx`, `system_alloc_in_linux_block.pdx` — Low-level allocator integration (3 files)

## Status

These files serve as **design placeholders and integration tests** for the Phase-4 stdlib expansion roadmap. They are **provenance** for the work described in:

> `design/roadmap/paideia-as-v0.13-through-v0.20.md:255` — Phase 4 stdlib capabilities roadmap

When Phase-4 grammar milestones open (v0.16+), these files will be:

1. **Rewritten** to use the then-current grammar
2. **Registered back** into the parse-test suite
3. **Elaborated** and **type-checked** against the Phase-4 elaborator

Until then, they remain in this directory as read-only design reference.

## Test Removal

As of v0.13 release, the parse-test entries for these files have been removed from `tests/parse_pdx.rs` to keep the test suite clean. Resumption of these tests will occur when the necessary grammar features land.
