use std::process::Command;

use super::driver_template;
use crate::common::fixture;
use crate::common::harness as build_harness;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reg {
    Rdi,
    Rsi,
    Rdx,
    Rcx,
    R8,
    R9,
    Rax,
}

impl Reg {
    pub fn name(&self) -> &'static str {
        match self {
            Reg::Rdi => "rdi",
            Reg::Rsi => "rsi",
            Reg::Rdx => "rdx",
            Reg::Rcx => "rcx",
            Reg::R8 => "r8",
            Reg::R9 => "r9",
            Reg::Rax => "rax",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Poison {
    pub reg: Reg,
    pub value: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetTy {
    I32,
    I64,
}

pub struct RuntimeCase<'a> {
    pub fixture_pdx: &'a str,
    pub entry: &'a str,
    pub ret_ty: RetTy,
    pub poison: &'a [Poison],
    pub expected: i64,
}

pub fn gcc_available() -> bool {
    Command::new("gcc")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

pub fn run_and_verify(case: &RuntimeCase) {
    if !gcc_available() {
        println!("[runtime harness] skipping: gcc unavailable");
        return;
    }

    // Resolve fixture path relative to workspace tests/build-emit
    let fixture_path = fixture::build_emit(case.fixture_pdx);

    // Compile fixture.pdx to fixture.o
    let outcome = build_harness::run_build(&fixture_path);
    outcome.assert_ok();

    let artifact_path = outcome
        .artifact
        .as_ref()
        .expect("artifact path should be set");

    // Get scratch directory from artifact path
    let scratch_dir = artifact_path.parent().expect("artifact should have parent dir");

    // Synthesize driver C code
    let driver_c = driver_template::synthesize_driver_c(case.entry, case.ret_ty, case.poison);
    let driver_c_path = scratch_dir.join("driver.c");
    std::fs::write(&driver_c_path, &driver_c)
        .expect("failed to write driver.c");

    // Compile and link: gcc driver.c fixture.o -no-pie -o bin
    let bin_path = scratch_dir.join("bin");
    let gcc_output = Command::new("gcc")
        .arg(&driver_c_path)
        .arg(artifact_path)
        .arg("-no-pie")
        .arg("-o")
        .arg(&bin_path)
        .output()
        .expect("failed to run gcc");

    if !gcc_output.status.success() {
        let stderr = String::from_utf8_lossy(&gcc_output.stderr);
        let stdout = String::from_utf8_lossy(&gcc_output.stdout);
        panic!(
            "gcc failed to link driver + fixture.\nFixture: {}\nDriver C:\n{}\nstderr:\n{}\nstdout:\n{}",
            case.fixture_pdx, driver_c, stderr, stdout
        );
    }

    // Run the binary and check exit code
    let run_output = Command::new(&bin_path)
        .output()
        .expect("failed to run binary");

    let actual_code = run_output
        .status
        .code()
        .expect("process should have exited normally");

    let expected_code = case.expected as i32;

    if actual_code != expected_code {
        let stderr = String::from_utf8_lossy(&run_output.stderr);
        let stdout = String::from_utf8_lossy(&run_output.stdout);
        panic!(
            "Exit code mismatch for {}::{}\n\
             Expected: {}\n\
             Actual: {}\n\
             Fixture: {}\n\
             Entry: {}\n\
             stdout: {}\n\
             stderr: {}",
            case.fixture_pdx, case.entry, expected_code, actual_code,
            case.fixture_pdx, case.entry, stdout, stderr
        );
    }
}
