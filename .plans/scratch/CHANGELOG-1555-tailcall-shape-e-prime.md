# CHANGELOG scratch — paideia-as#1555 — TCO Shape E' (trailing pop-bracket, upstream Push)

Wave: 36 (post-Wave-35 Shape-E completion).
Workspace version: 0.36.71 → 0.36.72.
Files touched:
  - `crates/paideia-as-ir/src/opt/tailcall.rs` (matcher + wrapper + tests)
  - `Cargo.toml` (workspace.package.version bump)
  - `CHANGELOG.md` (release entry)

## Shape E' — the 3-inst trailing pop-bracket window

Wave 35 (v0.36.64, paideia-as#1551) landed Shape E:
`[Push r; Call SymbolRef(s); Pop r; Ret]` (4-inst adjacent window)
→ `[Pop r; Jmp SymbolRef(s)]`. The motivating downstream sites —
`nvme_log_smart_fetch` and `nvme_log_error_info_fetch` in
`paideia-os/src/kernel/core/cap/nvme_admin_events.pdx` — did NOT
match that 4-inst window because their physical layout places the
`push rbx` at function entry (10+ instructions before
`call nvme_get_log_page`), not adjacent to the call. The debt
catalog surfaced this as a mismatch between the shape spec and the
motivating sites.

Shape E' closes the gap: the 3-inst window `[Call SymbolRef; Pop r; Ret]`
whose matching `Push r` lives upstream in the function prologue.

### Matcher (in `TailCallPass::apply`, between Shape E and Shape A)

Structural check on `ids[i..=i+2]`:
  - `ids[i]`: `Mnemonic::Call` with a `SymbolRef` operand (extracted
    via `call_target_symbol`).
  - `ids[i+1]`: `Mnemonic::Pop` with a single `Reg` operand
    (extracted via `pop_reg_operand`).
  - `ids[i+2]`: `Mnemonic::Ret`.

Owner-required (same discipline as Shape C):
`arena.instr_owner().get(call_id)` must be `Some`; an empty owner
aborts the shape.

Backward walk through the sorted `ids` list from position `i-1`:
  - Owner boundary: if `instr_owner().get(earlier_id)` differs from
    the Call's owner, the walk terminates (function boundary crossed).
  - Branch-in-span refusal: `Mnemonic::Jmp | Jcc(_) | Call | FarJmp`
    at any position sets `had_branch = true` and terminates. The
    wildcard `_ => false` arm keeps future `Mnemonic` variants safe
    by default (`Mnemonic` is defined in `paideia-as-runtime` — a
    different crate — so the wildcard also satisfies the
    non-exhaustive-match rule).
  - Stack-shape safety: the first `Push` encountered terminates the
    walk. If it matches the trailing Pop's register, `found_push`
    is set. If it doesn't (or if any `Pop` is encountered first),
    the walk terminates without `found_push` — the pass refuses
    silently to avoid rewriting an unbalanced bracket.

### Rewrite

The upstream `Push` is preserved untouched — it belongs to the
function's prologue, and other code (later Pop, callee-save
discipline through the body, register spills) may depend on it.

Mutations on the 3-inst window:
  - `ids[i]` (was `Call sym`) → `Mnemonic::Pop` + operands cleared
    and replaced with `Operand::Reg(popped)`.
  - `ids[i+1]` (was `Pop r`) → `Mnemonic::Jmp` + operands cleared
    and replaced with `Operand::SymbolRef { name: target_sym, addend: 0 }`.
  - `ids[i+2]` (Ret) → `instructions_mut().remove(ret_id)`.

Post-rewrite the physical layout at the tail is `[Pop r; Jmp sym]`
(in emission order, since `ids[i] < ids[i+1]`), so the callee-save
value is restored before the branch and the tail-jump reuses the
caller's return context. Iterator advances `i += 3`.

### Diagnostics

  - Success: `O1524: TCO direct trailing pop-bracket Call→Pop i<call_id>
    + Pop→Jmp i<pop_id> (target=<sym>) + drop Ret i<ret_id>
    (saved=r<reg>, prologue push i<push_id>)`.
  - Refusal (branch in upstream span): `O1516: TCO refused for
    i<call_id> (direct trailing pop-bracket r<reg> → <sym>) —
    branch between prologue push and call (owner=<name>)`.
  - Refusal (handler / cap boundary): `O1516: TCO refused for
    i<call_id> (direct trailing pop-bracket r<reg> → <sym>) —
    <blocker_reason> (owner=<name>)`.

