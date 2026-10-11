# paideia-as-emitter-pe

**Status:** Shipped — PE/COFF emitter for Microsoft x64 / UEFI binaries.

Emits Microsoft-x64 PE/COFF object files, used primarily for UEFI
targets. A SysV ↔ MS-x64 thunk sits at the calling-convention
boundary so paideia-as code (SysV internally) can be called from
UEFI firmware.

## Example

```sh
paideia-as build examples/01_hello_module.pdx --emit pe-coff -o /tmp/hello.obj
```

## See also

- [`../../design/toolchain/calling-convention.md`](../../design/toolchain/calling-convention.md)
  — SysV ↔ MS-x64 thunk shape.
- [`../../design/toolchain/attribute-link_section.md`](../../design/toolchain/attribute-link_section.md)
  — UEFI section discipline.
