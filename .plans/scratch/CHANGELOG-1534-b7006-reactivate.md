# CHANGELOG — paideia-as#1534 (PAS-DEBT-B7-006)

## Ticket
Reactivate `tests/uefi-smoke/tests/smoke.rs` boot smokes now that
m6-009+ / pa-r19-013 (#1018) has shipped a meaningful hello.efi.

## Site status

* **m6-008 landed**: `build_hello_efi()` (structural PE via
  `paideia-as-emitter-pe`) is present and un-ignored in
  `tests/uefi-smoke/src/lib.rs:100`. Wired into
  `hello_efi_builds_structurally_valid_pe` (which was already active).
* **pa-r19-013 landed (issue #1018)**: v0.19 UEFI-ABI MVP milestone
  completion. Shipped:
    - `tests/uefi-smoke/fixtures/hello.pdx` — 117-byte 2-arg MS x64
      identity function fixture (`fn(image, sys) -> image @abi("ms")`).
    - `build_hello_efi_via_paideia_as()` helper
      (`tests/uefi-smoke/src/lib.rs:228`) — invokes
      `cargo run -p paideia-as -- build --emit pe-coff <pdx>`.
    - 7 structural tests in
      `crates/paideia-as/tests/build_emit/uefi_stub.rs` (PE32+ magic,
      AMD64 machine, EFI-application subsystem, .text non-empty,
      MS-ABI callee prologue byte-pattern, etc.).
* **m6-009**: the "m6-009+" label in the ignore reason was aspirational
  shorthand for "any milestone that ships a meaningful `.efi`". Its two
  proxies (structural PE via m6-008, paideia-as-compiled PE via
  pa-r19-013 / #1018) both landed. `design/toolchain/bootstrap.md:39`
  cites m6-009 in the context of the NASM Stage-0a entry-point fixture
  (already present); it is not a still-open elaborator gap.
* **Fixture present**: `tests/uefi-smoke/fixtures/hello.pdx` ✓.
* **Runtime deps (skip-not-fail)**: QEMU + OVMF + `mkfs.vfat` + `mcopy`.
  `UefiEnv::probe()` already handles the QEMU+OVMF skip; the
  `mkfs.vfat`/`mcopy` gap was previously latent (would panic through
  `.expect("qemu spawn")` if the .fat image couldn't be built).

## Files touched

1. **`tests/uefi-smoke/tests/smoke.rs`** — rewritten:
   * Header rewritten to cite pa-r19-013 (#1018) + m6-008 landings and
     the deferred `Print`-primitive follow-up.
   * Added private `require_tool(&str) -> bool` helper (mirrors the
     shape used by
     `crates/paideia-as/tests/build_emit/pa10_007_data_symbol_names.rs:88`
     for consistency across the tree).
   * `#[ignore]` removed from `boot_and_print_under_ovmf`.
   * `#[ignore]` removed from `boot_and_print_paideia_compiled`.
   * Each boot test now skips (with a diagnostic `eprintln!`) when any
     of the four runtime tools is missing, layered as: `UefiEnv::probe()`
     → `require_tool("mkfs.vfat")` → `require_tool("mcopy")` → run.
   * Assertion floor (`!output.is_empty()`) unchanged with an inline
     comment naming the follow-up ("`Print` primitive, §m6-010+").

2. **`design/paideia-as-debt-catalog.md`** —
   * Row 494 (§8.2 entries table): B7-006 Symptom cell rewritten to
     record reactivation and the runtime-dep skip pattern; landing wave
     annotated "(landed)".
   * Row 639 (sprint queue): B7-006 status flipped from
     `blocked-on m6-009+` to
     `LANDED (#1534, Wave 3); tighten-hello follow-up open`.

## Not changed

* No UEFI boot code touched (per ticket constraint).
* `workspace.version` not bumped (per ticket constraint).
* `tests/uefi-smoke/src/lib.rs` unchanged — the helpers already
  implement the correct skip surface for the harness; the skip
  layering happens at the test-file boundary.
* `boot_and_capture_serial`'s internal `mkfs.vfat`/`mcopy` `Err` path
  is left in place (defense-in-depth) but is now unreachable via the
  reactivated tests because the tests skip earlier.
* No new `Print`-primitive fixture — that gates on m6-010+ (unfiled).

## Recommendation

Close #1534 on landing. Two follow-ups worth naming, both out of scope
for this ticket:

1. **Tighten-hello assertion.** File a fresh P3 ticket for
   §m6-010+ (`Print` via UEFI Boot Services) that lets these two boot
   smokes assert on real hello.efi output (e.g. `output.contains("H")`
   from an actually-printing image) rather than "OVMF banner
   non-empty". Site: same file. This is the *real* boot smoke;
   what we have here is essentially an OVMF liveness probe that
   confirms our PE loads and the entry-point returns cleanly.

2. **CI wiring.** The paideia-as pre-push hook / CI lanes must decide
   whether to install `qemu-system-x86_64`, `ovmf`, `mkfs.vfat`,
   `mcopy` on the runner. Today all four are effectively "skip on the
   developer's box" — so the reactivation buys us local-run coverage
   for anyone with the tools installed, but zero pre-merge coverage.
   File a P2 lane-provisioning ticket if enforcement is wanted.

## Build

Build not run — main should invoke `bash tools/build.sh` and re-invoke
me with the error tail if it fails. Changes are:

* Two `#[ignore]` attributes removed, one helper `fn require_tool`
  added, two skip stanzas layered upfront in
  `tests/uefi-smoke/tests/smoke.rs`. No new imports beyond
  `std::process::Command` (already re-exported by the harness crate).
* Two markdown-table cell edits in `design/paideia-as-debt-catalog.md`.

`cargo check --tests -p paideia-uefi-smoke` should be clean; the smoke
tests themselves will skip in a typical local build (missing
`mkfs.vfat`/`mcopy` on most dev machines).
