# Command-Dispatch Pattern for Semantic Terminal (pa-r18-006)

## 1. Overview

The command-dispatch pattern is the semantic foundation for a terminal command handler: given a user-supplied command name (as a string), route to the appropriate handler function and execute it. This pattern is the paideia-as-side contract that enables paideia-os phase 11 to implement the semantic shell (see `design/terminal/semantic-shell.md` §SH-D5).

The pattern is not a single shape, but rather a family of dispatch strategies, each trading memory, latency, and code clarity. This document canonicalises the target (aspirational) shape and documents three dispatch strategies that are buildable today.

## 2. Target Canonical Shape

The target shape, **not buildable end-to-end until #994, #995, #996, and #998b–e land**, is:

```
HashMap<Str, Closure>
  where Closure : fn(Str) -> ExitCode
```

In pseudo-Paideia:

```paideia
pub let dispatch_table : HashMap<Str, (Str)->u64> !{} @{} = {
  "ls"   -> |arg: Str| -> ls_impl(arg),
  "pwd"  -> |arg: Str| -> pwd_impl(arg),
  "echo" -> |arg: Str| -> echo_impl(arg),
  "exit" -> |arg: Str| -> exit_impl(arg),
}

pub let dispatch : (Str) -> u64 !{} @{} = fn(cmd_name: Str) -> {
  match dispatch_table.get(cmd_name) {
    Some(handler) => (handler)(cmd_name),
    None => 1u64,  // command not found
  }
}
```

**Blockers** (see §6 Migration Path for details):
- **#1233** (encoder fat-pointer primitives): Closure pair layout (`16 B = code* + env*`), indirect call `call [r14+8]`
- **#994** (closure grammar + lowering): `|x| body` literal, `\|T\|->R` type, env-record allocation
- **#995** (closure call): `(f)(args)` where `f: Closure`
- **#996** (hashmap stdlib): `HashMap<K, V>` container, `get(key)` method
- **#998b–e** (Str compare/hash/split): `command_name.eq(input)`, `hash(name)` for probing

## 3. Landed Substrate — Three Dispatch Shapes Today

The v0.17 + v0.18 substrate supports **three** dispatch strategies end-to-end. Each is production-ready within its constraints and should be the preferred choice until the target shape lands.

### 3.1 Enum-Tag Dispatch via `match` (Linear Cmp/Je Cascade)

**Substrate**: Unit-variant and payload-carrying enums (pa-r17-007, pa-r17-008), `match` expression lowering (pa-r17-002), arm dispatch to arbitrary expressions.

**Pattern**: Tag commands as enum variants. Match the enum to dispatch to handlers. Each arm compiles to a `cmp` + conditional jump.

**Pseudo-code**:

```paideia
enum Cmd { Ls, Pwd, Echo, Exit }

pub let dispatch : (Cmd) -> u64 !{} @{} = fn(c: Cmd) -> match c {
  Ls   => ls_impl(),
  Pwd  => pwd_impl(),
  Echo => echo_impl(),
  Exit => exit_impl(),
}
```

**Codegen**: Linear cmp/je chain. For N arms, approximately 2N bytes per arm (8-byte discriminant compare + 2-byte conditional jump). Latency: O(N) in worst case (last arm).

**Tradeoffs**:
- ✓ Simple, readable code; trivial parsing of input to enum tag.
- ✓ Type-safe: enum variant mismatch caught at elaboration.
- ✗ Latency scales with command count.

**Landed witness fixtures**: `pick4_arm_a.pdx`, `pick4_arm_b.pdx`, `pick4_arm_c.pdx`, `pick4_arm_d.pdx`, `pick5_arm_c.pdx`, `pick5_arm_e.pdx`, `match_enum_pattern.pdx`.

### 3.2 Enum-Tag Dispatch via `@jump_table` (O(1) Memory-Indirect)

**Substrate**: `@jump_table` attribute on `match` (pa-r15-009), codegen for unit-variant enum scrutinees (landed per witness fixtures below). See `design/toolchain/jump-table-attribute.md` for full grammar and density contract.

