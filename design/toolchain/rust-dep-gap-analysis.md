# Rust-dependency gap analysis

**Status:** Phase 4 m13-003 catalogue.
**Scope:** For every external Rust dependency across the 21 paideia-as crates, classify into one of three buckets:

- **(a)** Ports cleanly to paideia-stdlib (m11 already covers OR trivial extension).
- **(b)** Needs Phase 5 stdlib expansion (must be added to paideia-stdlib first).
- **(c)** External system service — can stay Rust (or FFI shim).

Output: gap-list of stdlib expansions blocking Tier 1 crate ports per the m13-001 inventory.

## 0. Method

Reads `Cargo.toml` of each crate. Excludes:
- `path = "..."` internal deps (all Tier 1/2/3 crates depend on each other; covered by m13-001).
- `workspace = true` deps that resolve through the root `Cargo.toml`'s `[workspace.dependencies]` table.
- Dev-dependencies that don't ship in the runtime path (insta, proptest, tempfile, jsonschema, axum) — testing infrastructure, host-only.

## 1. External dependency inventory

Aggregated across all crates' `[dependencies]` and `[build-dependencies]`:

| Crate dep         | Used by                                                  | Bucket | Rationale                                                                 |
|-------------------|----------------------------------------------------------|--------|--------------------------------------------------------------------------|
| `clap`            | paideia-as (CLI arg parsing)                             | (b)    | Argument-parser library; paideia-stdlib needs a CLI-args module. Future. |
| `miette`          | paideia-as (error rendering)                             | (b)    | Diagnostic-renderer; could be replaced by paideia-as-diagnostics output. |
| `thiserror`       | paideia-as, paideia-as-diagnostics, paideia-as-encoder, paideia-as-pq-sign, paideia-as-types | (a) | Error-trait derive; paideia-stdlib's `enum NumError`-style pattern covers it. |
| `smallvec`        | paideia-as-ast, paideia-as-encoder, paideia-as-emitter-pe, paideia-as-ir, paideia-as-types | (b) | Stack-allocated Vec; needs SmallVec<T, N> stdlib type. ~200 LoC port. |
| `static_assertions` | most crates                                            | (a)    | Compile-time const_assert macros; m11-008 `unsafe { }` sketches can host. |
| `serde`           | paideia-as-ast (test), paideia-as-diagnostics, paideia-lsp, paideia-pq-sign | (b) | Serialisation framework. Needs paideia-stdlib serialisation module + derive macros. Significant. |
| `serde_json`      | paideia-as-diagnostics, paideia-lsp, paideia-pq-sign     | (b)    | JSON encoder/decoder. Depends on `serde`.                                  |
| `toml`            | paideia-as-diagnostics, paideia-lsp, paideia-pq-sign     | (b)    | TOML encoder/decoder. Depends on `serde`.                                  |
| `unicode-ident`   | paideia-as-lexer                                         | (b)    | Unicode XID character classification. Needed for lexer identifier scanning. |
| `gimli`           | paideia-as-dwarf                                         | (b)    | DWARF reader/writer. Or hand-roll the small subset the dwarf crate uses. |
| `object`          | paideia-as-emitter-elf, paideia-as (dev)                 | (b)    | ELF/PE/Mach-O reader/writer. Most uses are read-only verification.        |
| `blake3`          | paideia-as-elaborator, paideia-as-emitter-pax, paideia-lsp, paideia-pq-sign | (b) | Hash function (BLAKE3). Needs paideia-stdlib hash module. ~500 LoC port. |
| `iced-x86`        | paideia-as-encoder (dev), paideia-as-emitter-elf (dev)   | dev    | x86 disassembler for round-trip tests. Drop in self-host path.            |
| `regex`           | paideia-as-test                                          | (b)    | Regex engine. Needs paideia-stdlib regex module OR drop and use string-prefix matching. |
| `lru`             | paideia-lsp                                              | (b)    | LRU cache. ~150 LoC port.                                                  |
| **PQ signing crates**: `argon2`, `chacha20poly1305`, `cryptoki`, `ed25519-dalek`, `hex`, `ml-dsa`, `rand_core`, `rand_chacha`, `reqwest`, `signature`, `yubihsm` | paideia-pq-sign | (c) | External crypto / HSM / HTTP libraries. Stay Rust or FFI shim; rewriting in `.pdx` is Phase 6+ work. |
| **LSP runtime**: `tower-lsp`, `tokio`                | paideia-lsp                                              | (c)    | Async runtime + tower middleware. Self-hosted LSP is Phase 6+; deferred.    |

