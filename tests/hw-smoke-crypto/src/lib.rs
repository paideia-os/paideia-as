//! v0.33 crypto hw-smoke harness (paideia-as#1394).
//!
//! Companion to `tools/hw-smoke-v0.33.md` and
//! `tests/hw-smoke-crypto/fixtures/hw_smoke_crypto_v0_33.pdx`. Boots the
//! fixture under QEMU and greps its serial output for the per-primitive
//! KAT pass/fail markers, proving that Argon2id (RFC 9106), ChaCha20-
//! Poly1305 (RFC 8439), and ML-KEM-768 (FIPS 203 / NIST ACVP) reproduce
//! their spec vectors byte-for-byte when invoked through the real
//! `.pdx` -> extern-C FFI-thunk call boundary, not merely via
//! `cargo test -p paideia-as-crypto` on the host.
//!
//! Modeled on `tests/uefi-smoke/src/lib.rs`'s probe/build/boot shape,
//! adapted for a raw ELF boot (matching `tools/run-smoke.sh`) instead of
//! a UEFI/OVMF boot, and for a link step that must additionally pull in
//! `libpaideia_satellite_runtime.a` (the FFI thunks' resolving archive)
//! since `run-smoke.sh` itself only links a bare `.o` against
//! `tests/build-emit/link.ld` and has no extra-archive support.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// hw-smoke environment probed from the host: just `qemu-system-x86_64`
/// on PATH. No OVMF/UEFI dependency -- the fixture boots as a raw ELF
/// via QEMU's `-kernel`, the same protocol `tools/run-smoke.sh` uses
/// for every other boot-smoke `.pdx` fixture in this repo.
#[derive(Debug, Clone)]
pub struct HwSmokeEnv {
    /// Path to `qemu-system-x86_64`.
    pub qemu_system_x86_64: PathBuf,
}

impl HwSmokeEnv {
    /// Probe the host for `qemu-system-x86_64` on PATH.
    pub fn probe() -> Option<Self> {
        let which_output = Command::new("which")
            .arg("qemu-system-x86_64")
            .output()
            .ok()?;
        if !which_output.status.success() {
            return None;
        }
        let qemu_path = String::from_utf8(which_output.stdout)
            .ok()?
            .trim()
            .to_string();
        Some(Self {
            qemu_system_x86_64: PathBuf::from(qemu_path),
        })
    }
}

/// Absolute path to this satellite's repo root (`tools/paideia-as`),
/// derived from `CARGO_MANIFEST_DIR` (`.../tools/paideia-as/tests/hw-smoke-crypto`).
pub fn repo_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../..");
    p.canonicalize().expect("repo root should exist")
}

