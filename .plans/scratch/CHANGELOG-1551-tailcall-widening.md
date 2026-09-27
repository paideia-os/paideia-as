# paideia-as#1551: tailcall pass — widen to alignment-pad + push/pop-bracketed shapes

## Summary

Widened `crates/paideia-as-ir/src/opt/tailcall.rs::TailCallPass::apply`
from three shapes to five, adding:

- **Shape D — SysV alignment-pad indirect** (paideia-as#1551 §Shape 4):
  `Sub Rsp, imm; Call Reg(r); Add Rsp, imm; Ret` → `Add Rsp, imm; Jmp Reg(r)`.
  Motivating sites: 14 vops.pdx dispatchers
  (`vops_read/write/open/close/lookup/create/unlink/stat/readdir/mkdir/rmdir/rename/readlink/symlink`),
  each of which uses the SysV `sub rsp,8; call rax; add rsp,8` pad
  literally adjacent to the call. Retires the paideia-os#2512 RETIRE-5
  workaround comment on the vops half.
- **Shape E — push/pop-bracketed direct** (paideia-as#1551 §Shape 5):
  `Push Reg(r); Call SymbolRef(s); Pop Reg(r); Ret` → `Pop Reg(r); Jmp SymbolRef(s)`.
  4-instruction window, both brackets naming the *same* register. See
  "Physical-layout mismatch" note below on the RETIRE-5 nvme half.

Both shapes reuse the PAS-DEBT-B3-002 `tco_arena_blocker` guard for
capability-boundary and effect-handler-install checks — no new safety
plumbing.

## Files changed

- `crates/paideia-as-ir/src/opt/tailcall.rs`:
  - **Extractors added** (lines 165-224): `push_reg_operand`,
    `sub_rsp_imm_operand`, `add_rsp_imm_operand`. `sub_rsp_imm_operand`
    and `add_rsp_imm_operand` gate on `Operand::Reg == crate::abi::RSP`
    (RegId(4)) and an `Imm64` second operand — anything else short-circuits.
  - **`apply` doc widened** (lines ~230-272): 5-shape catalogue now.
  - **Pattern D block** (lines ~285-369): 4-instruction window match
    (`Sub Rsp,imm; Call Reg; Add Rsp,imm; Ret`) → drop leading Sub,
    mutate Call→Jmp, preserve trailing Add, drop Ret. Emits `O1520`
    on success, `O1516` on blocker refusal.
  - **Pattern E block** (lines ~371-468): 4-instruction window match
    (`Push r; Call SymbolRef; Pop r; Ret` with matching regs) → mutate
    Push→Pop, mutate Call→Jmp, drop the trailing Pop and Ret. Emits
    `O1522` on success, `O1516` on blocker refusal.
  - **Test helpers added** (lines ~1420-1447): `push_reg_inst`,
    `sub_rsp_imm_inst`, `add_rsp_imm_inst`.
  - **4 new tests** (lines ~1450-1620):
    - `wave26_rewrites_align_pad_indirect_tail_call` — positive Shape D.
    - `wave26_preserves_mismatched_align_pad_imm` — negative Shape D
      (imm mismatch: sub 8 vs add 16 must not rewrite).
    - `wave26_rewrites_push_pop_bracketed_direct_tail_call` — positive
      Shape E.
    - `wave26_preserves_asymmetric_push_pop_bracket` — negative Shape E
      (push r3, pop r1 — mirror-invariance mismatch).

No changes to:
- Cargo.toml `workspace.package.version` (per task: paired with #1524 in
  this parallel wave; main consolidates the bump at wave-close).
- `tco_arena_blocker` (B3-002) — reused verbatim; both new shapes go
  through it exactly like the three existing patterns.
- The three existing shapes (Patterns A/B/C) — pattern-ordering places
  the two new blocks BEFORE Pattern A, but their leading mnemonics
  (Sub / Push) are disjoint from Pattern A's (Pop) and the 2-inst
  Call+Ret window, so no matching contention.

## Diagnostic-code allocations

- `O1520` — Shape D success (align-pad indirect Call→Jmp rewrite).
- `O1522` — Shape E success (push-pop-bracketed direct Call→Jmp rewrite).
- `O1516` — reused for both shapes' blocker refusals (existing code).

## Encoder pitfalls

Not applicable — this ticket writes zero new instructions. The pass
mutates existing `Instruction` records (mnemonic swaps) and removes
side-table entries. No `test rN,rN`, no `and r11, imm64`, no 2-op
`imul r,imm`, no reserved-word labels (`loop`, `if`, `let`, etc.).
The `Sub`/`Add`/`Jmp`/`Pop`/`Push` mnemonics all exist in
`paideia-as-runtime`'s `Mnemonic` enum verbatim (mnemonic.rs lines
20/22/32/132/136); no missing mnemonic gap.

## Non-exhaustive match reminder

No `match` on a foreign-crate `#[non_exhaustive]` enum was added. The
extractors use `matches!` for narrow shape checks and `if let` for
Some/None peeling — both are fall-through by construction.

## Physical-layout mismatch — Shape E vs nvme_admin_events reality

The task specifies Shape E as a 4-instruction window
`[Push r, Call sym, Pop r, Ret]`. I verified the actual motivating
sites in `paideia-os/src/kernel/core/cap/nvme_admin_events.pdx`
(nvme_log_smart_fetch @ line 740, nvme_log_error_info_fetch @ line 777):

```
push rbx                       // function entry
mov rbx, rdi                   // body starts here — 10+ intervening
lea r10, [rip + _nvme_log_smart_buf]   // instructions between the push
mov rax, rbx                            // and the call
shl rax, 12
add r10, rax
mov rdi, rbx
mov rsi, 2
mov rdx, r10
mov rcx, 128
call nvme_get_log_page         // the actual call
pop rbx                        // adjacent to ret
ret
```

The **physical** window at the tail is `[Call sym, Pop rbx, Ret]`
(3 instructions), NOT the 4-inst `[Push, Call, Pop, Ret]` the task
spec describes. The Push lives 10+ instructions upstream in the
prologue.

**Consequence:** the 4-inst Shape E block I landed will not fire on
the nvme_admin_events sites. It will fire on hypothetical tiny functions
whose entire body is literally `push r; call sym; pop r; ret` (adjacent),
which is the strict reading of the shape spec. I chose this conservative
literal reading because:

1. The task explicitly names the window as 4-instruction and specifies
   the rewrite as `[Pop r, Jmp sym]`. Widening to a 3-inst matcher
   `[Call sym, Pop reg, Ret]` would deviate from the specified shape
   and would need a separate safety justification (specifically: the
   "trust an earlier Push" reasoning has to be either proven at
   pass-time or bounded by structural analysis, otherwise a Pop with
   no matching Push would corrupt the caller's return frame).
2. Debugger/challenger step running after softarch per the loop tempo
   will decide whether to (a) accept the literal 4-inst matcher and
   file a follow-up for the 3-inst variant, or (b) widen this landing
   to the 3-inst variant with the safety argument attached.

The follow-up shape, if pursued, would match a 3-instruction window
`[Call SymbolRef(s), Pop Reg(r), Ret]` and rewrite to
`[Pop Reg(r), Jmp SymbolRef(s)]`. Safety argument: the Pop restores
a callee-save value the current function saved earlier in its
prologue; pulling the restore ahead of the tail-jump preserves the
callee-save contract for the caller-of-us because the tail-target
(SymbolRef) will honour the same contract. No new IR analysis needed
beyond the existing `tco_arena_blocker` handler/cap-boundary checks.

## Constraints observed

- **No touching the three existing shapes.** Patterns A/B/C code
  unchanged; only new blocks added before Pattern A.
- **No touching `tco_arena_blocker`.** Both new shapes call it with
  `(call_id, ret_id, owner)` identical to the sibling patterns.
- **No `cargo` invocations.** Per `feedback_no_background_builds.md`,
  builds are main-only.
- **No workspace.version bump.** Per task instruction, paired with
  #1524 — main consolidates the bump at wave close.
- **SysV-first stance honoured.** Shape D is SysV-only (references
  `crate::abi::RSP`). MS-x64 alignment pads use different immediates
  and would need a mirror block; not in scope this ticket.

## Not done (intentional)

- **No 3-inst `[Call sym, Pop r, Ret]` variant.** Deferred per the
  physical-layout mismatch note above. If the follow-up chooses to
  add it, the block goes between Pattern E and Pattern A in the
  match cascade; the extractors already exist (`pop_reg_operand`,
  `call_target_symbol`).
- **No MS-x64 alignment-pad mirror.** MS x64 uses a 32-byte shadow
  space and different alignment conventions; the pad idiom is not
  the same. Out of scope.
- **No integration test on real .pdx sites.** The pass carries its
  own unit tests; end-to-end validation happens when
  paideia-os#2512 retires and the compiled kernel image passes the
  QEMU smoke.

## Report-back (for main / debugger)

**Files + line ranges for the 2 new shapes:**
- Shape D matcher block: `crates/paideia-as-ir/src/opt/tailcall.rs` lines ~285-369
- Shape E matcher block: `crates/paideia-as-ir/src/opt/tailcall.rs` lines ~371-468
- Extractors: `sub_rsp_imm_operand` / `add_rsp_imm_operand` /
  `push_reg_operand` at lines ~178-224

**Test file paths + shape descriptions:**
- `crates/paideia-as-ir/src/opt/tailcall.rs` (same-file `#[cfg(test)]`):
  - `wave26_rewrites_align_pad_indirect_tail_call` (~L1451): positive
    Shape D — Sub 8 / Call r0 / Add 8 / Ret → Add 8 / Jmp r0.
  - `wave26_preserves_mismatched_align_pad_imm` (~L1503): negative
    Shape D — Sub 8 / Call r0 / Add 16 / Ret must NOT rewrite.
  - `wave26_rewrites_push_pop_bracketed_direct_tail_call` (~L1549):
    positive Shape E — Push rbx / Call nvme_get_log_page / Pop rbx /
    Ret → Pop rbx / Jmp nvme_get_log_page.
  - `wave26_preserves_asymmetric_push_pop_bracket` (~L1606): negative
    Shape E — Push r3 / Call / Pop r1 / Ret must NOT rewrite.

**Classifier / IR gaps discovered:**
- Shape E's specified 4-inst window does NOT match the physical
  nvme_admin_events layout. See "Physical-layout mismatch" section
  above. Follow-up would add a 3-inst variant.

**tco_arena_blocker guard applicability:**
- Both new shapes call `tco_arena_blocker(arena, call_id, ret_id,
  &owner)` exactly like the three existing patterns. The window
  spans `call_id..ret_id` (Shape D: ids[i+1]..ids[i+3]; Shape E:
  ids[i+1]..ids[i+3]), same as Pattern A. No boundary-analysis
  gap; the guard cleanly applies.

Build not run — main should invoke `bash tools/build.sh` and re-invoke
me with the error tail if it fails.
