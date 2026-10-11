# paideia-as-unicode fuzz harness (UNI-14)

Resolves `paideia-as#1601`. Hosts the `cargo-fuzz` targets for the
single hand-rolled, non-vetted-crate code path in `paideia-as-unicode`:
the streaming RFC 3629 `Utf8Decoder` and the NFC normalizer that every
string crossing an IPC boundary is run through (SH-D9.4).

## Targets

| name              | input       | asserts                                                                 |
|-------------------|-------------|-------------------------------------------------------------------------|
| `utf8_decoder`    | `&[u8]`     | the decoder terminates, never panics, keeps `position()` monotonic      |
| `utf8_roundtrip`  | `String`    | `nfc_normalize` is idempotent and its output satisfies `is_nfc`         |

The `utf8_decoder` target is the one UNI-14 was filed for — every other
module in the crate delegates to a vetted `crates.io` dependency
(`unicode-normalization`, `unicode-segmentation`, `unicode-width`,
`unicode-linebreak`, `unicode-properties`, `unicode-script`,
`unicode-blocks`, `caseless`), so the hand-rolled decoder is the only
code path with meaningful malformed-input attack surface in the crate.

`utf8_roundtrip` is a bonus target: `nfc_normalize` has an ASCII
fast-path we wrote ourselves (`is_ascii_fast_path_eligible`) and the
idempotence invariant that `Str::eq` depends on is cheap to assert on
every random valid UTF-8 string the fuzzer generates.

## Running

The targets need a nightly toolchain (libFuzzer's sanitizer integration
is nightly-only):

```
rustup toolchain install nightly
cargo +nightly install cargo-fuzz

cd crates/paideia-as-unicode/fuzz
cargo +nightly fuzz run utf8_decoder    -- -runs=1000     # 60 s smoke
cargo +nightly fuzz run utf8_roundtrip  -- -runs=1000
```

Both targets should complete 1000 runs without a crash in well under
60 seconds on a workstation. A long soak would drop the `-runs` cap and
let libFuzzer run until stopped — crashes land in `fuzz/artifacts/` as
minimized reproducers.

## CI integration

Fuzzing is **not** wired into any CI pipeline. The paideia-as repo's
local verification harness (`bash tools/build.sh`,
`bash tools/run-qemu.sh`) does not require nightly, and this crate
intentionally keeps its stable-toolchain discipline so a nightly ICE
cannot block a release.

The intended cadence is manual: run `cargo +nightly fuzz run <target>`
locally after a change to `src/utf8.rs` or `src/nfc.rs`, and before
every semantic-shell milestone close. Any crash the fuzzer finds
becomes a regression test under `crates/paideia-as-unicode/tests/`
(seed the test with the minimized input in `fuzz/artifacts/<target>/`)
rather than a check-in of the fuzz corpus itself.

## Workspace exclusion

The `fuzz/Cargo.toml` carries its own empty `[workspace]` table so
cargo treats it as a standalone workspace root. The parent
`tools/paideia-as/Cargo.toml` does **not** list `crates/paideia-as-unicode/fuzz`
as a member, and it does not need an `exclude = [...]` entry either —
the nested `[workspace]` declaration is what breaks workspace
auto-discovery. This matches the convention in the cargo-fuzz book and
keeps the fuzzer's sanitizer feature-unification out of the main
workspace's dependency graph (which is why an ordinary `cargo check`
of the parent workspace does not pull `libfuzzer-sys` in).

To check just the fuzz crate (does **not** require nightly for a plain
`cargo check`, only for `cargo fuzz run`):

```
cd crates/paideia-as-unicode/fuzz
cargo check
```
