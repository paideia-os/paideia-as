# hw-smoke KAT reproduction — v0.33 crypto primitives

Companion to paideia-as v0.33 (Argon2id KDF + ChaCha20-Poly1305 AEAD +
ML-KEM-768 KEM). Defines the Known-Answer-Test (KAT) vectors each
primitive MUST reproduce byte-for-byte when its FFI thunk is invoked
under `tools/run-smoke.sh` — i.e. the primitive is exercised from a
`.pdx`-compiled ELF booted by QEMU, not just from `cargo test` on the
host.

## §1 Purpose

`cargo test -p paideia-as-crypto` already pins the three primitives'
byte-exact behaviour on the *host* — the RFC / FIPS vectors live in
their respective modules and gate every commit through the workspace
test suite. That covers algorithmic conformance.

hw-smoke covers the *invocation* half of the contract: it proves that
the same bytes come out when the primitive is called through its C-ABI
thunk (`paideia_crypto_*`) from a boot-smoke ELF running under QEMU.
Failure modes that host tests cannot catch:

- SysV register/argument marshalling regressions in the elaborator's
  `stdlib_lowering::cryptoops` recipes.
- Static-data layout drift between the `.pdx` side's `Argon2idParamsC` /
  `AeadParamsC` structs and the Rust `#[repr(C)]` declarations in
  `crates/paideia-as-crypto/src/ffi/`.
- Link-time symbol resolution differences between the host build and
  the paideia-satellite-runtime staticlib.
- Bump-allocator ceiling violations under LOW_MEMORY Argon2id
  derivations on a 32 MiB QEMU guest.

Each section below fixes the input vector, the constant name (or literal
value) already pinned in-tree, the FFI thunk exercised, the boot-smoke
entry point that drives it, and the serial-log marker
`tools/run-smoke.sh` greps for on pass.

The three KAT entry points are independent — they share no state —
so a boot-smoke ELF MAY exercise them in any order, and a per-primitive
failure MUST be localised to that primitive's marker.

## §2 Argon2id KAT vector — RFC 9106 §5.3

**Source.** RFC 9106 §5.3, canonical Argon2id v=0x13 test vector.

**Inputs** (all bytes as fixed-value repetitions; length in bytes):

| Field             | Value            | Length |
|-------------------|------------------|--------|
| `password` (`P`)  | `0x01` repeated  | 32     |
| `salt` (`S`)      | `0x02` repeated  | 16     |
| `secret` (`K`)    | `0x03` repeated  | 8      |
| `associated_data` (`X`) | `0x04` repeated | 12 |
| `m_cost_kib` (`m`)| `32` (KiB)       | —      |
| `t_cost` (`t`)    | `3`              | —      |
| `p_cost` (`p`)    | `4`              | —      |
| `output_len` (`T`)| `32` (bytes)     | —      |

**Expected output.** 32-byte tag pinned in-tree as
[`paideia_as_crypto::kdf::argon2id::RFC_9106_ARGON2ID_TAG`]:

```
0d 64 0d f5 8d 78 76 6c  08 c0 37 a3 4a 8b 53 c9
d0 1e f0 45 2d 75 b6 5e  b5 25 20 e9 6b 01 e6 59
```

**FFI thunk.** `paideia_crypto_argon2id_derive` — declared in
`crates/paideia-as-crypto/src/ffi/argon2id.rs`, re-exported through the
satellite runtime. SysV mapping: `RDI = *const Argon2idParamsC`,
`RSI = *mut u8 (out)`, `RDX = out_len`, `RAX = return code`.

**Boot-smoke entry point.** `hw_smoke_argon2id_rfc9106_5_3`.
Constructs the `Argon2idParamsC` blob in static data with the field
values above, calls `Argon2id::derive` (the stdlib trait in
`crates/paideia-as-stdlib/pdx/crypto/argon2id.pdx`), and byte-compares
the 32-byte output against the pinned tag.

**Pass/fail assertion.**

- Pass: FFI return code equals `PDX_CRYPTO_OK` (0) AND
  `memcmp(out, RFC_9106_ARGON2ID_TAG, 32) == 0`. Emit
  `HWSMOKE_KAT_OK_ARGON2ID_RFC9106_5_3\n` on the serial port; return 0.
- Fail: any negative return code, or any output-byte mismatch. Emit
  `HWSMOKE_KAT_FAIL_ARGON2ID_RFC9106_5_3 rc=<n>\n` followed by the
  first mismatching byte offset and expected/actual bytes; return
  non-zero. `run-smoke.sh` greps for the `_OK_` marker.

## §3 ChaCha20-Poly1305 KAT vector — RFC 8439 §2.8.2

**Source.** RFC 8439 §2.8.2, canonical AEAD_CHACHA20_POLY1305 test
vector. The vector is pinned in-tree as six constants under
[`paideia_as_crypto::aead::chacha20_poly1305`]; hw-smoke MUST re-use
those constants directly rather than re-transcribing the RFC bytes.

**Inputs.**