**Pattern**: Identical enum definition as §3.1, but annotate the `match` with `@jump_table`.

**Pseudo-code**:

```paideia
enum Cmd { Ls, Pwd, Echo, Exit }

pub let dispatch : (Cmd) -> u64 !{} @{} = fn(c: Cmd) -> match c @jump_table {
  Ls   => ls_impl(),
  Pwd  => pwd_impl(),
  Echo => echo_impl(),
  Exit => exit_impl(),
}
```

**Codegen**: Rodata table of code addresses (one per variant). Runtime dispatch: normalize enum tag (via subtraction), bounds-check, then memory-indirect jump `jmp [rip + _table + tag*8]`. Latency: constant O(1).

**Density contract** (from `jump-table-attribute.md`):
- Coverage density: `(covered_arms / total_range) >= 50%`
- Range bound: `(max_tag - min_tag + 1) <= 256`
- Default arm required: If branching on a value that might be out of range.

**Tradeoffs**:
- ✓ O(1) latency; competitive with CPU branch prediction for typical command counts.
- ✓ Passes density check for all practical command counts (4–30 commands).
- ✗ Requires unit-variant enums (payload-carrying variants fall back to cmp/je; see §9 Backtrack Candidates).
- ✗ Slightly larger rodata footprint than cmp/je.

**Landed witness fixtures**: `pick4uv_arm_a.pdx`, `pick4uv_arm_b.pdx`, `pick4uv_arm_c.pdx`, `pick4uv_arm_d.pdx`, `pick5uv_arm_c.pdx`, `pick8uv_arm_e.pdx`.

### 3.3 Function-Pointer Indirect Dispatch

