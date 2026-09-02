# Changelog

## 0.29.1 — 2026-09-02

### Satellite runtime shim (R91-XREPO.M1 Phase A follow-up)

- **Restructure: `paideia-as-crypto` reverts to rlib-only; `paideia-satellite-runtime` becomes the workspace's single staticlib and absorbs the no_std runtime infrastructure.** The 0.29.1 first-pass refactor kept `crate-type = ["rlib", "staticlib"]` on `paideia-as-crypto` while turning the crate into a true `no_std + alloc` build. That was still the wrong architecture: emitting a bare staticlib forces the Rust compiler to demand `#[global_allocator]` + `#[panic_handler]` + `panic = "abort"` inside the crate producing the staticlib, and those three decisions do not belong to a leaf crypto library — they belong to the final ELF. Attempting to build the crate failed at compile time with three complementary errors:
    * `no global memory allocator found but one is required; link to std or add #[global_allocator] to a static item that implements the GlobalAlloc trait`
    * `` `#[panic_handler]` function required, but not found ``
    * `unwinding panics are not supported without std`
  The corrected shape:
    1. `crates/paideia-as-crypto/Cargo.toml` `[lib] crate-type = ["rlib"]` only. The `no_std + alloc` posture, the hand-rolled CPUID feature checks, the `default-features = false` on `argon2` / `chacha20poly1305` / `thiserror`, and the opt-in `std` feature all remain — the crate is still built to be safe to link into a satellite via a wrapper. It just no longer emits its own `.a`. Kernel-side consumers (`paideia-as-runtime` and friends) are unaffected: they consumed the rlib path exclusively and continue to do so.
    2. `crates/paideia-satellite-runtime/` becomes the only `crate-type = ["staticlib"]` in the workspace. Its `src/lib.rs` now provides, alongside the four FFI symbols it already carried, a hand-rolled bump allocator (4 MiB static pool, `AtomicUsize`-serialized offset, no free) as `#[global_allocator]`; a `#[panic_handler]` that halts the current thread via inline `hlt`; and an empty `rust_eh_personality` stub exported via `#[unsafe(no_mangle)]`. Rationale: satellite tools are one-shot processes with a bounded Rust-side heap footprint (argon2's memory-hard buffer at paideia-os parameter settings + Vec returns bounded by caller plaintext length), and a bump allocator is the smallest thing that satisfies `#[global_allocator]` without adding a Rust dependency (`linked_list_allocator` was considered — see the module doc comment for the swap path).
    3. Workspace `[profile.release]` gains `panic = "abort"`. Cargo does not allow `panic` in per-package profile overrides (the option is not on the `[profile.<name>.package.<name>]` allowlist), so this decision is workspace-wide by construction. Safety review: no workspace crate calls `catch_unwind` / `resume_unwind` (grep-verified across `crates/`, `tests/`, `tools/`); `cargo test` uses the `test` profile so `should_panic` tests continue to work; kernel rlib consumers run in bare-metal contexts where unwinding is a non-starter anyway. See the Cargo.toml comment for the full audit.
- **Kernel-side impact:** none. `paideia-as-runtime` consumes `paideia-as-crypto` as an rlib; that path is byte-behaviour-identical to before. Release-profile `panic = "abort"` matches the bare-metal semantics kernel builds already expect.
- **Design doc corrections:** `design/infrastructure/satellite-runtime-shim.md` §4.1 "Crypto/sign shim" and §3.1 "one crate emits both rlib and staticlib" updated to reflect the corrected architecture (paideia-as-crypto rlib-only, paideia-satellite-runtime as the wrapping staticlib that owns the allocator + panic handler + eh_personality). The "one Rust source of truth" R1 mitigation from §3.1 remains intact — it is now enforced by cargo's dependency graph via the path-dep in `paideia-satellite-runtime`, not by physically emitting two crate-types from `paideia-as-crypto`. §9 "Out of scope" retroactively records that a `#[global_allocator]` inside `paideia-satellite-runtime` has now landed as a hand-rolled bump allocator; the deferred `linked-list-allocator` feature-gated swap is still noted as a future option.
- **`paideia-as-crypto` refactored to true `no_std + alloc`.** The 0.29.0 landing added `crate-type = ["rlib", "staticlib"]` to `paideia-as-crypto` and asserted, in design §4.1 "Userspace safety", that the crate was already `no_std + alloc`. That claim was wrong: `src/rng/hardware.rs` used `std::is_x86_feature_detected!` at module scope, and every dependency (`argon2`, `chacha20poly1305`, `thiserror`) was pulled with its default `std` feature enabled. The resulting `libpaideia_satellite_runtime.a` carried ~80 unresolved libc / std / panic / unwind symbols (`malloc`, `mmap`, `__rust_alloc`, `_Unwind_*`, `read`, `write`, `getcwd`, and the `compiler_builtins` / `std::io` tails) that satellite `build.sh` lines using `ld -nostdlib --warn-common --fatal-warnings -T link.ld` cannot resolve. This landing fixes the architectural gap:
  1. `crates/paideia-as-crypto/src/lib.rs` becomes `#![cfg_attr(not(test), no_std)]` with `extern crate alloc;`. Under `cfg(test)` the crate is still std-linked so the existing test harness and integration tests (`tests/rfc_vectors.rs`) keep working unchanged; under any other compilation target (rlib for a workspace neighbour, staticlib for the satellite shim) it is a pure `no_std + alloc` build.
  2. `src/rng/hardware.rs` replaces the two `std::is_x86_feature_detected!("rdseed" / "rdrand")` calls with hand-rolled CPUID feature checks (`cpuid_has_rdseed` / `cpuid_has_rdrand`, direct `core::arch::x86_64::__cpuid` / `__cpuid_count`) — same bit sequence Intel SDM Vol. 2A §3.2 table 3-8 documents; no functional change, just a `core::arch` implementation of what the std macro wrapped. The unit test that pins the source-selection preference (`new_selects_source_by_cpu_capability`) now consults the same helpers so it cannot drift from the production path.
  3. `crates/paideia-as-crypto/Cargo.toml` sets `default-features = false` on every dependency that supports it: `argon2 = {..., features = ["alloc"]}`, `chacha20poly1305 = {..., features = ["alloc"]}`, `thiserror = {..., default-features = false}` (`thiserror` v2's `no_std + alloc` mode routes `#[derive(Error)]` through `core::error::Error`, available since Rust 1.81). A new `std` feature (opt-in, non-default) re-enables `thiserror/std` / `argon2/std` / `chacha20poly1305/std` for library consumers that need the full-fat surface.
  4. `crates/paideia-satellite-runtime/Cargo.toml` sets `default-features = false` on its `paideia-as-crypto` dep. This is redundant with `default = []` but makes the intent explicit so a future edit that flips `paideia-as-crypto`'s `default` list cannot silently reintroduce a std leak into the satellite archive.
  5. Alloc-heap types on the source-visible API (`Vec<u8>` returns from `Aead::seal` / `open`; `String` in `AeadError::Primitive` / `KdfError::Primitive`; `format!(...)` renders on the error path in `argon2id.rs` and `chacha20_poly1305.rs`) resolve to `alloc::` variants under `no_std` via explicit `use alloc::{...}` imports at every call site. The same names resolve to `std::` variants under `cfg(test)` — same underlying types (`alloc::vec::Vec` IS `std::vec::Vec`), no API surface change.
- **Consumer contract (as of the restructure above):** the satellite ELF no longer needs to supply its own `#[global_allocator]`, `#[panic_handler]`, or `rust_eh_personality` — the wrapping `paideia-satellite-runtime` staticlib now carries all three, so `-nostdlib` satellite `ld` lines close without additional Rust-side scaffolding. (The earlier "final ELF supplies allocator" contract was the pre-restructure shape; superseded — kept here as historical record of the intermediate architecture.)
- **Design doc corrections:** `design/infrastructure/satellite-runtime-shim.md` §4.1 "Userspace safety", §4.1 "satellite-runtime crate depends only on ml-dsa …" (contradicted §3.2's "Deliberately NO ml-dsa dependency" — code correctly followed §3.2), and §9 out-of-scope "Removing paideia-as-crypto's `std` dependency … just a size concern" all updated to reflect the actual link-time correctness constraint the refactor closes.
- **Retroactive correction to 0.29.0 entry below.** The 0.29.0 note that "the rlib output is byte-identical to before" was inaccurate: adding `crate-type = ["rlib", "staticlib"]` under the workspace's `lto = "fat"` release profile forces eager whole-crate codegen for the staticlib target, which can perturb the rlib artifact's on-disk bytes even though every `#[no_mangle]` symbol's exported name, signature, and behaviour is identical. The `[lib]` block's comment in `paideia-as-crypto/Cargo.toml` now spells this out explicitly; downstream consumers see no source-level or link-time change.
- **Version:** `workspace.version` bumped 0.29.0 → 0.29.1 (patch: corrects the load-bearing `no_std + alloc` correctness gap without changing any public API surface — every trait, type, function signature, and `#[no_mangle]` symbol remains identical). `find-paideia-as.sh` `MIN_VERSION` unchanged (0.4.0). No git tag this landing per the no-tag-in-session convention; the release tag moves at the next phase close alongside libpdx-audit Phase B and the paideia-os submodule bump.

## 0.29.0 — 2026-09-02

### Satellite runtime shim (R91-XREPO.M1 Phase A)

- **Issue #1348 — satellite tools cannot resolve paideia-as FFI intrinsics at link time.** `.pdx` code compiled by paideia-as emits `call` relocations against four intrinsics — `paideia_crypto_argon2id_derive`, `paideia_crypto_chacha20_poly1305_seal`, `paideia_crypto_chacha20_poly1305_open`, `mldsa65_sign_runtime_entry` — that today are shipped only as `.rlib` object code linked into the paideia-os kernel build. Satellite host tools (`mkfs.pdxfs`, `mount.pdxfs`, `umount.pdxfs`) build `.o` cleanly then fail at the final `ld` step with four unresolved symbols. **What lands (Phase A, crypto+sign side):** two coordinated Cargo-side changes that together produce `target/release/libpaideia_satellite_runtime.a`, a single satellite-linkable archive carrying all four symbols.
  1. `crates/paideia-as-crypto/Cargo.toml` gains `crate-type = ["rlib", "staticlib"]`. The exported ABI is unchanged (every `#[no_mangle]` symbol keeps its name, signature, and behaviour); the rlib artifact's on-disk bytes MAY differ under the workspace's `lto = "fat"` release profile because the staticlib target forces eager whole-crate codegen — see the correction in the 0.29.1 entry above. The additional staticlib output produces `libpaideia_as_crypto.a` carrying the three crypto FFI thunks as a single source of truth. This is the R1 (byte-match divergence) mitigation from `design/infrastructure/satellite-runtime-shim.md` §3.1 physically enforced by Cargo's dependency graph — there is exactly one Rust source for the AEAD (RFC 8439) and Argon2id (RFC 9106) bodies in the workspace, and both the kernel-side rlib consumer and the satellite-side staticlib consumer draw from it.
  2. New crate `crates/paideia-satellite-runtime/` with `crate-type = ["staticlib"]`, path-dep on `paideia-as-crypto`. `src/lib.rs` re-exports the three crypto thunks with `pub use` (Rust-level; `ld` finds one definition of each via the original `#[unsafe(no_mangle)]` in `paideia-as-crypto::ffi`) and defines a fail-closed `mldsa65_sign_runtime_entry` returning `PDX_MLDSA_ERR_NO_SIGNER = -6`. The `-6` sentinel is band-consistent with `paideia-pq-sign::ffi`'s existing `-1..-3` codes (`-4`/`-5` reserved for cross-primitive alias reuse in the paideia-as-crypto band). Design §3.2 rationale: satellite tools that currently ship do NOT sign at runtime — `mkfs.pdxfs`'s default is the PUBLIC key used only for BLAKE3 `sig_key_hash` — but the intrinsic MUST resolve at link time (`ld` has no dead-code elimination at `.o` granularity). Consequence tracked as a libpdx-volume follow-up (§3.2 last paragraph): `pdxb_sign_superblock` must propagate `NO_SIGNER` upward as a user-visible error, not swallow it as success (design R4).
- **Deliberately deferred to later phases:**
  - **Phase B (libpdx-audit#19):** the audit shim (`audit_broker_satellite.pdx` + `syscall_shim_satellite.pdx` + a `--profile=satellite` build path) lands in the `libpdx-audit` repo per §4.2. Independent of Phase A; unblocks the second half of the link-line gap.
  - **paideia-os submodule bump:** the kernel build reads paideia-as through a submodule pin; that pin moves to 0.29.0 in a separate paideia-os commit after this landing per the design's §5 step ordering (kernel build unaffected because the rlib path is byte-identical).
  - **Satellite `build.sh` cascade:** `--extra-archive PATH` support in `mkfs.pdxfs` / `mount.pdxfs` / `umount.pdxfs` (§4.1 last paragraph) lands as one PR per satellite repo, all trivially parallelizable, once Phase A + Phase B are both on disk.
  - **`paideia-pq-sign` staticlib for a future real-signer satellite:** deferred until the first signing satellite (`fsck.pdxfs` / `pkg.paideia-os`) is scoped. Would need a `no-hsm` feature gating out yubihsm/cryptoki/reqwest from the satellite build.
- **Version:** `workspace.version` bumped 0.28.1 → 0.29.0 (minor: additive workspace surface — one new crate, one new Cargo `crate-type` value on `paideia-as-crypto`; no source-level breakage anywhere else, no existing symbol renamed, moved, or altered). `find-paideia-as.sh` `MIN_VERSION` unchanged (0.4.0). No git tag this landing per the no-tag-in-session convention; the release tag moves at the next phase close alongside libpdx-audit Phase B and the paideia-os submodule bump.

## 0.28.1 — 2026-09-02

### Crypto

- **Issue #1347 — `mldsa65_verify` intrinsic (FIPS 204 §7.3).** New extern-C thunk `mldsa65_verify_runtime_entry` in `paideia-pq-sign::ffi`, projecting `MlDsa65Marker::verify` onto the SysV 6-register calling convention: `(msg_ptr, msg_len, sig_ptr, sig_len, pubkey_ptr, pubkey_len) -> i64`. Return-code surface matches the paideia-as-crypto FFI sentinels: `0` = valid, `-1` = InvalidParam (NULL where required), `-2` = InvalidLength (`pubkey_len != 1952` or `sig_len != 3309` — length gate is upstream of the borrow so a caller shape bug is never folded into an authentication failure), `-3` = AuthenticationFailed. Companion stdlib intrinsic declaration in `paideia-stdlib/pdx/mldsa.pdx` (added `fn verify` alongside the existing `fn sign` in the `MlDsa65` trait); elaborator recipe in `stdlib_lowering::mldsaops` routes `MlDsa65::verify` to the new symbol with `ArgConvention::SysVRegs`. **New tests:** sign→verify happy-path round-trip; single-bit signature flip rejects with `-3`; wrong pubkey/sig length rejects with `-2`; NULL sig/pubkey rejects with `-1`; empty-message-with-NULL-msg-ptr round-trip; signature-over-different-message rejects with `-3`. Unblocks `libpdx-volume#16` `pdxb_verify_superblock` and the pre-existing `pdxb_verify_inode_tail` v1.0.0 stub — both call the verify intrinsic and depend on the -2/-3 split to separate the malformed-header repair path from the eviction path.
- **Version:** `workspace.version` bumped 0.28.0 → 0.28.1 (patch: additive crypto intrinsic — new `mldsa65_verify_runtime_entry` symbol + one new `MlDsa65::verify` trait method, no source-level breakage). `find-paideia-as.sh` `MIN_VERSION` unchanged (0.4.0).

## v0.28.0

### Crypto

- **Issue #1341 (R100-PREP-005d) — Ed25519 sign + verify (RFC 8032 §5.1) + SHA-512 (FIPS 180-4 §6.4).** New `paideia-as-crypto::curve::ed25519` module: `ed25519_public_from_secret(sk) -> [u8; 32]`, `ed25519_sign(sk, msg) -> [u8; 64]`, `ed25519_verify(pk, msg, sig) -> bool`. Also new `paideia-as-crypto::hash::sha512` (portable u64 message schedule, 80-round compression, one-shot + streaming — mirrors the shape of the SHA-256 code from #1338). The Ed25519 point representation is extended twisted-Edwards `(X, Y, Z, T)` with `T = X*Y/Z`; group law is Hisil/Wong/Carter/Dawson 2008 unified add (add-2008-hwcd-3) plus dedicated doubling. Scalar mult for secret scalars is constant-time double-and-add with a `Point::cswap` on every bit. **Decision surfaced:** the previous #1341 stall was on a SUPERCOP `ref10`-transcription of `sc_muladd` using signed 21-bit limbs — the tail-carry chain was too easy to get subtly wrong. This round replaces the whole scalar-arithmetic path with a bit-serial modular reduction over 512-bit intermediates in 8 u32 limbs: `O(512)` iterations with a single conditional subtract per bit — much slower per op (still ≪ 1 ms per `sc_muladd` on modern x86_64), but the code is small enough to audit against the definition of `L = 2^252 + 27742317777372353535851937790883648493` directly, and the four-limb compare + subtract are trivially constant-time. **Point decompression** similarly ditches the SUPERCOP `beta = u * v^3 * (u * v^7)^((p-5)/8)` sqrt template — the intermediate `v^3` / `v^7` factoring is error-prone — for the direct-form `w = u * v^-1`, `candidate = w^((p+3)/8)`, `if candidate^2 != w: candidate *= sqrt(-1)`. Same asymptotic cost (dominant is the 253-square exponentiation), simpler and less bug-prone. **New tests:** RFC 8032 §7.1 TEST 1 / TEST 2 / TEST 3 / TEST SHA(abc) all pass sign + verify byte-for-byte; negative-verify (mutated sig / mutated msg / mutated pk / S ≥ L all reject without panic); round-trip pin on a stable-shape secret. SHA-512 additionally: FIPS 180-4 §D.4 "abc", §D.5 two-block, empty-input pin, streaming-equivalence across chunk sizes, §D.6 one-million-'a' as `#[ignore]`. Unblocks TLS 1.3 raw-key server auth in `libpdx-net` M3.
- **Version:** `workspace.version` bumped 0.27.4 → 0.28.0 (minor: additive crypto surface — new `ed25519` module + new `hash::sha512` module — no source-level breakage).

## v0.27.4

### Test hygiene

- **Issue #1345 — boot snapshot `paideia_os_m3_four_file_text_byte_snapshot` panics when PaideiaOS is absent.** `find_paideia_os` in `paideia_os_m3_829_byte_snapshot.rs` was the last panic-on-missing holdout — its m5_835 / phase1_rebuild siblings already return `Option<PathBuf>` and skip cleanly with an `eprintln!`. In a fresh clone of paideia-as (not embedded as a submodule of paideia-os), no ancestor contains `src/kernel/boot/kernel_main.pdx`, so the panic hard-failed the boot suite. Fix: change `find_paideia_os` to return `Option<PathBuf>` and have the test skip with an `eprintln!` referencing #1345 when it returns `None`. Preserves diagnostic value when PaideiaOS *is* available (via `PAIDEIA_OS_PATH` env or via the canonical submodule layout).
- **Version:** `workspace.version` bumped 0.27.3 → 0.27.4 (patch: test-only fix).

## v0.27.3

### Test hygiene

- **Issue #1344 — SARIF snapshot needs insta refresh (version drift).** `crates/paideia-as-diagnostics/src/sarif.rs` embeds `env!("CARGO_PKG_VERSION")` in the SARIF tool-driver `version` / `semanticVersion` fields, so every workspace version bump broke the `sarif::snapshot_multi_diagnostic` snapshot cosmetically. Rather than accepting the drift once (which just resets the timer to the next bump), wrap the `assert_snapshot!` call in `insta::with_settings!` with a regex filter that normalizes the tool-driver `semanticVersion` + `version` pair to `"0.0.0-snap"`. The regex matches the two fields together as they appear in the tool-driver block only — the outer SARIF-schema `"version": "2.1.0"` is left alone. Snapshot re-captured once with the placeholder in place; future patch/minor bumps now pass without re-accepting.
- **Version:** `workspace.version` bumped 0.27.2 → 0.27.3 (patch: test-hygiene only; SARIF emitter source unchanged).

## v0.27.2

### Test hygiene

- **Issue #1343 — VT-d QI descriptor snapshot regression (p_dev_iotlb PASID mask).** The `vtd_qi_smoke_descriptor_fields_match_intel_vtd_spec` field-decode extracted PASID for descriptors #14/#15 (type-8 `p_dev_iotlb`) with a 20-bit mask (`0xFFFFF`), copy-pasted from the type-6 `p_iotlb` decode where PASID is at bits 32-51 and legitimately 20-bit. The fixture packs `p_dev_iotlb` as three consecutive 16-bit fields (PASID at 16-31, SID at 32-47, MIP at 48-63) — decoding PASID with 20 bits spilled SID's low nibble into the PASID value (`0xD` low nibble of `0xABCD` → PASID read as `0xD0200`). Descriptor #14 accidentally passed because its SID (`0x0100`) has a zero low nibble. Fix is test-only: narrow the PASID mask to `0xFFFF` for both `p_dev_iotlb` decode blocks and add a code comment noting the field-width difference from `p_iotlb`. Encoder output is unchanged — the three other `vtd_qi_smoke_*` tests (byte-exact rodata, descriptor count/size, type coverage) all passed already.
- **Version:** `workspace.version` bumped 0.27.1 → 0.27.2 (patch: test-only fix; encoder/fixture bytes unchanged).

## v0.27.1

### Test fixtures

- **Issue #1342 — M0305 mismatch in `ms_x64_5arg_call` fixture.** `tests/build-emit/ms_x64_5arg_call.pdx` declared `module MsX645ArgCall`, but M0305's PascalCase transform (`crates/paideia-as-elaborator/src/file_module.rs::expected_module_name`) splits the stem on `_`/`-` and can only uppercase a segment's first ASCII letter — the `5arg` segment starts with a digit so its `a` stays lowercase, yielding `MsX645argCall`. Same class of rename that landed today for the paideia-os `sched_wait.pdx` / `elevate_broker_dispatch.pdx` fixtures. Fix: rename the module identifier (fixture-side only — the five build_emit tests reference the file basename, not the module name).
- **Version:** `workspace.version` bumped 0.27.0 → 0.27.1 (patch: test-fixture-only fix).

## v0.27.0

### Crypto

- **Issue #1340 (R100-PREP-005c) — X25519 typed intrinsic (RFC 7748 §5).** New `paideia-as-crypto::curve` module with two submodules: `curve::field25519` (constant-time radix-2^51 arithmetic in `GF(2^255 - 19)` — `add`/`sub`/`mul`/`square`/`invert`/`cswap`/`mul_a24`/`from_bytes`/`to_bytes`, all touching every limb regardless of secret data) and `curve::x25519` (`x25519_scalarmult(scalar, point)` and `x25519_public_from_secret(secret)`). The field code is deliberately factored out because Ed25519 (#1341) shares the same prime `p = 2^255 - 19` — the Montgomery-ladder and twisted-Edwards forms disagree only on the curve group law, not the underlying field. Scalar clamping (RFC 7748 §5: `k[0] &= 248; k[31] &= 127; k[31] |= 64;`) and u-coordinate top-bit masking (RFC 7748 §5 `decodeUCoordinate`) both live inside `x25519_scalarmult`. **Decision surfaced:** used `a24 = (A - 2) / 4 = 121665` from RFC 7748 §5 verbatim, not the `121666` value used in some non-RFC formulations (dalek's `A_PLUS_2_DIVIDED_BY_4` is misnamed — its value is 121665 too; the initial 121666 attempt failed all RFC vectors and pinned the bug in the first CI run). Field inversion uses the standard 254-square + 11-multiply addition chain over `p - 2` (Bernstein `curve25519-donna` notes). Zero-dep. **New tests:** RFC 7748 §5.2 first two published vectors, iteration test after 1 iter (`422c8e7a…3079`), iteration test after 1000 iters (`684cf59b…2c51`), 1M-iter test as `#[ignore]` (CI-time-prohibitive, run with `--ignored`); RFC 7748 §6.1 Diffie-Hellman worked example (Alice+Bob keys and shared secret); field-arithmetic property tests (`(a+b)-b = a`, `a * a^-1 = 1`, `cswap` under 0 vs 1, byte-round-trip with the top-bit mask honoured, `mul_a24` matches `mul(a, 121665)`).
- **Version:** `workspace.version` bumped 0.26.0 → 0.27.0 (minor: additive crypto surface, no source-level breakage). `find-paideia-as.sh` `MIN_VERSION` unchanged.

## v0.26.0

### Crypto

- **Issue #1339 (R100-PREP-005b) — HMAC-SHA256 + HKDF-Extract/Expand (RFC 2104 / FIPS 198-1 / RFC 5869).** New `paideia-as-crypto::kdf::hkdf` module: `hmac_sha256(key, msg)`, `hkdf_extract(salt, ikm)`, and `hkdf_expand(prk, info, out)`. HMAC follows the two-pass `H((K' ⊕ opad) ‖ H((K' ⊕ ipad) ‖ msg))` construction with `K'` derived per RFC 2104 §2 (zero-pad if `len(K) ≤ 64`, else hash-then-zero-pad). HKDF-Extract routes through HMAC-SHA256 with the RFC 5869 §2.2 empty-salt equivalence (empty salt ≡ 32 zero bytes) handled explicitly. HKDF-Expand iterates `T(i) = HMAC(PRK, T(i-1) ‖ info ‖ [i])` and enforces the RFC 5869 §2.3 `L ≤ 255 * HashLen` upper bound with a typed `HkdfExpandError::OutputTooLong` error. Zero-dep, built on the `Sha256Ctx` from #1338. Unblocks TLS 1.3's key schedule in `libpdx-net` M3. **New tests:** RFC 4231 HMAC-SHA-256 vectors §4.2 (case 1), §4.3 (case 2), §4.4 (case 3), §4.5 (case 4), §4.7 (case 6 — long-key hashed-key path), §4.8 (case 7 — long-key + long-data); RFC 5869 HKDF-SHA-256 vectors §A.1, §A.2 (82-byte OKM, all fields non-empty), §A.3 (empty salt/info); boundary tests for the `L = 255*32` max-length success and `L = 255*32 + 1` rejection; RFC 5869 §2.2 equivalence pin (empty salt vs 32-zero salt).
- **Version:** `workspace.version` bumped 0.25.0 → 0.26.0 (minor: additive crypto surface, no source-level breakage). `find-paideia-as.sh` `MIN_VERSION` unchanged.

## v0.25.0

### Crypto

- **Issue #1338 (R100-PREP-005a) — SHA-256 typed intrinsic (FIPS 180-4 §6.2).** New `paideia-as-crypto::hash::sha256` module: one-shot [`sha256`](crates/paideia-as-crypto/src/hash/sha256.rs) and streaming [`Sha256Ctx`] (`new` / `update` / `finalize`) built from a portable u32 message schedule + 64-round compression, entirely branch-free on secret data. The round constants K (FIPS 180-4 §4.2.2) and initial hash H(0) (§5.3.3) are compile-time constants; padding follows §5.1.1 (append 0x80, zero-pad, 64-bit big-endian bit length). No external dependency, matching the crate's zero-dep posture (ChaCha20-Poly1305 in `aead::` and Argon2id in `kdf::` follow the same pattern). Foundational primitive — unblocks HMAC-SHA256 / HKDF (#1339) and Ed25519 (#1341, which additionally needs SHA-512). **Deferred:** SHA-NI hardware acceleration lands as a follow-up once we're on real hardware; the portable u32 core stays as the fallback and the trait shape does not change. **New tests:** FIPS 180-4 §D.1 "abc", §D.2 448-bit two-block, §D.3 one-million-'a's (streamed with an odd 997-byte chunk to prove non-block-aligned boundaries), empty-input pin, streaming-equivalence across chunk sizes 1/32/55..65/128/199/200, padding-boundary distinctness across 55/56/57/63/64/65-byte inputs, and the widely-cited "quick brown fox" pair (avalanche smoke).
- **Version:** `workspace.version` bumped 0.24.1 → 0.25.0 (minor: additive crypto surface, no source-level breakage). `find-paideia-as.sh` `MIN_VERSION` unchanged — no consumer has a hard dependency on this intrinsic yet.

## v0.24.1

### Lexer / Parser / AST

- **Issue #1337 — regular string literals accept the full `\xNN` (0x00..=0xFF) byte range.** The lexer's `scan_string` already treated `\xFF` as a valid escape, but the downstream extractor (`process_string_escape` in `crates/paideia-as-lexer/src/scan_string.rs`) rejected any hex escape with `byte_val > 127` as "hex escape out of ASCII range in string". The parser turned that extractor error into an E0004 diagnostic whose *catalog brief* is "unterminated string literal" — so `pub let case7_rec : [u8; 32] = "PDXARGV\0\xFF\xFF…"` (libpdx-argv `tests/parse_schema_record.pdx:270`) surfaced as an unterminated-string error even though the source was well-formed. **What lands:** `extract_string_content` and its helpers (`extract_regular_string_content`, `extract_raw_string_content`, `process_string_escape_into`) now return `Vec<u8>` and preserve every `\xNN` byte literally instead of routing through a `char`. `ExprData::StringLiteral(String)` becomes `ExprData::StringLiteral(Vec<u8>)` so byte-array (`[u8; N]`) initializers written as regular string literals round-trip byte-exactly (previously a high-byte `\xNN` would have been re-encoded as a two-byte UTF-8 sequence — wrong for byte arrays). Callers that need valid UTF-8 (the `@guid`/`@include_str`/`@include_bytes` embed parsers in `parse_primary/embed.rs`) now decode via `String::from_utf8` and emit their existing P0278/P0279 diagnostic on non-UTF-8 payloads. The emit path (`cmd_build.rs`'s AST→IR `literal_bytes` walk) drops the redundant `s.as_bytes().to_vec()` in favour of `bytes.clone()`. Unblocks libpdx-argv ENH-002 regression tests (case7 — `flag_count = 0xFFFFFFFFFFFFFFFF` bit-63-set schema record). **New tests:** `string_high_byte_hex_escape_xff`, `byte_string_full_xff_range`, `extract_byte_string_full_xff_range` in `scan_string::tests` — pin the lexer regression and confirm the extractor now round-trips `\x00..\xFF` cleanly for both regular and byte strings.
- **Version:** `workspace.version` bumped 0.24.0 → 0.24.1 (patch: bug fix, no source-level breakage). `find-paideia-as.sh` `MIN_VERSION` unchanged.

## v0.24.0

### Encoder / Elaborator / IR

- **Issue #1333 (R89-XREPO.PAS-001) — scalar f32/f64 arithmetic codegen substrate (register-register only).** `Type::Float(u16)` was already a first-class `paideia-as-types` variant with real `layout.rs`/`unify.rs` support (not merely a display-string token as an earlier scoping-doc grep suggested — that grep only found the two diagnostic-message call sites in `check_expr.rs`/`check_fn_ptr_sig.rs`); the actual gap was purely in the encoder, which had zero scalar-SSE mnemonics. **What lands:** a new `RegId` band 53–68 for XMM0–XMM15 (`crates/paideia-as-runtime/src/instruction.rs`), kept disjoint from the pre-existing YMM band (37–52, #1004) because XMM selects the legacy-SSE non-VEX encoder path rather than VEX; 20 new `Mnemonic` variants — `MovSd`/`MovSs`, `AddSd`/`AddSs`, `SubSd`/`SubSs`, `MulSd`/`MulSs`, `DivSd`/`DivSs`, `Sqrtsd`/`Sqrtss`, `Ucomisd`/`Ucomiss`, `Comisd`/`Comiss`, `Cvtsi2sd`/`Cvtsi2ss`, `Cvttsd2si`/`Cvttss2si`, `MovdBitcast{to_xmm}`/`MovqBitcast{to_xmm}` — encoded in a new `crates/paideia-as-encoder/src/encode_sse.rs` (mirrors the per-family-file convention `encode_vex.rs` established for AVX2), wired into `encode_instruction.rs`'s dispatch and both of `Mnemonic`'s exhaustive `arity()`/`estimated_size()` matches. `MNEMONIC_TABLE` (`unsafe_walker.rs`) gets the string spellings — `movd`/`movq` get `_ld`/`_st` suffixes (mirroring the `vmovdqu_ld`/`vmovdqu_st` precedent) since bitcast direction can't be inferred from the mnemonic alone — and `unsafe_walker/register.rs` gets `xmm0`–`xmm15` name→`RegId` parsing. ABI: `paideia-as-ir::abi` gains `ArgClass::Float`, `XMM_ARG_REGS` (XMM0–XMM7), `XMM_RET`, and `map_args`/`map_return` now track independent Integer/Float register counters per SysV AMD64 ABI §3.2.3 (a mixed `f(i64, f64, i64)` call correctly maps to `RDI, XMM0, RSI`, not `RDI, XMM0, RDX`); MS x64 gets the simpler unified-index model as a documented follow-up. Effect/capability: no new effect introduced — raw-asm SIMD mnemonics (`vpxor`, `vmovdqu`, #1004) already ship under the same generic `unsafe`-block gating as GP-register mnemonics, with no dedicated per-instruction-family effect; inventing one for SSE would break that convention. **Deferred (see issue #1333 for follow-up):** memory-operand scalar-float forms (`movsd xmm, [mem]` spill/reload — every mnemonic here is register-register only); `maxss`/`minss`/`roundss` family (explicitly optional in the issue); the full compiler-backend wiring — parser/typer float-literal inference, an xmm register-allocator pool, and `emit_walker.rs`/`lower.rs` branching on `Type::Float` to pick these mnemonics for `let x: f64 = ...; y = x * 2.0` source. That last piece is the actual remaining gap for source-level float arithmetic; this release ships the encoder + ABI substrate it will lower onto. IR-level note: this compiler already models arithmetic generically via operator-string dispatch (`operators.rs`'s `KNOWN_OPERATORS`/`BINARY_OPERATORS`) plus the operand's `Type`, not per-op-per-type `IrKind` variants — adding a parallel `FloatBinOp`/`FloatCmp` IR enum as originally scoped would have duplicated that mechanism rather than extended it, so no new IR node kind was added; the missing piece is the `Type::Float` branch in codegen, tracked under the deferred backend-wiring item above. **New tests:** 24 in `crates/paideia-as-encoder/tests/simd/scalar_float.rs` (byte-exact for every mnemonic + operand-shape rejection + 2 iced-x86 round-trips), 3 `xmm_id_from_regid`/`encode_xmm_xmm`/`encode_cvtsi2s` unit tests in `encode_sse.rs`, 6 `resolve_mnemonic_*`/register-band tests in `unsafe_walker.rs`, 7 `ArgClass::Float` ABI-mapping tests in `abi.rs`.
- **Issue #1334 (R89-XREPO.PAS-002) — version/CHANGELOG discipline for the scalar-float encoder substrate.** This entry; `workspace.version` bumped 0.23.0 → 0.24.0 (minor: additive encoder/ABI surface, no source-level breakage — no existing `.pdx` source references XMM registers or these mnemonics). `find-paideia-as.sh` `MIN_VERSION` is unchanged.

## v0.23.0

### Elaborator / Crypto

- **Issue #1330 (R64v2.PAS-001) — `mldsa65_sign` typed intrinsic for ML-DSA-65 in-`.pdx` signing.** New `MlDsa65::sign(seed_ptr, msg_ptr, msg_len, sig_out_ptr) -> i64` trait call (`crates/paideia-stdlib/pdx/mldsa.pdx`) routes through a new `stdlib_lowering::mldsaops` recipe (mirrors #1305's `cryptoops`) to an extern-C thunk, `mldsa65_sign_runtime_entry`, in a new `paideia-pq-sign::ffi` module. Calling convention: caller allocates a 3309-byte (`MLDSA65_SIG_LEN`) output buffer (RCX) and the thunk writes the encoded signature into it, returning a status code in RAX — chosen over an sret record-return convention since the encoder does not yet support one (same rationale as `cpuidops`'s deferred `cpuid_leaf` record). `paideia-pq-sign`'s crate-level `#![forbid(unsafe_code)]` is downgraded to `#![deny(unsafe_code)]` so the new `ffi` module (the only module touching raw pointers) can locally opt back in, mirroring `paideia-as-crypto::ffi`. Effect/capability row `!{crypto, mem} @{paideia.crypto}` reuses the capability `Argon2id`/`ChaCha20Poly1305` already declare. **New tests:** 2 recipe-shape unit tests in `mldsaops::tests`; 5 FFI-level tests in `paideia_pq_sign::ffi::tests` (sign→verify round-trip, cross-check against the direct `Signer::sign` call, null-seed / null-output rejection, empty-message-with-null-pointer handling).
- **Issue #1331 (R64v2.PAS-002) — version/CHANGELOG discipline for the `mldsa65_sign` intrinsic.** This entry; `workspace.version` bumped 0.22.0 → 0.23.0 (minor: additive stdlib intrinsic, no source-level breakage). `find-paideia-as.sh` `MIN_VERSION` is unchanged — no consumer has a hard dependency on this intrinsic yet (libpdx-volume#7 is a future consumer; that repo is skeleton-only).

## v0.22.0

### Elaborator / Parser / Encoder

- **Issue #1326 (paideia-os R51.M2 `nvme_ns_dual_kind_mint` unblock)** — SysV lambdas may now take more than 6 arguments. **Why:** paideia-as capped lambda arity at 6 primitive `u64` params (P0276 at the parser, T0521 at SysV call sites) even though neither the AST (`ExprData::Lambda.params: Vec<NodeId>`) nor the type/effect system encoded any such bound — the cap was a codegen gap, not a language-surface restriction. It blocked paideia-os R51.M2's `nvme_ns_dual_kind_mint` (9 args), forcing a struct-pointer-packing workaround (`cb6291e`) that this release lets paideia-os revert as a follow-up. Design record: `design/compiler/lambda-arity-stack-spill.md`. **What lands, across six phases:**
  1. **Parser** (`b631012`) — deletes the `params.len() > 6` guard in `parse_lambda_fn`; params of any count now flow to codegen. P0276 catalog entry rewritten to "reserved" wording rather than retired, so old diagnostic references still resolve.
  2. **Caller-side stack-spill** (`d71b723`) — `emit_call.rs` mirrors the MS x64 #1277 caller pattern for SysV: args 6+ marshal to `[rsp + 8*(i-6)]` ahead of `CALL` (Literal / Var / EnumCons / module-level `Object`-const dispatch), with `sysv_stack_arg_bytes + sysv_stack_arg_pad` composing additively with the pre-existing `#1195` bridge-parity pad to keep every `sub rsp` / `add rsp` pair 16-byte-aligned at the `CALL`. The old T0521 "max 6 arguments" rejection is gone.
  3. **Callee-side stack intake** (`887b95d`) — new `BindingHome::StackSlot(i32)` (`local_binding_table.rs`) and `resolve_var_operands` arm lower a stack-passed param to `[rbp + 16 + 8*(i-6)]`, anchored on the frame-pointer prologue every non-`@no_frame`, non-unsafe-bodied lambda already emits. New diagnostic **B1708** (error): a lambda whose body never emits that prologue — `@no_frame` *or* an `unsafe { ... }` body — cannot accept more than 6 params, since there is no `rbp` to read them back through.
  4. **Encoder verification** (`967024a`) — a 23-arg (17 stack-arg) golden-byte + iced-x86 round-trip fixture confirms the disp8→disp32 boundary at `[rsp/rbp + 128]` encodes correctly; no encoder production changes needed (SIB-relative `mov` already handles arbitrary displacement).
  5. **Scratch-materialisation coverage** (`39b3e93`) — regression tests for module-level `Object`-const (RIP-relative load) and nullary `EnumCons` (variant-index immediate) arguments at stack positions 7+, plus a paideia→`@abi("sysv")` bridge fixture proving `sysv_stack_arg_bytes+pad` composes correctly with the bridge-parity pad.
  6. **Test corpus + docs + version (this release)** — cross-module registration fixture (`sysv_x64_7arg_cross_module_call`, two independently-built modules) proving arg count plays no role in symbol identity; an unsafe-bodied (as opposed to `@no_frame`) B1708 regression at the 7-arg boundary (`sysv_x64_7arg_unsafe_body_b1708`); and a smoke fixture using the real `nvme_ns_dual_kind_mint` field names (`ns_slot, blk_slot, parent_slot, ns_rights, blk_rights, nsid, lba_size, block_count, parent_ctrl_row`) rather than placeholder `a..i` (`sysv_x64_nvme_ns_dual_kind_mint_shape`). `design/toolchain/calling-convention.md` §2.3.1 cross-links the new design doc from the System V bridge section. SARIF catalog snapshot regenerated for the version bump (mechanical, same pattern as the v0.21.0 bump).
  **The ≤6-arg fast path is byte-identical:** every new code path is gated on `arg_count > 6` (caller) / `idx >= 6` (callee); `sysv_stack_arg_count == 0` collapses `sysv_bump` back to exactly what v0.21.0 emitted.
  **Verified:** 6 new tests this phase (440 total across the `#1326` corpus, phases 2–6); `paideia-as` `build_emit` suite 426 passed / 12 failed (12 pre-existing, identical set confirmed via `git stash` against `39b3e93`); `paideia-as-elaborator` lib 984 passed / 1 failed (pre-existing, same test as phases 3–5); `paideia-as-elaborator`'s `codegen` integration test binary fails to *compile* independently of this work (`emit_call_sysvregs.rs` missing the `extern_target` field added by #1305 in v0.21.0 — confirmed pre-existing via `git stash`, out of scope for #1326).
  **Version bumped 0.21.0 → 0.22.0** (minor): additive language capability, no source-level breakage, no ABI change to ≤6-param lambdas.
  **Follow-up (paideia-os, not part of this release):** revert `nvme_ns_dual_kind_mint` to its original 9-arg signature and audit for other struct-pointer-packing workarounds of the arity cap.

## v0.21.0

### Elaborator

- **Issue #1305 (v0.33-004; unblocks paideia-os R48 user management)** — `stdlib_lowering` recipes for `Argon2id::derive` and `ChaCha20Poly1305::seal` / `open`. **Why:** the traits landed in `paideia-as-crypto` at #1302 (Argon2id, RFC 9106) and #1303 (ChaCha20-Poly1305, RFC 8439) but nothing routed `.pdx` call sites through them; `design/user/model.md` §2.1 passphrase unlock and §9.2 sealed-`user_sk` cannot begin until a `.pdx` caller can reach these primitives. **What lands:** three orthogonal pieces. (1) `LoweringRecipe` gains an `extern_target: Option<String>` field (`crates/paideia-as-elaborator/src/stdlib_lowering/mod.rs`); when `Some(sym)`, `emit_call_args_and_call` splices the (usually empty) recipe body, rewrites the CALL destination to `sym`, and falls through to the normal CALL / scratch-pop / postlude path so caller-save preservation is unchanged. Every pre-existing recipe gains `extern_target: None`; a new unit test (`preexisting_sysvregs_recipes_have_no_extern_target`) pins that invariant across `MsrOps::rdmsr`, `CpuidOps::cpuid_leaf_ad` and `ChecksumOps::ipv4_checksum` — a future refactor that flips one to `Some(_)` would silently emit an unresolved CALL, and the pin catches it. (2) A new family submodule `stdlib_lowering::cryptoops` dispatches `Argon2id::derive` → `paideia_crypto_argon2id_derive`, `ChaCha20Poly1305::seal` → `paideia_crypto_chacha20_poly1305_seal`, `ChaCha20Poly1305::open` → `paideia_crypto_chacha20_poly1305_open` — trait names on the source side match the Rust type names so a reader tracing a call from `.pdx` to Rust never has to memorise a translation table. (3) A new C-ABI shim module `paideia-as-crypto::ffi` exposes those three symbols under `#[unsafe(no_mangle)] pub unsafe extern "C" fn` with `#[repr(C)]` parameter bundles (`Argon2idParamsC`, `AeadParamsC`) — flattened from the trait's GAT-lifetime `Params<'a>` to plain pointer + length pairs so `.pdx` code can populate them by hand. Return-code contract (0 OK, -1 InvalidParam, -2 InvalidLength, -3 AuthenticationFailed, -4 Primitive, -5 BufferTooSmall) is shared across all three thunks. **`#[deny(unsafe_code)]` scope preserved:** the crate-level deny stays; `ffi/mod.rs` opts in via `#![allow(unsafe_code)]` at the module level (the only module that touches raw pointers). **New tests:** 5 recipe-shape assertions in `stdlib_lowering::tests` (`argon2id_derive_recipe_targets_ffi_thunk`, `chacha20_poly1305_seal_recipe_targets_ffi_thunk`, `chacha20_poly1305_open_recipe_targets_ffi_thunk`, `unknown_argon2id_method_returns_none`, `unknown_chacha20_poly1305_method_returns_none`); the existing-recipe pin above; 7 FFI-level vector tests in `paideia_as_crypto::ffi::tests` that re-run the RFC 9106 §5.3 Argon2id vector and the RFC 8439 §2.8.2 ChaCha20-Poly1305 vector through the extern thunks (byte-exact against the same tag / ciphertext constants the trait tests pin) plus AEAD tag-mismatch, undersized-output and undersized-sealed error-code checks. **Not in this landing:** a `.pdx` end-to-end fixture linking against `paideia-as-crypto` at build time — the CLI linker path for extern-C rlibs is R49+ tooling work, and the FFI vector tests already prove the C thunks reproduce the RFC bytes. **New `.pdx` file:** `crates/paideia-stdlib/pdx/crypto.pdx` declares the two source-visible traits so consumers have a canonical spelling and register / return-code contract.

- **Issue #1311 (paideia-os R30.M2 #1054/#1055 unblock)** — `not r64` is reachable from `.pdx` source. **Why:** the same shape as #1295 — every layer below the source-level name already existed. `Mnemonic::Not` is defined at `crates/paideia-as-runtime/src/instruction.rs:201` and documented as emitting `not r64` (REX.W F7 /2); `encode_not` lives at `crates/paideia-as-encoder/src/encode_instruction.rs:727`; the dispatch arm `Mnemonic::Not => encode_not(inst, buf)` is at line 373; and two encoder unit tests (`encode_not_rax_emits_48_f7_d0`, `encode_not_rax_round_trips_through_iced_x86`) assert the bytes. What did not exist was the `("not", Mnemonic::Not)` row in `MNEMONIC_TABLE` (`crates/paideia-as-elaborator/src/unsafe_walker.rs`), so the elaborator rejected the instruction with U1605 "unknown mnemonic: not" and a fully-encoded instruction was unwritable. The adjacent `("bswap", Mnemonic::Bswap)` row is present, which is what makes this an omission rather than a decision — `Not` and `Bswap` sit next to each other in the enum. **Surfaced by:** paideia-os R30.M2 (#1054/#1055), the AML evaluator's arithmetic operators. ACPI 6.5 §19.6 defines `Not`, `Nand` and `Nor`, all three a one's complement of a 64-bit value; the workarounds are an all-ones mask register plus `xor` (an extra register and an extra instruction on the hot path of three operators) or `neg` + `sub 1`, and `Mnemonic::Neg` does not exist either. **What lands:** the one table row. No encoder, IR or schema change. **New test:** `crates/paideia-as/tests/milestone/pa_r30_1311_not_r64.rs` builds `tests/data/pa_r30_1311_not_r64.pdx` through the real CLI and asserts the emitted `.text` contains `48 F7 D0` (`not rax`), `48 F7 D1` (`not rcx` — different r/m, so a mis-set `/2` opcode extension cannot hide behind the rax case) and `49 F7 D4` (`not r12` — the REX.B path). The test deliberately starts from **source**, not from a hand-built `Instruction`: the pre-existing encoder tests did the latter and passed throughout the entire period the bug existed, which is exactly the coverage gap that let a table row go missing. **Verified:** workspace regression clean — 5405 passed, and the 4 failures (`paideia_os_m3_four_file_text_byte_snapshot` plus three `typed_encoder_diagnostics` cases) are all present on `c036289` with this change stashed.

- **Issue #1308 (p0 — silent memory corruption in shipping paideia-os code)** — `[v; N]` repeat-array literals now allocate `N * sizeof(elem)` bytes. **Why:** `crates/paideia-as-elaborator/src/lower/array_repeat.rs::extract_repeat_count` was a stub that unconditionally returned `None`, so `expand_array_repeat` took its "count is not a literal" fallback and emitted a **single** element child for every `[v; N]`. The declared type `[T; N]` was still honoured by the type system, so the front end reported the full array while the storage allocator reserved one element, with no diagnostic anywhere on the path — the promised P0211 was referenced in three comments and emitted by nobody. Two symbols in the shipping paideia-os kernel image were affected: `runqueue : [u64; 2]` (`src/kernel/core/sched/runqueue.pdx`) and `_loader_seed_empty_sidecar : [u64; 2]` (`src/kernel/core/loader/seed_caps.pdx`), both linking at 8 bytes instead of 16 per `readelf -sW`. Every write to index 1 landed in whichever symbol the linker placed next; that this had not yet crashed was luck, not safety. **What lands:** `extract_repeat_count` resolves the count by reading the literal's source text back out of the `SourceMap` — integer literals are stored in the AST as bare `Placeholder` nodes carrying only a span, so the value is not otherwise recoverable during lowering (same technique as `unsafe_walker::immediate::extract_integer_from_span` and `cmd_build::layout`). Accepts decimal / `0x` / `0o` / `0b`, `_` digit separators and integer type suffixes. `expand_array_repeat` gains `source_map`, `sink` and `span` parameters and now returns `count` copies of the element node; the second lowering pass maps each occurrence through `ast_to_ir` to the same IR node, which is exactly repeat-literal semantics (one shared element value, N slots). **Failure is loud, not lossy:** a non-constant count, a zero count, or a count above `MAX_REPEAT_COUNT` (1 << 20 elements — an OOM guard, since expansion materialises N structural children) emits the new **P0211** and returns *no* children, so the data pass cannot emit an under-allocated symbol and the build fails. Returning empty rather than one element is deliberate: a one-element fallback is the exact defect being removed. **New diagnostic:** `P0211` "Array repeat count must be a constant" in `crates/paideia-as-diagnostics/catalog.toml`. **New tests:** 6 unit tests in `array_repeat::tests` (base/separator/suffix parsing, N=512 expansion, N=2 not special-cased, and P0211 for non-constant / zero / oversized counts); 2 existing `lower::tests` rewritten from the old buggy contract — `lower_array_repeat_with_non_literal_count` now asserts 0 children + P0211, `lower_array_repeat_nested_structs` now uses a real literal count and asserts the `RecordCons` element is replicated 3×; 1 new `lower_array_repeat_with_literal_count_expands_to_n_children`. Byte-exact integration tests in `crates/paideia-as/tests/build_emit/array_storage_arity.rs`: `[u64; 2] = [0; 2]` → symbol size 16 with both qwords zero; `[u64; 512] = [0; 512]` → 4096 zero bytes (catches small-N special-casing); `[u32; 4] = [7; 4]` → 16 bytes of `07 00 00 00` (proves the repeated *value* and the per-element width are both honoured, not just the count).

### Build / data emission

- **Issue #1309 (p0 — second, distinct silent under-allocation path)** — module-level array-literal storage is now reconciled against the declared `[T; N]` arity. **Why:** found while fixing #1308, and the root cause of the `_frame_meta` anomaly flagged there as "separate but adjacent". The ArrayLit branch of the data pass in `crates/paideia-as/src/cmd_build.rs` packs one element per child that is an `IrKind::Literal` with a resolvable value, then emits whatever it accumulated — it never compared that against the declared arity. Two silent failure modes fell out: (1) a short initialiser list, e.g. paideia-os `_frame_meta : [u64; 1024]` whose literal was written with 992 elements (62 rows of 16 instead of 64) and linked at 7936 bytes, leaving indices 992..1023 aliasing the next symbol; (2) an element that is not an encodable constant — a named binding, an arithmetic expression, a negative literal (which parses as `ExprPrefix`, not `ExprLiteral`) — falling through both `if`s with no byte emitted and no diagnostic, so `[u64; 4] = [1, 2, k, 4]` produced a 24-byte symbol. **What lands:** after packing, the emitted element count is compared against the declared arity (`declared_array_len_from_type`, already present for the StringLiteral path) or, absent an annotation, against the number of elements actually written. A mismatch emits the new **T0576** and the symbol is *not* pushed into `data_entries`, so any build that survives this pass has byte-exact array storage. The two messages distinguish the cases: "N elements but the declared type is [_; M]" vs "N elements but only K could be encoded as constants". **New diagnostic:** `T0576` "Array initialiser arity mismatch". **New tests:** `short_array_initialiser_is_rejected_with_t0576` (`[u64; 4] = [1, 2, 3]`) and `unencodable_array_element_is_rejected_with_t0576` (`[u64; 4] = [1, 2, k, 4]`) in `crates/paideia-as/tests/build_emit/array_storage_arity.rs`. The correct-arity cases in the same file pin that a well-formed `[u64; N]` still emits `N * 8` bytes unchanged. **Scope:** the guard requires that this branch actually claimed the array — it fires only when at least one element encoded. An array in which *nothing* encodes is not a partially-emitted symbol but a shape this branch does not own: paideia-os `_klog_files : [u64; 205] = [(rip + name_file_0), ...]` is an array of symbol-address relocations, produces no data entry, and is absent from the linked kernel image entirely. That gap is real but materially different — a reference to a missing symbol is a loud undefined-symbol link error, not a silent short read — and is tracked separately as **#1310** rather than reported here as an arity mismatch, which would be a misleading message for an unimplemented element shape. **Cross-repo:** paideia-os `_frame_meta` is a genuine source error — the literal really is 32 elements short — and is repaired on the paideia-os side against this compiler. **Design doc:** `design/toolchain/array-storage-allocation.md` records the invariant, both allocation paths, and the rationale for each choice (empty-not-single fallback, `MAX_REPEAT_COUNT`, reject-not-pad, and the #1310 scope boundary).

### Diagnostics

- **SARIF catalog snapshot refreshed.** `integration__sarif__snapshot_multi_diagnostic` enumerates the full diagnostic catalog including the tool's `version` / `semanticVersion` fields, and was left stale at `0.20.1` by the v0.21.0 version bump in c768935 — it was already failing on `origin/main` before this landing, independently of the array work (verified by `git stash`). Regenerated so it carries the correct `0.21.0` version alongside the new `P0211` and `T0576` entries; without the refresh the two new catalog entries would have had no test coverage at all. Net effect on the suite is one *fewer* failure, not one more.

- **Issue #1307 (paideia-os R29.M2-002 #1024 unblock)** — Effect-row → cap-set coupling walker. **Why:** `next-wave-softarch.md` §3 R29 structural witness: every driver process' effect row is checked at link time; a driver claiming `!{mmio_read}` in its effect row without holding `MmioMemCap` (spelled `paideia.mmio` in source) must fail elaboration. Prior to this landing `paideia-as build` silently accepted `let f : (u32) -> u64 !{Mmio} @{} = ...` with no diagnostic — the C1300 cap-inference path exists but is driven by injection tables populated only by tests, never by AST-level function signatures. **What lands:** three components. (1) `crates/paideia-as-effects/src/effect_cap_binding.rs` — a static registry `BINDINGS: &[EffectCapBinding]` mapping root effect names to the built-in cap they require: `RawMem` → `paideia.raw_mem`; `Mmio` / `MmioRead` / `MmioWrite` / `mmio_read` / `mmio_write` → `paideia.mmio`. Non-root effects (`Io`, `Net`, …) are intentionally absent — those are user-handled via `with handle E` blocks, so the coupling constraint applies at the handler-installation site rather than at every caller. Exposes `required_cap_for_effect(name) -> Option<&'static str>` and `effects_requiring_cap(cap) -> impl Iterator`. (2) `crates/paideia-as-elaborator/src/effect_cap_coupling.rs` — an AST-level walker (`check_effect_cap_coupling`) that recurses `Module → Structure → Let`, extracts the effect-row + cap-set idents of every function-shaped type annotation, and for each effect with a required cap verifies the cap is present in `@{...}`. Emits `C1301` per (function, missing cap) pair, deduplicated. Dotted cap paths (`paideia.mmio`) are recovered from the source text of the `@{...}` span rather than the parser's split-by-dot Ident nodes — this avoids extending the AST for the R29 landing. (3) `crates/paideia-as-elaborator/src/cap_infer.rs` gains `pub const C_EFFECT_REQUIRES_CAP: u16 = 1301` alongside the existing `C_MISSING_CAP: u16 = 1300`. **Wired into cmd_build:** `crates/paideia-as/src/cmd_build.rs` invokes the walker after `parse_source_file`, gated on `!parse_errored && !lex_errored` (same gate as `validate_file_module_mapping` — a broken tree yields cascade noise). Failing diagnostics push into the main sink; `paideia-as build` exits 1 as with any error diagnostic. **New tests:** 8 in `effect_cap_binding::tests` (mapping correctness, unique-keys invariant, non-root effect exclusion); 9 in `effect_cap_coupling::tests` (accept: matching cap, empty row, unknown effect silent, over-declaration; reject: `Mmio`+`@{}`, `mmio_read`+`@{}`, `RawMem`+`@{}`, dedup of two-effect same-cap case); 2 in `tests/effects-corpus/tests/runner.rs` (`effect_cap_coupling_reject_fixture_emits_c1301`, `effect_cap_coupling_accept_fixture_emits_no_codes`) that exercise the CLI end-to-end via `paideia-as build --emit placeholder`. Corpus fixtures land at `tests/effects-corpus/corpus/reject/r_mmio_effect_without_cap.pdx` (+ `.expect` naming C1301) and `tests/effects-corpus/corpus/accept/effect_cap_pair_satisfied.pdx`. The effects-corpus stderr parser (`parse_codes_from_stderr`) now recognizes `C` prefix in addition to `F` / `T`. **Verified:** paideia-as-elaborator lib 958/958 green; paideia-as-effects 2 + 8 green; paideia-effects-corpus 3/3 green (with one pre-existing ignored aspirational reject-corpus test); full workspace regression clean modulo the 1 pre-existing `paideia_os_m3_four_file_text_byte_snapshot` failure noted in prior CHANGELOG entries (paideia-os kernel evolved beyond the snapshot; unrelated to this change). All 8 example fixtures under `examples/` that use `!{RawMem} @{paideia.raw_mem}` continue to elaborate without spurious C1301. **Cross-repo:** paideia-os#1024 (R29.M2-002 elaborator witness) consumes this at the same HEAD via a submodule bump. **Version bumped 0.20.1 → 0.21.0** — this is the first source-driven cap-enforcement landing (structural, not injection-table-driven), so the minor bump reflects the new externally-visible surface.

## v0.20.1

### Encoder

- **Issue #1295-b (paideia-os R21.M2 #832 unblock)** — RIP-relative memory operands for `vmovdqu` (both load and store forms). **Why:** the v0.18 #1004 encoder wired `Operand::MemSib { base, index: None, scale: X1, disp }` for the base-register-plus-disp memory form (e.g. `vmovdqu ymm0, [rax + 8]`), but every real paideia-os SIMD consumer references .rodata / .bss patterns via labels — the `[rip + label]` idiom lowers to `Operand::MemRipRelSym { name, addend }`, which the encoder rejected with `EncodeError::OperandShape`. Surfaced immediately by paideia-os#832's YMM-preservation fixture (`vmovdqu_ld ymm0, [rip + _ymm_pattern_a]` — the natural spelling for seeding YMM0 from a .rodata pattern). **What lands:** two new helpers in `crates/paideia-as-encoder/src/encode_vex.rs` — `encode_vmovdqu_ymm_riprel(buf, dst_id, disp) -> usize` and `encode_vmovdqu_riprel_ymm(buf, disp, src_id) -> usize` — each emits the VEX (2- or 3-byte, chosen by dst/src high-bit), opcode 6F / 7F, ModR/M `0x05 | ((ymm_id & 7) << 3)` (mod=00, rm=101 for RIP-rel), and disp32 placeholder; returns the disp32's absolute byte offset for the caller to translate to instruction-local and attach a `RelocKind::PcRel32` reloc. Four new match arms in `encode_vmovdqu` (`crates/paideia-as-encoder/src/encode_instruction.rs`) dispatch on `MemRipRelSym` (with symbol → PcRel32 reloc) and `MemRipRel` (compile-time disp → direct write) for both load and store forms. Follows the byte_offset instruction-local convention already established by the mov/lea RIP-rel arms (#1143) — capture `start = buf.bytes.len()` before the encoder call, compute `disp_abs - start` for `RelocSite.byte_offset`. **Verified:** paideia-os#832 fixture (`tests/kernel/cpu/ymm_preserve.pdx`) compiles, links, boots under `qemu -cpu max` and emits both `YMM PRESERVE A OK` / `YMM PRESERVE B OK` fingerprints — round-trips YMM0 through xsave_save_for/xsave_restore_for correctly with distinct pids (60, 61) and distinct patterns (0xAA / 0xBB). Objdump confirms byte-exact encodings: `c5 fe 6f 05 <PC32-reloc>` (2-byte VEX load) and `c5 fe 7f 05 <PC32-reloc>` (2-byte VEX store). No regression to workspace tests beyond the pre-existing `paideia_os_m3_four_file_text_byte_snapshot` failure.

### Elaborator

- **Issue #1295 (paideia-os R21.M2 #832 unblock)** — Parser wiring for AVX2 mnemonics. **Why:** #1004 (v0.18) landed the encoder + IR primitives for `vmovdqu` / `vpxor` / `vpcmpeqb` / `vpmovmskb` byte-exact against Intel SDM Vol 2B VEX.256 encodings, but the string-to-Mnemonic table in `crates/paideia-as-elaborator/src/unsafe_walker.rs::MNEMONIC_TABLE` never gained the corresponding source spellings. Consequence: any `.pdx` `unsafe { vmovdqu ymm0, [rip + src] }` failed at elaboration with U1605 "unknown mnemonic" — the encoder shipped bytes that no source file could reach. Surfaced by paideia-os R21.M2 #832 planning (YMM-preservation regression fixture: two tasks pinned to same CPU each seed YMM0 with a distinct pattern, get preempted by the LAPIC timer, and re-check YMM0 to prove sched_switch's XSAVE/XRSTOR round-trip preserved the YMM subset — needs `vmovdqu_ld ymm0, [rip + pattern]` and `vmovdqu_st [rip + verify], ymm0`). **What lands:** 5 MNEMONIC_TABLE entries. The `Mnemonic::Vmovdqu` variant carries an `is_store: bool` field that can't be inferred from the mnemonic string, so two spellings map to the two variants; the three three-operand VEX-encoded arithmetic ops each get one spelling. `("vmovdqu_ld", Mnemonic::Vmovdqu { is_store: false })` — ymm dst from ymm src or [mem] src (VEX.256 F3 0F 6F /r); `("vmovdqu_st", Mnemonic::Vmovdqu { is_store: true })` — [mem] dst from ymm src (VEX.256 F3 0F 7F /r); `("vpxor", Mnemonic::Vpxor)`; `("vpcmpeqb", Mnemonic::Vpcmpeqb)`; `("vpmovmskb", Mnemonic::Vpmovmskb)`. The `_ld` / `_st` suffix convention parallels the existing `_d`/`_q` width-suffix idiom in the same table. **New tests:** 5 in `unsafe_walker::tests` — `resolve_mnemonic_vmovdqu_ld`, `resolve_mnemonic_vmovdqu_st`, `resolve_mnemonic_vpxor`, `resolve_mnemonic_vpcmpeqb`, `resolve_mnemonic_vpmovmskb`. Encoder byte-exact + iced round-trip tests already exist for every operand shape (they were the v0.18 #1004 delivery); this landing only opens the front-door path from source to those bytes. **Verified:** elaborator unit suite green; workspace regression clean. Cross-repo: paideia-os R21.M2 #832 fixture (`tests/kernel/cpu/ymm_preserve.pdx`) consumes this at the same HEAD via a submodule bump.

### Encoder

- **Issue #1294 (paideia-os R21.M1 #826 unblock)** — Zero-arity `xgetbv` (`0F 01 D0`) and `xsetbv` (`0F 01 D1`) extended-control-register access mnemonics. **Why:** paideia-os R21 XSAVE hinges on programming XCR0 (state-component enable mask) before any XSAVE/XRSTOR variant executes — without XCR0.YMM=1 an `xsaveopt` covering YMM raises `#UD`. XCR0 can only be written via `xsetbv`; no MSR or CR alternative exists. paideia-os#826 also wants an XGETBV-based fixture to observe the configured mask, so both halves land together. **What lands:** `Mnemonic::Xgetbv` + `Mnemonic::Xsetbv` variants in `crates/paideia-as-runtime/src/instruction.rs` (arity 0, `estimated_size` = 3 exact — no REX, fixed 3-byte encoding). Encoder dispatch + `encode_xgetbv_inst` / `encode_xsetbv_inst` functions in `crates/paideia-as-encoder/src/encode_instruction.rs` push `[0x0F, 0x01, 0xD0]` / `[0x0F, 0x01, 0xD1]` respectively; both reject any explicit operand with `EncodeError::OperandCount` (mirrors `wrmsr`/`rdmsr` shape). Scheduling classifier (`crates/paideia-as-ir/src/opt/schedule.rs::classify_instruction`) treats both as `InstructionClass::Other` — privileged system-ISA that must not reorder around surrounding memory ops. String resolver in `crates/paideia-as-elaborator/src/unsafe_walker.rs` gains `("xgetbv", Mnemonic::Xgetbv)` and `("xsetbv", Mnemonic::Xsetbv)` table entries so `unsafe {...}` blocks can call them directly (no typed stdlib wrapper — the same pattern as raw `rdmsr`/`wrmsr` in paideia-os `msr.pdx`). **New tests:** 4 in `crates/paideia-as-encoder/tests/system/supervisor_mode_agnostic.rs` — `xgetbv_mode32_equals_mode64` / `xsetbv_mode32_equals_mode64` (byte-exact + mode-agnostic — mirrors the CLI/STI/HLT/RDMSR/WRMSR shape), `xgetbv_rejects_explicit_operand` / `xsetbv_rejects_explicit_operand` (guard: any operand → `EncodeError`, so a caller's mistake surfaces at encode time rather than mis-encoding subsequent instructions). Plus 2 resolver tests in `unsafe_walker::tests` (`resolve_mnemonic_xgetbv`, `resolve_mnemonic_xsetbv`). **Explicitly not in this landing:** typed `XcrOps { xgetbv(idx) -> u64; xsetbv(idx, val) }` stdlib wrapper — mirrors #1284 (MsrOps) which is also open; the paideia-os R21.M1 consumer uses raw mnemonic in an `unsafe {}` block just as paideia-os `syscall/msr.pdx` does with `rdmsr`/`wrmsr`. **Verified:** encoder integration suite 693/693 passing; elaborator new resolvers 2/2 passing; full workspace regression clean modulo the 1 pre-existing `paideia_os_m3_four_file_text_byte_snapshot` failure noted in prior CHANGELOG entries (paideia-os kernel evolved beyond the snapshot; unrelated to this change).

### Stdlib

- **PA-R18-M2-003 (paideia-os#767)** — `PerCpuOps` trait extended with `read_u64` / `write_u64` / `cmpxchg64` for per-CPU control-block runtime access. **Why:** paideia-os R18.M2 wires the per-CPU CB via `WRMSR IA32_GS_BASE = &_percpu_cbs[cpu_idx]` (paideia-os#766) but had no accessor that returned the u64 payload at a given offset — the existing `percpu_inc` / `percpu_add` methods only cover atomic-counter mutation with a compile-time-literal offset, and the raw `[gs:disp32]` no-base SIB form remains unsupported for plain `mov`/`lea` (the CHANGELOG PA-R13-002 known-gap). **What lands:** three new methods on `crates/paideia-stdlib/pdx/percpu.pdx` with `SysVRegs` lowering recipes at `crates/paideia-as-elaborator/src/stdlib_lowering.rs`:
  - `read_u64(off: u64) -> u64`             → `mov rax, [gs:rdi + 0]`         (RDI = off, RAX = return)
  - `write_u64(off: u64, val: u64) -> ()`   → `mov [gs:rdi + 0], rsi`         (RDI = off, RSI = val)
  - `cmpxchg64(off, expected, new) -> u64`  → `mov rax, rsi; lock cmpxchg [gs:rdi + 0], rdx` (RDI = off, RSI = expected, RDX = new, RAX = observed old value)

  All three use `ArgConvention::SysVRegs`, so callers can pass runtime-computed offsets — the elaborator's `emit_call` marshals args into RDI/RSI/RDX ahead of splicing. Recipes lean on the already-proven `[gs:reg + disp]` encoder path (see `tests/addressing/gs_relative.rs::mov_rax_gs_rax_disp0`); no new encoder work required. The `[Atomic, RawMem]` effect row on `cmpxchg64` matches `percpu_inc/add`; `read_u64` is `[RawMem]` only (plain aligned qword load has no cross-CPU ordering under x86-64 without an `mfence`); `write_u64` is `[RawMem]` only for the same reason. **Not in this landing:** `read_u32`/`write_u32`/`cmpxchg32` and 128-bit `cmpxchg16b` variants — filed for follow-up when a paideia-os consumer needs them. Also not landed: the disp32-only `[gs:absolute]` encoding for `mov`/`lea` — the PA-R13-002 gap; not needed for this ticket since SysVRegs carries the offset in a register. **New tests:** 4 recipe-shape assertions in `crates/paideia-as-elaborator/tests/lowering/stdlib_percpu.rs` (`read_u64_lowers_to_mov_gs_rdi_disp0`, `write_u64_lowers_to_mov_gs_rdi_disp0_rsi`, `cmpxchg64_lowers_to_mov_rax_rsi_then_lock_cmpxchg`, `read_write_cmpxchg_do_not_use_literal_extraction` — the last one pins that SysVRegs recipes must accept non-literal args, guarding against a regression that would defeat the point of R18-M2-003). `percpu_pdx_parses_cleanly` (stdlib parse-roundtrip test) refreshed by the extended trait declaration; passes. Full elaborator suite 942 + 26 + 78 + 40 = 1086 tests passing. Cross-repo: paideia-os#767 consumes this at the same HEAD via a submodule bump.

### Elaborator

- **Issue #1276 phase 3** — Default SysV frame-pointer prologue/epilogue emission for non-`@no_frame` lambdas. Wires the phase-1 `LetInfo::no_frame` plumbing (paideia-as 8a9e935) and the phase-2 paideia-os `@no_frame` annotation pass (paideia-os b368508, 42 sites) into the elaborator's actual byte emission so paideia-os#716 (`klog_walk_rbp` inside `klog_panic`) can walk real rbp chains once phase-4 wires the call site. **What lands:** the walker now emits `push rbp; mov rbp, rsp` at every non-`@no_frame`, non-`Unsafe`-body Lambda entry and `mov rsp, rbp; pop rbp` immediately before the terminal `ret`; `@no_frame` lambdas and Unsafe-bodied lambdas (whose user asm owns its own `ret`) are unchanged. Emission is LAZY — `visit_lambda` arms `EmitPassState::pending_frame_prologue` for the lambda id and the first `emit_inst` call whose `current_function` matches injects the two prologue instructions ahead of the body's first instruction; if the body-shape arm never fires an `emit_inst` (e.g. the App arm's `+`/`<<` imm-out-of-range early-out at `crates/paideia-as-elaborator/src/emit_visit_lambda.rs:811,875`), the pending arm is silently dropped and B1704 (`function_symbol_no_offset`) continues to fire on the empty function symbol exactly as pre-#1276. The lazy path also overrides `lambda_first_instr[L]` to name the `push rbp` id so the ELF symbol starts AT the frame prologue, not after it — otherwise callers would jump into the function past `push rbp`, `mov rsp, rbp` would restore rsp to the caller's frame, and `pop rbp; ret` would corrupt both. `emit_ret` mirrors the flag via `EmitPassState::was_frame_prologue_emitted` — the epilogue emits only when the paired prologue actually fired. **Files:** `crates/paideia-as-elaborator/src/emit_pass_state.rs` gains `lambda_no_frame` / `pending_frame_prologue` / `emitted_frame_prologue` HashSets and `mark_lambda_no_frame` / `arm_pending_frame_prologue` / `take_pending_frame_prologue` / `mark_frame_prologue_emitted` / `is_lambda_no_frame` / `was_frame_prologue_emitted` methods; `crates/paideia-as-elaborator/src/emit_visit_lambda.rs::visit_lambda` arms the pending prologue after registering closure captures (before closure_frame_meta's `sub rsp, N`) and downgrades the closure-frame arm to `arm_pending_first_instr_unless_claimed`; `crates/paideia-as-elaborator/src/emit_walker.rs::emit_inst` injects the prologue lazily; `emit_ret` emits `mov rsp, rbp; pop rbp` before `ret` when `was_frame_prologue_emitted` is true; `walk_inner` gains a pre-pass over every Let→Lambda binding to propagate `LetInfo::no_frame` into `lambda_no_frame` BEFORE the main flat-walker loop visits the Lambda (Lambda ids are smaller than their wrapping Let's, so the in-loop Let-handler mark would arrive too late); the Let handler still redundantly marks for belt-and-suspenders. **Parser:** `crates/paideia-as-parser/src/parse_item/let_item.rs::parse_let_decl_with_visibility` now rejects `@no_frame` on non-`ExprLambda` RHS with P0250, so downstream phases can trust `LetInfo::no_frame == true` implies a Lambda RHS. **New tests:** 5 emit unit tests in `crates/paideia-as-elaborator/src/emit_walker_tests/frame_prologue.rs` (`emit_prologue_by_default`, `no_prologue_with_no_frame`, `epilogue_leave_ret`, `no_epilogue_with_no_frame`, `lambda_first_instr_points_at_prologue`); 1 parser test `no_frame_on_non_fn_binding_errors`. **Baseline refresh:** 26 build_emit / milestone tests + 1 EmitFixture helper (`lambda_with_frame`) updated to expect the new prologue+epilogue bytes; 11 lower-level elaborator unit tests that construct bare Lambdas (no Let wrapper) mark the lambda `@no_frame` via `mark_lambda_no_frame` to preserve their virtual-ID / estimated_offset invariants. **Verified:** full workspace suite 5198 passed / 0 new failures (5 pre-existing: `paideia_os_m3_four_file_text_byte_snapshot`, `typed_encoder_diagnostics` ×3, `accept_corpus_emits_no_s_codes` all confirmed on baseline via `git stash`). **Followup:** phase 4 bumps the paideia-os submodule, wires `klog_walk_rbp` into `klog_panic` per paideia-os#716, and closes out both issue chains.

- **Issue #1276 phase 1** — `@no_frame` symbol-attribute parser + AST/IR plumbing (inert). Unblocks [paideia-os#716](https://github.com/paideia-os/paideia-os/issues/716) (`klog_walk_rbp` wire-up into `klog_panic`), whose enabling landing (Option C — walkable-by-default stack frames with an `@no_frame` opt-out for hand-crafted trampolines/ISRs/syscall entries) is staged across four phases per the parent issue. **This phase:** the parser now recognizes bare-flag `@no_frame` on let bindings (`crates/paideia-as-parser/src/parse_item/let_item.rs::parse_optional_symbol_attributes`), stores it in `LetSymbolAttrs::no_frame`, and threads it into the AST as a new `no_frame: bool` field on `ItemData::Let`. The IR side-table `LetInfo` gains a matching `no_frame: bool` field (`crates/paideia-as-ir/src/let_meta.rs`), and both the elaborator's `populate_let_meta` pass (`crates/paideia-as-elaborator/src/lower.rs`) and cmd_build's seeding pass (`crates/paideia-as/src/cmd_build.rs`) propagate the flag AST → IR. **Explicitly not in this landing:** no elaborator emit change — `emit_visit_lambda` / `emit_walker::emit_ret` still ignore `no_frame`, and function bodies still emit today's prologue-less shape. Byte-exact output is unchanged for every pdx source in-tree (verified: 20-mode smoke green in paideia-os). P0250 unknown-attribute diagnostic message updated to include `no_frame` in the accepted list. **New tests:** three parser unit tests in `crates/paideia-as-parser/src/parse_item/let_item.rs::tests` — `parse_no_frame_attribute` (attribute captured), `no_frame_absent_by_default` (default is `false`), `no_frame_composes_with_other_attributes` (guards against a refactor that short-circuits the attribute loop after `no_frame`). Phase-2 (paideia-os annotation pass over hand-crafted asm — see the audit in #1276) and phase-3 (elaborator emit change) follow.

### Tooling

- **Issue #1275** — `klog-migrate` honors `// klog-migrate: skip` opt-out annotation. Surfaced by [paideia-os#704](https://github.com/paideia-os/paideia-os/issues/704) verify (via paideia-as#1274 adversarial re-run): `kernel_main.pdx` HEAD keeps two `uart_rx_smoke_prefix_msg` sites raw because `klog_s1` appends its own trailing `\n`, which would split the two-line `"UART RX: abc"` wire fingerprint. Before this fix, every dry-run reported those intentional-raw sites as noise. **Fix:** the scanner now checks a case-insensitive `(?i)klog-migrate\s*:\s*skip` regex against the byte range spanning every source line the match touches (`lea` line through `call uart_puts` line, inclusive). A hit sets `Match::skip = Some(SkipReason::Annotation)`; the migrator filters annotated matches from rewriting and reports them separately. `--check` treats a file whose only remaining hits are annotated as clean (exit 0) — the marker is an explicit "reviewed and kept as-is" signal. **New API surface:** `Match` gains a `skip: Option<SkipReason>` field; `migrate()` returns a new `MigrateReport { source, rewritten, skipped_by_annotation, warnings }` instead of the previous `(String, usize, Vec<Warning>)` tuple. **New fixtures:** `skip_annotation_lea.pdx`, `skip_annotation_call.pdx`, `skip_annotation_between.pdx`, `skip_annotation_case_insensitive.pdx` (each with a `.expected.pdx` peer that is byte-identical to the input — the whole point of the annotation is no-rewrite). The four fixtures pin the annotation on the `lea` line, on the `call` line, on an intermediate line, and in an all-caps spelling; each pair adds one round-trip test + one `--check` cleanliness test. Two additional CLI tests cover the stderr advisory format and the mixed annotated-plus-plain case (annotated site preserved, plain site rewritten in the same pass). Two out-of-scope tests (annotation on preceding / following line) confirm the check window is line-tight. **Test count:** 39 unit + 33 CLI = 72 (up from 31 + 26 = 57 at #1274). Design doc `design/toolchain/klog-migration-helper.md` gains a full "Opt-out annotation" section describing syntax, scoping rule, CLI behavior, and interaction with `--fail-pattern`. Cross-repo: paideia-os bump annotates both `kernel_main.pdx` `uart_rx_smoke_prefix_msg` sites (L5867, L8068 at HEAD; the CHANGELOG-quoted L5864/L8033 were the surrounding comment block); post-annotation dry-run reports 0 rewritable + 2 skipped-by-annotation. No behavior change in `kernel_main.pdx` — the annotation is a comment; only the tool's report changes.

- **Issue #1274** — `klog-migrate` default `--fail-level` corrected from `5` to `1` (LEVEL_ERROR). Surfaced by [paideia-os#704](https://github.com/paideia-os/paideia-os/issues/704) verify: paideia-os's `src/kernel/core/klog/level.pdx` defines `LEVEL_ERROR=1`, `LEVEL_INFO=3`, `LEVEL_TRACE=5`, and `klog_emit_core` (`src/kernel/core/klog/emit.pdx`) gates emission with `cmp rdi, KLOG_COMPILE_LEVEL=3; ja emit_skip`. The v0.20.1 delivery (#1272) defaulted `--fail-level` to `5`, meaning every `*_fail_msg` / `*_err_msg` witness the tool emitted mapped to LEVEL_TRACE and was silently dropped by the compile-time gate — 84 fail/err sites in `kernel_main.pdx` produced no visible output. paideia-os inline-fixed all 84 at commit 463b16f; this fix aligns the upstream tool default with paideia-os's canonical LEVEL scheme so any future migration of any target lands emit-visible on the first pass. **Fix:** `RenderOpts::defaults().fail_level = 1` and CLI `--fail-level` `default_value_t = 1`, with updated doc-comments citing #1274. **New fixtures:** `level_error_fail.pdx`, `level_error_err.pdx`, `level_info_ok.pdx` (each with a `.expected.pdx` peer) — three-way pin that `*_fail_msg` → `mov rdi, 1;`, `*_err_msg` → `mov rdi, 1;`, and `*_ok_msg` → `mov rdi, 3;` respectively; each fixture pair adds one migration test + one idempotency check. **Test count:** 31 unit + 26 CLI = 57 (up from 31 + 20 = 51 at #1273). Design doc `design/toolchain/klog-migration-helper.md` gains a full LEVEL literal mapping table cross-referenced to paideia-os's `level.pdx` and `emit.pdx`. Adversarial re-run on kernel_main.pdx: `--check` reports 2 rewritable sites, both intentionally-preserved raw `uart_puts` calls at lines 5864 and 8033 (comment: `klog_s1 would append its own newline after "UART RX: ", splitting "UART RX: abc" into two lines and breaking the fingerprint's contiguity check`). The two sites use `uart_rx_smoke_prefix_msg` (non-fail/non-err) and would render as `mov rdi, 3;` (LEVEL_INFO) if migrated — no #1274 LEVEL-mapping bug is involved and no existing `mov rdi, 1;` site is reverted. This surfaces a separate design gap (scanner has no opt-out annotation) filed as [#1275](https://github.com/paideia-os/paideia-as/issues/1275); not a #1274 blocker. Cross-repo: paideia-os submodule bumps to this HEAD; the kernel_main.pdx binary itself is unchanged (fix is tool-only; the 84 LEVEL_ERROR sites were already at `mov rdi, 1;` from paideia-os 463b16f).

- **Issue #1273** — `klog-migrate` scanner accepts semicolon-optional style at both `lea` and `call` sites. The original v0.20.1 delivery (#1272) fixed the pattern to a rigid 12-token window that hard-required `;` at positions 8 (after `]`) and 11 (after `uart_puts`). `.pdx` accepts newline-terminated statements alongside semicolon-terminated ones, and real kernel sources mix both styles freely: on `paideia-os/src/kernel/boot/kernel_main.pdx` the fixed window silently skipped 54 valid migration targets (140 rewritten instead of 194). **Fix:** refactor `scan_tokens` from a fixed-width window walk to a cursor-based `try_match_at` returning `Option<(Match, tokens_consumed)>`, with positions 8 and 11 both greedily consumed when present but never required. Match extent narrows to the last-consumed token's `byte_end()`; the rendered replacement always emits the semicolon-terminated form, so migrated output is stylistically uniform regardless of source-style. New fixtures `no_semi_lea.pdx`, `no_semi_call.pdx`, `no_semi_both.pdx` (each with a `.expected.pdx` peer) pin all three semicolon-optional variants; new scan unit tests + 6 CLI tests cover the fix (31 unit + 20 CLI = 51 total, up from 27 + 14 = 41). Adversarial re-run on kernel_main.pdx: 140 → 194 rewritable, with the 4 residual `call uart_puts` sites correctly left alone (1 comment mention + 3 sites using `mov rdi, r12` / `mov rdi, rsi` non-lea RDI setup). Doc refresh: `design/toolchain/klog-migration-helper.md` updated to describe the 10..=12-token variable-width pattern and lists the actual fixture corpus (was documenting non-existent `same_line.pdx` / `string_safe.pdx`).

- **Issue #1272** — New workspace member `tools/klog-migrate` (bin `paideia-as-klog-migrate`): a tokenizer-driven `.pdx` rewriter that replaces the direct-UART fingerprint (`lea rdi, [rip + <MSG>]; call uart_puts;`) with the structured `klog_s1(LEVEL, SUBSYS, MSG)` 4-instruction block. Uses `paideia_as_lexer` to identify the 12-token pattern, so matches inside `//` comments or string literals are impossible by construction. Preserves per-line indentation, reuses existing NUL-terminated msg symbols as tag pointers (no new `.pdx` rodata emitted), and elevates fail/err messages to `LEVEL_ERROR` via a configurable regex. Supports `--check`, `--diff`, `--in-place`, `--level`, `--subsys`, `--fail-level`, `--fail-pattern`. Warns on stderr when a rewrite consumes a trailing `//` comment between the two matched instructions. 28 unit + 14 CLI tests. Cross-repo unblock for [paideia-os#704](https://github.com/paideia-os/paideia-os/issues/704) (mirrors [paideia-os#717](https://github.com/paideia-os/paideia-os/issues/717)). Dry-run against real `kernel_main.pdx` reports 140 rewritable sites out of 198 total `call uart_puts` occurrences (the remainder are bare `call uart_puts` calls without a preceding `lea rdi, [rip + SYM]`, correctly left alone). Design doc: `design/toolchain/klog-migration-helper.md`. **Followup #1273 loosens the scanner to include semicolon-optional variants.**

## v0.20.1 — unsafe_walker store-direction retarget fix

### Critical bug fixes

- **Issue #1251** — `unsafe_walker` `Mnemonic::Mov` retarget dispatch was missing the store direction. The elaborator recovered destination width for `mov reg, imm` (#827) and `mov reg, [mem]` (#930), but had no equivalent branch for `mov [mem], reg`. Every narrow-suffix store (`mov [rax], ecx`, `mov [rdi], dx`, `mov [rsi], dl`) silently collapsed to a 64-bit REX.W store, ignoring the source register's declared width. The encoder's narrow-store path (pa-r17-006 / #984) was already in place; only the elaborator retarget branch was missing. **Fix:** add `is_store` matching `[MemSib, Reg]`, reading width from the source register (operand position 1) instead of the destination. Byte-exact regression test locks all three widths (W8/W16/W32) plus a W64-unchanged guard.

  Latent downstream hazards in paideia-os cleared by this fix:
  - `src/kernel/core/syscall/dispatch.pdx` (`wait4` `wstatus` store) — 8 bytes written to a 4-byte user slot (see paideia-os #668, #672).
  - `src/kernel/core/syscall/handlers/sys_exit.pdx`, `sys_wait.pdx` — `state` / `exit_status` u32 field stores.
  - `src/kernel/core/apic/eoi.pdx`, `ioapic.pdx` — LAPIC/IOAPIC MMIO stores per SDM Vol.3A §10.4.1 (see paideia-os #646).

  Scope note: `[MemRipRelSym, Reg]` bare-symbol stores tracked as follow-up (mirrors is_load which also doesn't cover MemRipRelSym).

---

## v0.20.0 — SELF-HOST: runtime library + API freeze (released)

Self-hosting foundation release. Eight planned issues closed. Delivers stable public API for runtime library (`paideia-as-runtime` + `paideia-as-emit`) used by JIT/WASM/eventual self-hosting consumers. Includes design audit identifying self-hosting blockers ranked by effort, enabling v0.21+ planning.

### Key changes

- **Issue #1019** — Extract `paideia-as-runtime` crate (PA-R20-001). No_std compatible runtime library (~3k LoC) with stable Instruction + IrNodeId types. Re-exports from `paideia-as-ir` maintain backward compatibility; no source-site edits required for existing code. 8 runtime integration tests + 2 AC-lock canaries verify type identity and import paths. Unblocks WASM/JIT consumers; paves path to self-hosting.

- **Issue #1020** — Stable `emit_instruction(&mut CodeBuffer, Instruction)` public API (PA-R20-002). New `paideia-as-emit` crate with entry point for runtime JIT code emission. Non-exhaustive `EmitError` enum for extensibility. Pre-flight checks reject unresolved relocations and Var operands. Buffer rollback on error maintains "unchanged on Err" contract. 17 comprehensive test cases cover success, error, and discipline paths.

- **Issue #1023** — WASM i32.add dynamic-emit example (PA-R20-003). First end-to-end proof the v0.20 SELF-HOST substrate works for downstream consumers. Runnable example lowers WASM i32.add opcode to x86_64 bytes via public paideia-as-emit API. 266-LOC fixture demonstrates full decode-lower-emit pipeline.

- **Issue #1024** — `resolve_symbols` operand-level rewrite API (PA-R20-004). Second stable public entry point on paideia-as-runtime. Rewrites every reloc-producing operand in Instruction slice to forms emit_instruction accepts (no SymbolRef, LabelRef, MemRipRelSym, MemSymIndexed remain). New module `paideia-as-runtime/src/resolve.rs` + SymbolTable, LabelMap types. 12 test cases verify single/batch rewrite + error paths.

- **Issue #1025** — `sign_runtime_buffer` PQ signing wrapper (PA-R20-005). Thin wrapper for paideia-stdlib's post-quantum signing APIs. Accepts runtime buffer + key material, returns signed PAX blob. Pre-flight checks validate buffer format. Deferred full key-wrapping to follow-up.

- **Issue #1026** — API-freeze test suite (PA-R20-006). Land paideia-as-runtime and paideia-as-emit API freeze tests via syn parsing + insta snapshots. Freeze captures stable public surface of both crates and enforces breaking-change discipline through snapshot diffs in PR review. ~700 LOC across two test modules + 4 canaries verify surface stability.

- **Issue #1027** — v0.20 self-hosting audit (PA-R20-008). 751-LOC design document enumerating gaps between Rust-hosted paideia-as v0.20 and eventual `.pdx` self-hosted compiler. Assesses 23 workspace crates component-by-component. Identifies three tiers of blockers: (A) 8 language features (~6–12 months), (B) 5 stdlib libraries (~3–6 months), (C) 4 syscall surfaces (~1–2 months). Ranks five hard blockers: monomorphization, runtime evaluator, file I/O, serde, BLAKE3. Surfaces 13 gaps deferred to v0.21 (pa-r21-XXX issues). Refreshes Phase 4-m13 planning; reflects true v0.20 codebase (~145k LoC Rust vs m13-quoted ~93k).

- **Issue #1028** — v0.20 integration + release notes (PA-R20-010). Milestone-close hygiene: CHANGELOG entry, workspace version bump (v0.16 → v0.20), release notes. Verifies 5101 workspace tests green. Closes v0.20 SELF-HOST milestone.

### Non-milestone follow-ups landed this cycle

- **Issue #1237** — Root-walker seeding simplification.
- **Issue #1122** — Option C to end-to-end .efi drift detection.
- **Issue #1234** — RecordCons Borrow accepts data-symbol targets.
- **Issue #1113** — PE cross-section relocation resolution.
- **Issue #1171** — Primitive-payload width discriminator.
- **Issue #1177** — Stack-form LEA regressions.
- **Issue #1238** — emit_closure_cons capture handling.
- **Issue #1236** — Parser: OrOr split for zero-parameter closures.
- **Issue #1243** — T0556 diagnostic snapshot promotion.
- **Issue #1244** — Unsafe-block raw asm + call statement ordering.
- **Issue #1228** — BulkMemOps stdlib lowering recipes + encoding.
- **Issue #1230** — Central operator registry.

### Breaking changes

None. v0.20 adds public API surface (`paideia-as-runtime`, `paideia-as-emit`) without breaking existing consumers. Re-exports from `paideia-as-ir` maintain backward compatibility.

---

## v0.19.0 — UEFI-ABI (unreleased)

### Key changes

- **Issue #1107** — `paideia-as build --target <triplet>` shortcut for output format selection (PA-R6-M4-002). Phase 6 MVP adds 4 target triplets (`uefi-x86_64`, `elf-kernel-x86_64`, `elf-user-x86_64`, `pax-x86_64`) as user-friendly aliases for emit formats (PE32+, ELF64, ELF64, PAX). Target and emit flags conflict (clap enforces `conflicts_with`). Omitting both defaults to `.placeholder` (backward compat unchanged). New grammar: target triplets follow `<arch>-<format>` pattern; MVP locks x86_64 + 4 formats; extension points documented for future architectures/formats. New CLI enum `Target` (4 variants) + mapping function `resolve_target()` in cmd_build.rs. Test coverage: 8 integration tests (PE32+ magic validation, byte-identity with --emit, ELF64 magic, PAX magic, conflict error, invalid value error, backward compat), 6 adversarial mutation probes with quoted RED output. New documentation: `design/toolchain/cli-target-triplets.md`.

- **Issue #996 / #1242** — HashMapU64U64 fixed-capacity open-addressing stdlib (PA-R18-003), revived after the #1241 label-offset fix. The original dd98dc7 delivery (reverted as 2a2b60b after runtime crashes) turned out to have three independent fixture bugs, none of them elaborator/emit defects:
  1. Every `unsafe { block: {...} }` function body (`fibhash`, `hashmap_u64_new/put/get/contains/len`) was missing an explicit trailing `ret`. The elaborator does not synthesize an epilogue for `IrKind::Unsafe` bodies (by design — the raw block is emitted byte-for-byte), so execution fell through into the next function's bytes and eventually segfaulted.
  2. `hashmap_u64_new`'s zeroing loop tested `r8` as the loop counter but indexed and incremented `rsi`, an uninitialized register — an infinite/runaway loop that walked off the `.bss` arrays.
  3. `hashmap_u64_get`'s `OptionU64` discriminant convention was inverted relative to the enum's declared order (`Some(u64), None` → `Some`=0, `None`=1): the asm returned 1 for found/Some and 0 for not-found/None, so a successful `get` after `put` decoded as `None` in `match`.
  Also discovered along the way (both now avoided in the fixtures, tracked as follow-ups rather than blocking this revival): match scrutinees must be a bound `Var` (a direct call expression as scrutinee produces U1650 "Match node shape violation" instead of the T0556 "unsupported scrutinee" diagnostic — see #1243); a function body caps out at 4 in-flight let-literal bindings (T0527, existing documented limit); and a call-expression *statement* mixed into an `unsafe` block is deferred to a later emission pass and ends up ordered after all raw asm in that block, even after a `ret` (see #1244). `hashmap_u64_collisions.pdx`'s `entry` was rewritten as pure raw-asm calls to sidestep both. All 5 runtime canaries (`new_returns_empty`, `put_get_roundtrip`, `get_missing_returns_none`, `contains_present`, `collision_probes_sum`) pass. Baseline 4954 → 4959.

- **Issue #1016** — UEFI header emission helpers (PA-R19-011). Declare PE32+ header constants and field-offset patching interface via new `crates/paideia-stdlib/pdx/uefi.pdx`. Four-layer design: (1) PE/COFF subsystem and magic constants (UEFI_SUBSYSTEM_APPLICATION, PE32PLUS_MAGIC, COFF_MACHINE_AMD64, section characteristics), (2) 512-byte pre-baked UEFI_HEADER_TEMPLATE blob with @include_bytes + @link_section + @align, (3) field offsets for in-place patching (OFF_COFF_TIMESTAMP, OFF_OPT_SUBSYSTEM, etc.), (4) UefiHeaderOps trait declaration (set_entry, set_size_of_image, set_subsystem). New `uefi_header_template.bin` binary generated via feature-gated `regen-uefi-template` build.rs (uses paideia-as-emitter-pe to construct header). Unit tests: 4 in `crates/paideia-stdlib/tests/parse_pdx.rs` (parse cleanly, valid PE32+ magic, defaults reproducible, offsets match spec). Integration fixture: `tests/uefi-stdlib-fixture/` with 3 tests (compiles, patches entry RVA, link section placement). No changes to Rust PE emitter (`paideia-as-emitter-pe/`).

### Critical bug fixes

- **Issue #1185** — Nested field access (`a.b.c`) silently dropped in assignments and read contexts. Root cause: (1) Store-lvalue classifier (`store_lvalue.rs::is_lvalue_infix_assignment`) rejects nested FieldAccess LHS, causing the `=` to remain as `IrKind::App` instead of `IrKind::Store`; (2) emit_block_body's App arm silently drops the assignment when callee has no `binding_names()` entry; (3) source-text extraction on a nested receiver span like `"Outer.Inner"` inserts wrong flat symbol name in module_field_refs (silent miscompile). **Fix:** Add nested-FA diagnostic check in `lower/field_access.rs::populate_field_access_info` that fires T0541 before source-text extract, blocking both read and write paths with clear error message: "Nested field access (a.b.c) is not yet supported. Introduce a temporary: `let tmp = a.b; tmp.c = ...`". Two new test fixtures (read and write) + integration tests verify T0541 fires cleanly instead of silent drop.

- **Issue #1099** — SysV function calls with 1+ args emit dead-code MOVs after RET. Root cause: arg MOV IDs (1_000_000+) sorted after CALL/RET IDs (L*2, L*2+1), producing broken byte order. **Fix:** Unified CALL/RET ID scheme across SysV and MS to 1_050_000+L*100 (CALL) and 1_150_000+L*100 (RET), ensuring MOVs sort before CALL and RET sorts last. Fixed 3 indirect-call sites in `emit_lambda.rs`. Updated `record_lambda_entry` to use first-MOV ID when args exist. New unit test in `text_emitter.rs` + integration test in `codegen/call_byte_order.rs`. Regression probe: `tools/verify-byte-order.sh`.

### Key changes

- **Issue #1009** — MS x64 arg-marshalling encoder audit. 18 byte-exact test fixtures lock in behavior across all four ModR/M patterns for MS x64 calling convention registers (RCX, RDX, R8, R9): P1 movabs r_ms, imm64 (10-byte), P2 mov r_ms, r_sysv (REX.B/R correct), P3 mov r_ms, [rsp+N] (RSP SIB escape), P4 mov [rsp+N], r_ms. Auxiliary tests verify sequence consistency and iced-x86 roundtrip compliance. Encoder already correct (no source changes); test-only PR. New module: `crates/paideia-as-encoder/tests/mov/mov_ms_x64_args.rs`.

- **Issue #1006** — `@abi("ms" | "sysv")` calling-convention annotation on let bindings with function-shaped values (lambdas only). Phase 19 MVP: parse acceptance + semantic validation gates. `@abi("ms")` emits U1620 (deferred to #1011 for MS x64 prologue/epilogue codegen); `@abi("sysv")` and unannotated lambdas build normally. Non-lambda bindings with `@abi` emit P0286 error. Parser diagnostics: P0285 for invalid/malformed arguments (unknown string, non-string, missing parens, case violation).

- **Issue #1007** — Pure argument classification and slot-mapping layer for MS x64 calling convention. Extends `crates/paideia-as-ir/src/abi.rs` with `ArgClass`, `ArgSlot`, `ReturnSlot` types, `MS_ARG_REGS`/`MS_SHADOW_SPACE_BYTES` constants, and `map_args`/`map_return` functions. Supports both SysV and MS x64 calling conventions. No emit-side changes; used by elaborator (#1008, #1011) to classify and map function arguments.

- **Issue #1014** — `@include_str("...")` and `@include_bytes_as_str("...")` compile-time text embed primitives. `@include_str` performs UTF-8 validation (P0281 diagnostic on error); `@include_bytes_as_str` accepts any bytes without validation. Both lower to `IrKind::StringLiteral` with interned rodata symbols (`__str_<hash>`), giving users truncate/pad control via existing `[u8; N]` type annotation machinery.

- **Issue #1018** — UEFI stub integration test (PA-r19-013). FINAL issue of v0.19 UEFI-ABI milestone. 7 structural tests verify the 2-arg MS x64 identity function fixture compiles to PE/COFF with correct magic, machine type, subsystem, optional header, section layout, and callee prologue byte patterns. 1 boot smoke test (ignored) validates end-to-end paideia-as → PE/COFF → OVMF+QEMU pipeline. New fixture: `tests/uefi-smoke/fixtures/hello.pdx`. New test module: `crates/paideia-as/tests/build_emit/uefi_stub.rs`. New helper: `build_hello_efi_via_paideia_as()` in `tests/uefi-smoke/src/lib.rs`. Marks v0.19 milestone completion.

- **Issue #1103** — PE data-section wiring (pa-r19-013-followup). Completes data emission for PE/COFF: `@include_bytes`, `@include_str`, `@guid`, `@link_section` now emit into `.rdata`, `.data`, or `.bss` sections and custom-named sections in the PE image. New `NamedSectionError` type for 8-byte PE section name validation. Four new methods in `section.rs`: `add_rodata_bytes`, `add_data_bytes`, `add_bss_space`, `add_bytes_to_named_section`. Five new unit tests verify section append, alignment padding, BSS-only-size, custom-section deduplication, and name-length gating. Three new integration tests: `include_bytes_emits_exact_file_bytes_pe` (PE variant of #1013), `uefi_stub_with_include_bytes_has_rdata_and_text`, `uefi_stub_link_section_appears_in_pe`. MVP gates: cross-section relocations skipped (deferred #1105); COFF symbol table omitted; SectionKind::Text deferred. New diagnostic P0289 (section name exceeds 8 bytes).

- **Issue #1125** — Diagnostic code split: U1610 → U1615 for imm64 r11 collision. PA-R13-010 macro-expansion (`or/and/xor r64, imm64`) reserves R11 as scratch register. When the destination register is r11, expansion cannot proceed. The collision guard's diagnostic code is split from U1610 (which is reserved for label-reference unknowns) to U1615 (unsafe-block discipline category, range 1600–1699). Updated `imm64_expand.rs` constant, test vectors in `imm64_bitops.rs`, and diagnostic catalog entry.

### Breaking changes

- **Issue #1110** — Output selection is now mandatory (PA-R6-M4-003). Omitting both `--target` and `--emit` on `paideia-as build` is now a usage error (exit code 2) instead of silently defaulting to `--emit placeholder`. The clap `ArgGroup(required=true)` enforces this at parse time. **Migration:** Restore old smoke-test behavior explicitly with `--emit placeholder`. All four output formats (`placeholder`, `elf64`, `pax`, `pe-coff`) remain fully available; this change requires callers to choose one explicitly. Existing scripts using `--emit` or `--target` are unaffected.

- **Issue #1107** — `paideia-as build --target <triplet>` (PA-R6-M4-002) adds user-friendly target triplets as alternatives to `--emit` formats. See Issue #1110 above for interaction with output selection requirements. `--target` and `--emit` conflict at parse time; callers must use one or the other (not both).

- **Issue #1108** — `paideia-as check` no longer auto-writes SARIF sidecar at `<input>.sarif.json`. Use `--sarif <PATH>` flag explicitly on both `check` and `build` commands to emit diagnostics in SARIF 2.1.0 format to a specified file path. Removes implicit output coupling and aligns with symmetric flag treatment across subcommands.

- **LetInfo (v0.19)** — `Copy` trait removed from `LetInfo` struct (in `paideia-as-ir`). The struct now carries an `Option<String>` field for `@link_section` support (issue #1015), which is not `Copy`. Dependent code must clone LetInfo instead of copying. Migrating: change patterns like `let info = table.get(id).unwrap().clone()` or adjust iterator patterns to use references.

## v0.18.0 — STDLIB: Option<T>, Result<T,E>, and Str types

**In development:** Milestones PA-R18-004 (#997) and PA-R18-005 (#998, downgraded scope due to #998a).

Foundational stdlib types `Option<T>`, `Result<T,E>`, and `Str` now have concrete implementations in `crates/paideia-stdlib/pdx/`. Generic enum declarations establish the surface API for Option/Result; hand-monomorphized aliases (`OptionU64`, `ResultU64U64`) provide kernel-testable implementations. The `Str { ptr: *u8, len: u64 }` type enables string operations via pointer parameters (module-level constants deferred due to issue #998a, a gap in data-symbol Borrow elaboration). Four Option/Result canaries verify constructor + match-based extraction round-trips.

### Key changes

- **Issue #997** — Option<T> and Result<T,E> in stdlib (PA-R18-004). Delivers:
  - Generic `enum Option<T> { Some(T), None }` in `pdx/option.pdx` + documented method surface.
  - Generic `enum Result<T, E> { Ok(T), Err(E) }` in `pdx/result.pdx` + method surface.
  - Hand-monomorphized `enum OptionU64 { Some(u64), None }` in `pdx/option_u64.pdx` + free-function `option_u64_unwrap_or`.
  - Hand-monomorphized `enum ResultU64U64 { Ok(u64), Err(u64) }` in `pdx/result_u64_u64.pdx` + free functions `result_u64_u64_unwrap_or_ok` / `result_u64_u64_unwrap_or_err`.
  - 20 fixture files (12 modified, 8 new) for parse-cleanliness testing.
  - 4 runtime canaries: `option_some_extract` (exit 42), `option_none_default` (exit 99), `result_ok_extract` (exit 42), `result_err_extract` (exit 7).
  - Deferred combinators (map/and_then/map_err/ok_or) to issue #997b (blocked on closure primitives #995).
  - Deferred impl<T> method blocks to issue #997c (blocked on monomorphization #994/#995).
  - Deferred auto-prelude/use grammar to issue #997d (structural).

- **Issue #998** — Str type + field accessors in stdlib (PA-R18-005, DOWNGRADED SCOPE). Scope reduced due to #998a (RecordCons Borrow field targeting data symbols not yet supported). Delivers:
  - Canonical `struct Str { ptr: *u8, len: u64 }` in `pdx/str.pdx` (16-byte record).
  - Field-read accessors: `fn(s: *Str) -> u64` (str_len) and `fn(s: *Str) -> *u8` (str_ptr) via FieldAccess(Deref(Var)).
  - 7 parse-clean fixtures (2 ptr-form accessors + 4 parse-only API shapes + 1 type definition).
  - 5 modified fixture stubs aligned to Str { ptr, len } naming.
  - Deferred module-level Str constants to #998a (data-symbol Borrow gap).
  - Deferred unsafe-asm `str_byte_at` to #998a (requires module constants).
  - Deferred string comparison, hashing, substring, split to issues #998b–e (blocked on deref/loop/by-value args/by-value return).

- **Issue #999** — Command-dispatch pattern documentation (PA-R18-006). Canonicalises the dispatch pattern for semantic-shell command routing as a hash-table mapping command names to closures (target shape, blocked on #994/#995/#996/#998b–e). Documents three dispatch strategies buildable today: (1) enum-tag dispatch via cmp/je cascade, (2) enum-tag dispatch via @jump_table O(1), (3) function-pointer indirect dispatch. Deliverables: design documentation at `design/toolchain/command-dispatch-pattern.md` (~265 LOC), reference fixture `tests/build-emit/pa_r18_006_command_dispatch_shell.pdx`, runtime canary returning exit code 3, build-emit integration test. Witness fixtures demonstrate enum-tag dispatch + @jump_table codegen on unit-variant commands (Ls, Pwd, Echo, Exit); expected exit code 3. Blocks paideia-os phase 11 semantic-shell implementation; migration path table shows when each blocker lands.

## v0.17.0 — CONTROL-FLOW: pure functions with if/match/while/loop

**In development:** Milestone PA-R17-012 (#990).

Pure function bodies now support full control-flow lowering: `if`-then-else, `match`, `while`, and `loop`. T0532 diagnostic stub (issue #913) retired; lower.rs now transfers AST children for control-flow nodes (If/Match/Loop/For), and IR kinds Loop/While properly distinguish infinite loops from conditional ones. Three upstream fixes: (1) child-transfer match arms in lower.rs; (2) ExprLoop kind refinement to map Loop/While separately; (3) emit_control_flow.rs pattern coordination with emit_block_body_arm.

### Key changes

- **PA-R17-012** (issue #990) — Retire T0532 stub for pure-function control flow. Implement three upstream fixes: (1) add match arms for ExprData::If/Match/Loop/For in lower.rs child-transfer pass; (2) refine ExprLoop to map to IrKind::Loop or IrKind::While based on LoopKind; (3) coordinate emit_control_flow.rs visit_branch/visit_while with emit_block_body_arm pattern. Delete check_body_shape.rs module; T0532 code (113 in cmd_build.rs) now exclusive to address-of validation (pa-r17-003 #981). Test coverage: 12 unit tests for if/match/while/loop in pure fns + nested conditionals + trailing positions + break/continue + regression (no T0532).

## v0.16.0 — COW-ATOMICS: atomic RMW substrate for phys_alloc + CoW refcount

**Released:** Tag pushed at PA-R16-012 closure (v0.16.0 release).

COW-ATOMICS release. 12 planned issues + 5 in-flight backtracks filed. Focus is the encoder + stdlib substrate for copy-on-write filesystem allocation: locked bit-test-and-set/clear/complement, locked compare-and-swap (both 32-bit and 128-bit), locked fetch-and-add completion, plus stdlib forward-declarations for bitmap scanning, per-page reference counting, lock-free freelist operations, and spinloop-hint pause instruction.

  Encoder additions:
    PA-R16-001 (#967)  bt/bts/btr/btc r/m32/r/m64, r32/r64  — bit test register forms.
    PA-R16-002 (#968)  lock bts/btr/btc [mem], imm8         — locked bit ops, atomic page alloc fast path.
    PA-R16-003 (#969)  lock cmpxchg [m], r32                — 32-bit CAS; CoW refcount decrement.
    PA-R16-004 (#970)  lock cmpxchg16b [m]                  — 128-bit CAS; freelist ABA-safe pop.
    PA-R16-005 (#971)  lock xadd [m], r32/r64 (completion)  — landed at PA-R15-002 (v0.15.0).
    PA-R16-006 (#972)  lock and/or/xor [m], r64             — atomic RMW; flag clear/set/toggle.
    PA-R16-007 (#973)  pause (F3 90)                        — spinloop hint in CAS retry loops.
    PA-R16-008 (#974)  bsf / bsr / tzcnt (W64)              — bitmap-scan intrinsics for first-free.

  Language + stdlib additions:
    PA-R16-009 (#975)  refcount.pdx — per-page CoW refcounts — trait RefcountOps (incr/decr/decr_and_test).
    PA-R16-010 (#976)  bitmap.pdx — free-page tracking       — trait BitmapOps (8 fns for bit manipulation).
    PA-R16-011 (#977)  freelist.pdx — ABA-safe pool          — trait FreelistOps (push/pop/empty).
    PA-R16-012 (#978)  CoW FS integration canary             — v0.16 canary composing all atomics surface.

  Backtracks filed in-release (5 open; codegen + elaborator work):
    PA-R16-004a (#1033) compile-time CPU-feature declaration + gating mechanism.
    PA-R16-004b (#1034) register-clobber and implicit-operand tracking for LOCK-prefixed mnemonics.
    PA-R16-005-backtrack (#1035) reclassify LOCK atomics + fences as InstructionClass::AtomicLocked — LANDED at af14387.
    PA-R16-007-backtrack (#1036) elaborator lowering for stdlib trait methods (PauseOps::spin_hint, PerCpuOps, MmioOps, BytesOps, ChecksumOps).
    PA-R16-backtrack (#1037) pre-existing stdlib parse failures — Phase-4 modules using unsupported 'use paideia.raw_mem;' import syntax.

### Detailed bullets

- **PA-R16-012** (issue #978) — CoW FS canary trait declaration modelling a minimal copy-on-write filesystem physical-page allocator via v0.16 atomics substrate (BitmapOps, RefcountOps, FreelistOps, PauseOps, PhysAllocOps). Multi-trait module test (grammar probe for single module = structure holding 5 traits); `check` and fixture assertions both green. Canary gates paideia-os Phase 16 M1 work.
- **PA-R16-011** (issue #977) — freelist.pdx forward-declared FreelistOps trait with freelist_push/pop/empty. Lock-free ABA-safe fast-path pool backed by lock cmpxchg16b. Trait interface stable; lowering to lock cmpxchg16b primitives deferred to elaborator (#1036). Companion to #975 (refcount) and #976 (bitmap).
- **PA-R16-010** (issue #976) — bitmap.pdx forward-declared BitmapOps trait with 8 functions: bitmap_get/word_count (read-only), bitmap_set/clear/toggle (atomic via lock bts/btr/btc), bitmap_first_free (linear scan fallback), bitmap_claim_first_free (atomic bit-search + claim). Lowering to bt/lock bts/bsr intrinsics deferred to #1036. Companion to #977 (freelist) and #975 (refcount).
- **PA-R16-009** (issue #975) — refcount.pdx forward-declared RefcountOps trait with refcount_incr/decr/decr_and_test. Atomic per-page reference counters for CoW share tracking; all three methods carry `!{Atomic}`. Lowering to lock xadd + cmpxchg deferred to #1036. Companion to #976 (bitmap) and #977 (freelist).
- **PA-R16-008** (issue #974) — Bitmap-scan intrinsics: bsf r64, r/m64 (bit-scan-forward), bsr r64, r/m64 (bit-scan-reverse), tzcnt r64, r/m64 (trailing-zero count). bsf/bsr: AMD/Intel forms; tzcnt: AMD BMI extension (F3 prefix). Encoding forms: RM (no REX required) + SIB variants. No W64 handling for bsf/bsr (no 32-bit form); tzcnt carries both W32/W64. Companion to #976 bitmap_first_free scaling.
- **PA-R16-007** (issue #973) — `pause` spinloop-hint mnemonic (F3 90). 2-byte instruction; REP prefix (0xF3) + NOP (0x90). Retires the bare-NOP spinloop antipattern. Scheduling class Other. Test coverage: byte-exact (F3 90) + iced-x86 round-trip. Lowers to pause in CAS retry loops for #970/#971/#977 atomics.
- **PA-R16-006** (issue #972) — `lock and [m], r64` / `lock or [m], r64` / `lock xor [m], r64` (locked bitwise RMW). Encoding: F0 [REX.W] [23/0B/33] /r (base+disp and SIB forms). Operand order: memory destination (ModR/M /r form), register source. Scheduling class Other. Test coverage: 12 byte-exact (3 mnemonics × 4 bases/SIBs) + iced-x86 round-trip. Enables atomic flag-set/clear/toggle in kernel spinlocks + device drivers.
- **PA-R16-005** (issue #971) — `lock xadd [m], r32/r64` (fetch-and-add). Already landed in v0.15.0 PA-R15-002. This PA-R16 issue marks the completion of the lock + arithmetic family in v0.16.
- **PA-R16-004** (issue #970) — `lock cmpxchg16b [m]` (128-bit CAS on 16-byte-aligned operand). Encodes as F0 REX.W 0F C7 /1 (10-byte upper bound). Implicit operand pairing: EDX:EAX (comparand), RCX:RBX (new value). Memory operand must be 16-byte aligned else #GP. Unblocks lock-free freelist (issue #977). Scheduling class Other. Test coverage: 4 byte-exact tests (16-byte-aligned `[rdi]`, `[r8]`, `[rsp]`, `[rbp]` with escape handling) + iced-x86 round-trip verifying implicit operands.
- **PA-R16-003** (issue #969) — `lock cmpxchg [m], r32` (32-bit compare-and-swap). Encodes as F0 0F B1 /r (9-byte upper bound). Implicit RAX comparand (8-byte for r64 variant from v0.12 #917); new value in second register operand. Test coverage: 4 byte-exact tests + iced-x86 round-trip. Unblocks CoW refcount decrement-and-test atomic primitive (issue #975).
- **PA-R16-002** (issue #968) — `lock bts [base+disp], imm8` / `lock btr [base+disp], imm8` / `lock btc [base+disp], imm8` (locked bit-test operations with immediate). Encoding: F0 0F [AB/B3/BB] /0 ib (7-byte). Immediate range i8 (0..63 for standard x86, extended by AMD). Memory form is locked; register form unsupported in R16. Also encodes SIB variant `lock bts [base+index*scale+disp], imm8`. Scheduling class Other. Test coverage: 12 byte-exact tests (3 mnemonics × 4 addressing modes) + iced-x86. Unblocks fast-path bitmap atomic set/clear (issue #976).
- **PA-R16-001** (issue #967) — `bt / bts / btr / btc r/m64, r64` (bit-test register forms). Encoding: 0F [A3/AB/B3/BB] /r (no LOCK prefix for register form). Operand order: bit string (register or memory) / bit index. Each produces CF from selected bit; bts/btr/btc also modify the bit. Register destination forms (bts r64, r64) modify destination in-place. Scheduling class Other. Test coverage: 16 byte-exact tests (4 mnemonics × 4 REX combinations) + iced-x86 round-trip. Companion register form to #968 locked-memory operations.

Cross-cut discipline:
- Every issue closed via softarch → workerbee → debugger triangle.
- One debugger REJECT in-release (#978) fixed before landing — no placeholder shipped.
- Five backtrack issues filed during #970/#973/#974 and scheduled to v0.16 for elaborator + codegen follow-up.
- Additive-only Mnemonic / Operand growth held.
- SARIF snapshot regenerated at v0.16.0 (no new diagnostics; encoders only).

---

## v0.15.0 — NET-PRIMITIVES: bit ops, checksums, endian scalars (paideia-os network stack foundation)

**Released:** Tag pushed at PA-R15-011 closure (v0.15.0 release).

Net-primitives release. 11 planned issues + 2 in-flight backtracks filed. Focus is the encoder + stdlib substrate for the paideia-os network stack: bit rotation, atomic increment/decrement/fetch-and-add, carry-chain arithmetic, byte swap, population count, plus stdlib forward-declarations for byte parsing, IPv4 checksum, per-CPU counters, and the @jump_table attribute for O(1) protocol dispatch.

  Encoder additions:
    PA-R15-001 (#956)  bswap r32                           — bswap_d mnemonic, 0F C8+rd no REX.W.
    PA-R15-002 (#957)  lock xadd [mem], r32/r64            — fetch-and-add for shared counters.
    PA-R15-003 (#958)  lock add/sub [mem], imm8/imm32/r64  — atomic increment/decrement w/ smart imm8/imm32 select.
    PA-R15-004 (#959)  rol/ror r32/r64, imm8/cl            — bit rotation, short-form D1 for imm=1.
    PA-R15-005 (#960)  adc/sbb r32/r64, r/m32/r64          — carry-chain arithmetic (RM-encoded).
    PA-R15-006 (#961)  popcnt r32/r64, r/m32/r64           — F3 0F B8 /r, Nehalem+ baseline.

  Language + stdlib additions:
    PA-R15-007 (#962)  bytes.pdx — byte-parsing surface     — 14 fns u8/u16/u32/u64 BE/LE R/W.
    PA-R15-008 (#963)  checksum.pdx — RFC 1071 ipv4_checksum — forward-declared intrinsic.
    PA-R15-009 (#964)  @jump_table attribute                — parser+AST+diagnostics; codegen deferred to #1032.
    PA-R15-010 (#965)  percpu.pdx — GS-relative counter idiom — trait interface + design doc.
    PA-R15-011 (#966)  UDP echo canary                      — v0.15 canary combining the surface.

  Backtracks filed in-release (both open under v0.15 milestone; codegen work):
    PA-R15-009a (#1031) encode_jmp_mem_rip_index_scale primitive (FF 24 SIB + disp32).
    PA-R15-009b (#1032) @jump_table codegen — rodata table + memory-indirect jmp.

### Detailed bullets

- **PA-R15-011** (issue #966) — UDP echo canary trait declaration modelling IPv4+UDP surface via v0.15 stdlib shapes (bytes/checksum/percpu). Trait-only fixture; `check` and `build --emit elf64` both green. Debugger caught two false-green iterations (grammar mismatch, unresolved bare mnemonics) — both fixed before landing.
- **PA-R15-010** (issue #965) — percpu.pdx forward-declared PerCpuOps trait with percpu_inc/percpu_add, `!{Atomic, RawMem}` effect and `@{paideia.raw_mem}` capability. design/toolchain/percpu-idiom.md documents the planned lowering (F0 65 [REX] FF 04 25 disp32 for `lock inc gs:[disp32]`). Codegen deferred; interface stable.
- **PA-R15-009** (issue #964) — @jump_table attribute on match, between scrutinee and `{`. Parser + AST + P0270/P0271 diagnostics live now. P0272–P0275 registered in catalog; elaborator emission tracked by #1032. Density contract: `covered*2 >= range` AND `range <= 256`. Filed backtracks #1031 (missing FF 24 SIB primitive) and #1032 (codegen synthesis pass). Debugger caught false-green tests + broken fixture grammar + regressed SARIF snapshot + doc/catalog code mismatch across two iterations — all fixed.
- **PA-R15-008** (issue #963) — checksum.pdx forward-declared ChecksumOps trait with `ipv4_checksum(hdr: u64, len: u64) -> u16`. RFC 1071 reference-vector fixtures (8) exercise parse-cleanliness; adc-chain + fold lowering deferred to codegen.
- **PA-R15-007** (issue #962) — bytes.pdx forward-declared BytesOps trait with 14 fns (u8, u16/u32/u64 × BE/LE × R/W). Signatures use raw u64 addresses (no slice types yet parseable in .pdx). 6 IPv4 header fixtures document intended use. Capability `@{paideia.raw_mem}`.
- **PA-R15-006** (issue #961) — popcnt r32/r64, r/m32/r/m64. F3 prefix → REX → 0F B8 → ModR/M order per SDM §2.1.1. W32 REX suppression when no high registers. Nehalem+ CPUID baseline (paideia-os target).
- **PA-R15-005** (issue #960) — adc/sbb r32/r64, r/m32/r64. RM-encoded (opcode 13/1B → dst in ModR/M.reg, opposite of add/sub 01/29 MR forms). W32 REX-byte suppression preserved. Reads/writes CF.
- **PA-R15-004** (issue #959) — rol/ror r32/r64, imm8/cl. Short-form D1 automatically selected for imm=1. Non-CL/ECX count register rejected via `E0036: rol variable count requires CL`. imm outside i8 → `E0035`.
- **PA-R15-003** (issue #958) — lock add/sub [mem], imm8/imm32/r32/r64. 12 primitives with smart immediate selection: fits in i8 → 83+imm8; else fits in i32 → 81+imm32; else `E0033: imm out of i32 range`. `/0` (add) / `/5` (sub) digits; opcodes 83/81/01/29.
- **PA-R15-002** (issue #957) — lock xadd [mem], r32/r64. F0 [REX.W] 0F C1 /r. Scheduling class Other alongside LockCmpxchg/Xchg. iced round-trip asserts `has_lock_prefix() == true`.
- **PA-R15-001** (issue #956) — bswap r32 (bswap_d mnemonic). 0F C8+rd with optional REX.B for r8d..r15d; REX.W never set (else decodes as bswap r64). All 16 GPRs verified via iced round-trip.

Cross-cut discipline:
- Every issue closed via softarch → workerbee → debugger triangle.
- Two debugger REJECTs in-release (#964, #966) fixed before landing — no placeholder shipped.
- Two backtrack issues filed during #964 and scheduled to milestone v0.15 for codegen follow-up.
- Additive-only Mnemonic / Operand growth held.
- SARIF snapshot regenerated at v0.15.0 including P0270–P0275.

---

## v0.14.0 — DRIVER-SUBSTRATE: MMIO, ring buffers, fnptr in unsafe (paideia-os drivers foundation)

**Released:** Tag pushed at PA-R14-012 closure (v0.14.0 release).

Driver-substrate release. 12 planned issues + 1 in-flight backtrack landed. Focus is idiomatic MMIO, cache-control ops, ring buffer synthesis, driver-side effect vocabulary, and first-class optimization pipeline.

  Encoder additions:
    PA-R14-001 (#944)  mov_[bwdq] narrow-width mem stores  — store-form counterpart to v0.13 #930.
    PA-R14-002 (#945)  mov r32,[mem] audit + tests         — audit-only end-to-end coverage.
    PA-R14-002b (#1030) mov [rip+sym] narrow-width         — surfaced during #945 audit; landed in-release.
    PA-R14-003 (#946)  movnti [mem], r32/r64               — non-temporal store.
    PA-R14-004 (#947)  sfence / lfence                     — store + load barriers.
    PA-R14-005 (#948)  wbinvd/invd/clflush/clflushopt       — cache-control instructions.
    PA-R14-006 (#949)  prefetch[nta/t0/t1/t2] family       — cache-hint instructions.

  Language + toolchain additions:
    PA-R14-007 (#950)  MMIO helper stdlib skeleton         — trait MmioOps + capability paideia.mmio.
    PA-R14-008 (#951)  @ring(slots=N, slot_size=M)         — first 1-to-N attribute synthesis.
    PA-R14-009 (#952)  fnptr dispatch unsafe pattern docs  — combines pa-r13-003 with unsafe blocks.
    PA-R14-010 (#953)  driver-side effect vocabulary       — MmioRead/Write, CachePolicy, NonTemporal, DmaBarrier.
    PA-R14-011 (#954)  peephole cmp reg,0 -> test reg,reg  — first optimization pass. -O 1 flag.
    PA-R14-012 (#955)  AHCI FIS driver corpus integration test — v0.14 canary.

### Detailed bullets

- **PA-R14-012** (issue #955) — AHCI FIS ring driver stub + integration test. Composes @ring synthesis, mov_q stores, movnti/clflush/sfence, cmp/jz dispatch, register-offset constants in .rodata. First real cross-feature exercise; designated v0.14 canary.
- **PA-R14-011** (issue #954) — Peephole `cmp reg, 0` → `test reg, reg` optimization. First optimization pass in the pipeline. New `-O/--optimize <level>` CLI flag (default 0). Rewrite fires when cmp with imm 0 is followed by jz/jnz/je/jne. Correctness: test and cmp both clear CF/OF and set ZF/SF/PF identically for zero comparison. Saves 4 bytes per site. Debugger caught 2 in-flight bugs: CLI plumbing was missing in first pass; instruction_table was cloned BEFORE optimization in second pass (stale IR bug — silent no-op). Both fixed before landing.
- **PA-R14-010** (issue #953) — Driver-side effect vocabulary. Adds CachePolicy, NonTemporal, DmaBarrier to abi.pdx alongside #950's MmioRead/MmioWrite. Documentation covers composition rules and consumer map. Effect registry interning + signature composition deferred to v0.15+. Debugger caught invalid `.pdx` fixture syntax (`effect Name = {}` vs correct `effect Name { }`) — fixed before landing.
- **PA-R14-009** (issue #952) — Fnptr dispatch unsafe pattern docs + 11 integration tests. Documents 4 canonical dispatch shapes: reg-indirect load+call, memory-indirect base+disp, SIB table dispatch, RIP-relative. Combines pa-r13-003 (indirect call) with unsafe-block source. First-class typed fn-ptr support deferred to v0.17 pa-r17-004.
- **PA-R14-008** (issue #951) — `@ring(slots=N, slot_size=M)` attribute. First 1-to-N attribute synthesis in paideia-as: a single Let with @ring emits 4 symbols (_slots BSS align 64, _head/_tail .data, _mask .rodata = slots-1 LE). Parser diagnostics P0253/P0260/P0261. E2E ELF verification. Debugger caught missed SARIF snapshot regen.
- **PA-R14-007** (issue #950) — MMIO helper stdlib skeleton. Trait MmioOps with 8 fn signatures (r8/r16/r32/r64 × read/write) carrying !{RawMem, MmioRead|Write} and @{paideia.mmio}. Capability paideia.mmio added to BUILTIN_CAPABILITIES. Effect declarations MmioRead/MmioWrite added to abi.pdx.
- **PA-R14-006** (issue #949) — `prefetchnta/t0/t1/t2` family. 4 flat Mnemonic variants; arity 1 with /0../3 opcode extension. Debugger caught missing MNEMONIC_TABLE entries + missing iced round-trip in first pass; fixed before landing.
- **PA-R14-005** (issue #948) — `wbinvd`, `invd`, `clflush [mem]`, `clflushopt`. Full cache-control instruction set. arity-0 pair + arity-1 pair with SIB variants.
- **PA-R14-004** (issue #947) — `sfence` (0F AE F8) + `lfence` (0F AE E8). Completes the fence trio with the pre-existing mfence.
- **PA-R14-003** (issue #946) — `movnti_d` / `movnti_q` non-temporal stores. Mnemonic::Movnti{width}; 4 primitives (base+disp + SIB × W32 + W64).
- **PA-R14-002b** (issue #1030) — `mov r{8,16,32,64}, [rip + sym]` narrow-width RIP-relative loads. encode_mov_sized was missing arms for MemRipRel/MemRipRelSym at narrow widths; only W64 went through the pre-existing rip-relative path. Filed by workerbee during pa-r14-002 audit; retired in-release.
- **PA-R14-002** (issue #945) — Full mov r32, [mem] audit. Load-form narrow-width paths already worked (delivered as part of #930); this issue landed 8+2 tests covering the r32 load-form surface + surfaced the rip-relative gap #1030.
- **PA-R14-001** (issue #944) — `mov_[bwdq] [mem], imm` distinct-mnemonic store forms. Zero IR schema change (reuses Mnemonic::MovSized{width}). First workerbee attempt tried to extend Operand::MemSib with a `size_hint` field — cascaded into ~20 struct-literal sites and syntax errors from botched sed. Reverted; redesigned to the mnemonic-only approach. Landed clean with 16 tests.

## v0.13.0 — GAP-CATCH: R14B workaround-retirement wave (paideia-os user substrate)

**Released:** Tag pushed at PA-R13-014 closure (v0.13.0 release).

R14B-cycle encoder + elaborator additions and bug fixes commissioned by paideia-os R14B (higher-half kernel → shell demo). Fourteen planned issues + one in-flight backtrack landed. Retires the three biggest cross-repo escalations filed during paideia-os R14B (paideia-os#927 narrow-load, #928 REX.B SIB drop, #929 indirect call) plus every workaround pattern noted in `design/roadmap/paideia-as-tactical-issues.md` §3.

  Encoder additions:
    PA-R13-001 (#930)  narrow-width mov r8/r16/r32,[mem]  — retires paideia-os#927.
    PA-R13-002 (#931)  REX.B on SIB base + dest ModR/M    — retires paideia-os#928.
    PA-R13-003 (#932)  indirect call [mem] and call reg   — retires paideia-os#929.
    PA-R13-004 (#933)  ud2 (0F 0B)                        — enables unreachable-tail idiom.
    PA-R13-005 (#934)  dec/inc r64 (REX.W FF /1, /0)      — retires sub-1/add-1 loop workarounds.
    PA-R13-006 (#935)  test r64, imm32 (REX.W F7 /0)      — retires and+cmp workaround.
    PA-R13-007 (#936)  test r/m64, r64 audit + tests      — audit-only (already existed).
    PA-R13-008 (#937)  cld / std (FC / FD)                — direction-flag discipline.
    PA-R13-009 (#938)  imul three-operand form audit      — audit-only + test coverage.
    PA-R13-011 (#940)  rep_movsb robustness audit         — audit-only (v0.12 label-drain fix was the real cure).
    PA-R13-012 (#941)  rep_stosq robustness audit         — audit-only, same story.
    PA-R13-013 (#942)  setcc r8 family (16 conditions)    — via Mnemonic::Setcc(Cond), mirrors Jcc(Cond).
    PA-R13-014 (#943)  bswap r64 (REX.W 0F C8+rd)         — endianness conversion substrate.

  Elaborator additions:
    PA-R13-010 (#939)  or/and/xor r64, imm64 macro-expansion via movabs r11 + reg-reg — retires mov+shl+or workaround.

  Backtracks addressed in-release:
    PA-R13-001b (#1029) SIB base=RBP/R13 disp=0 escape — centralised SIB tail into emit_mem_sib_disp() reused by 5 call sites.

### Detailed bullets

- **PA-R13-014** (issue #943) — `bswap r64` (REX.W 0F C8+rd). Endianness conversion; no ModR/M byte (opcode-plus-register form). Byte-exact + iced round-trip; 16-register length sweep. `bswap rax` = `48 0F C8`; `bswap r15` = `49 0F CF`. Substrate for future network stack (Phase 8) and disk-format code.
- **PA-R13-013** (issue #942) — SETCC family via `Mnemonic::Setcc(Cond)`. Extended `Cond` enum with `Parity`/`NotParity` (for setp/setnp). One variant + payload avoids 16-mnemonic bloat; aliases (setz/sete, seta/setnbe, setb/setc/setnae, ...) share IR variants. Encoder primitive derives SETCC opcode from JCC opcode (SDM invariant `SETCC = JCC + 0x10`). RegId sentinel range 33..36 for spl/bpl/sil/dil (bare REX required). 21 tests: byte-exact for all 16 conditions + 4 iced round-trip.
- **PA-R13-012** (issue #941) — `rep_stosq` audit-only. Encoder was correct (`F3 48 AB`); R14B workaround in elf_lite_load was stale. Added tests/build-emit/rep_stosq_smoke.pdx + integration driver + negative encoder unit test. Follow-up: retire elf_lite manual qword zero loop in paideia-os.
- **PA-R13-011** (issue #940) — `rep_movsb` audit-only. Encoder correct (`F3 A4`); R14B workaround was working around v0.12's label-drain bug (fixed in v0.12 #924), not a rep_movsb defect. Same treatment: fixture .pdx + Rust driver + negative encoder unit test. Follow-up: retire elf_lite byte-copy loop in paideia-os.
- **PA-R13-010** (issue #939) — `or/and/xor r64, imm64` macro-expansion. Elaborator-level 1→2 expansion at Instruction-build time (in unsafe_walker's retarget block, next to MovSized). Reserves R11 as expander scratch; collision guard emits diagnostic 1615 when dst == r11. Trigger: `mnemonic ∈ {Or, And, Xor}` ∧ `[Reg, Imm64(v)]` ∧ `v != (v as i32 as i64)`. Emits `movabs r11, imm64; <op> dst, r11`. Labels alias to the movabs (head of expansion). Follow-ups filed: dynamic scratch rotation when dst == r11 (avoid E1710 fatal); unsafe-block r11 clobber-audit (W1711). Retires the paideia-os workaround pattern where 64-bit immediate bit-packing (aspace_map MAP_HUGE sentinel, tss.pdx IOMAP_BASE, idt.pdx IST masks) was emulated via mov+shl+or.
- **PA-R13-009** (issue #938) — Three-operand `imul r64, r/m64, imm8/imm32` audit-only. Primitives already existed (encode.rs:593/609); dispatch correctly selected imm8 form when `imm == (imm as i8 as i64)`. Added 14 new tests to `tests/imul_three_op.rs`: spec vectors, boundary quartet (±127/128, -129), REX.R+REX.B combinations, 4 iced round-trip. Retires the elf_lite_load two-register `mov r11, 56; imul r10, r11` workaround pattern.
- **PA-R13-008** (issue #937) — `cld` (0xFC) and `std` (0xFD). 1-byte arity-0 mnemonics; extends encode_zero_operand sentinel table (0x84 → CLD, 0x85 → STD). Retires the paideia-os workaround where elf_lite_load relied on the SysV kernel-entry DF=0 assumption instead of emitting explicit `cld` before `rep_movsb`. 6 tests: byte-exact + mode-agnostic + iced round-trip.
- **PA-R13-007** (issue #936) — `test r/m64, r64` reg-reg form audit-only. `test_reg64_reg64` already existed at encode.rs:1441. Added 8 tests to `tests/test_reg.rs`: 6 byte-exact spanning all 4 REX combinations + 2 iced round-trip. Prepares peephole lowering `cmp r,0 → test r,r` deferred to v0.14.
- **PA-R13-006** (issue #935) — `test r64, imm32` (REX.W F7 /0). Special-cases RAX to short form (`48 A9 <imm32>`, 6 bytes); other GPRs use general form (`REX.W F7 /0 modrm <imm32>`, 7 bytes). Values > i32 range → `EncodeError::Unsupported` (no imm64 direct encoding — the elaborator's imm64 macro-expansion at #939 doesn't apply to TEST; imm64 test emulation is the "and+cmp workaround" pointed at by the error message). No 83 /X ib subgroup for TEST. 14 tests: 4 required byte-exact vectors + rax short-form guard + i32 boundary tests + 3 iced round-trip.
- **PA-R13-005** (issue #934) — `dec` and (scope-expanded) `inc` r64. Both encode as REX.W FF /1 and /0 respectively (differ only by ModR/M reg field). Legacy 40+rd / 48+rd short forms deliberately avoided — those bytes are repurposed as REX prefixes in long mode. Retires the paideia-os workaround pattern where loop counters used `sub r,1` instead of `dec r` (see paideia-os#512, #521). 16 tests spanning both mnemonics × 4 REX shapes + iced round-trip.
- **PA-R13-004** (issue #933) — `ud2` (0F 0B). Undefined instruction, always raises #UD. Enables idiomatic unreachable-tail slots after iretq / sysretq / any never-return sink. Retires the paideia-os workaround at enter_userland_initial (which used `hlt` because ud2 was absent). Once paideia-os bumps the submodule, that site can express `ud2` and get real #UD handling via the wired handle_ud (from paideia-os#644 IDT vector wiring). Arity-0 sentinel pattern.
- **PA-R13-003** (issue #932) — Indirect `call [mem]` and `call reg` (retires paideia-os#929). New IR variant `Operand::MemRipRelSym { name, addend }` disambiguates bracketed `[rip + sym]` from bare `SymbolRef`; parser change makes bare-symbol path unambiguously `SymbolRef`, bracketed path unambiguously `MemRipRelSym`. Four new encoder helpers: `call_reg64`, `call_mem_base_disp`, `call_mem_sib_disp`, `call_mem_rip_rel`. `encode_call` gains 5 new arms (Reg, base+disp MemSib, indexed MemSib, MemRipRel, MemRipRelSym). Ripple: parallel `MemRipRelSym` arms added to `encode_mov`, `encode_lea`, `encode_lgdt`, `encode_lidt`. Unblocks paideia-os syscall dispatch table (paideia-os#536) and all future function-pointer tables (VFS vops Phase 6, driver ops Phase 7, UEFI protocols Phase 12, WASM opcode dispatch Phase 10).
- **PA-R13-002** (issue #931) — REX.B on SIB base + dest ModR/M reg field (retires paideia-os#928). `emit_indexed_load` / `emit_indexed_store` were dropping REX.R (silently forcing dest to RAX) AND REX.B (aliasing r12→rsp, r13→rbp). Fixed both helpers with real dest/src param; W8 special case forces REX prefix for r8-r15 to select SPL/BPL/SIL/DIL over AH/CH/DH/BH. `encode_mov` SIB-load path redirected to pre-existing `mov_reg64_mem_sib_disp` (which already computed all four REX bits correctly). `encode_lea` SIB path refactored to delegate to `emit_mem_sib_disp` (made pub(crate)) for free RBP/R13 disp=0 escape. Retires the aspace_teardown 4-level SIB workaround from paideia-os R15.M1 (silent r12→rsp aliasing).
- **PA-R13-001b** (issue #1029, in-flight backtrack) — SIB base=RBP/R13 with disp=0 escape (Intel SDM Vol 2 §2.1.5 Table 2-3). Both new `mov_reg_mem_sib_disp_sized` from #930 AND pre-existing `mov_reg64_mem_sib_disp` / `mov_mem_sib_disp_reg64` / `emit_indexed_load` / `emit_indexed_store` were choosing mod=00 purely from `disp == 0`, without the RBP/R13 escape (which needs mod=01 with disp8=0 to distinguish from "no base, disp32 follows"). Centralized SIB tail into `emit_mem_sib_disp` helper; refactored 5 call sites. Filed by workerbee during #930 review; landed in-release before continuing (backtracking discipline).
- **PA-R13-001** (issue #930) — narrow-width `mov r8/r16/r32, [mem]` (retires paideia-os#927). Reuses existing `Mnemonic::MovSized{width}` (was imm-only) by extending its operand-shape ladder; no new IR variant. Elaborator retargets `Mov [Reg, MemSib]` to `MovSized{width}` when destination register name resolves to a narrow width via `register_name_width`. Four new encoder primitives: `mov_reg8_mem_base_disp` (8A /r), `mov_reg16_mem_base_disp` (66 8B /r), `mov_reg32_mem_base_disp` (8B /r, no REX.W), `mov_reg_mem_sib_disp_sized` (width-parameterized SIB). W64 delegates to existing helpers for byte-identical regression. AH/CH/DH/BH guard: EncodeError when W8 dest RegId 4-7 combined with REX-requiring memory. Adds T0533 diagnostic (warn-only in v0.13) for width mismatch. Retires the paideia-os masked-read workaround at 9+ sites in `src/kernel/core/loader/elf_lite.pdx`. 28 tests: 24 byte-exact + 4 iced round-trip.

## v0.12.0 — R13 encoder-gap batch (paideia-os SMP/security substrate)

**Released:** Tag pushed at PA-R13-011 closure (v0.12.0 release).

R13-cycle encoder additions and bug fixes commissioned by paideia-os R13 preflight and R14 kickoff. Nine encoder items landed:

  Encoders added:
    PA-R13-001 (#914)  ltr r16              — TSS load; unblocks ring-3 exception delivery.
    PA-R13-002 (#915)  [gs:.] / [fs:.]      — segment-prefix memory operand for per-CPU state.
    PA-R13-003 (#916)  xchg [mem], r64      — atomic exchange; spinlock acquire.
    PA-R13-004 (#917)  lock cmpxchg [m], r64— compare-and-swap; CAS-based spinlocks.
    PA-R13-005 (#918)  mfence               — memory ordering barrier; TLB shootdown handshake.
    PA-R13-007 (#920)  fxsave / fxrstor     — FPU state save/restore; context switch + signals.
    PA-R13-010 (#923)  sub r64, imm8/imm32  — immediate subtraction; retires add-r,0xFF...FF workaround.

  Bug fixes:
    PA-R13-008 (#921)  mutable-scalar literal → .data (was .rodata; would fault under SMEP + W^X).
    PA-R13-011 (#924)  back-to-back label aliasing (was silently dropping preceding label).

Also included: PA-R12-001..004, PA-R11-006, PA10-006w/x/y, PA-R10-001, #872 canary — post-v0.11.0 patches that were bundled with this release.

### Detailed bullets

- **PA-R13-008** (issue #921) — `pub let mut X : u64 = <literal>` now emits to `.data` instead of `.rodata`. Root cause: two branches in `crates/paideia-as/src/cmd_build.rs` (`IrKind::Literal` and `IrKind::ArrayLit`) unconditionally called `DataEntry::new_rodata(...)` without consulting `let_meta().mutable`, while the sibling `IrKind::StringLiteral` branch (added in PA-R12-001) already had the correct mutable-aware routing. Fix: gate DataEntry construction on `let_meta().mutable` in both branches, mirroring StringLiteral. Impact: mutable-scalar and mutable-array globals with literal initializers now land in a proper `.data` PT_LOAD segment; previously they were writable only because paideia-os's boot_stub identity map lacked W^X enforcement, and would have faulted once SMEP + W^X activated. Test coverage in `build_emit_pa_r13_008_mut_literal_data.rs`: `pub let mut u64 = 0` → `.data`, `pub let u64 = 0` → `.rodata` (no over-correction), `pub let mut [u64; N] = uninit` → `.bss` (Placeholder branch unaffected), `pub let mut [u64; N] = [...]` → `.data` (ArrayLit branch). Architectural smell called out for a follow-up: `EmitWalker::populate_data_table` implements the correct routing but is only referenced from unit tests; `cmd_build.rs` re-implements the walk inline and drifted. Recommend unifying in a follow-up.
- **PA-R13-011** (issue #924) — back-to-back labels in unsafe blocks now alias correctly to the next instruction. Before this fix, `label1: label2: mov ...;` silently dropped `label1` because the elaborator's Pass 2 stored the pending label in a scalar `Option<String>` and each new label declaration overwrote the previous one; `mov` only got `label2`, and any jump to `label1` emitted U1610 "unresolved label 'label1'". Fix: replaced the scalar with a `Vec<String>` "pending_labels" list; every label pushes; the next instruction consumes the whole list, aliasing all names to the same `IrNodeId` (→ same byte offset). Retires the paideia-os workaround pattern of duplicating identical `mov rax, 0; ret` blocks under each error tail label (see paideia-os #430 kind_process.pdx). Single-site fix in `crates/paideia-as-elaborator/src/unsafe_walker.rs` Pass 2 loop; parser, encoder, and downstream fixup passes untouched. Test coverage: unit tests verify Vec drain behavior (`pass_two_pending_labels_is_vec`, `pass_two_label_drain_consumes_all`, `pass_two_label_clear_on_encode_fail`); integration test at `tests/build-emit/back_to_back_labels.pdx` exercises the full parser → elaborator → encoder pipeline with two-label and three-label sequences.
- **PA-R13-010** (issue #923) — `sub r64, imm8` (REX.W `83 /5 ib`) and `sub r64, imm32` (REX.W `81 /5 id`) immediate forms added to encoder. Mirrors the existing `add r64, imm8`/`imm32` primitives at `encode.rs:1312`/`1326`; only delta is the ModR/M reg-field extension (`/0` → `/5`, base byte `0xC0` → `0xE8`). `encode_sub` gains the `[Reg, Imm64]` arm with the identical range-selection predicate as `encode_add`: values in -128..=127 take the 4-byte 8-bit form and record a tightening; values in i32::MIN..=i32::MAX take the 7-byte 32-bit form and record a tightening; values outside i32 return `Unsupported("64-bit immediate sub not yet supported")`. Register coverage: rax, rcx, r9, r12 (byte-exact) + iced-x86 round-trip. No new mnemonic variants — `Mnemonic::Sub` already carried arity-2 and 10-byte estimated size. Unblocks paideia-os R13 handler prologues that shrink RSP by small immediates (issue #430).
- **PA-R13-007** (issue #920) — `fxsave [base + disp]` and `fxrstor [base + disp]` (floating-point state save/restore) one-operand instructions added to encoder. Encoding: `0F AE /0` for fxsave, `0F AE /1` for fxrstor (9-byte upper bound). REX.B for r8–r15 base; no REX.W. Saves/restores x87/MMX/SSE state to/from 512-byte memory region. Mnemonic table entry added; IR arity 1; estimated size 9 bytes. Scheduling pass treats as Other (conservative FP state access). Test coverage: 12 byte-exact tests (fxsave: `[rdi]`, `[rdi+8]`, `[r8]`, `[rsp]`, `[rbp]`, `[r15+0x100]`; fxrstor: same 6 bases) + 2 iced-x86 round-trip tests + 1 negative shape test (rejects reg operand). Unblocks paideia-os R13 m2-002 FP state management in context save/restore.
- **PA-R13-005** (issue #918) — `mfence` (memory fence) zero-operand instruction added to encoder. Encoding: `0F AE F0` (3 bytes). Serializing memory barrier: all preceding loads and stores complete before subsequent ones. Mnemonic table entry added; IR arity 0; estimated size 3 bytes. Scheduling pass treats as Other (conservative barrier). Test coverage: 2 tests (byte-exact `0F AE F0` + iced-x86 round-trip). Unblocks paideia-os R13 m2-002 memory barrier support.
- **PA-R13-004** (issue #917) — `lock cmpxchg [base + disp], reg64` (locked compare-and-swap) two-operand instruction added to encoder. Encoding: `F0 REX.W 0F B1 /r` (10-byte upper bound). Compares implicit RAX with r/m64; if equal writes reg64, else loads r/m64 into RAX. Memory form requires LOCK prefix (F0); register-register form unsupported in R13. Mnemonic table entry added; IR arity 2; estimated size 10 bytes. Scheduling pass treats as Other (conservative atomic). Test coverage: 4 byte-exact tests (`[rdi], rcx`; `[rdi+8], r10`; `[r8], rcx`; `[rsp], rax`) + 1 iced-x86 round-trip verifying `has_lock_prefix()`. Unblocks paideia-os R13 m2-002 atomic compare-and-swap operations.
- **PA-R13-003** (issue #916) — `xchg [base + disp], reg64` (exchange register with memory) two-operand instruction added to encoder. Encoding: `REX.W 87 /r` (8-byte upper bound). Memory form is implicitly locked per Intel SDM Vol 2A — no explicit LOCK prefix required. Register-register form unsupported in R13. Mnemonic table entry added; IR arity 2; estimated size 8 bytes. Scheduling pass treats as Other (conservative atomic). Test coverage: 6 byte-exact tests (`[rdi], rax`; `[rdi+8], r10`; `[r8], rax`; `[rdi], r15`; `[rsp], rax`; `[rbp], rax`; all with SIB/BP escape handling) + 1 iced-x86 round-trip. Unblocks paideia-os R13 m2-002 atomic exchange operations.
- **PA-R13-002** (issue #915) — GS/FS-relative memory operands `[gs:...]` and `[fs:...]` with segment prefix emission (0x65/0x64). AST SegPrefix enum added; parser detects `gs:` or `fs:` token pair after `[` and captures segment. Elaborator translates AST SegPrefix to IR SegPrefix; wraps inner operand (MemSib, MemDisp, MemRipRel) in new Operand::MemSeg variant. Encoder pre-pass: finds MemSeg operand, emits prefix byte, unwraps inner, and shifts reloc/label offsets by +1. MemSeg pattern-matches applied conservatively (passthrough or skip) in analysis passes per softarch design. Rejects gs-relative symbol references (deferred). Test coverage (`crates/paideia-as-encoder/tests/gs_relative.rs`, 11 tests, byte-exact): `mov`/`lea` load and store forms with `[gs:reg+0]`, `[gs:reg+disp8]`, `[fs:reg+0]`, `[gs:reg+index*scale+disp]` (positive and negative disp, scale 2 and 4), REX.R+REX.B combined (`r10`/`r9`), and store direction (`[gs:reg+disp], reg`); 2 iced-x86 round-trip tests confirm `decoded.segment_prefix()` is `GS`/`FS`. Known gap (pre-existing, not introduced here): the disp32-only/no-base SIB form (`[gs:0]`, `[gs:absolute]`) is undeliverable because `Operand::MemSib` requires a base register and the no-base `Operand::MemDisp` variant has no `mov`/`lea` encoder support at all yet — tracked as a follow-up on `MemDisp` encoding, not part of this ticket.
- **PA-R13-001** (issue #914) — `ltr r16` (load task register) mnemonic added to encoder. Encoding: `0F 00 /3` per Intel SDM Vol 2A; REX.B (0x41) prepended for r8–r15; no REX.W. Register operand collapses onto `RegId(0..15)` via `register_name_to_regid`, so `ltr rax` / `ltr ax` / `ltr r10` all resolve identically and emit the 16-bit form the SDM mandates. Corrects the paideia-os R13 arch-pins audit's `/1` opcode extension and its `ltr r10 → 0F 00 D2` byte sequence (which would actually decode as `lldt dx`); correct `ltr r10` encoding is `41 0F 00 DA`. Register coverage: ax, cx, r8, r10, r15 (byte-exact + iced round-trip). Unblocks paideia-os #424 (TSS install + `ltr`).
- **PA-R12-004** (issue #913) — Pure function bodies containing `if`/`match`/`while`/`loop` expressions now fail elaboration with diagnostic T0532 instead of silently compiling to sequential `mov rax, imm64` instructions with no branching or `ret`. Root cause: `lower_ast_to_ir` does not transfer children for `ExprData::If` (emit_walker sees Branch nodes with 0 children); combined with a skeletal Branch lowering that assumes the condition is already in RAX, the emitter silently drops the fn body and points the symbol at unrelated module-level constant loads. Full Option A lowering (evaluate condition into RAX, emit test/jcc, walk arm bodies, merge into ret) is tracked as a follow-up on #913. Interim: wrap the body in `unsafe { block: { ... } }` (the pattern every existing paideia-os handler uses). Reported from paideia-os R12 m5-001 (paideia-os #412) via request_mmio_mapping compiling to sequential movabs causing infinite loop in cap_dispatch_smoke. **Superseded by PA-R17-012 (#990): T0532 stub retired; control-flow support in pure functions now available.**
- **PA10-006x** (issue #877) — `unsafe_walker::try_parse_symbol_memory` now packs an addend from all sum-of-terms shapes containing `rip`, not just `[rip + sym]`. Supported: `[rip + sym + N]`, `[rip + sym - N]`, `[rip + (sym + N)]`, `[(rip + sym) + N]`, and commuted `[N + rip + sym]`. Previously these all emitted `SymbolRef { addend: 0 }`, dropping the displacement silently. Encoder already honors `addend` (#876); no encoder change required. Addend overflow of `i32` is now detected and rejected instead of truncated.
- **PA10-006w** (issue #876) — `mov [rip + sym], r64` store form (REX.W 89 /r rip-relative). Symmetric to the existing `mov r64, [rip + sym]` load; opcode 0x8B → 0x89, REX.R still tracks the register operand (now the source). Emits R_X86_64_PC32 with -4 field bias. Paideia-os boot path previously worked around the missing form via `lea r_scratch, [rip + sym]; mov [r_scratch], src`; direct store now available. Register coverage: rax, rdi, r8 (byte-exact) + rax (iced-x86 roundtrip).
- **PA-R10-001** (issue #908, commit b1ecea7) — SIB+BP escape encoding bug fix. RSP base always emits SIB byte escape; RBP base with disp=0 forces mod=01/disp8=0. Reported from paideia-os R10 substrate audit.
- **PA-R12-002** (issue #911) — REX.X now emitted for r8-r15 as SIB index register in `mov r64, [base + index*scale]` (and 8/16/32-bit variants) and the symmetric store form. Bug: `emit_indexed_load` / `emit_indexed_store` hard-coded the REX prefix and dropped the SIB.index high bit, silently masking r8-r15 to r0-r7. Reported from paideia-os R12 m3-001 (paideia-os #408) via `cap_handler_page`'s `mov rax, [rsi + r9 * 8]` producing `48 8B 04 CE` instead of `4A 8B 04 CE`. REX.B for r8-r15 as base and REX.R for r8-r15 as dest in `emit_indexed_*` remain out of scope for this fix (tracked separately).
- **PA-R12-001** (issue #910) — `pub let X : [u8; N] = "string"` inside `module` now emits a `.rodata` symbol. Bug: cmd_build.rs's RHS data-table cascade handled Literal/ArrayLit/Placeholder branches but silently dropped StringLiteral, producing no symbol despite clean compilation. Reported from paideia-os R12 m1-002 (paideia-os #405) tags.pdx. Fallback B in paideia-os moved 6 tag strings into boot_stub.S; that fallback can now revert to inline .pdx form. Byte payload is truncated to declared N (if literal is longer) or zero-padded to N (if shorter). Immutable → .rodata; mutable → .data.
- **PA-R11-006** (issue #909) — `div r64` (REX.W F7 /6) and `idiv r64` (REX.W F7 /7) mnemonics added to the encoder surface. Bug: unsigned/signed 64-bit divide had no IR variant, no MNEMONIC_TABLE entry, and no encoder wrapper, so `unsafe { div rcx }` failed at elaboration. Defensive filing during R11 (hex TICK-counter workaround didn't need divide, non-blocking); paideia-os R12+ needs divide for decimal counters. Follows Not's REX.W F7 /N form. Register coverage: rax, rcx, rbx, r8, r15 (byte-exact + iced roundtrip).
- **Issue #872: pml4/pdpt symbol canary** — Cross-repo regression test for paideia-os boot_stub.S page-table symbol sizing. Test GAS-assembles boot_stub.S and validates `pml4` and `pdpt` symbols each reserve at least 4096 bytes via address arithmetic (symbol start to next symbol in section or section end). Uses address arithmetic rather than st_size because GAS `.skip N` emits st_size=0, making size unreliable. Canary guards against silent truncation of page-table declarations in boot-phase allocation. Test added to paideia_os_phase1_rebuild.rs.
- **PA10-006y** (issue #878) — Per-symbol alignment attribute `@align(N)` for item-level let declarations. Postfix syntax `let mut pml4 : [u64; 512] = uninit @align(4096)` drives explicit alignment on data symbols. Widened DataEntry.align from u8 to u32 to support alignment up to 2^30 bytes. Parser validates power-of-two and range [1, 2^30]; AST thread extracts align and seeds let_meta; routing blocks use explicit_align with sensible defaults (8 for Placeholder/Literal/ArrayLit, 1 for StringLit). Three new parser diagnostics: P0250 (unknown attribute), P0251 (malformed syntax), P0252 (invalid value). Unblocks paideia-os page-table symbol declaration with explicit 4096-byte alignment in boot_stub kernel.
- **PA-R12-003** (issue #912) — Hex literals with top bit set (values > 0x7FFF_FFFF_FFFF_FFFF) silently encoded as 0. `mov rax, 0xFFFFFFFFFFFFFFFD` now correctly emits `48 b8 fd ff ff ff ff ff ff ff` instead of `48 b8 00 00 00 00 00 00 00 00`. Root cause: both `unsafe_walker::extract_integer_from_span` and `cmd_build::parse_integer_literal` used `i64::from_str_radix`, which rejects values > i64::MAX. Both now use `u64::from_str_radix` for hex/oct/bin with bit-preserving `as i64` cast. Decimal parsing unchanged. Also replaces `-n` with `n.wrapping_neg()` on negative-decimal reconstruction to fix latent `i64::MIN` panic. Reported from paideia-os R12 m3-002 (paideia-os #409) via INVOKE_DENIED (0xFFFFFFFFFFFFFFFD) silently returning 0.

## v0.11.0 — Phase 15 m6 round: 32-bit mode encoding substrate complete (v1.5 closure)

**Released:** Tag pushed at m6-003 closure (v0.11.0 release).

paideia-as v1.5 round closes Phase 15 m6 feature work. Scope: 32-bit mode (Mode32) support across the encoder surface, enabling boot-stub x86 assembly to be targeted via `.pdx` substrate (deferred pending cross-module symbol export). Phase 15 m1–m6 implements real 32-bit instruction dispatch, symbol-relative memory operands with offsets, and supervisor-mode roundtrip verification.

### Milestones

- **PA15-m1-001 — bits surveyor** — Document 32-bit mode implications across elaborator/encoder/emitter; establish Mode16/Mode32/Mode64 enum in IR; lay ground for multi-mode instruction dispatch.
- **m2-001 — #![bits=N] inner attribute** — Module-level bits annotation; parser integration; per-module InstrMode enumeration + propagation to root Instruction IR nodes.
- **m2-002 — InstrMode field on Instruction** — Add InstrMode to Instruction IR type; encoders dispatch on InstrMode; m2-002a follow-up: scope-stack bits propagation (interim layering).
- **m3-001 — mode-aware mov r32 dispatch** — Real Mov r32, r32/imm32 forms with Mode32-aware encoding; operand width inference; register mapping updates.
- **m3-002 — mov r32, [abs32]** — Memory-source forms for 32-bit registers in Mode32; absolute addressing [abs32] support with relocation handling.
- **m3-003 — mov [abs32], imm32** — Memory-destination register encoding for Mode32; immediate-to-memory stores with width inference.
- **m3-004 — or r32, imm8/imm32** — Bitwise-or for 32-bit registers; sign-extended immediates; mode-dependent encoding variants.
- **m4-001 — lgdt [abs32]** — Descriptor-table load (supervisor mnemonic) with Mode32 addressing; GDT entry relocation support.
- **m4-002 — mode-agnostic supervisor verification** — 10-test corpus verifying 32-bit supervisor mnemonics (lgdt, lidt, ltr, movzx, etc.); cross-mode semantic equivalence checks.
- **m5-001 — symbolic ljmp Abs32 reloc** — Far-jump relocation for Mode32 selectors + offsets; PLT32 relocation in boot stubs; selector encoding.
- **m5-002 — mov sreg, r16** — Segment-register moves for 16-bit operands; instruction-form selection based on width; cross-module symbol reference plumbing.
- **m6-001a — [sym + N] parser/lowering** — Parse memory operand with symbol + offset; lower to MemoryOperand with symbol reference + displacement; elaborator symbol lookup + offset integration.
- **m6-001b — ljmp selector,offset parser tests** — Parser surface for ljmp with explicit selector,offset form; semantic lvalue dispatch; 6-test parser suite.
- **m6-001c — lea r32, sym Mode32** — LEA (load effective address) for Mode32 registers with symbol references; relocation table output for symbol + offset.
- **m6-001d — [sym + N] non-rip case integration** — Memory operands with symbol + offset outside RIP-relative context; full Mode32 integration; mode-specific relocation branches.
- **m6-001e — or r32, imm32 + mov [abs] imm32 sign-bit-set fix** — Fix sign-extension trap in imm32 encoding for Mode32 or-immediate and mov-to-memory immediate; regress test suite updates.

### Highlights

- **3119 workspace tests** (+215 from v0.10.0 baseline at 2904; all-green; no regression).
- **32-bit mode substrate complete**: all Mode32 instruction forms, addressing modes, and relocation paths now present in the encoder. Boot stub assembly can be written in `.pdx` targeting Mode32.
- **Supervisor mnemonic verification**: 10-test corpus validates lgdt, lidt, ltr, movzx, and other privileged instructions in mode-agnostic contexts.
- **Symbol-relative memory addressing**: [sym + offset] form works across Mode16/Mode32/Mode64 with proper displacement encoding and relocation generation.
- **Far-jump relocation ready**: ljmp selector,offset ready for cross-module symbol reference + selector encoding (boot stub entry points).
- **PaideiaOS B2 → B3 unblock**: boot_stub.S migration to `.pdx` Mode32 surface **deferred to v0.12.0** pending:
  - Cross-module symbol export (issue #900, PA16 carryover).
  - Elaborator U1606 fix for symbol-offset lookup in non-module contexts (issue #871, PA16 carryover).

### Operational deferrals (Phase 15+ carryover)

- **Boot stub migration to .pdx**: m6-002 deferral → v0.12.0. Blocks on #900 (cross-module symbol export) and #871 (elaborator U1606 symbol resolution).
- **Nested scope bits propagation**: m2-002a interim layering remains pending full nested scope propagation (deferred Phase 16+).

## v0.10.0 — PA10 PVH/string-literals/bitwise-arith/narrow-Mov closure (PA10-001..006 substrate unblocked)

**Released:** Tag pushed at PA10-006 closure (v0.10.0 release).

paideia-as PA10 v0.10 round closes phase 10 feature work. Scope: PVH ELF note generation for QEMU -kernel acceptance (PA10-001); string literal lowering to .rodata (PA10-002); bitwise arithmetic encoders for AND/OR/XOR (PA10-003); narrow-form Mov instructions for r8-imm and r16-imm (PA10-004); nested let-of-Var in deep block bodies (PA10-005); end-to-end closure fixture demonstrating all features compose (PA10-006 + PA10-006a-l recovery commits).

### Milestones

- **PA10-001 — PVH ELF note** — Multiboot2/PVH note generation for QEMU -kernel; marks kernel-main as PVH-compatible.
- **PA10-002 — string literals** — String literal lowering to .rodata; FNV-1a interning; R_X86_64_64 relocations for references.
- **PA10-003 — bitwise arithmetic** — Real Imul/And/Or/Xor encoders with reg-reg, reg-imm8, reg-imm32 forms; sign-extension trap guards.
- **PA10-004 — narrow Mov** — Mov instructions for r8-imm8, r16-imm16 with high-byte register support.
- **PA10-005 — nested let-of-Var** — Scope stack with flat fallback for variable lookup in deep block bodies.
- **PA10-006 — closure fixture** — boot_observable.pdx integration test exercising PA10-001..005 end-to-end; qemu-smoke conditional on tool availability.
- **PA10-006a-l — recovery commits** — ljmp immediate/two-operand, RIP-relative addressing, I/O port mnemonics, integer literal immediates.

### Highlights

- **2991 workspace tests** (+134 from v0.9.0 baseline at 2857; all-green).
- **PVH note infrastructure complete**: kernel emitted as PVH-Note ELF; QEMU -kernel acceptance path validated.
- **PA10-001..005 composed end-to-end**: boot_observable.pdx fixture demonstrates all features working together; disasm validation confirms narrow Mov, AND/OR/XOR, control flow present.
- **String literal IR infrastructure**: complete pipeline from parser → elaborator → emit, with deduplication and relocation generation.
- **Bitwise operation encoders**: full reg-reg/reg-imm8/reg-imm32 coverage with byte-exact test vectors.
- **Recovery commits PA10-006a-l**: ljmp selector:offset, [rip + symbol] addressing, in_al/out_al mnemonics, integer immediates in operands.
- **PaideiaOS R8 substrate unblocked**: kernel can now boot via QEMU -kernel, output observable via serial, and leverage bitwise operations for low-level manipulation.

### Operational deferrals (Phase 10+ carryover)

- **Full QEMU smoke validation**: boot_observable smoke test conditional on qemu-system-x86_64 + ld; linker script format validation deferred.
- **String literal expression syntax**: parser support for string/byte literals in top-level let-bindings (phase 11+).
- **Module-level string constants**: require module-language const elaboration (phase 11+).

## v0.9.0 — Phase 9 m1–m3 substrate closure (bare-if/nested-ArrayRepeat/SIB encoder + full paideia-os unquarantine)

**Released:** Tag pushed at m3-003 closure (v0.9.0 release).

paideia-as PA9 v0.9 round closes 3 issues across m1–m3. Scope: gap fixes to bare-if (no-else), nested ArrayRepeat, general SIB MOV encoder; 9 paideia-os checkpoint-2 kernel files unquarantined; and 5-file rewrite campaign on paideia-os side. First complete Phase-2-capability-system + Phase-3-IPC kernel build.

### Milestones

- **m1 — substrate fixes** — PA9-m1-001: Bare-if without else arm (single-path control flow); PA9-m1-002: nested ArrayRepeat in IR elaboration (multi-level array init); PA9-m1-003: general SIB form encoder for [base + index*scale + disp] addressing modes.
- **m2 — paideia-os rewrite campaign** — 5 checkpoint-2 kernel file rewrites to native paideia-as; removal of legacy syntax workarounds; cross-file consistency audit.
- **m3 — unquarantine + cleanup** — Restore 9 quarantined paideia-os kernel files (.quarantine/src/kernel/* → src/kernel/*); workspace test regen (2834 → 2857+ tests); version bump 0.8.0 → 0.9.0; phase-transition-9.md retrospective.

### Highlights

- **2857+ workspace tests** (+23 from v0.8.0 baseline; all-green including paideia-os checkpoint-2 fixtures).
- **9 paideia-os files unquarantined**: checkpoint-2 kernel build now produces clean kernel.elf (44864 bytes) with full Phase-2 + Phase-3 structures.
- **SIB encoder complete**: general x86-64 addressing [base + index*scale + disp] now supported; enables complex memory operands in kernel code.
- **Bare-if control flow**: enables simpler kernel control structures without forced else-arm workarounds.
- **Cross-repo verification**: kernel.elf end-to-end smoke test passed with Phase-2 capability system + Phase-3 IPC messaging in place.

### Operational deferrals (Phase 9 m4+ carryover)

- **paideia-os R6.5 IRQ subsystem**: resumed after Phase 9 m3 close.
- **paideia-os D7 driver backlog**: resumed after Phase 9 m3 close.

## v0.8.0 — Phase 8 checkpoint (elaborator gap closure + regression verification)

**Released:** Tag pushed at m7-003 closure (v0.8.0 release).

paideia-as PA8 v0.8 round closes 5 issues across m6–m7. Scope: regression verification (v0.7→v0.8 semantic surface), elaborator gap closure (if-as-tail, array/record literals, cast operator, unsafe blocks, supervisor mnemonics, memory operands), and end-to-end orchestration fixture.

### Milestones

- **m6-001 — debug-trace gating** — Gate 34 eprintln! debug traces in EmitWalker behind cfg(debug_assertions) for clean release builds.
- **m6-002 — diagnostics audit** — Add B1704 catalog entry (function symbol missing offset); regenerate SARIF snapshot; verify all PA8-added diagnostic codes (T0526–T0528, B1702–B1704) present in catalog.
- **m7-001 — checkpoint2 orchestration** — Write comprehensive .pdx fixture (checkpoint2_orchestration.pdx) exercising V2–V11 (m2–m5 milestones) in single cohesive module; add integration test validating structure.
- **m7-002 — kernel unquarantine attempt** — Attempt unquarantine of 9 paideia-os kernel files (.quarantine/src/kernel/*); document that all remain blocked on Module-language support (Phase 9+); defer to future phases.
- **m7-003 — closure ceremony** — Bump workspace.version 0.7.0 → 0.8.0; append v0.8.0 to CHANGELOG; write phase-transition-8.md retrospective (~150 lines); regenerate SARIF snapshot; git tag v0.8.0; bump paideia-os submodule.

### Highlights

- **2483 workspace tests** (same baseline as m6 start; no regression, all-green).
- **Regression verification clean**: v0.7→v0.8 elaborator surface passes all existing test suites.
- **Checkpoint 2 fixture complete**: end-to-end orchestration covering all m2–m5 features ready for Phase 9 continued elaboration.
- **Kernel unquarantine deferred**: 9 files blocked on Module language; cross-filed for Phase 9+ follow-up.
- **Diagnostics surface hardened**: all new Phase 8 codes catalogued and SARIF-compliant.

### Operational deferrals (Phase 9 carryover)

- **Kernel checkpoint 2**: Full unquarantine requires Module-language functor/signature support (Phase 9 m3–m4).
- **Memory-operand general form**: Phase 8 m5-002 covers [base + disp]; [base + index*scale + disp] and RIP-relative deferred.
- **String literals, multiboot2 notes**: Deferred to Phase 9+ per design roadmap.

## v0.7.0 — Phase 7 completion (elaborator/encoder surface for PaideiaOS Phase-2)

**Released:** Tag pushed at m6-004 closure (v0.7.0 release).

paideia-as PA7-completion round closes 20 issues across 6 milestones (m1–m6). Scope: implement missing elaborator/encoder surface to accept real PaideiaOS kernel code (checkpoint 1 unquarantine) and prepare for checkpoint 2 (capability/IPC/scheduling structures).

### Milestones

- **m1 — symbol export + PLT32** — unsafe_exported_fn IR node; PLT32 relocation off-by-one fix; symbol export parser/encoder closure. Enables PaideiaOS checkpoint 1 boot-layer unquarantine (4 G2-blocked files).
- **m2 — operand resolution** — unsafe-body IR lowering; Let-literal scratch binding; Operand::Var structural resolution; PaideiaOS R1.5/R2.5 four-file rebuild regression suite.
- **m3 — parser quality** — free `handle` identifier; optional arrow in fn-literals; unit-typed block trailing `;` support.
- **m4 — expression surface** — bitwise NOT prefix operator; EXPR as TYPE cast syntax; width-threaded integer literals; iced-x86 cast/arith round-trip witness.
- **m5 — l-value assignment** — pointer-deref l-values (`*p = expr`); field l-values (`(*p).f = expr`) via chained Deref IR.
- **m6 — round closure** — PaideiaOS boot_orchestration_v2 integration smoke test; PA7-completion verification script; phase-transition-7.md retrospective; v0.7.0 tag + submodule bump.

### Highlights

- **2760+ workspace tests** (+109 from v0.6.0 at 2651, +4.1%).
- **Checkpoint 1 unquarantined**: 4 PaideiaOS G2-blocked files now build cleanly.
- **Elaborator/encoder milestones complete**: symbol export, unsafe blocks, operand binding, l-value assignment all realized.
- **Checkpoint 2 awaiting**: 9 PaideiaOS capability/IPC/scheduler files remain quarantined; require unit-block-expr and module-level-const elaboration (Phase 8).
- 7 new diagnostic codes: P0158, T0527, P0101.
- Integration with PaideiaOS stabilized via tools/paideia-as submodule pin + smoke test gate.

### Operational deferrals (Phase 8+ carryover)

- **G11–G15**: Supervisor mnemonics, memory operand general form, array initializers, string literals, Multiboot2 ELF Note generation. Documented in design/DESIGN.md roadmap.
- **Checkpoint 2 elaboration**: Unit-typed blocks with if-statement-as-final-expression (emit_block_body Branch handling); module-level constant syntax/elaboration.

## v0.6.0 — Phase 6 (build-emit surface expansion + self-hosting groundwork)

**Released:** Tag pushed at m7-003 closure (this PR).

paideia-as Phase 6 closes 7 milestones across 37 issues, PRs #737–#776. Scope: (1) activate build-emit surface beyond Phase 5's narrow (paideia-os Phase-1 boot code) scope to reach full-program codegen; (2) begin Tier 1 self-hosting crate ports to `.pdx` and prove cross-compile infrastructure. Cross-repo escalation from paideia-os Phase 2 continued unbroken per `feedback_phase6_to_paideia_os_resume.md`.

### Milestones

- **m1 — records + lowering** — struct field access + RecordLayoutTable codegen; record-expression lowering; record-pattern binding; field-access lvalue contexts; EmitWalker record-cons visitor + cmd_build wiring; corpus regression tests.
- **m2 — generics + monomorphisation** — generic-type parameter real lowering; monomorphisation table walk-time codegen; multi-instance struct vs single monomorphic version; generic-trait associated-type scaffolding.
- **m3 — struct walker + traits** — struct-field-walker activation pipeline; trait-method codegen stubs (activation deferred to Phase 7); trait-object placeholder codegen; call-site trait-bound resolution wiring.
- **m4 — control-flow encoders** — branch-condition real rewrites (phase 3 m3-001 upgraded); match-discriminant encoder phase; loop-unroll real rewrite; break / continue target-tracking + stack unwinding.
- **m5 — static-data triple (.text / .rodata / .data / .bss)** — .bss section uninitialized-static codegen; array-literal type-inference (.rodata vs .data); cross-section linking (PC32 + GOT); static-initialiser evaluation frame.
- **m6 — end-to-end smoke (paideia-os Phase-2 unblock)** — cap_smoke.pdx fixture + boot-header multiboot2; 18 paideia-os boot files build verification; runtime cap_smoke.link.ld + tools/run-cap-smoke.sh driver; byte-sequence assertion + reloc-table verification; workspace test total 2619+ (QEMU smoke pending paideia-os integration).
- **m7 — documentation + closure** — phase-transition-6.md retrospective; STATUS.md m1–m7 closure; this v0.6.0 tag + CHANGELOG; phase-6-decision-gate-g8.md documenting Phase 7 entry criteria (self-hosting prerequisites).

### Highlights

- **2619 workspace tests** across the workspace (+203 from Phase 5 close at 2416).
- **Full build-emit surface** now complete: records, generics, traits, borrowed-refs, stdlib types (String / Vec / Option / Result) all lower to machine code.
- **18 paideia-os boot files build cleanly**: multiboot2 headers, GDT loaders, interrupt stubs — all verified byte-sequence + relocation table. Execution gated by paideia-os Phase 2 QEMU integration.
- **Tier 1 self-hosting proof-of-concept validated**: paideia-as-lexer partial port + paideia-as-parser bootstrap fixture demonstrate `.pdx` can express all required AST + type structures. No architectural surprises.
- 6 new diagnostic codes: P0220, P0221 (generic resolution); T0513–T0518 (trait resolution); U1607–U1611 (unsafe-walker phase-5 deferrals).
- 3 new GitHub labels: `phase:6`, `area:walker-activation`, `area:bug-fix-from-paideia-os`.
- Cross-repo escalation from paideia-os Phase 2 maintained unbroken; one early blocker (cap_smoke multiboot2 header format) resolved m6, no others reached m7.

### Operational deferrals (Phase 7 carryover)

- **Full Tier 1 self-host ports** — paideia-as-lexer, paideia-as-ast, paideia-as-parser, paideia-as-diagnostics complete porting Phase 7 m1+. Tier 2/3 follow Phase 7+ per `rust-dep-gap-analysis.md`.
- **The originally-planned Phase 5 self-hosting work** — 5 stdlib expansions (SmallVec, Unicode XID, serde-family, BLAKE3, Lru) + Tier 1-3 paideia-as port to `.pdx`. All ship Phase 7+ per `phase-6-decision-gate-g8.md` prerequisites.
- **Associated-type codegen** — trait-method resolution per impl block deferred. Phase 7+.
- **Full const-generics** — const `N: usize` lowering; Phase 7+.
- **Curried multi-arg lambda eta-reduction** — Phase 7+.
- **LEA symbolref direct RIP-relative encoding** — Phase 7+ optimisation.
- **paideia-lsp + paideia-pq-sign self-hosting** — async runtime decisions deferred. Phase 7+.
- **NIST ACVP test vectors for ML-DSA-65** — gates on upstream `ml-dsa` crate; stays open.
- **Stage-0b GAS AT&T-syntax variants** — Phase 7+.

See `design/toolchain/phase-transition-6.md` for the full retrospective and Phase-7 carryover catalogue. `phase-6-decision-gate-g8.md` documents Phase 7 entry checkpoint: all stdlib expansions must ship + Tier 1 architectural feasibility must be GREEN before Phase 7 starts.

---

## v0.5.0 — Phase 5 (build-emit activation; paideia-os Phase-1 unblock)

**Released:** Tag pushed at m7-003 closure (this PR).

paideia-as Phase 5 closes 7 milestones across 38 issues, PRs #695–#733. Scope: make `paideia-as build --emit elf64` produce real machine code from `.pdx` source, enough to unblock paideia-os Phase-1 kernel bring-up. The originally-planned Phase 5 (self-hosting) shifts to Phase 6+.

This Phase was a cross-repo escalation response: paideia-os Phase-1 work on 2026-06-20 surfaced that `paideia-as build` was emitting a fixed placeholder (`lea 0x1(%rdi), %rax; ret`) regardless of source content. Phase 5 wired the full EmitWalker → UnsafeWalker → InstructionSideTable → emit chain so user code reaches the binary.

### Milestones

- **m1 — elaborator: real per-construct lowering** — EmitWalker skeleton + per-construct visitors for Let(Literal) and Lambda body (identity / double / add-immediate) + Unsafe delegation + cmd_build chain.
- **m2 — encoder: boot intrinsics** — 20 new x86_64 mnemonics with encoders covering all PaideiaOS Phase-1 needs: control-flow (cli / sti / hlt / nop / swapgs / cpuid), I/O ports (in/out × 3 widths), MSRs (wrmsr / rdmsr / int), CR0-4 + CR8 moves, DR0-7 moves, descriptor-table loads (lgdt / lidt), returns (iret / iretq / sysret), rep stosq, far-jmp m16:64.
- **m3 — unsafe-block payload walker** — IrKind::RawInstruction preserving AST back-pointer; operand parser for register names + memory references + immediates; mnemonic-name resolver (30+ mnemonics); UnsafeWalker::run consuming pending blocks; cmd_build wiring after EmitWalker. New diagnostics U1605 (unknown mnemonic) + U1606 (malformed operand).
- **m4 — initialised static-data surface** — `[T; N]` array type parsing; `[expr, expr, ...]` array literals; DataSideTable + `.rodata` / `.data` section population; R_X86_64_PC32 relocation linking. New diagnostic P0210 (empty array needs type annotation).
- **m5 — symbol export + relocations** — top-level binding SymbolTable; `Operand::SymbolRef { name, addend }` + `RelocSite` + `EncodeOutput`; real symbol-table emission with proper STT_FUNC / STT_OBJECT / STB_GLOBAL bindings; undefined-symbol entries for cross-file references; real `.text` from InstructionSideTable iteration (`lower_add_one` placeholder finally killed).
- **m6 — end-to-end smoke (paideia-os Phase-1 unblock)** — uart_smoke.pdx fixture; link.ld + run-smoke.sh driver; byte-sequence assertion test (+ fixes UnsafeWalker bug that was processing only the first instruction per block); QEMU smoke under cargo test gated by qemu availability; **add_one byte-identical regression** — the closure marker for the paideia-os Phase-1 unblock, with 4 separate chain bugs fixed in lower.rs / emit_walker.rs / cmd_build.rs / encode_instruction.rs.
- **m7 — documentation + closure** — phase-transition-5.md retrospective; STATUS.md update; this v0.5.0 tag + CHANGELOG; examples build-clean parity.

### Highlights

- **2416 workspace tests** across the workspace (+244 from Phase 4 close at 2172).
- **paideia-os Phase-1 unblocked**: `cargo test -p paideia-as --test build_emit_smoke add_one_byte_identical` is the closure marker. All three lambda shapes lower to byte-identical x86_64:
  - `fn (x) -> x` → `48 89 F8 C3` (mov rax, rdi; ret).
  - `fn (x) -> x + 1` → `48 8D 47 01 C3` (lea rax, [rdi + 1]; ret).
  - `fn (x) -> x + x` → `48 8D 04 3F C3` (lea rax, [rdi + rdi]; ret).
- 4 new diagnostic codes: U1605, U1606, P0210, M0305 enforcement.
- 4 new GitHub labels: `phase:5`, `gated:downstream-paideia-os`, `area:emit-activation`, `area:boot-intrinsics`.
- Continuous-tempo loop (no per-milestone pause) executed cleanly across 7 milestones.

### Operational deferrals (Phase 6+ carryover)

- **The originally-planned Phase 5 self-hosting work**: 5 stdlib expansions (SmallVec, Unicode XID, serde-family, BLAKE3, Lru) + Tier 1-3 paideia-as port to `.pdx`. All shifts to Phase 6+.
- **Surface lowering for records / generics / traits / borrowed-refs / stdlib types**: still placeholder in `paideia-as build` for these. Phase 5 was scoped narrowly to paideia-os Phase-1 needs (let / fn / lambda / unsafe / *T). Phase 6+ activates the rest.
- **Full m1-003 lambda body shapes**: covers identity, double, add-immediate. Curried 2-arg `add l r → l + r` not yet lowered. Phase 6+.
- **General RIP-relative addressing**: only for far-jmp m16:64 (one mnemonic). General-case `mov rax, [rip + symbol]` works via SymbolRef but conservatively encoded. Phase 6+.
- **paideia-lsp + paideia-pq-sign self-hosting**: Phase 6+ (async runtime + crypto crate decisions).
- **NIST ACVP test vectors for ML-DSA-65**: gates on upstream `ml-dsa` crate; stays open.
- **Stage-0b GAS AT&T-syntax variants**: still `.intel_syntax noprefix` only.

See `design/toolchain/phase-transition-5.md` for the full retrospective and Phase-6 carryover catalogue.

---

## v0.4.0 — Phase 4 (substrate expansion for PaideiaOS readiness)

**Released:** Tag pushed at m14-003 closure (this PR).

paideia-as Phase 4 closes fourteen milestones across 101 enumerated issues, PRs #592–#693. PaideiaOS-aware re-ordering applied: m7 → m9 → m10 → m8 → m11 → m1 → m2 → m3 → m4 → m5 → m6 → m12 → m13 → m14.

### Milestones

- **m7 — records + enums** — `struct` types with layout (RecordLayoutTable); pattern bindings + P0199 (refutable-let); record codegen; `enum` sum types with 3 payload shapes (EnumLayoutTable); match exhaustiveness T0512; enum discriminant + match codegen; RecordCons / FieldAccess / EnumCons / EnumDiscriminant IR; corpus regression. Closes records / enums for PaideiaOS kernel data structures.
- **m9 — generics + traits** — `<T>` grammar (P0200); Type::Var with HrKind::Star / Arrow; trait declarations (P0201) + impl blocks (P0202); trait-bound resolution (T0514); coherence (T0513); monomorphisation table; associated types; derive-macro infrastructure (Eq / Hash / Debug). Closes parametric polymorphism for stdlib + PaideiaOS subsystem reuse.
- **m10 — allocator + memory model** — Allocator trait + Layout; BumpAllocator; Arena; SystemAllocator with C1401/C1402 cfg-gates; Box<T>. Q3 dual-default resolved: Arena for PaideiaOS targets, SystemAllocator for host. Closes allocation discipline for kernel-vs-host context.
- **m8 — strings + loops** — string + byte-string literals (E0010/E0011); Type::Str fat pointer; heap String; for / while / loop / break / continue keywords; Loop / Break / Continue IR + LoopMetaTable; m3-006 unroll over explicit loops. Closes the control-flow + text substrate.
- **m11 — stdlib bring-up** — Option / Result / Vec / String + Str ops / HashMap / Stdin/Stdout/Stderr (IO effect + paideia.io capability) / File + Read + Write traits / Iterator + Map/Filter adapters; 135-LoC stdlib-smoke kitchen-sink. Closes the runtime-library surface.
- **m1 — walker hookups** — Call / Match / Handle / Branch walker surfaces; PositionIndex + NameResolutionTable population; macro-fusion / branch-hint / align / pool-constants 4-pass would-fire-to-real-rewrite flip. Closes the Phase 3 m3-007 deferral chain.
- **m2 — encoder real-rewrites** — PE/COFF + DWARF + PAX emitters consume InstructionSideTable; per-emit DDC fixture. Closes Phase-2-m9 honesty-disclaimer chain.
- **m3 — runtime integrations** — real cryptoki PKCS#11 + yubihsm runtime integration; reqwest RFC 3161 TSA fetch (`verify --tsa-token`); hardware-lane activation guide. Closes Phase-3-m6 runtime-deferral.
- **m4 — borrowed references grammar** — `&T` / `&mut T` types + expressions; Type::Ref interner; substructural Affine/Linear; IR Borrow / BorrowMut / Deref + BorrowSideTable; codegen as pointers.
- **m5 — region calculus** — RegionId + RegionGraph + transitive closure; lexical region inference; lifetime-variable surface syntax; per-binding region metadata in PositionIndex; Rust-style elision rules + L2001.
- **m6 — borrow checker** — BorrowWalker (S0906/S0907, renamed from spec'd A0700/A0701), LifetimeWalker (S0908, was A0702), MutationWalker (S0909, was A0703); two-phase borrows for method receivers; NLL precise drop + LastUseAnalyzer; ExtendedBorrowDiagnostic with SARIF relatedLocations; 40-fixture corpus. Closes safe-aliasing discipline for PaideiaOS kernel code.
- **m12 — paideia-as tooling** — `paideia-as test` runner (discovery + listing; execution gates on Phase 5 runtime evaluator); `paideia-as fmt` CLI (file / stdin / --check); `paideia-as doc` HTML generator with cross-reference linking. Package manager deferred to Phase 5+.
- **m13 — self-hosting groundwork** — port-target inventory (21 crates, 3 tiers); m13-002 mini-lexer bootstrap fixture in tests/self-hosting/; Rust-dep gap analysis (10 stdlib expansions identified — SmallVec, Unicode XID, serde/serde_json/toml, BLAKE3, Lru, etc.); stage-1 + DDC fixture; Phase 5 opening conditions.
- **m14 — documentation closure** — phase-transition-4.md retrospective; STATUS.md update; this v0.4.0 tag + CHANGELOG; examples README + stdlib walkthrough refresh.

### Highlights

- **2172 workspace tests** across 29+ crates and 26+ test harnesses (+343 from Phase 3 close at 1829).
- Full borrowed-reference + region + borrow-checker stack ships — paideia-as has a Rust-equivalent safe-aliasing story for PaideiaOS subsystem code.
- Stdlib bring-up (Option / Result / Vec / String / HashMap / Iterator + IO traits) is sufficient for kernel scaffolding and self-host bring-up.
- 18 new diagnostic codes (P0196..P0202, T0511..T0514, S0906..S0909, L2001, C1401..C1402, E0010..E0011, M0900) — every code in its category's reserved range.
- PaideiaOS-mode (no PR / direct-push) workflow eliminated ~50 issues of PR-overhead while keeping the cargo-green gate.
- 20-example tutorial-ordered catalog under `examples/` rewritten mid-Phase to reflect current syntax.
- Self-hosting groundwork: inventory + gap analysis + bootstrap fixture + DDC harness in place for Phase 5 m1 kickoff.

### Operational deferrals (Phase 5 carryover)

- Walker chain activation: per-walker activation in the full elaborator IR walk (m1-005..006 / m6-001..003 walkers are unit-tested but not yet activated globally; lands incrementally as the elaborator threads them).
- CLI parser consolidation: drop the older `-> ret !{Eff}` form OR migrate to the newer `-!{Eff}->` form (lex/parser layer drift exposed by examples rewrite).
- paideia-as build end-to-end for the new surface (gates on walker activation).
- L2001 elision-rule per-fn-signature activation.
- TSA token attachment as .paideia.sig sub-record (Phase 3 m8-001 scaffolded; m4 emit-stage not threaded).
- `record` vs `struct` keyword pick + migration.
- Test execution via Phase 5 runtime evaluator (m12-001 discovers; execution gates on m13).
- 5 stdlib expansions before Tier 1 self-host port: SmallVec, Unicode XID tables, serde-equivalent, BLAKE3, Lru cache.
- paideia-lsp + paideia-pq-sign self-hosting → Phase 6+ (async runtime + crypto crate decisions deferred).
- NIST ACVP test vectors for ML-DSA-65 (#525 stays open per its AC; gates on upstream ml-dsa crate).
- Stage-0b GAS AT&T-syntax variants (current: .intel_syntax noprefix only).

See `design/toolchain/phase-transition-4.md` for the full retrospective + Phase 5 carryover catalogue.

---

## v0.3.0 — Phase 3 (substrate-deferral cleanup)

**Released:** Tag pushed at m9-004 closure (this PR).

paideia-as Phase 3 closes nine milestones across 56 enumerated issues (plus 3 cross-cutting). Issue #525 (NIST ACVP test vectors for ML-DSA-65) intentionally stays open per its own AC pending upstream `ml-dsa` crate support. PRs #475–#589.

### Milestones

- **m1 — pointer types + raw memory** (PRs #475–#487) — `*T` in the type grammar; `index_*` + `ptr_sub*` intrinsic families (40 entries); `RawMem` effect + built-in `paideia.raw_mem` capability; `IrKind::Load`/`Store` + `LoadStoreSideTable`; SIB-form encoder (`48 8b 04 cf` etc.); examples 15/16/17 retired to compiles-end-to-end status. Closes the Phase 2 §15 borrowed-references open question with the documented deferral.
- **m2 — per-node IR payload** (PRs #488–#493) — `Instruction { mnemonic, operands, encoding_hint }` schema + `InstructionSideTable` keyed by `IrNodeId` (mirrors m3-007 `HandlerSideTable` / m1-006 `LoadStoreSideTable`); `encode_instruction` dispatch entry with iced-x86 round-trip tests; elaborator populate chokepoint; opt-pass helper signatures migrated to consume `&InstructionSideTable`.
- **m3 — opt-pass real-rewrites** (PRs #494–#502) — 5 passes ship real rewrites: peephole (5/8 rules), schedule, dse, encode-tight, tailcall (structural). 4 passes ship as documented would-fire pending m4 encoder/emit-stage integration: macro-fusion, branch-hint, align, pool-constants. Per-pass regression corpus `tests/opt-regression/`.
- **m4 — elaborator-driven LSP** (PRs #503–#509) — `PositionIndex` + `NameResolutionTable` side-tables; lookup paths wired through hover, definition, references, completion, inlay-hints handlers. m8-014 latency probe reactivated. Per-walker population (the insert side) lands incrementally as the walkers grow.
- **m5 — stage-0b GAS source** (PRs #569–#571) — `src/toolchain/stage-0/entrypoint.s` (`.intel_syntax noprefix`) is 1:1 with the NASM stage-0a entry-point; `tools/ddc/run.sh` byte-compares both `.text` sections (verified locally: `48 8d 47 01 c3`). `docs/g4-prep.md` §5 Stage-0b row flips checked. Activates the dual stage-0 Wheeler-CTTTDC argument.
- **m6 — hardware HSM integration** (PRs #572–#576) — `Pkcs11Signer` (cryptoki backend), `YubiHsmSigner` (with the hybrid-fallback rule for YubiHSM2's missing PQ firmware), `HybridSigner<H, S>` composer, `HsmSigner` trait with `is_hardware()`, `Q0902 hsm-no-pq-support` diagnostic, hardware-lane test corpus (`#[ignore]`'d). Runtime crate integrations (real cryptoki / yubihsm sessions) deferred.
- **m7 — substructural + effects cleanup** (PRs #577–#582) — S0902 (linear let-shadow), S0904 (affine consumed across match arms), S0905 (ordered out of order across handler) wired with detection logic + reject corpus fixtures. Row-polymorphic scope subsumption (`check_scope_subsumption_with_row_poly`) closes the m7-004 D-row from `phase-transition-2.md` §1.
- **m8 — signature lifecycle** (PRs #583–#586) — RFC 3161 timestamping client (`paideia-pq-sign::timestamp` + CLI subcommand); JSON-lines revocation list + `verify --revocation-list --ignore-revocation`. #525 (NIST ACVP test vectors) stays open per its own AC; upstream tracking in `tests/pq-corpus/ML_DSA_ACVP_STATUS.md`.
- **m9 — documentation closure** (PRs #587–#590) — `design/toolchain/phase-transition-3.md` retrospective; STATUS.md update; examples README refresh; this v0.3.0 tag.

### Highlights

- **1829 workspace tests** across 27+ crates and 24+ test harnesses (+215 from Phase 2 close at 1614).
- 12 design-clarification deferrals resolved; 2 stay deferred; 2 scope-changed; 2 resolved-with-gating-note.
- The dual stage-0 Wheeler-CTTTDC argument has both legs operational; `tools/ddc/run.sh` byte-compares.
- Hybrid PQ signing gains lifecycle handles (timestamping + revocation) and hardware HSM backends.
- Pointer types retire the most common Phase 2 `unsafe` wrappers; example 17_strlen has zero unsafe escapes.

### Operational deferrals (Phase 4 carryover)

- **Walker-side IR insert points**: PositionIndex / NameResolutionTable / Instruction populate paths cover the m2-003 minimum (Load / Store) but not the full IR-kind tree. Per-walker inserts during linearity / effect / capability walks land at Phase 4.
- **Real cryptoki / yubihsm runtime integrations**: m6 ships the scaffolds + the `HsmSigner` trait; live-device exercise needs the runtime crates plus operator validation.
- **Macro-fusion / branch-hint / align / pool-constants real rewrites**: ship as would-fire; activation lands at the m4 encoder/emit-stage integration.
- **RFC 3161 TSA HTTP fetch**: m8-001 ships synthetic-token scaffold; real fetch needs `reqwest`.
- **GitHub Actions billing restoration**: CI workflows still disabled at the org level from Phase 2; activation pairs with billing restoration.
- **NIST ACVP test vectors for ML-DSA-65**: #525 stays open until upstream `ml-dsa` crate ships them.

### Documentation

Phase 3 ships per-milestone closure appendices:

- `design/toolchain/phase-transition-3.md` (m9-001) — the retrospective.
- `design/toolchain/pointer-types-phase3.md` (m1-013) — pointer types catalogue.
- `design/toolchain/per-node-ir-payload-phase3.md` (m2-006) — IR schema + side-table catalogue.
- `design/toolchain/optimization-passes.md` Phase-3-m3 closure section (m3-009).
- `design/toolchain/lsp-phase3.md` (m4-007) — m8 + m4 LSP architecture.
- `design/toolchain/bootstrap.md` §3-§4 Phase 3 closure (m5-003).
- `design/security/pq-trust-root.md` Phase 3 m6 + m7 + m8 sections (m6-005, m7-005, m8-004).
- `tests/pq-corpus/ML_DSA_ACVP_STATUS.md` (m8-003) — open-issue tracker.
- `docs/release-signing.md` Hardware HSM backends section (m6-004).
- `docs/g4-prep.md` §5 Stage-0b row checked (m5-002).

### Decision gate

G4 was stamped during Phase 2 m11-004 prep; G5 (the Phase 3 closure gate) follows the same framework. The Phase 4 plan will introduce G5's formal checklist.

## v0.2.0 — Phase 2 (substrate complete)

**Released:** Tag pushed at m11-006 closure.

paideia-as Phase 2 ships the full substrate for PaideiaOS subsystem migration. Eleven milestones across 130+ closed PRs (#347–#470). The toolchain went from "phase-1 ELF64 smoke" to "ready to compile a capability-system module end-to-end with deterministic build, hybrid PQ signing, vendor DWARF, LSP tooling, and the opt-pass catalog."

### Milestones

- **m1** (IR walker wiring) — `IrArena.children_table` + LinearityWalker (S0900/0901/0903/0907) + EffectRowWalker (F1100/01/02/05/06) + CapWalker (C1300) + linearity-regression / end-to-end corpora + ABI doc + cross-build smoke. PRs #347–#360.
- **m2** (typed-elaborator reflection) — `Term` handle + quote / antiquote + `splice` + `elab` builtin + macro hygiene + reflection-corpus. PRs #361–#372.
- **m3** (full algebraic effects) — row polymorphism + let-generalization + handler well-typedness + deep-handler compilation + R15 / SysV bridge + RowDiff diagnostic. PRs #374–#387.
- **m4** (PAX + paideia-link) — 96-byte PaxHeader + 64-byte SectionTable + 12 vendor section content types + BLAKE3 content hash + paideia-link 4-phase pipeline + pax-introspect tool. PRs #388–#400.
- **m5** (ML modules + functors) — Signature / Structure / Functor AST + module-kind machinery + structure / sig matching + applicative-functor cache + pack / unpack + sharing-constraint checker + `.paideia.functors` PAX section + file → module mapping. PRs #401–#413.
- **m6** (PE/COFF emitter) — PE/COFF headers + shared encoder lift + section emission + `.reloc` + UEFI imports + UEFI thunk + EFI subsystem + `--emit pe-coff` + UEFI smoke harness + cross-build fixture. PRs #414–#423.
- **m7** (PQ signing) — Ed25519 + ML-DSA-65 wrappers + hybrid signature scheme + PAX header signature integration + delegation-scope check (Q0901) + release-artifact signing + soft-HSM + verification corpus. PRs #424–#431.
- **m8** (LSP server) — tower-lsp scaffold + workspace manifest reader + textDocument sync + publishDiagnostics + parse cache + hover + definition / references + incremental engine + completion + code actions + paideia-fmt + semantic tokens + inlay hints + tree-sitter grammar + VS Code / Helix / Emacs / Neovim configs + LSP harness. PRs #432–#445.
- **m9** (optimization pass catalog) — OptPass trait + peephole + scheduling + macro fusion + DSE + REX/EVEX tightening + branch-hint / align / pool-constants + tail-call elimination + loop unrolling + catalog composition + O-code registration. PRs #446–#457.
- **m10** (DDC bring-up) — dual-build orchestrator + byte-level differ + build-determinism env contract (SOURCE_DATE_EPOCH + PDX_PATH_PREFIX_MAP) + format-gate corpus + nightly CI workflow + release-pipeline gate + operational docs. PRs #458–#465.
- **m11** (Phase 2 closure) — DWARF vendor ID + vendor section content builders + capability-system smoke + G4 prep checklist + retrospective. PRs #466–#470.

### Highlights

- ~1614 workspace tests across 26+ crates + 23+ test harnesses.
- 8 design-clarification items resolved (AS3 / AS5 / AS7 / AS8 / OS §3.2 / OS §4 N1 / OS §6 ¶1 / OS §6 ¶5). 7 deferred to Phase 3 with documented mitigation paths. 1 scope-changed.
- Dual stage-0 bootstrap commitment recorded (`design/toolchain/bootstrap.md`).
- Opt-pass catalog ships 11 passes (O1500–O1512) with callable helpers; real per-node rewrites flip on when the IR exposes per-node instruction payloads.
- LSP at parity with tower-lsp 0.20: 11 textDocument handlers wired.
- Full hybrid PQ signing: Ed25519 (RFC 8032 §7.1 KAT) + ML-DSA-65 (FIPS-204) with AND semantics; 3373-byte signature ≈ 3.4 KB.

### Operational deferrals

- **GitHub Actions billing block**: CI workflows (`ci.yml`, `cross-build.yml`, `ddc.yml`, `release.yml`) shipped but disabled at the org level. `cargo test --workspace` is the gate today. Activation pairs with billing restoration.
- **Stage-0b GNU `as` entry-point**: the dual-stage-0 commitment is documented (`bootstrap.md`); the GAS source is Phase 3 work.
- **Per-node IR instruction payloads**: required to flip the m9 opt passes from "would-fire" markers to real rewrites. Helper functions are unit-tested today.
- **Elaborator-driven LSP semantics**: m8-006..009 use lexical stand-ins. m8-008 QueryEngine is in place; per-position type queries land in Phase 3.

### Documentation

Every Phase 2 design doc has a phase-2-outcome appendix:

- `design/toolchain/calling-convention.md` (m3-011 + m6-005)
- `design/toolchain/paideia-link.md` (m4-013)
- `design/toolchain/macros-phase1.md` (m2-012)
- `design/security/pq-trust-root.md` (m7-008)
- `design/toolchain/optimization-passes.md` (m9-012)
- `design/toolchain/bootstrap.md` (m10-007)
- `design/toolchain/debug-info.md` (m11-001)
- `design/toolchain/phase-transition-2.md` (m11-005) — the retrospective.
- `docs/ddc.md` (m10-007)
- `docs/build-determinism.md` (m10-003)
- `docs/release-signing.md` (m7-005)
- `docs/g4-prep.md` (m11-004) — the G4 verification checklist.

### Decision gate G4

G4 stamping is pending the reviewer note in `docs/g4-prep.md` §6. Once stamped, paideia-as enters Phase 3 with its scope set by the m11-005 retrospective's §5 carryover list.

## v0.1.0 — Phase 1 (decision gate G2)

The phase-1 closing release. See the original `STATUS.md` for the per-deliverable PR map. Highlights:

- Lexer / parser / AST + diagnostics (PRs #29–#62).
- Substructural lattice + effect rows + handlers + macros + hygiene (PRs #122–#139).
- IR + ANF + effect rewrite (PRs #140–#141).
- ELF64 emitter + x86_64 encoder + DWARF stubs (PRs #142–#147).
- Linearity-regression corpus harness (PRs #149–#152).
- `paideia-as build --emit elf64` CLI (PR #148).
