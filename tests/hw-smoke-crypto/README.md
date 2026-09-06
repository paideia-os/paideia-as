# v0.33 Crypto KAT hw-smoke

## Overview

Boot-smoke harness for paideia-as#1394: proves that Argon2id (RFC 9106),
ChaCha20-Poly1305 (RFC 8439), and ML-KEM-768 (FIPS 203 / NIST ACVP)
reproduce their spec vectors byte-for-byte when invoked through the
**real `.pdx` -> extern-C FFI-thunk call boundary**
(`stdlib_lowering::cryptoops`), not merely via `cargo test -p
paideia-as-crypto` on the host. See `tools/hw-smoke-v0.33.md` for the
full design (per-primitive vectors, register contracts, marker names).

## Scope

`cargo test -p paideia-as-crypto` already pins the three primitives'
byte-exact behaviour on the host — the RFC/FIPS/ACVP vectors live in
their respective modules and gate every commit through the workspace
test suite. That covers algorithmic conformance.

This harness covers the *invocation* half of the contract: a `.pdx`
source file (`fixtures/hw_smoke_crypto_v0_33.pdx`) calls
`Argon2id::derive`, `ChaCha20Poly1305::{seal,open}`, and
`MlKem768::{keygen,encaps,decaps}` exactly as `stdlib_lowering`
lowers them, is compiled by `paideia-as build --emit elf64`, linked
against `libpaideia_satellite_runtime.a` (which resolves the six
`paideia_crypto_*` symbols), and booted under QEMU. Each probe
byte-compares its primitive's output against the pinned KAT constant
and writes a pass/fail marker to COM1 (0x3F8). Failure modes this
catches that host tests cannot:

- SysV register/argument marshalling regressions in
  `stdlib_lowering::cryptoops`.
- Static-data layout drift between the `.pdx`-side params blobs
  (`Argon2idParamsC` / `AeadParamsC`, hand-laid-out as `[u64; N]`
  arrays) and the Rust `#[repr(C)]` declarations in
  `crates/paideia-as-crypto/src/ffi/`.
- Link-time symbol resolution differences against
  `libpaideia_satellite_runtime.a`.

## KAT vectors and spec sources

| Primitive | Spec source | Vector |
|---|---|---|
| Argon2id | RFC 9106 Sec.5.3 | canonical Argon2id v=0x13 test vector |
| ChaCha20-Poly1305 | RFC 8439 Sec.2.8.2 | canonical AEAD_CHACHA20_POLY1305 test vector |
| ML-KEM-768 KeyGen | FIPS 203 Sec.6.1 / NIST ACVP-Server commit `65370b8` | `keyGen`, testGroup 2, tcId 26 |
| ML-KEM-768 Encaps | FIPS 203 Sec.6.2 / NIST ACVP-Server commit `65370b8` | `encapsulation`, testGroup 2, tcId 26 |
| ML-KEM-768 Decaps | FIPS 203 Sec.6.3 / NIST ACVP-Server commit `65370b8` | `decapsulation`, testGroup 5, tcId 88 (first "no modification" entry) |

Every byte constant in the fixture is transcribed **mechanically** (by
a one-off generator script, not by hand) from the single-source-of-truth
`pub const` declarations in `crates/paideia-as-crypto/src/{kdf,aead,kem}/*.rs`,
so a vector drift between the host tests and this hw-smoke would be a
generator bug, not a hand-transcription typo.

## Honesty note vs. `tools/hw-smoke-v0.33.md`

The design doc's FAIL markers carry a mismatching byte offset and the
expected/actual bytes. This harness's FAIL markers are fixed strings
(`HWSMOKE_KAT_FAIL_<...>`, no dynamic payload) — there is no stdlib
primitive yet for formatting an integer to hex from `.pdx` source, so
a per-byte diagnostic is out of reach for a boot-smoke probe. The
boot-smoke signal here is *which primitive regressed*, not *which
byte*; a full binary diff on a real failure is the host-side `cargo
test -p paideia-as-crypto` suite's job.

Unsafe-block grammar (issue #1077/#1088 in this repo): `unsafe` blocks
accept only raw asm mnemonics, labels, and zero-arg call-expression
statements — `let`/`while`/`if` inside `unsafe` still fire the U1614
diagnostic. All branching, looping, and the byte-compare helper
therefore live in ordinary (non-`unsafe`) functions; every `unsafe`
block in the fixture is pure straight-line asm (mirrors
`tests/build-emit/boot_observable.pdx`).

## Markers

Per-primitive pass: `HWSMOKE_KAT_OK_<NAME>`. Per-primitive fail:
`HWSMOKE_KAT_FAIL_<NAME>`. All six pass: aggregate
`HWSMOKE_KAT_OK_V0_33_ALL`. See `tests/smoke.rs` for the exact marker
strings asserted.

## Running

```sh
# Diagnostic only, always runs, no QEMU required:
cargo test -p paideia-hw-smoke-crypto

# The actual boot: needs qemu-system-x86_64 on PATH. Compiles
# paideia-as + paideia-satellite-runtime in release mode, links the
# fixture against libpaideia_satellite_runtime.a, boots under QEMU,
# and greps the captured serial output for all six OK markers plus
# the aggregate marker.
cargo test -p paideia-hw-smoke-crypto -- --ignored --nocapture
```

## Architecture

- `src/lib.rs`
  - `HwSmokeEnv::probe()` — detect `qemu-system-x86_64` on PATH.
  - `build_kat_elf(root, out_elf)` — `cargo run --release -p paideia-as
    -- build --emit elf64 <fixture> -o <obj>`, then `cargo build
    --release -p paideia-satellite-runtime`, then `ld -T
    tests/build-emit/link.ld <obj> <archive> -o <out_elf>`.
  - `boot_and_capture_serial(env, elf)` — spawn QEMU with the same
    flags `tools/run-smoke.sh` uses (`-serial file:<log> -display none
    -no-reboot -no-shutdown -m 32M`), hard-killed via `timeout 10`
    (the fixture always reaches `hlt` and idles).
- `fixtures/hw_smoke_crypto_v0_33.pdx` — the boot-smoke `.pdx` source:
  inline trait redeclarations (matches
  `tests/build-emit/trait_call_lambda_body.pdx`'s only precedent for a
  stdlib trait call in this corpus), KAT constant data, params-blob
  static layouts, a `BytesOps::get_u8`-based `bytes_eq` helper, six
  probe functions, thirteen marker-emission leaf functions, and a
  safe (non-`unsafe`) `_start` orchestrator.
- `tests/smoke.rs` — env-check (active) + boot-and-verify (`#[ignore]`'d).

## Not build-verified by the authoring agent

This harness and fixture were authored by a sub-agent that is
structurally forbidden from invoking `bash tools/build.sh` or any
QEMU-boot script (builds are main-only in this project's workflow —
a sub-agent's turn ends before a backgrounded build's completion
notification would arrive). The `.pdx` syntax choices here are
corroborated against working `tests/build-emit/*.pdx` fixtures and the
`unsafe`-block grammar's git history (see the honesty note above), but
this is the **first** `.pdx` source in the corpus to call any of the
v0.33 (or earlier: Sha256/X25519/Hkdf/MlDsa65/…) crypto stdlib traits
end-to-end — every other landed crypto trait has zero `.pdx`-level call
sites anywhere in this repo. Treat a first build/boot attempt here as
genuinely untested integration surface, not a near-certain pass.