**Substrate**: Function-pointer types (pa-r17-001 / #979), address-of function symbol (pa-r17-004 / #981), indirect call via RIP-relative (pa-r17-004c, #1040). See `design/toolchain/function-pointer-types.md` and `design/toolchain/fnptr-unsafe-pattern.md`.

**Pattern**: Store handlers as function pointers in a record or local array. Call indirectly via the pointer.

**Pseudo-code**:

```paideia
pub let handle_ls   : () -> u64 !{} @{} = fn(_: ()) -> 1u64
pub let handle_pwd  : () -> u64 !{} @{} = fn(_: ()) -> 2u64
pub let handle_echo : () -> u64 !{} @{} = fn(_: ()) -> 3u64
pub let handle_exit : () -> u64 !{} @{} = fn(_: ()) -> 0u64

// Dispatch via function pointer
pub let dispatch : (() -> u64) !{} @{} = &handle_echo
pub let entry : () -> u64 !{} @{} = fn(_: ()) -> (dispatch)(())
```

**Codegen**: Function pointer stored in a register or memory location. Call emits `call [reg]` or `call [rip+offset]` (RIP-relative indirect). Latency: one memory dereference + one indirect branch (2–3 cycles on modern CPUs).

**Tradeoffs**:
- ✓ O(1) latency.
- ✓ Decouples command naming from dispatch structure (no enum required).
- ✓ Composes with other dispatch shapes (nest fn-ptr arrays inside jump-table arms; see §5).
- ✗ No type safety: function signature mismatch is a runtime error.
- ✗ Requires care to avoid invalid pointers.

**Landed witness fixtures**: `pa_r17_004c_call_rip_fnptr.pdx`, `pa_r19_1100_gap2_fnptr_indirect_args.pdx`.

## 4. Worked Example

The following fixture demonstrates the recommended approach for a semantic-shell command set: **enum dispatch with `@jump_table` and unit-variant handlers**.

```paideia
// pa_r18_006_command_dispatch_shell.pdx
// Canonical example: command dispatch using enum tag + @jump_table.
// Expected exit code: 3 (Echo handler).

module PaR18006CommandDispatchShell = structure {
  // Unit-variant enum: each command is a tag with no payload.
  enum Cmd { Ls, Pwd, Echo, Exit }

  // Handlers: plain functions returning exit codes.
  // In production, each would interpret command arguments;
  // here they are bare for simplicity.
  pub let handle_ls   : () -> u64 !{} @{} = fn(_: ()) -> 1u64
  pub let handle_pwd  : () -> u64 !{} @{} = fn(_: ()) -> 2u64
  pub let handle_echo : () -> u64 !{} @{} = fn(_: ()) -> 3u64
  pub let handle_exit : () -> u64 !{} @{} = fn(_: ()) -> 0u64

  // Dispatch: match on command tag with O(1) jump table.
  // Density check: 4 arms, range 0..3 → 100% coverage, passes.
  pub let dispatch : (Cmd) -> u64 !{} @{} = fn(c: Cmd) -> match c @jump_table {
    Ls   => handle_ls(()),
    Pwd  => handle_pwd(()),
    Echo => handle_echo(()),
    Exit => handle_exit(()),
  }

  // Entry: invoke dispatch on Echo command.
  pub let entry : () -> u64 !{} @{} = fn(_: ()) -> dispatch(Cmd::Echo)
}
```

**Rationale for design choices**:
1. **Unit-variant enum**: Simplifies dispatch logic and guarantees `@jump_table` codegen (no fallback to cmp/je).
2. **Qualified constructor** `Cmd::Echo`: Hygiene; avoids confusion with bare constructors in nested scopes.
3. **`@jump_table` annotation**: Explicitly signals intent for O(1) dispatch; density contract is satisfied trivially (4 arms, range 4, 100%).
4. **Direct function calls in arms**: Each arm calls the corresponding handler inline. In production, handlers would accept command arguments.

**Progression to the target shape**:
When #994–996 land, this fixture becomes:

```paideia
// Target: after #994, #995, #996, #998b land
enum Cmd { Ls, Pwd, Echo, Exit }

pub let dispatch_table : HashMap<Str, (Str)->u64> = {
  "ls"   -> |arg: Str| -> ls_impl(arg),
  "pwd"  -> |arg: Str| -> pwd_impl(arg),
  "echo" -> |arg: Str| -> echo_impl(arg),
  "exit" -> |arg: Str| -> exit_impl(arg),
}

pub let dispatch : (Str) -> u64 = fn(cmd_name: Str) -> {
  match dispatch_table.get(cmd_name) {
    Some(handler) => (handler)(cmd_name),
    None => 1u64,
  }
}
```

## 5. Composition Rules

The three shapes compose to handle complex dispatch scenarios:

### Nested Dispatch: Enum-Tag Selector → Fn-Ptr Handler Selection

**Use case**: Commands with different parameter signatures (e.g., `ls` takes a single string path, `cat` takes two paths).

**Pattern**: Match on command enum first; each arm returns (or calls) a function pointer tailored to that command's signature.

```paideia
enum Cmd { Ls, Cat, Diff }

// Each command has its own signature
pub let ls_impl   : (Str) -> u64 !{} @{} = fn(path: Str) -> { /*...*/ }
pub let cat_impl  : (Str) -> u64 !{} @{} = fn(path: Str) -> { /*...*/ }
pub let diff_impl : (Str, Str) -> u64 !{} @{} = fn(p1: Str, p2: Str) -> { /*...*/ }

// Dispatch returns a handler function pointer
pub let dispatch : (Cmd) -> (() -> u64) !{} @{} = fn(c: Cmd) -> match c {
  Ls   => &ls_impl,
  Cat  => &cat_impl,
  Diff => &diff_impl,  // Note: different signature; requires care
}
```

Codegen: Enum tag match (cmp/je or jump-table) selecting which function pointer to return; caller then invokes the pointer.

### Fn-Ptr Array Indexed by Enum Tag

**Use case**: Homogeneous handlers (all same signature) for maximum performance.

**Pattern**: Build an array of function pointers, one per command, indexed by enum tag.

```paideia
enum Cmd { Ls, Pwd, Echo, Exit }

pub let handlers : () -> u64 !{} @{} [] = [
  &handle_ls,
  &handle_pwd,
  &handle_echo,
  &handle_exit,
]

pub let dispatch : (Cmd) -> u64 !{} @{} = fn(c: Cmd) -> {
  // Convert enum to u64 index, bounds-check, load pointer, call.
  let handler = handlers[c as u64];
  (handler)(())
}
```

Codegen: Array index computation + memory load + indirect call. Latency: ~3–4 cycles (array indexing + pointer dereference).

## 6. Migration Path to the Canonical Shape

As blockers land, the dispatch patterns above become obsolete. This table maps each blocker to the section(s) it supersedes.

| Issue | Title | Lands In | Supersedes | Rationale |
|-------|-------|----------|-----------|-----------|
| #1233 | encoder fat-pointer primitives | v1.x (deferred) | §2 Target notes | Enables 16-B closure pair layout; fundamental for closures |
| #994  | closure grammar + lowering | v0.19 (backlog) | §2 Target; aspirational closure syntax | Enables `\|T\|->R` types and `\|x\| body` literals |
| #995  | closure call | v0.19 (backlog) | §2 Target; `(f)(args)` form | Enables calling closures as first-class values |
| #996  | hashmap stdlib | v0.19 (backlog) | §2 Target; `HashMap<K, V>` container | Enables keyed dispatch maps; replacement for enum-based dispatch |
| #998b | Str comparison (`eq`) | v0.18 (deferred) | §2 Target; hash-table probe step | Enables command-name equality testing for open-addressing |
| #998c | Str hashing (hash function) | v0.18 (deferred) | §2 Target; hash probe | Enables hash computation for O(1) table lookup |

**Recommended action for paideia-os phase 11**: Implement command dispatch using one of the shapes in §3. Do not wait for the target shape (§2); the v0.18 shapes are production-ready and sufficient for a 30–50 command shell. When #994–996 land in v0.19+, a bulk rewrite to the target shape is a trivial refactor (enum dispatch → HashMap dispatch + closures).

## 7. Cross-References

**Paideia-as toolchain**:
- `design/toolchain/jump-table-attribute.md` — Syntax and density contract for O(1) dispatch.
- `design/toolchain/function-pointer-types.md` — Function-pointer types, address-of syntax, effect/capability annotations.
- `design/toolchain/fnptr-unsafe-pattern.md` — Safety constraints for function pointers and indirect calls.
- `design/toolchain/records-enums-phase4.md` — Enum definition and pattern matching semantics.

**Paideia-as blockers**:
- `design/paideia-as/v0.18-issue-994-closures.md` — Closure grammar, lowering, and environment capture.
- `design/paideia-as/v0.18-issue-997-option-result-stdlib.md` — Option type for nullable results (e.g., `HashMap::get` return type).
- `design/paideia-as/v0.18-issue-998-string-str-stdlib.md` — Str type and module-const gaps.

**Paideia-os terminal design**:
- `design/terminal/semantic-shell.md` §SH-D5 — Command-module functor design consuming this dispatch pattern.
- `design/terminal/command-registry.md` — On-disk registry format; runtime dispatch table derives from this.

**Related testing and validation**:
- `feedback_workerbee_verify_claims.md` — Anti-fabrication witnesses for documentation claims.

## 8. Non-Goals

This document does **not** cover:

1. **Dynamic command registration** (runtime plugin loading) — out of scope; paideia-os phase 11 uses a static command set.
2. **Runtime reflection** (inspecting command signatures at runtime) — out of scope; dispatch tables are compile-time constructs.
3. **Parameter-polymorphic dispatch** (template specialization of dispatch logic) — out of scope; handled by function overloading or generic types in a later phase.
4. **Capability-scoped dispatch tables** (varying the command set based on kernel capability) — separate paideia-os concern per `design/capabilities/`.
5. **Performance optimization beyond O(1)** (e.g., AVX2-accelerated hash probing via pa-r18-011) — optional; the three shapes in §3 are sufficient.

