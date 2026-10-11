# paideia-as-unicode AISSUE Dispatch Map

**Round:** `round:unicode-completion`
**Label:** `area:unicode`
**Mirror of:** paideia-fmt FMT-0..FMT-17 (closed Wave 72 in v0.36.82).
**Target close:** paideia-as **v0.37.0** on UNI-16 landing.

This map is the authoritative dispatch sheet for the softarch →
main-build → debugger loop that completes the `paideia-as-unicode`
crate. It mirrors the discipline that landed `paideia-fmt`:

- Each UNI-N wave runs **softarch (implement, no build) → main (build)
  → debugger (verify every iteration, patch on failure) → main rebuild
  → commit** (per `feedback_paideia_os_loop_shape.md` and the paideia-as
  loop-shape memo).
- This loop runs **concurrently with the paideia-os driver AISSUE
  loop**; both share the same main. Main must not block one on the
  other — serve whichever is next-ready.
- Workspace version bumps **at phase close** (UNI-16 → v0.37.0 — a
  minor bump matching the paideia-fmt → v0.36.82 precedent at FMT-17).
- Final acceptance report lands at
  `design/toolchain/paideia-as-unicode-acceptance.md`, shape mirroring
  `paideia-fmt-acceptance.md`.

---

## 1. Issue number ↔ UNI-N map

