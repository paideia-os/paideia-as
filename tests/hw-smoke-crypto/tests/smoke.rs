//! v0.33 crypto hw-smoke tests (paideia-as#1394).
//!
//! `env_check_describes_availability` always runs (diagnostic only,
//! mirrors `tests/uefi-smoke`'s pattern). `boot_and_verify_kat_markers`
//! is `#[ignore]`'d -- it needs `qemu-system-x86_64` on PATH and takes
//! several seconds (compiles `paideia-as` + `paideia-satellite-runtime`
//! in release mode, links, boots) -- run it explicitly:
//!
//! ```sh
//! cargo test -p paideia-hw-smoke-crypto -- --ignored --nocapture
//! ```

use paideia_hw_smoke_crypto::{HwSmokeEnv, boot_and_capture_serial, build_kat_elf, repo_root};

#[test]
fn env_check_describes_availability() {
    match HwSmokeEnv::probe() {
        Some(_) => println!("qemu-system-x86_64 present; hw-smoke boot test will run."),
        None => println!("qemu-system-x86_64 absent; hw-smoke boot test will be skipped."),
    }
}

/// The six per-primitive OK markers from `tools/hw-smoke-v0.33.md` Sec.5.
const EXPECTED_OK_MARKERS: [&str; 6] = [
    "HWSMOKE_KAT_OK_ARGON2ID_RFC9106_5_3",
    "HWSMOKE_KAT_OK_CHACHA20_POLY1305_RFC8439_2_8_2_SEAL",
    "HWSMOKE_KAT_OK_CHACHA20_POLY1305_RFC8439_2_8_2_OPEN",
    "HWSMOKE_KAT_OK_ML_KEM_768_KEYGEN_ACVP_TC26",
    "HWSMOKE_KAT_OK_ML_KEM_768_ENCAPS_ACVP_TC26",
    "HWSMOKE_KAT_OK_ML_KEM_768_DECAPS_ACVP_TC88",
];

const AGGREGATE_OK_MARKER: &str = "HWSMOKE_KAT_OK_V0_33_ALL";

#[test]
#[ignore = "hw-smoke boot gated on qemu-system-x86_64 (paideia-as#1394)"]
fn boot_and_verify_kat_markers() {
    let Some(env) = HwSmokeEnv::probe() else {
        eprintln!("Skipped: qemu-system-x86_64 not found on PATH.");
        return;
    };

    let root = repo_root();
    let out_elf = std::env::temp_dir().join("paideia_hw_smoke_crypto.elf");
    build_kat_elf(&root, &out_elf).expect("hw-smoke fixture build+link failed");

    let serial = boot_and_capture_serial(&env, &out_elf).expect("qemu spawn/boot failed");

    println!("--- captured serial output ---\n{serial}\n--- end serial output ---");

    let mut missing = Vec::new();
    for marker in EXPECTED_OK_MARKERS {
        if !serial.contains(marker) {
            missing.push(marker);
        }
    }
    assert!(
        missing.is_empty(),
        "missing hw-smoke KAT OK marker(s): {missing:?}\nfull serial output:\n{serial}"
    );

    // Aggregate marker is a MAY per tools/hw-smoke-v0.33.md Sec.5; assert
    // it too since all six per-primitive markers were just confirmed
    // present (so the aggregate summing to 6 is implied) and its
    // absence would itself indicate an orchestrator regression.
    assert!(
        serial.contains(AGGREGATE_OK_MARKER),
        "missing aggregate marker {AGGREGATE_OK_MARKER}\nfull serial output:\n{serial}"
    );
}