/// Build the hw-smoke KAT fixture into a bootable ELF.
///
/// Three steps, mirroring `tools/run-smoke.sh` plus the extra-archive
/// link `paideia-satellite-runtime`'s own doc comments describe for
/// satellite host-tool builds:
///
/// 1. `cargo run --release -p paideia-as -- build --emit elf64 <fixture> -o <obj>`
/// 2. `cargo build --release -p paideia-satellite-runtime` (produces
///    `libpaideia_satellite_runtime.a`, which resolves the six
///    `paideia_crypto_*` FFI thunks the fixture calls).
/// 3. `ld -T tests/build-emit/link.ld <obj> <archive> -o <out_elf>`
///
/// # Errors
///
/// Returns an error (with captured stderr) if any step's process exits
/// non-zero, or if expected build artifacts are missing afterward.
pub fn build_kat_elf(root: &Path, out_elf: &Path) -> std::io::Result<()> {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/hw_smoke_crypto_v0_33.pdx");
    let obj = out_elf.with_extension("o");
    let _ = std::fs::remove_file(&obj);
    let _ = std::fs::remove_file(out_elf);

    // Step 1: compile the .pdx fixture to an ELF64 relocatable object.
    let build_out = Command::new(env!("CARGO"))
        .current_dir(root)
        .arg("run")
        .arg("--release")
        .arg("--quiet")
        .arg("-p")
        .arg("paideia-as")
        .arg("--")
        .arg("build")
        .arg("--emit")
        .arg("elf64")
        .arg(&fixture)
        .arg("-o")
        .arg(&obj)
        .output()?;
    if !build_out.status.success() {
        return Err(std::io::Error::other(format!(
            "paideia-as build --emit elf64 failed: {}",
            String::from_utf8_lossy(&build_out.stderr)
        )));
    }

    // Step 2: ensure the satellite-runtime staticlib archive is built.
    // It resolves paideia_crypto_argon2id_derive,
    // paideia_crypto_chacha20_poly1305_{seal,open}, and
    // paideia_crypto_ml_kem_768_{keygen,encaps,decaps} -- the six
    // extern-C thunks the fixture calls.
    let runtime_out = Command::new(env!("CARGO"))
        .current_dir(root)
        .arg("build")
        .arg("--release")
        .arg("--quiet")
        .arg("-p")
        .arg("paideia-satellite-runtime")
        .output()?;
    if !runtime_out.status.success() {
        return Err(std::io::Error::other(format!(
            "cargo build -p paideia-satellite-runtime failed: {}",
            String::from_utf8_lossy(&runtime_out.stderr)
        )));
    }
    let archive = root.join("target/release/libpaideia_satellite_runtime.a");
    if !archive.exists() {
        return Err(std::io::Error::other(format!(
            "expected archive not found: {}",
            archive.display()
        )));
    }

    // Step 3: link against tests/build-emit/link.ld (ENTRY(_start), the
    // same script every other boot-smoke .pdx fixture in this repo
    // uses) plus the satellite-runtime archive.
    let link_script = root.join("tests/build-emit/link.ld");
    let ld_out = Command::new("ld")
        .arg("-T")
        .arg(&link_script)
        .arg(&obj)
        .arg(&archive)
        .arg("-o")
        .arg(out_elf)
        .output()?;
    if !ld_out.status.success() {
        return Err(std::io::Error::other(format!(
            "ld failed: {}",
            String::from_utf8_lossy(&ld_out.stderr)
        )));
    }
    if !out_elf.exists() {
        return Err(std::io::Error::other("ld reported success but no ELF was produced"));
    }
    Ok(())
}

/// Boot `elf_path` under QEMU and capture its serial (COM1 / 0x3F8)
/// output.
///
/// Mirrors `tools/run-smoke.sh`'s QEMU invocation exactly (`-serial
/// file:<log>`, `-display none`, `-no-reboot`, `-no-shutdown`, `-m
/// 32M`) with a hard `timeout` in front since the fixture always
/// reaches `hlt` and idles rather than exiting on its own.
///
/// # Errors
///
/// Returns an error if QEMU fails to spawn or the serial log cannot be
/// read back.
pub fn boot_and_capture_serial(env: &HwSmokeEnv, elf_path: &Path) -> std::io::Result<String> {
    let log_path = std::env::temp_dir().join("paideia_hw_smoke_crypto_serial.log");
    let _ = std::fs::remove_file(&log_path);

    let status = Command::new("timeout")
        .arg("10")
        .arg(&env.qemu_system_x86_64)
        .arg("-kernel")
        .arg(elf_path)
        .arg("-serial")
        .arg(format!("file:{}", log_path.display()))
        .arg("-display")
        .arg("none")
        .arg("-no-reboot")
        .arg("-no-shutdown")
        .arg("-m")
        .arg("32M")
        .status()?;
    // `timeout` returns 124 on a forced kill, which is the EXPECTED
    // outcome here (the fixture halts and idles); only a QEMU spawn
    // failure (status unavailable) is treated as an error above via
    // `?`. A non-timeout non-zero exit is unusual but not fatal to the
    // marker check below -- the serial log is inspected regardless.
    let _ = status;

    Ok(std::fs::read_to_string(&log_path).unwrap_or_default())
}
