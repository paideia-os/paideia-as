# CHANGELOG scratch — Wave υ (paideia-as υ-01 / υ-02)

Target workspace version: **0.36.4** (per Wave υ dispatch note; main consolidates
against parallel λ scratch before bumping `workspace.version`).

## υ-01 + υ-02 — ML-DSA-65 compact-ABI FFI in `paideia-as-crypto`

**What lands.** A `no_std + alloc` post-quantum digital-signature
surface inside `paideia-as-crypto`, sibling of the ML-KEM-768 surface
landed at v0.33 (issue #1352). Two extern-C thunks
(`mldsa65_verify`, `mldsa65_sign`) carry the compact task-spec ABI —
`u32` / `u64` returns, no `pk_len` on the wire — that the kernel and
satellite `.pdx` runtime prefer over the pre-existing std-linked
`paideia-pq-sign::ffi::mldsa65_{sign,verify}_runtime_entry` thunks.

- **`crates/paideia-as-crypto/Cargo.toml`**: new `ml-dsa = { workspace
  = true, default-features = false, features = ["alloc"] }` dep;
  `std` feature adds `ml-dsa/std`. Workspace `ml-dsa = "0.1"` already
  present (paideia-pq-sign consumer); crate-level `default-features =
  false` keeps std out of the graph for satellite / kernel builds.
- **`crates/paideia-as-crypto/src/sig/`** (NEW module): `mod.rs`
  documents the shared "one marker struct per parameter set, fixed-
  size buffers on the surface" invariants; `ml_dsa_65.rs` wraps the
  `ml-dsa` `SigningKey` / `VerifyingKey` in a
  `MlDsa65::{sign, verify}` API returning `Vec<u8>` /
  `Result<bool, SigError>`. Deterministic sign variant (`rnd = [0;
  32]`) — matches `paideia-pq-sign::mldsa::MlDsa65Marker::sign`.
- **`crates/paideia-as-crypto/src/ffi/ml_dsa_65.rs`** (NEW): two
  `#[unsafe(no_mangle)]` thunks with the SysV register mapping
  documented on each. Null-guarded; `sig_len` and `out_sig_max`
  gates precede any dereference. 10 unit tests: round-trip
  sign→verify, flipped-signature rejection, wrong-message rejection,
  wrong-length rejection, NULL rejection on every reachable pointer,
  undersized-output rejection, empty-message round-trip.
- **`crates/paideia-as-crypto/src/ffi/mod.rs`**: new `pub mod
  ml_dsa_65` + `pub use ml_dsa_65::{mldsa65_sign, mldsa65_verify}`.
- **`crates/paideia-as-crypto/src/lib.rs`**: new `pub mod sig` + root
  re-exports `MlDsa65`, `MlDsa65SigError`, `MLDSA65_PK_LEN`,
  `MLDSA65_SEED_LEN`, `MLDSA65_SIG_LEN`.
- **`crates/paideia-satellite-runtime/src/lib.rs`**: `pub use
  paideia_as_crypto::ffi::mldsa65_{sign,verify}` so satellite
  `ld -nostdlib` link lines resolve the new symbols. The pre-existing
  `mldsa65_{sign,verify}_runtime_entry` fail-closed stubs are
  UNTOUCHED — they continue to serve the `MlDsa65` (non-compact)
  dispatch until the migration wave that flips the two.
- **`crates/paideia-as-elaborator/src/stdlib_lowering/cryptoops/mldsa65.rs`**
  (NEW): dispatch arm for the new `MlDsa65C` trait. Routes `sign`
  and `verify` to `mldsa65_sign` / `mldsa65_verify` via
  `extern_recipe`. 3 unit tests pin the recipe shape + symbol names.
- **`crates/paideia-as-elaborator/src/stdlib_lowering/cryptoops/mod.rs`**:
  new `mod mldsa65;` + `try_lower_mldsa65_c` entry point.
- **`crates/paideia-as-elaborator/src/stdlib_lowering/mod.rs`**: new
  `"MlDsa65C" => cryptoops::try_lower_mldsa65_c(...)` dispatch arm
  alongside the pre-existing `"MlDsa65" => mldsaops::try_lower(...)`
  arm. Two traits coexist; migration path documented in the trait
  file's header.
- **`crates/paideia-as-stdlib/pdx/crypto/mldsa_65_c.pdx`** (NEW):
  `trait MlDsa65C { fn sign(...); fn verify(...); }` — compact-ABI
  method signatures, SysV register documentation, effect row
  `!{crypto, mem} @{paideia.crypto}`.

**Why two traits.** The pre-existing `MlDsa65` trait routes to
`paideia-pq-sign` runtime-entry symbols consumed by std-linked CLI
tooling (`libpdx-volume` `pdxb_sign_superblock`, `umount.pdxfs`
`unmount_op_sign_locked`, and the `.pdxpkg` / `.pdxtrust` chains
once those repos exist). Retiring those symbols out from under
those call sites in this wave is out of scope. `MlDsa65C` is the
compact SysV-clean surface preferred by kernel- and satellite-linked
`.pdx` code, targeting the `no_std + alloc` thunks. A future wave
migrates every `MlDsa65::*` call site to `MlDsa65C::*` and retires
the pq-sign runtime-entry surface.
