# paideia-as-intrinsic

**Status:** Shipped (scaffolded 2026-09-03 for Wave 0 Batch 3; substantive content lands in the v0.26/v0.27/v0.30 M1 intrinsic rows).

AML / DMA / GPU / Vulkan intrinsic bindings for paideia-as. Hosts
the `atomic128`, `dma_buffer`, `include_signed`, `f16` and sibling
intrinsic surfaces that downstream paideia-os driver and compositor
code links against.

## Example

Consumed via the stdlib trait registry from `.pdx` source — the
intrinsics do not have a direct user-facing API in this crate.

## See also

- `paideia-as-elaborator/stdlib_lowering.rs` — direct-mnemonic
  lowering for bodyless intrinsic methods.
- [`../../design/toolchain/stdlib-trait-lowering.md`](../../design/toolchain/stdlib-trait-lowering.md).
