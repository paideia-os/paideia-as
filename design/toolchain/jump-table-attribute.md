# @jump_table Attribute for Match Expressions (Issue #964)

## Overview

The `@jump_table` attribute enables dense O(1) dispatch for match expressions on integer-literal patterns. When applied to a match scrutinee, the elaborator analyzes arm coverage and, if density requirements are met, generates a rodata table of code addresses and emits memory-indirect jump instructions instead of a linear cmp/je chain.

This is essential for protocol dispatch in the network stack (IPv4 protocol field, TCP option kinds, UDP port dispatch) and other performance-critical pattern matching.

## Grammar

```
match_expr ::= match <expr> [@jump_table] { <match_arms> }
match_arms ::= <match_arm> ( , <match_arm> )* [,]
match_arm  ::= <int_literal> => <expr>
            | _ => <expr>
```

Syntax:
- The `@jump_table` keyword appears after the scrutinee expression and before the opening brace.
- No parameters or parentheses; `@jump_table` is a flag attribute.
- All arms except the default (`_`) must be integer literals.
- A default arm (`_`) is mandatory.

Example (dense):
```paideia
match protocol @jump_table {
  6  => tcp_handler(),    // TCP
  17 => udp_handler(),    // UDP
  _  => unknown_handler(),
}
```

Example (sparse, rejected):
```paideia
match flags @jump_table {
  0x01 => flag_1_handler(),
  0x1000 => flag_2_handler(),  // Too large gap; density check fails
  _ => default_handler(),
}
```

## Density Contract

When `@jump_table` is set on a match, the elaborator computes:

- **min_arm**: minimum integer-literal arm value
- **max_arm**: maximum integer-literal arm value
- **range**: `max_arm - min_arm + 1`
- **covered_count**: number of distinct integer-literal arms
- **density**: `covered_count / range`

Constraints:
1. **Coverage density**: `covered_count * 2 >= range` (at least 50% coverage)
2. **Range bound**: `range <= 256` (8-bit dispatch index; prevents unreasonably large tables)
3. **Default arm required**: A default (`_`) arm must be present (P0274)
4. **Integer-literal-only arms**: All non-default arms must be integer literals (P0275)

If any constraint is violated:
- P0274 if no default arm
- P0275 if any non-default arm is not an integer literal
- P0272 if density is too sparse or range exceeds 256

When P0272 fires, the elaborator falls back to cmp/je dispatch.

## Synthesis Contract (Deferred to pa-r15-009b)

When density checks pass and `@jump_table` is valid, codegen will:

1. **Normalize the scrutinee**: `sub rax, min_arm` (shift values into 0..range)
2. **Bounds check**: `cmp rax, range; ja _default`
3. **Dispatch**: `jmp [rip + _jt + rax*8]` (memory-indirect jump via rodata table)
4. **Rodata synthesis**: Emit a read-only table `_jt` with `range` code-address entries

This codegen is currently deferred (P0273: "codegen deferred to follow-up issue"). The cmp/je fallback remains in effect for now.

## Diagnostics

| Code   | Severity | Category | Message | Rationale |
|--------|----------|----------|---------|-----------|
| P0270  | Error    | Parser   | Unknown match attribute | Typo in attribute name |
| P0271  | Error    | Parser   | Malformed match attribute | Missing identifier after `@` |
| P0272  | Warning  | Elaborator | Density too sparse | Insufficient coverage for jump table; falling back to cmp/je |
| P0273  | Warning  | Elaborator | Codegen deferred | Full codegen deferred to pa-r15-009b; using cmp/je fallback |
| P0274  | Error    | Elaborator | Missing default arm | `@jump_table` requires a default `_` arm |
| P0275  | Error    | Elaborator | Non-integer arms | `@jump_table` requires all non-default arms to be integer literals |

## Rationale

1. **Performance**: Dense jump tables provide O(1) dispatch with minimal branch prediction overhead, essential for protocol parsing.
2. **Safety**: The default arm requirement ensures all scrutinee values are handled.
3. **Explicitness**: Requiring `@jump_table` makes dispatch strategy explicit and auditable.
4. **Phased implementation**: Parser + AST recognition in Phase 15 m1 (issue #964); codegen in Phase 15 m2 (pa-r15-009b).

## Example

```paideia
fn dispatch_ipv4_protocol(protocol: u8) -> u32 {
  match protocol @jump_table {
    6  => 0x0001,  // TCP
    17 => 0x0002,  // UDP
    1  => 0x0004,  // ICMP
    _  => 0xFFFF,  // Unknown
  }
}
```

**Elaborator analysis:**
- min_arm = 1, max_arm = 17, range = 17, covered = 3
- density = 3 / 17 ≈ 17.6% < 50% → P0272 (too sparse)
- Falls back to cmp/je chain

```paideia
fn dispatch_ipv4_protocol_dense(protocol: u8) -> u32 {
  match protocol @jump_table {
    1  => 0x0004,  // ICMP
    2  => 0xFFFF,  // (unused)
    3  => 0xFFFF,  // (unused)
    4  => 0xFFFF,  // (unused)
    5  => 0xFFFF,  // (unused)
    6  => 0x0001,  // TCP
    17 => 0x0002,  // UDP
    _  => 0xFFFF,  // Unknown
  }
}
```

**Elaborator analysis:**
- min_arm = 1, max_arm = 17, range = 17, covered = 7
- density = 7 / 17 ≈ 41% < 50% → P0272 (still sparse)

For dense dispatch, create contiguous ranges:

```paideia
fn dispatch_tcp_options(kind: u8) -> u32 {
  match kind @jump_table {
    0  => 0x0001,  // EOL
    1  => 0x0002,  // NOP
    2  => 0x0004,  // MSS
    3  => 0x0008,  // WSCALE
    _  => 0xFFFF,  // Unknown
  }
}
```

**Elaborator analysis:**
- min_arm = 0, max_arm = 3, range = 4, covered = 4
- density = 4 / 4 = 100% ✓
- Passes density check; P0273 emitted (codegen deferred)
- Elaborator emits cmp/je chain for now

## Status

- **Phase 15 m1** (#964): Parser + AST recognition + validation diagnostics + design doc ✓
- **Phase 15 m2a** (#1031): Encoder primitive `encode_jmp_mem_rip_index_scale` (backtrack issue)
- **Phase 15 m2b** (#1032): Codegen pass for jump-table synthesis + rodata + memory-indirect jmp (backtrack issue)

Backtrack issues track deferred codegen work; this issue (#964) lands parser + AST + design + tests, with elaborator emitting P0273 (codegen deferred) and cmp/je fallback in effect.

---

**References:**
- Issue #964 (parent): `@jump_table` attribute and codegen
- Issue #1031: `encode_jmp_mem_rip_index_scale` primitive
- Issue #1032: Jump-table codegen + rodata synthesis
- Design doc: [`ring-attribute.md`](ring-attribute.md) (precedent for attribute-driven elaboration)
