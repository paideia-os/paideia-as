# Standard library (Phase 4 m11)

**Status:** Phase 4 m11 closure appendix.
**Scope:** Documents the paideia-stdlib bring-up shipped across m11-001..008.

## 0. What's in m11

Phase 4 m11 ships the **smallest stdlib that makes PaideiaOS subsystem code idiomatic**. The contents:

| Module             | Provides                                               | Issue   |
|--------------------|--------------------------------------------------------|---------|
| `option.pdx`       | `Option<T>` + 5 methods                                | m11-001 |
| `result.pdx`       | `Result<T, E>` + 5 methods                             | m11-001 |
| `vec.pdx`          | `Vec<T>` dynamic array + 9 methods                     | m11-002 |
| `string_ops.pdx`   | `String` / `Str` ops (push_str, len, as_str, etc.)     | m11-003 |
| `hashmap.pdx`      | `HashMap<K, V>` open-addressing + 7 methods            | m11-004 |
| `io.pdx`           | Stdin / Stdout / Stderr + print!/eprintln! sketches    | m11-005 |
| `file.pdx`         | File + Read + Write traits                             | m11-006 |
| `iterator.pdx`     | Iterator trait + Map/Filter adapters + 6 default methods| m11-007 |

Plus carry-overs from earlier milestones:
- `alloc.pdx` (m10-001 Allocator trait + Layout).
- `bump.pdx` / `arena.pdx` / `system_alloc.pdx` (m10-002..004 allocators).
- `box.pdx` (m10-005 Box<T>).
- `string.pdx` (m8-003 heap String).

Smoke corpus: `tests/stdlib-smoke/` workspace member with 8 end-to-end programs, including a 135-LOC kitchen-sink that composes Vec + HashMap + String + IO (m11-008).

## 1. Honesty: what's parser-only vs activated

Per the m11 pattern, every module ships **parser-clean .pdx** that exercises the surface syntax. Full elaborator-side activation gates on:

- **m1 walker hookups**: the elaborator's per-AST-kind walk surface — needed for method-dispatch resolution, generic-argument substitution, trait-impl lookup.
- **m9-006 monomorphisation pass**: needed for actually-generated code per `Vec<u64>` vs `Vec<u8>` instantiation.
- **m4-m6 borrow stack**: needed for `&mut Self` method receivers; current move-Self workaround retires.

Today the stdlib's surface is the test of paideia-as's grammar coverage. As the elaborator catches up, the stdlib activates incrementally — same pattern as Phase 3 m4 LSP (lookup paths shipped first, walker-side population follows).

## 2. Dependency graph

```
                  alloc.pdx (m10-001)
                       │
              ┌────────┼────────┐
              │        │        │
         bump.pdx  arena.pdx  system_alloc.pdx
              │        │        │
              └────┬───┴────────┘
                   │
              box.pdx (m10-005)
                   │
         ┌─────────┼──────────┐
         │         │          │
  vec.pdx     string.pdx   hashmap.pdx
         │         │          │
         └─────────┼──────────┘
                   │
            iterator.pdx
                   │
              ┌────┴────┐
              │         │
          io.pdx    file.pdx
              │         │
              └────┬────┘
                   │
        stdlib-smoke/ (m11-008)
```

m9 (generics + traits) is the cross-cutting dependency for every module that uses `<T>` or `trait`. m9 closed before m11; m11 builds on it as the natural progression.

## 3. Q3 dual-default + stdlib

Per m10-006's Q3 resolution, the ambient allocator is target-dependent:

- **PaideiaOS targets**: Arena.
- **Host targets**: SystemAllocator.

Every stdlib module that allocates (Vec, String, HashMap, Box) reads the ambient allocator at call time. Allocator-generic stdlib API (every container takes an explicit `A: Allocator` parameter) is **deferred** to m11 follow-up — the first round picks the ambient and uses it.

## 4. Diagnostic catalog additions

m11 introduces no new diagnostic codes — every effect / capability / trait-bound check is reused from earlier milestones:

- `!{RawMem} @{paideia.raw_mem}` from m1-005.
- `!{IO} @{paideia.io}` from m11-005.
- Trait-bound errors via T0514 (m9-005).
- Coherence errors via T0513 (m9-004).

This is intentional. m11's job is composition, not new error surface.

## 5. Out of scope for Phase 4

The following are NOT in m11 and not planned for Phase 4:

- **Async / await**: requires effect handlers + cooperative scheduler. Phase 5+ design.
- **Threading primitives** (Mutex, RwLock, Atomic): requires Phase 5+ atomic-instruction encoder + scheduler interaction.
- **Networking** (TcpStream, UdpSocket): requires Phase 5+ IO multiplexing + DNS resolver.
- **Filesystem walking** (read_dir, etc.): not in m11-006; m12 tooling might add.
- **Char type** (Unicode codepoint): Str/String use `u8` today; Char comes with proper Unicode-handling pass.
- **Cell / RefCell** (interior mutability): requires Phase 5+ thinking about the borrow-checker / linearity interaction with shared mutation.
- **Smart pointers beyond Box**: Rc/Arc require reference counting + Send/Sync. Phase 5+.
- **Standard collections beyond HashMap/Vec**: BTreeMap, BTreeSet, BinaryHeap, VecDeque, LinkedList — future m11 follow-up if PaideiaOS subsystems demand them.
- **Serialization / formatting** (format!, Debug derivation): m9-008 ships derive infrastructure; m11 follow-up wires per-type Debug output once the elaborator chokepoint activates.

## 6. PaideiaOS subsystem impact

With m11 shipped, PaideiaOS subsystem authors gain access to:

- **Bounded growth**: `Vec<T>` for syscall arg lists, IPC ring buffers, process tables.
- **Keyed lookup**: `HashMap<K, V>` for capability tables, file-descriptor maps, process scheduler queues.
- **Heap-owned strings**: `String` for printk-style output, syscall argument paths, device names.
- **Trait-driven iteration**: `Iterator` over kernel collections without writing tail-recursive accumulators by hand.
- **Result-based error returns**: `Result<T, IoError>` for fallible syscall implementations.

With these, the next PaideiaOS milestone (kernel-banner-via-capability-smoke) becomes substantially less unsafe-block-heavy. The earlier "kernel data structures live inside `unsafe { }`" concern recedes once Vec / HashMap / String are idiomatic.

## 7. Forward links

- **m1 walker hookups**: activates method-dispatch, generic substitution, trait-impl lookup — the load-bearing elaborator work for the stdlib to work end-to-end.
- **m13 self-hosting groundwork**: paideia-as itself can start using paideia-stdlib once m1 walker chokepoint is closed.
- **PaideiaOS m1**: first subsystem to use stdlib types. Likely a serial-console handler using String + Iterator.
- **Phase 5+ async / threading / networking**: built on top of m11's foundation.
