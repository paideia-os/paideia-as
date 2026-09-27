# paideia-as #1548 — B4-005 true-imm64 auto-staging for arithmetic

## Change

- shared helper at `crates/paideia-as-encoder/src/imm64_stage.rs`: 90 lines
  (fresh module) — `stage_imm64_r11(buf, operand_reg, imm, collision_msg) ->
  Result<Reg64, EncodeError>` emits `movabs r11, imm64` and returns
  `Reg64::R11`, or `EncodeError::Unsupported(collision_msg)` when
  `operand_reg == R11`. Two internal unit tests (happy path + collision).
  Wired into `crates/paideia-as-encoder/src/lib.rs` as `mod imm64_stage;`
  (private, `pub(crate)` items — no public API surface added).
- encode_cmp (`crates/paideia-as-encoder/src/encode_instruction/cmp_test.rs`,
  ~L30–L60): imm64 arm replaced the `EncodeError::Unsupported(
  "cmp imm64 not supported…")` fallback with a call to
  `stage_imm64_r11` + `cmp_reg64_reg64`. +18 net lines (including comment
  block citing #1526 / paideia-os#2509 sites — cow_write, gpe_io, gpe_ack,
  ec_event, thermal_policy, journal_ondisk, journal_csum, block_cache_flush).
- encode_and, encode_or (Mode64), encode_xor (`crates/paideia-as-encoder/
  src/encode_and_or_xor.rs`): three symmetric edits, each replacing the
  `EncodeError::Unsupported("… x86_64 has no … r/m64, imm64 form …")`
  return with `stage_imm64_r11` + `<mnem>_reg64_reg64`. +14 net lines each
  (~42 total). Import added: `use super::imm64_stage::stage_imm64_r11;`.
  The `or` Mode32 arm is untouched — imm always fits in u32/i32 range there.
- encode_add, encode_sub (`crates/paideia-as-encoder/src/encode_instruction/
  arith.rs`): two symmetric edits, each replacing the
  `EncodeError::Unsupported("64-bit immediate add/sub not yet supported")`
  return with `stage_imm64_r11` + `add_reg64_reg64` / `sub_reg64_reg64`.
  +15 net lines each (~30 total). Uses fully-qualified path
  `crate::imm64_stage::stage_imm64_r11` (no import — arith is a sibling
  submodule that already glob-imports many names via `use super::*;`).

Total: ~180 lines added across 5 files (helper + 3 dispatchers).

## Test hooks

- 8 new unit tests in `crates/paideia-as-encoder/src/encode_instruction/
  tests.rs` (appended, ~130 lines):
  1. `b4_005_cmp_rax_imm64_stages_via_r11` — byte-exact assertion:
     `49 BB 88 77 66 55 44 33 22 11  4C 39 D8` (movabs + cmp rax, r11).
  2. `b4_005_and_rax_imm64_stages_via_r11` — `… 4C 21 D8`.
  3. `b4_005_xor_rax_imm64_stages_via_r11` — `… 4C 31 D8`.
  4. `b4_005_or_rax_imm64_stages_via_r11` — `… 4C 09 D8`.
  5. `b4_005_sub_rax_imm64_stages_via_r11` — `… 4C 29 D8`.
  6. `b4_005_add_rax_imm64_stages_via_r11` — `… 4C 01 D8`.
  7. `b4_005_r11_collision_returns_unsupported_and_writes_no_bytes` —
     table-driven over all 6 mnemonics; asserts
     `EncodeError::Unsupported(msg)` with `msg.contains("r11 collision")`
     and the buffer stays empty (no leaked movabs on the failure path).
  8. `b4_005_imm_fitting_i32_still_uses_short_form_no_staging` —
     regression guard: table-driven over all 6 mnemonics with imm=0x1000;
     asserts `buf.len() < 10` (so the movabs+op 13-byte path did NOT fire).
- 2 new unit tests inside `imm64_stage.rs` (helper-scoped):
  happy path (byte-exact movabs) + collision (returns Unsupported, buffer
  empty).
- No existing goldens should break — the auto-staging arm fires only when
  the caller passes an imm64 that does NOT round-trip through i32
  (previously that path returned an `EncodeError::Unsupported`; there
  cannot be a golden fixture that expected bytes from a failed encode).
  The i32-round-trip short forms (imm8 / imm32) are unchanged.

## Risk

- **R11 collision on `<op> r11, imm64`**: the movabs would clobber the
  operand register. Guarded — every arm calls `stage_imm64_r11` which
  returns `Unsupported` with a mnemonic-specific message before writing
  any bytes. Test #7 covers all 6 mnemonics. This mirrors the #1526
  guard verbatim.
- **Existing paideia-os hand-rolled r11-staging sites do NOT regress**:
  they already emit `movabs r11, imm64` + `<op> r64, r11` explicitly as
  two IR instructions; the encoder sees `<op> r64, r11` (a reg-reg
  form), which goes down the existing reg-reg arm and is unchanged.
  The new auto-stage only fires on the `<op> r64, imm64` IR shape —
  which today errors out, so a callsite starts working rather than
  changing bytes.
- **`or` Mode32 arm untouched**: imm is bounded by `imm_i64 <= u32::MAX
  as i64` in that path; no true imm64 reaches it, so no auto-stage needed
  and no behavioural change.
- **Encoder pitfall reminder**: `test rN,rN` is a separate mnemonic; not
  in this wave's scope. `test r64, imm64` also has no imm64 form but
  its callers are rare and the fallback message ("64-bit immediate test
  not yet supported; use and+cmp workaround") is left untouched to keep
  this wave scoped.
- **2-op `imul r,imm`**: outside this wave. `imul` uses `encode_imul.rs`
  which is a different family (three-operand plus two-operand forms
  with distinct opcode encoding); a follow-up ticket can retire the
  remaining hand-rolled sites there.

## Version bump

- Bump workspace.version 0.36.57 → 0.36.58 in `Cargo.toml`.
- CHANGELOG.md gains a `v0.36.58 — 2026-09-26 — Wave 18` entry citing
  #1548 and paideia-os#2509.
