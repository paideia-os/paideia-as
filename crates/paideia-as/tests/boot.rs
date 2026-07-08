//! Boot-path integration tests.
//!
//! Consolidates the historical `boot_*.rs`, `paideia_os_*.rs`, `qemu_smoke.rs`,
//! `cap_smoke_runtime.rs`, and `pa8_m7_001_checkpoint2_orchestration.rs`
//! binaries. Each ex-file becomes a submodule of `boot`; the leaf `#[test]`
//! names are unchanged so existing `cargo test -- <name>` filters and CI
//! globs still hit the same tests.

mod common;

mod boot {
    pub mod cap_smoke_runtime;
    pub mod checkpoint2_orchestration;
    pub mod observable;
    pub mod orchestration_smoke;
    pub mod orchestration_v2;
    pub mod paideia_os_checkpoint2_m2_canary;
    pub mod paideia_os_m3_829_byte_snapshot;
    pub mod paideia_os_m4_003_unsafe_regression;
    pub mod paideia_os_m5_835_lapic_ipi_snapshot;
    pub mod paideia_os_phase1_rebuild;
    pub mod paideia_os_r1_5_r2_5_rebuild;
    pub mod qemu_smoke;
}