## Safety guard extension — `tco_arena_blocker_with_earlier`

The existing `tco_arena_blocker` (PAS-DEBT-B3-002, paideia-as#1515)
checks the window between Call and Ret for handler-install evidence
and capability-boundary mismatch. Its internal logic is preserved
untouched per the wave directive.

A new wrapper `tco_arena_blocker_with_earlier(arena, earlier_span_start:
Option<IrNodeId>, call_id, ret_id, owner_name)`:
  1. Runs `tco_arena_blocker(arena, call_id, ret_id, owner_name)`
     first. If it returns `Some(reason)`, propagate it — unchanged
     behaviour for the trailing window.
  2. When `earlier_span_start` is `Some(push_id)`, additionally
     scans `[push_id, call_id)`:
       - Structural `IrKind::Handle` / `IrKind::HandlerValue` nodes
         via `arena.get(id).kind`.
       - `HandlerSideTable` entries whose Handle-id or op-body-id
         lands in the range.
     Any hit returns `Some(TcoBlocker::EffectHandlerInstalling)`.
  3. When `earlier_span_start` is `None`, the widened scan is skipped
     — the wrapper is a pass-through for Shapes A/D/E/B/C which do
     NOT call it.

Capability-boundary checks stay attached to the call site itself
(already handled by `tco_arena_blocker`). The earlier span is
straight-line code in the same enclosing function, so its declared
caps match by construction — no additional cap check is warranted.

Backward compatibility for the base callers (Shapes A/D/E/B/C) is
preserved by keeping `tco_arena_blocker`'s signature and behaviour
unchanged — no existing caller sees any difference.

## Motivating-site fit

The 2 fetchers in `paideia-os/src/kernel/core/cap/nvme_admin_events.pdx`
present exactly the shape Shape E' targets:

```
nvme_log_smart_fetch:
    push rbx
    ... straight-line body (no branches, no nested pushes) ...
    call nvme_get_log_page
    pop rbx
    ret
```

After Wave 36 (this wave) landing, both fetchers rewrite to:

```
nvme_log_smart_fetch:
    push rbx
    ... straight-line body ...
    pop rbx
    jmp nvme_get_log_page
```

This eliminates the return-stack round-trip on the fast path.
Neither fetcher installs an effect handler or crosses a capability
boundary between its prologue Push and the Call, so both pass
`tco_arena_blocker_with_earlier`.

## Tests added (four)

1. `wave27_shape_e_prime_rewrites_trailing_pop_bracket_with_upstream_push`
   — positive: `push r3; mov; mov; call sym; pop r3; ret` →
   `push r3; mov; mov; pop r3; jmp sym`. Asserts each slot's mnemonic
   + operand after rewrite; asserts O1524 diagnostic includes shape
   name, saved reg, and the prologue push id.

2. `wave27_shape_e_prime_refuses_when_branch_intervenes_between_push_and_call`
   — negative: intra-function `jmp .Lbody` between Push and Call
   → no rewrite, O1516 emitted with "branch between prologue push
   and call" reason.

3. `wave27_shape_e_prime_refuses_when_handler_installed_before_call`
   — negative: `IrKind::Handle` node at id 2 between Push (id 1)
   and Call (id 4); a `mov` at id 3 defeats Shape E's 4-inst
   adjacent window so Shape E' is what fires. Widened blocker
   returns `EffectHandlerInstalling`; O1516 emitted with
   "handler-install boundary" reason.

4. `wave27_widened_blocker_matches_base_when_earlier_span_none`
   — guard-helper unit test: `tco_arena_blocker_with_earlier` with
   `earlier_span_start = None` returns `None` (base behaviour); with
   `Some(push_node)` and a Handle in the widened range, returns
   `Some(TcoBlocker::EffectHandlerInstalling)`.

## Non-goals for this wave

  - No touch to Shapes A/D/E/B/C or to the base `tco_arena_blocker`.
    Shape E's own handler-safety at the prologue-push edge (if the
    4-inst adjacent window happens to have a Handle IR node between
    push and call — vanishingly rare, since Shape E requires
    adjacency) is out of scope; that pattern would need a separate
    hardening pass.
  - No integration test at the .pdx level. Local unit tests exercise
    the matcher, the walk-backward, the widened blocker, and the
    rewrite. End-to-end verification lands when
    `paideia-os/src/kernel/core/cap/nvme_admin_events.pdx` is
    re-lowered through the debt-loop cascade.
  - No new opt-pass dispatch entry; Shape E' rides on `TailCallPass`.