## 2. Per-bucket totals

- **(a) Ports cleanly** (no stdlib work needed): `thiserror`, `static_assertions`. Trivial.
- **(b) Needs Phase 5 stdlib expansion**: `clap`, `miette`, `smallvec`, `serde` family (3 crates), `unicode-ident`, `gimli`, `object`, `blake3`, `regex`, `lru` — **10 distinct concerns**.
- **(c) External system service / stays Rust**: PQ signing crates (11) + LSP runtime crates (2) — **13 deps**.

The (b) list is the critical-path: every Tier 1 / Tier 2 crate port from m13-001 depends on at least one (b) item.

## 3. Tier-1 blocking gap list

Per the m13-001 inventory, Tier 1 ports require:

### paideia-as-lexer (4.8k LoC)

- `unicode-ident` (b) — XID char-class tables. **Phase 5 stdlib must ship Unicode tables.**

### paideia-as-ast (5.7k LoC)

- `smallvec` (b) — must ship `SmallVec<T, N>` in paideia-stdlib.
- `static_assertions` (a) — trivial.

### paideia-as-parser (15k LoC)

- (none external).
- Internal: paideia-as-ast, paideia-as-diagnostics, paideia-as-lexer (cascade).

### paideia-as-diagnostics (3.9k LoC)

- `serde` + `serde_json` + `toml` (b) — three serialisation libraries. **The largest single Phase 5 stdlib gap.**
- `thiserror` (a) — trivial.
- `static_assertions` (a) — trivial.

Tier 1 minimum stdlib expansions: `SmallVec<T, N>`, `Unicode XID tables`, `serde/serde_json/toml` (or a minimal serialisation framework).

## 4. Tier-2 blocking gap list

### paideia-as-types (3.9k LoC)

- `smallvec` (b), `thiserror` (a), `static_assertions` (a).

### paideia-as-effects (1.5k LoC)

- (none external).

### paideia-as-ir (10.4k LoC)

- `smallvec` (b), `static_assertions` (a).

### paideia-as-elaborator (19.3k LoC)

- `blake3` (b) — hash function. **Needs paideia-stdlib hash module.**

### paideia-as-encoder (2.2k LoC)

- `smallvec` (b), `thiserror` (a).

### paideia-as-linker (1.7k LoC)

- (none external).

### paideia-as-dwarf (0.8k LoC)

- `gimli` (b) — DWARF library. **Could be replaced by hand-rolled subset.**

Tier 2 incremental gaps beyond Tier 1: `blake3` hash, `gimli` DWARF (or hand-roll). Both feasible.

## 5. Tier-3 blocking gap list

### paideia-as-emitter-elf (2.5k LoC)

- `object` (b) — ELF reader/writer. **Mostly read-only verification in dev path; the actual emit doesn't depend on object. Could drop.**
- `static_assertions` (a).

### paideia-as-emitter-pe (2.5k LoC)

- `smallvec` (b), `static_assertions` (a).

### paideia-as-emitter-pax (5.4k LoC)

- `blake3` (b), `static_assertions` (a).

Tier 3 doesn't introduce new gaps beyond `object` (which is mostly dev-only).

## 6. DEFERRED beyond Phase 5

Per m13-001 §5:

### paideia-pq-sign (4.6k LoC)