| UNI | Issue | Title | Size |
|-----|-------|-------|------|
| UNI-0  | [#1587](https://github.com/paideia-os/paideia-as/issues/1587) | crate spec + fingerprint discipline doc | S |
| UNI-1  | [#1588](https://github.com/paideia-os/paideia-as/issues/1588) | UAX #15 full normalization forms (NFD/NFKC/NFKD) | M |
| UNI-2  | [#1589](https://github.com/paideia-os/paideia-as/issues/1589) | Unicode case folding + `eq_ignore_case_unicode` | M |
| UNI-3  | [#1590](https://github.com/paideia-os/paideia-as/issues/1590) | UAX #14 Line Breaking + `wrap_at_width` | L |
| UNI-4  | [#1591](https://github.com/paideia-os/paideia-as/issues/1591) | UAX #31 identifier predicates + UTS #39 profile | M |
| UNI-5  | [#1592](https://github.com/paideia-os/paideia-as/issues/1592) | UCD adapter: GC + Script + Block lookup | S |
| UNI-6  | [#1593](https://github.com/paideia-os/paideia-as/issues/1593) | fingerprint golden files for every corpus test | S |
| UNI-7  | [#1594](https://github.com/paideia-os/paideia-as/issues/1594) | full UCD corpus sweep harness (five data files) | L |
| UNI-8  | [#1595](https://github.com/paideia-os/paideia-as/issues/1595) | criterion benches: throughput baseline | M |
| UNI-9  | [#1596](https://github.com/paideia-os/paideia-as/issues/1596) | `WidthMode` locale resolver (resolve TODO) | S |
| UNI-10 | [#1597](https://github.com/paideia-os/paideia-as/issues/1597) | property-test coverage expansion (15+ invariants) | M |
| UNI-11 | [#1598](https://github.com/paideia-os/paideia-as/issues/1598) | public API audit + freeze + `public_api.golden` | S |
| UNI-12 | [#1599](https://github.com/paideia-os/paideia-as/issues/1599) | consumer wire-up: shell-lex / -ast / -completion | M |
| UNI-13 | [#1600](https://github.com/paideia-os/paideia-as/issues/1600) | LSP integration surface (UTF-16/UTF-32 col converters) | M |
| UNI-14 | [#1601](https://github.com/paideia-os/paideia-as/issues/1601) | cargo-fuzz target for `Utf8Decoder` | S |
| UNI-15 | [#1602](https://github.com/paideia-os/paideia-as/issues/1602) | README + examples sweep | S |
| UNI-16 | [#1603](https://github.com/paideia-os/paideia-as/issues/1603) | **final acceptance** + v0.37.0 bump | M |

Size legend: S = 1 wave, M = 2 waves, L = 3 waves.
Total: 17 tickets ≈ 28 waves of softarch+debugger+main work.

---

## 2. Dependency DAG

```
                    UNI-0 (spec)
                   /    |      \
                  v     v       v
               UNI-1  UNI-2   UNI-5
                |      |       |
                +---+--+       |
                    |          |
                    v          |
                  UNI-3     UNI-9
                    |          |
                    +--+-------+
                       |
                       v        UNI-6 (fingerprint — orthogonal)
                     UNI-4         |
                       |           |
    +------------------+-----------+----+------+----+
    |            |          |          |      |    |
    v            v          v          v      v    v
  UNI-7       UNI-8      UNI-10    UNI-12   UNI-13 UNI-14 UNI-15
   (sweep)    (bench)    (props)   (wire)   (lsp)  (fuzz) (docs)
    \            \         \         /       /      /     /
     \            \         \       /       /      /     /
      +------------+---------+-----+-------+------+-----+
                              |
                              v
                           UNI-11 (API freeze)
                              |
                              v
                           UNI-16 (acceptance + v0.37.0)
```

**Chain-head independents (filable + dispatchable immediately, parallel-safe):**
UNI-0, UNI-1, UNI-2, UNI-5, UNI-6, UNI-9, UNI-14 — seven can fire in
parallel if the softarch budget allows. The three with the most
downstream pressure are UNI-0 (spec that every later ticket cites),
UNI-1 (NFD/NFKC/NFKD blocks UNI-4 + UNI-7), and UNI-2 (case fold
blocks UNI-4 + UNI-10 + UNI-12).

---

## 3. Suggested wave groupings

Each row is a parallel dispatch wave; waves run strictly sequentially.
Each softarch dispatch MUST cite the encoder-pitfall checklist (§5).

| Wave | Parallel tickets | Rationale |
|------|-----------------|-----------|
| W1 | UNI-0, UNI-1, UNI-2 | spec + two orthogonal normalization/case foundations |
| W2 | UNI-5, UNI-6, UNI-9, UNI-14 | second-tier chain-heads; all independent of W1 |
| W3 | UNI-3 (3-wave), UNI-4 | line-break + identifier (both depend on UNI-1/UNI-2) |
| W4 | UNI-7, UNI-8, UNI-10 | corpus sweep + benches + property tests (all consume W1..W3) |
| W5 | UNI-12, UNI-13, UNI-15 | consumer wire-up + LSP + docs |
| W6 | UNI-11 | public API freeze (single-threaded; needs every API to have landed) |
| W7 | UNI-16 | final acceptance + v0.37.0 bump |

W1..W3 collectively close the primitive-adding half of the round.
W4..W5 are the harness + integration half. W6..W7 are the capstone.

If the softarch pool is small (one at a time), the serial order is
UNI-0 → UNI-1 → UNI-2 → UNI-5 → UNI-6 → UNI-9 → UNI-14 → UNI-3 →
UNI-4 → UNI-7 → UNI-8 → UNI-10 → UNI-12 → UNI-13 → UNI-15 → UNI-11 →
UNI-16. This serializes on L-size tickets (UNI-3, UNI-7) consuming
three waves each, so the full round is ~28 waves of main-build +
debugger ticks (the FMT precedent was ~24).

---

## 4. First-wave trio (dispatchable now)

**UNI-0 (#1587)** — `design/toolchain/paideia-as-unicode-spec.md`.
Author the formal reference, enumerating in-scope UAXes (#11, #14,
#15, #29, #31, #44), deferred UAXes (#9 Bidi, UTS #46 IDNA), invariants,
and fingerprint discipline. No code change; module docs updated to
cross-link.

**UNI-1 (#1588)** — Full UAX #15 forms. Add `nfd_normalize`,
`nfkc_normalize`, `nfkd_normalize`, `is_nfd`, `is_nfkc`, `is_nfkd`, and
a `Form` enum + `normalize(form, &str)` dispatcher. Blocks UNI-4
(identifier NFKC_CF) and UNI-7 (full-corpus sweep).

**UNI-2 (#1589)** — `src/case.rs` with `to_lower`, `to_upper`,
`case_fold` (default), `case_fold_turkic`, `eq_ignore_case_unicode`.
Adds `unicode-case-mapping = "0.5"` to workspace deps. Blocks UNI-4
and UNI-12.

All three are independent, independently dispatchable, and together
cover the three "foundations" the rest of the round layers on top of.
Dispatch all three softarchs in a single main message to run in
parallel.

---

## 5. Encoder-pitfall checklist (cite in every softarch dispatch)

Per `feedback_pdx_encoder_pitfalls.md`, softarchs keep re-hitting the
same six pitfalls. Every UNI-N dispatch must open with this fence:

```
Before writing any .pdx (or Rust) code, re-read `feedback_pdx_encoder_pitfalls.md`.
Specifically: no `test rN,rN` (use `cmp rN, 0`); no `and r11, imm64`
(load imm into reg then and); 2-op `imul r,imm` is invalid (use 3-op
`imul rd, rs, imm`); no multiline `pub let = "..."` (keep on one line);
module basename must match the `module <name> { ... }` declaration;
fingerprint tag size is 32 bytes (0x20), not 33.

The present ticket is Rust-only (no .pdx code is produced by UNI-0..UNI-16),
so the pitfalls above are only load-bearing where the softarch generates
example `.pdx` snippets inside doc tests. For Rust: avoid
`unsafe` (crate declares `#![forbid(unsafe_code)]` — a new `unsafe`
block fails compile); every `pub fn` must have `#[doc]`; every corpus
test must stamp a fingerprint tag; every normalize/segment call must
go through this crate's public surface, not a bare `unicode-*` call.
```

For Rust-only tickets (UNI-0, UNI-5, UNI-6, UNI-8, UNI-9, UNI-10,
UNI-11, UNI-13, UNI-14, UNI-15, UNI-16) the second paragraph is the
operative one.

For tickets that touch a consumer crate (UNI-12), the first paragraph
is operative: shell-lex / shell-ast / shell-completion ship `.pdx`
fixtures that might be edited for new identifier predicates.

---

## 6. Standing contract preservation

- **Concurrent with paideia-os driver loop.** Both AISSUE rounds share
  the same main. Main must not block either on the other; serve
  whichever subagent is next-ready.
- **softarch → main-build → debugger-every-iteration.** Each UNI-N
  wave follows this exact shape (`feedback_debugger_every_iteration.md`).
  A green build is NEVER sufficient; the debugger adversarially verifies
  the diff on every iteration.
- **Workspace version bumps at phase close.** Only UNI-16 bumps the
  workspace version (to **0.37.0**). Intermediate UNI-N landings keep
  the version pinned.
- **CHANGELOG scratch pattern.** Each softarch writes
  `.plans/scratch/CHANGELOG-UNI-N-<slug>.md` on its wave (per
  `feedback_paideia_as_parallel_changelog.md`); main consolidates into
  the top of `CHANGELOG.md` at UNI-16.
- **GitHub closing-keyword discipline.** Each wave's commit cites
  `Closes paideia-os/paideia-as#NNNN.` with the keyword repeated
  per issue (per `feedback_github_closing_keywords.md` and
  `feedback_github_closes_keyword_syntax.md`). `paideia-as#NNNN`
  shorthand is NOT accepted — use `paideia-os/paideia-as#NNNN` or the
  bare `#NNNN` (same-repo).
- **No background builds.** softarchs never invoke `bash tools/build.sh`
  or any smoke; builds are main-only
  (`feedback_no_background_builds.md`).
- **Pipe-swallows-exit awareness.** Any main-side build invocation that
  tails output must capture exit-code via `>log; echo $?` or `set -o
  pipefail`; never `| tail -N` alone (`feedback_pipe_swallows_exit.md`).

---

## 7. Final acceptance destination

- **Doc:** `design/toolchain/paideia-as-unicode-acceptance.md` (UNI-16).
- **Version:** paideia-as workspace `0.37.0`.
- **Tag:** `v0.37.0` on the paideia-as `main` branch.
- **CHANGELOG:** top entry enumerates UNI-0..UNI-16 landings.
- **Status stamp:** `crates/paideia-as-unicode/src/lib.rs` module-doc
  updated to `Status: COMPLETE as of paideia-as v0.37.0`.

---

## 8. Not-in-scope for this round

Explicitly deferred (future tickets, not blockers):

- **UAX #9 Bidi** — R229 renderer is monospace left-to-right only.
- **UTS #46 IDNA** — not a shell concern; URL/hostname crate lives
  elsewhere.
- **UTS #39 Confusables (full)** — only the Moderately-Restricted
  profile from UNI-4 lands; full confusables matrix is a follow-up.
- **Streaming NFC** — current `nfc_normalize` is `&str → String`; a
  streaming variant for the LSP incremental path is UNI-13's
  `stream_is_nfc` only (not full streaming normalize).
- **Hyphenation (UAX #14 extension)** — UNI-3 lands break opportunities
  only, not Knuth-Liang hyphenation.
- **UCA #10 Collation** — needed by shell-sort command; deferred to a
  separate round (UCA-0..UCA-N) once UNI closes.

These are enumerated in UNI-16's acceptance report §Deferred as
`gh issue create`-ready fence blocks, mirroring the paideia-fmt F1..F5
follow-ups.