| Field       | Constant                            | Length |
|-------------|-------------------------------------|--------|
| `key`       | `RFC_8439_SEC_2_8_2_KEY`            | 32     |
| `nonce`     | `RFC_8439_SEC_2_8_2_NONCE`          | 12     |
| `aad`       | `RFC_8439_SEC_2_8_2_AAD`            | 12     |
| `plaintext` | `RFC_8439_SEC_2_8_2_PLAINTEXT`      | 114    |

`plaintext` is the ASCII string
`"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it."`.

**Expected output.** `ciphertext || tag` (130 bytes total):

| Field         | Constant                            | Length |
|---------------|-------------------------------------|--------|
| `ciphertext`  | `RFC_8439_SEC_2_8_2_CIPHERTEXT`     | 114    |
| `tag`         | `RFC_8439_SEC_2_8_2_TAG`            | 16     |

**FFI thunks.** `paideia_crypto_chacha20_poly1305_seal` and
`paideia_crypto_chacha20_poly1305_open` — declared in
`crates/paideia-as-crypto/src/ffi/chacha20_poly1305.rs`. Both consume
an `AeadParamsC` (`RDI`) plus in/out buffer pointers and lengths; `RAX`
carries the return code.

**Boot-smoke entry point.** Split into two independent probes so a
seal-only regression stays distinguishable from an open-only regression:

- `hw_smoke_chacha20_poly1305_rfc8439_2_8_2_seal` — seals the RFC
  plaintext, byte-compares the concatenated `ciphertext || tag` against
  `RFC_8439_SEC_2_8_2_CIPHERTEXT || RFC_8439_SEC_2_8_2_TAG`.
- `hw_smoke_chacha20_poly1305_rfc8439_2_8_2_open` — opens
  `ciphertext || tag`, byte-compares the recovered plaintext against
  `RFC_8439_SEC_2_8_2_PLAINTEXT`.

**Pass/fail assertion.**

- Seal pass: return code `PDX_CRYPTO_OK` (0) AND both `memcmp`s equal.
  Emit `HWSMOKE_KAT_OK_CHACHA20_POLY1305_RFC8439_2_8_2_SEAL\n`.
- Open pass: return code `PDX_CRYPTO_OK` (0) AND plaintext `memcmp`
  equal. Emit `HWSMOKE_KAT_OK_CHACHA20_POLY1305_RFC8439_2_8_2_OPEN\n`.
- Fail (either probe): negative return code (in particular, the open
  probe MUST NOT return `PDX_CRYPTO_ERR_AUTH_FAILED` — a genuine auth
  failure on the canonical vector indicates the Poly1305 MAC path is
  broken) or any output mismatch. Emit
  `HWSMOKE_KAT_FAIL_CHACHA20_POLY1305_RFC8439_2_8_2_{SEAL,OPEN} rc=<n>\n`
  and return non-zero.

## §4 ML-KEM-768 KAT vectors — NIST ACVP

**Source.** NIST ACVP-Server repository, commit `65370b8` (2024-08),
`ML-KEM-{keyGen,encapDecap}-FIPS203/internalProjection.json`. Three
independent vectors — one per FIPS 203 primitive — pinned in-tree as
`pub const` constants at the bottom of
`crates/paideia-as-crypto/src/kem/ml_kem_768.rs`. hw-smoke MUST re-use
these constants by name; DO NOT duplicate the byte tables here.

**Vector 1: KeyGen** (ACVP `function=keyGen`, `testGroup 2`, `tcId 26`).

| Role     | Constant       | Length (bytes) |
|----------|----------------|----------------|
| Input `d` | `ACVP_KG_D`   | 32             |
| Input `z` | `ACVP_KG_Z`   | 32             |
| Expected `ek` | `ACVP_KG_EK` | 1184        |
| Expected `dk` | `ACVP_KG_DK` | 2400        |

**Vector 2: Encaps** (ACVP `function=encapsulation`, `testGroup 2`,
`tcId 26`; independent of Vector 1 — different `ek`).

| Role     | Constant       | Length (bytes) |
|----------|----------------|----------------|
| Input `ek` | `ACVP_EN_EK` | 1184           |
| Input `m`  | `ACVP_EN_M`  | 32             |
| Expected `c` | `ACVP_EN_C` | 1088          |
| Expected `K` | `ACVP_EN_K` | 32            |

**Vector 3: Decaps** (ACVP `function=decapsulation`, `testGroup 5`,
`tcId 88` — the first "no modification" entry, so `K` is the genuine
shared secret rather than an implicit-rejection fallback).

| Role     | Constant       | Length (bytes) |
|----------|----------------|----------------|
| Input `dk` | `ACVP_DE_DK` | 2400           |
| Input `c`  | `ACVP_DE_C`  | 1088           |
| Expected `K` | `ACVP_DE_K` | 32            |

**FFI thunks.** `paideia_crypto_ml_kem_768_{keygen, encaps, decaps}` —
declared in `crates/paideia-as-crypto/src/ffi/ml_kem_768.rs`,
re-exported through the satellite runtime. Buffer sizes are compile-
time constants of the parameter set (see the trait declaration in
`crates/paideia-as-stdlib/pdx/crypto/ml_kem_768.pdx`); the thunks
cast raw pointers to fixed-size array references without runtime
length checks.

