# Function-Pointer Types (pa-r17-001)

**Status**: Design specification for issue #979.

## 1. Overview

Paideia-as supports first-class function-pointer types via the syntax `(T1, T2, ...) -> R !{effects} @{capabilities}`. The grammar production treats parentheses followed by `->` as function-pointer syntax, with optional effect rows and capability annotations on the return type. The AST variant `TypeData::FnPtr` carries parameter types, return type, optional effect set, and optional capability set.

## 2. Motivation

Function-pointer types are essential for systems programming on PaideiaOS:

- **VFS vops tables**: Virtual filesystem operation tables (read, write, seek, etc.) are stored as records containing function pointers with specific signatures.
- **Driver dispatch tables**: Hardware drivers define dispatch tables of function pointers for interrupt handlers, I/O operations, and state transitions.
- **Syscall tables**: The kernel syscall interface maps syscall numbers to handler function pointers with consistent effect and capability constraints.
- **Callback registration**: Capability-aware callback mechanisms require precise typing of function pointers with effect and capability annotations.

See also `design/toolchain/fnptr-unsafe-pattern.md` for the rationale behind function-pointer safety constraints.

## 3. Grammar

The function-pointer type production in Phase 1:

```
Type         ::= TypeInner TypeModifier*
TypeInner    ::= TypeName | ParenOrFnPtr | LinearClass | ...
ParenOrFnPtr ::= '(' TypeList? ')' FnPtrTail?
FnPtrTail    ::= '->' Type EffectRow? CapabilitySet?
```

**Key points**:
- Empty parentheses `()` followed by `->` parse as zero-parameter function-pointer type.
- Single element in parentheses `(T)` followed by `->` parses as one-parameter function-pointer type, *not* as a grouped type (which is handled by the non-arrow case).
- Multiple elements `(T1, T2, ...)` followed by `->` parse as multi-parameter function-pointer type.
- Without `->`, parentheses group a type or tuple: `(T)` is type `T`, and `(T1, T2)` is a tuple type.

## 4. Precedence

Operator precedence (highest binding first):

| Tier | Operator | Description |
|------|----------|-------------|
| 1 | `*`, `&`, `&mut` | Pointer, reference, mutable reference |
| 2 | `[T; N]` | Array type |
| 3 | `(T1, T2, ...)` | Tuple or function-pointer parameter list |
| 4 | `-> R` | Function-pointer return type (only after params) |
| 5 | `!{...} @{...}` | Effect row and capability set (suffix on return) |

**Example**: `*((u32) -> u32) -> u32` parses as `(* ((u32) -> u32)) -> u32` (pointer to fn-ptr param, returning u32).

## 5. Semantic Model (Type Only)

The semantic model for function-pointer types is **type-only** in Phase 1:

- `TypeData::FnPtr { params, ret, effects, capabilities }` represents a function-pointer type.
- `params: Vec<NodeId>` contains type nodes for each parameter.
- `ret: NodeId` is the return type node.
- `effects: Option<NodeId>` is an effect row (e.g., `!{io, memory}`), or `None` if absent.
- `capabilities: Option<NodeId>` is a capability set (e.g., `@{SysRead}`), or `None` if absent.

**No value-level operations** are defined in Phase 1:
- Taking the address of a function: `&fn_name` (function reference, issue #981)
- Calling a function pointer: `call fptr(args)` (issue #982)

These will be addressed in subsequent phases.

## 6. Effect Row Treatment

Effect rows on function-pointer types annotate the *return position only*:

```
(u32, u64) -> R !{eff1, eff2}  // ✓ effects on return
(u32 !{io}, u64) -> R           // ✗ effects on parameter (forbidden)
```

The effect annotation describes the computational effects that occur when the function is called. Per the paideia-os capability model, effects are not attached to individual parameters; they are a property of the entire computation. This aligns with the standard ML typing of effects in higher-order languages.

## 7. Capability Annotation

Capability annotations are retained from the Arrow variant:

```
(u32, u64) -> u32 @{CapRead, CapWrite}  // explicit caps
(u32, u64) -> u32 @{}                   // empty cap set (no special caps required)
(u32, u64) -> u32                       // no caps (equiv. to unrestricted)
```

Capabilities constrain which operations the function may perform (e.g., only syscalls that require `CapRead` capability). The paideia-os stdlib will enforce these constraints at the signature level.

## 8. Examples

### Virtual filesystem read operation:

```
record {
  read:   (*inode_t, *buffer, count) -> i64 !{io}
  write:  (*inode_t, *buffer, count) -> i64 !{io}
  seek:   (*inode_t, offset) -> i64
  close:  (*inode_t) -> ()
}
```

### Interrupt handler dispatcher:

```
type irq_handler = (*context) -> () !{Interrupt} @{IrqWrite, ContextRead}
```

### Capability-aware callback:

```
(fn_ptr: (*request) -> *response @{NetRead}) -> ()
```

### Nested function pointers:

```
( fn: ((u32) -> u32) ) -> ((u64) -> i64)
```

## 9. AST Representation

The parser emits `TypeData::FnPtr` nodes (previously `TypeData::Arrow`):

```rust
TypeData::FnPtr {
    params: vec![param_ty_1, param_ty_2],
    ret: return_ty,
    effects: Some(effect_row),
    capabilities: None,
}
```

Pattern matching in the elaborator, visitor traits, and reflection APIs uses the variant name `FnPtr`. The `TermHead::TypeFnPtr` discriminant allows classification without field access.

## 10. Non-goals

- **No new AST variant beyond rename**: The AST already had `TypeData::Arrow`; this issue renames it to `FnPtr` and completes the reflection surface.
- **No value-level construction**: Creating function pointers from Rust function names is issue #981.
- **No lowering**: The elaborator maps function-pointer types to IR types (not lowered further) in Phase 1.
- **No paideia-os integration**: The stdlib will use function-pointer types for vops and capability dispatch, but that is outside the paideia-as scope.
- **No lexer keyword additions**: The `->` operator and `!{...} @{...}` syntax already exist; no new keywords are needed.

## 11. Cross-references

- `design/toolchain/fnptr-unsafe-pattern.md` — rationale for function-pointer safety constraints and why effects/caps are necessary
- `design/phase-4/records-enums.md` — record types that contain function-pointer fields (vops shape)
- `design/phase-1/tactical-issues.md` §7 — function-pointer types in the phase-1 roadmap
- Issue #981 — `&fn_name` syntax for taking function references
- Issue #982 — `call fptr` syntax for function-pointer calls