Depends on 11 external crypto / HSM / HTTP crates. Self-hosting requires either:
- (i) `.pdx` ports of all cryptographic primitives (Ed25519, ML-DSA-65, BLAKE3, ChaCha20-Poly1305, Argon2id, HKDF). **Significant** — easily 5k+ LoC of constant-time crypto.
- (ii) FFI shim where `.pdx` calls into Rust-compiled crypto libraries. Less work but introduces an FFI boundary.

Decision deferred to Phase 6+.

### paideia-lsp (4.2k LoC)

Depends on `tower-lsp` + `tokio` async runtime. paideia-as doesn't ship async/await today. Self-hosted LSP gates on:
- (i) Async runtime design (Phase 6+).
- (ii) JSON-RPC implementation (depends on serde port).
- (iii) Concurrent file watching (needs threading primitives).

Decision deferred to Phase 6+.

## 7. Total stdlib expansion needed before Tier 1

Distilled gap list:

1. **`SmallVec<T, N>`** — stack-allocated Vec. ~200 LoC. Blocks paideia-as-ast.
2. **Unicode XID character classification tables** — 32KB-ish data table + 50 LoC accessor. Blocks paideia-as-lexer.
3. **Serialisation framework** — minimum: serde-equivalent trait + derive macro + JSON encoder/decoder + TOML encoder/decoder. **Large** — easily 5-10k LoC. Blocks paideia-as-diagnostics.

Alternative: drop the SARIF JSON output (paideia-as-diagnostics' only serde use); replace TOML config with hand-coded ASCII format. Avoids the serde port but loses SARIF compatibility.

## 8. Phase 5 stdlib expansion ordering

Per the m11-009 stdlib forward links, the natural extension ordering:

1. **Phase 5 stdlib-001**: `SmallVec<T, N>` — unblocks paideia-as-ast.
2. **Phase 5 stdlib-002**: Unicode XID tables + accessor — unblocks paideia-as-lexer.
3. **Phase 5 stdlib-003** (large): `serde`-equivalent + `serde_json` minimum + `toml` minimum — unblocks paideia-as-diagnostics.
4. **Phase 5 stdlib-004**: BLAKE3 — unblocks paideia-as-elaborator + paideia-as-emitter-pax.
5. **Phase 5 stdlib-005**: `Lru` cache type — unblocks paideia-lsp (gated on async runtime, but the cache itself is independent).

After these 5 stdlib expansions, Tier 1 + Tier 2 (excluding lsp + pq-sign) can port. Tier 3 emitters port immediately afterward.

## 9. Recommendation

**Phase 5 m1 entry should bundle stdlib-001 + stdlib-002** before attempting any Tier 1 crate port. The Unicode tables especially have a large data-payload footprint; getting them landed in paideia-stdlib first avoids a stop-the-world refactor mid-port.

stdlib-003 (serde family) is the largest single gap. Recommend treating it as a Phase 5 sub-milestone of its own — possibly Phase 5 m2.

Tiered remaining work in approximate magnitude:

- **Tier 1 (Phase 5 m3-m5)**: lexer + parser + AST + diagnostics — ~30k LoC of `.pdx` to write.
- **Tier 2 (Phase 5 m6-m9)**: types + effects + IR + elaborator + encoder + linker + dwarf — ~40k LoC.
- **Tier 3 (Phase 5 m10-m11)**: emitters — ~10k LoC.

Phase 5 total: ~80k LoC of `.pdx` plus the 5 stdlib expansions. Comparable in shape to Phase 4 (~100k LoC of Rust + design); larger than Phase 3 (~60k LoC).

## 10. Forward links

- **m13-004**: stage-1 hash + DDC fixture. The byte-identity contract for self-host bring-up.
- **m13-005**: Phase 5 opening conditions. What must be true before Phase 5 starts.
- **Phase 5 plan**: m13-001's tier sequencing + this gap analysis combined.
- **Phase 6 design**: lsp + pq-sign self-hosting + async runtime + FFI shim decisions.