**Boot-smoke entry points** (one per primitive so a per-op regression
is localised):

- `hw_smoke_ml_kem_768_keygen_acvp_tc26` — calls `MlKem768::keygen`
  with `(ACVP_KG_D, ACVP_KG_Z)`, byte-compares `(ek, dk)` against
  `(ACVP_KG_EK, ACVP_KG_DK)`.
- `hw_smoke_ml_kem_768_encaps_acvp_tc26` — calls `MlKem768::encaps`
  with `(ACVP_EN_EK, ACVP_EN_M)`, byte-compares `(c, K)` against
  `(ACVP_EN_C, ACVP_EN_K)`.
- `hw_smoke_ml_kem_768_decaps_acvp_tc88` — calls `MlKem768::decaps`
  with `(ACVP_DE_DK, ACVP_DE_C)`, byte-compares `K` against
  `ACVP_DE_K`.

The three probes are independent (different `(d, z)` / `ek` / `dk` /
`c` inputs) so each exercises a distinct code path in the wrapper — a
KeyGen regression cannot mask an Encaps regression, and vice versa.

**Pass/fail assertion.**

- Pass (each probe): return code `PDX_CRYPTO_OK` (0) AND every output
  buffer `memcmp` equal to its pinned constant. Emit
  `HWSMOKE_KAT_OK_ML_KEM_768_{KEYGEN,ENCAPS,DECAPS}_ACVP_TC<n>\n`.
- Fail: negative return code or any output mismatch. Emit
  `HWSMOKE_KAT_FAIL_ML_KEM_768_{...} rc=<n>\n` with first mismatching
  offset and expected/actual bytes; return non-zero.

Note (FIPS 203 §6.3): `Decaps` is total — a tampered ciphertext yields
the implicit-rejection fallback `J(z, c)`, not an error. The Decaps
probe uses the "no modification" ACVP vector precisely so a genuine
mismatch flags a wrapper regression rather than expected fallback
behaviour. Tamper-detection coverage lives in the host-side unit tests
and is out of scope for hw-smoke.

## §5 Boot-smoke driver contract

`tools/run-smoke.sh <pdx_path> <expected_marker>` builds the `.pdx`,
links the ELF, boots it under QEMU with the serial port piped to
`/tmp/qemu_serial.log`, and greps for `<expected_marker>`. The
per-primitive markers in §§2–4 are chosen so a single grep target
resolves each probe unambiguously:

- `HWSMOKE_KAT_OK_ARGON2ID_RFC9106_5_3`
- `HWSMOKE_KAT_OK_CHACHA20_POLY1305_RFC8439_2_8_2_SEAL`
- `HWSMOKE_KAT_OK_CHACHA20_POLY1305_RFC8439_2_8_2_OPEN`
- `HWSMOKE_KAT_OK_ML_KEM_768_KEYGEN_ACVP_TC26`
- `HWSMOKE_KAT_OK_ML_KEM_768_ENCAPS_ACVP_TC26`
- `HWSMOKE_KAT_OK_ML_KEM_768_DECAPS_ACVP_TC88`

A single umbrella boot-smoke ELF MAY chain all six probes and emit a
final `HWSMOKE_KAT_OK_V0_33_ALL\n` on total success, in which case
`run-smoke.sh` greps for that aggregate marker. Per-primitive markers
are still emitted so a partial-failure log localises the regression.

## §6 References

- RFC 9106 — *Argon2 Memory-Hard Function for Password Hashing and
  Proof-of-Work Applications*, §3.1 (parameter ranges), §4
  (recommended profiles), §5.3 (canonical test vector).
- RFC 8439 — *ChaCha20 and Poly1305 for IETF Protocols*, §2.3
  (nonce), §2.5 (key / tag sizes), §2.8 (AEAD construction), §2.8.2
  (canonical test vector).
- FIPS 203 — *Module-Lattice-Based Key-Encapsulation Mechanism
  Standard*, ML-KEM-768 parameter set (§7), §6 (KeyGen / Encaps /
  Decaps), §6.3 (implicit rejection).
- NIST ACVP-Server, commit `65370b8`:
  `gen-val/json-files/ML-KEM-{keyGen,encapDecap}-FIPS203/internalProjection.json`.
- In-tree constants: `crates/paideia-as-crypto/src/kdf/argon2id.rs`
  (§5.3 tag), `crates/paideia-as-crypto/src/aead/chacha20_poly1305.rs`
  (§2.8.2 vector), `crates/paideia-as-crypto/src/kem/ml_kem_768.rs`
  (ACVP KG / EN / DE constants).
- Consumer traits: `crates/paideia-as-stdlib/pdx/crypto/{argon2id,
  chacha20_poly1305, ml_kem_768}.pdx`.
- Link-line shim: `crates/paideia-satellite-runtime/src/lib.rs`
  (re-exports the six FFI thunks into `libpaideia_satellite_runtime.a`).
