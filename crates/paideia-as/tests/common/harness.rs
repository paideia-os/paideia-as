//! Shared harness for invoking the `paideia-as` CLI from integration tests.
//!
//! The old-per-file pattern was:
//! ```ignore
//! fn cargo_run(args: &[&str]) -> Output {
//!     Command::new(env!("CARGO")).arg("run").arg("--quiet").arg("--").args(args)
//!         .env("NO_COLOR", "1").output().unwrap()
//! }
//! ```
//! duplicated across 58 files. This module centralizes it and adds:
//!  * a `BuildOutcome` with lazy artifact handling
//!  * a `ScratchDir` so no test races on `/tmp/foo.o`
//!  * a small `stderr_contains` / `assert_ok` / `assert_diag` surface

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};

use super::fixture::ScratchDir;

/// Emit format passed via `--emit <fmt>`.
#[derive(Clone, Copy, Debug)]
pub enum EmitFmt {
    /// No `--emit` flag (must be paired with `--target`, or build fails with usage error).
    None,
    Elf64,
    Pax,
    PeCoff,
    /// Free-form; passed verbatim after `--emit`.
    Raw(&'static str),
}

impl EmitFmt {
    fn args(self) -> Option<[&'static str; 2]> {
        match self {
            EmitFmt::None => None,
            EmitFmt::Elf64 => Some(["--emit", "elf64"]),
            EmitFmt::Pax => Some(["--emit", "pax"]),
            EmitFmt::PeCoff => Some(["--emit", "pe-coff"]),
            EmitFmt::Raw(s) => Some(["--emit", s]),
        }
    }
}

/// Target triplet passed via `--target <triplet>`.
#[derive(Clone, Copy, Debug)]
pub enum TargetTriplet {
    /// No `--target` flag.
    None,
    UefiX86_64,
    ElfKernelX86_64,
    ElfUserX86_64,
    PaxX86_64,
    /// Free-form; passed verbatim after `--target`.
    Raw(&'static str),
}

impl TargetTriplet {
    fn args(self) -> Option<[&'static str; 2]> {
        match self {
            TargetTriplet::None => None,
            TargetTriplet::UefiX86_64 => Some(["--target", "uefi-x86_64"]),
            TargetTriplet::ElfKernelX86_64 => Some(["--target", "elf-kernel-x86_64"]),
            TargetTriplet::ElfUserX86_64 => Some(["--target", "elf-user-x86_64"]),
            TargetTriplet::PaxX86_64 => Some(["--target", "pax-x86_64"]),
            TargetTriplet::Raw(s) => Some(["--target", s]),
        }
    }
}

/// Options for [`run_build_with`]; construct via `BuildOpts::new(fixture)`.
pub struct BuildOpts {
    pub input: PathBuf,
    pub emit: EmitFmt,
    pub target: TargetTriplet,
    pub extra_args: Vec<String>,
    /// If `Some`, `-o <path>` is appended; artifact recorded in the outcome.
    pub output: Option<PathBuf>,
    pub scratch_tag: &'static str,
}

impl BuildOpts {
    pub fn new<P: AsRef<Path>>(input: P) -> Self {
        Self {
            input: input.as_ref().to_path_buf(),
            emit: EmitFmt::Elf64,
            target: TargetTriplet::None,
            extra_args: Vec::new(),
            output: None,
            scratch_tag: "build",
        }
    }

    pub fn emit(mut self, fmt: EmitFmt) -> Self {
        self.emit = fmt;
        self
    }

    pub fn target(mut self, triplet: TargetTriplet) -> Self {
        self.target = triplet;
        self
    }

    pub fn output<P: AsRef<Path>>(mut self, out: P) -> Self {
        self.output = Some(out.as_ref().to_path_buf());
        self
    }

    pub fn extra(mut self, arg: impl Into<String>) -> Self {
        self.extra_args.push(arg.into());
        self
    }

    pub fn scratch_tag(mut self, tag: &'static str) -> Self {
        self.scratch_tag = tag;
        self
    }
}

/// Outcome of a CLI invocation. Owns the scratch dir when one was created,
/// so artifacts survive as long as the outcome is in scope.
pub struct BuildOutcome {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
    pub artifact: Option<PathBuf>,
    _scratch: Option<ScratchDir>,
}

impl BuildOutcome {
    fn from_output(
        out: Output,
        artifact: Option<PathBuf>,
        scratch: Option<ScratchDir>,
    ) -> Self {
        Self {
            status: out.status,
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            artifact,
            _scratch: scratch,
        }
    }

    /// Panic with stderr attached if the CLI did not exit 0.
    pub fn assert_ok(&self) -> &Self {
        assert!(
            self.status.success(),
            "paideia-as exited with {:?}\nstdout:\n{}\nstderr:\n{}",
            self.status,
            self.stdout,
            self.stderr
        );
        self
    }

    /// Return the exit code (`None` if killed by a signal).
    pub fn exit_code(&self) -> Option<i32> {
        self.status.code()
    }

    /// True iff stderr contains the given diagnostic code (`T0535`, `U1614`, ...)
    /// or free-form substring.
    pub fn stderr_contains(&self, needle: &str) -> bool {
        self.stderr.contains(needle)
    }

    /// Assert the build failed AND stderr mentions the given diagnostic code.
    /// The old style was 30-40 lines per file; this collapses it to one call.
    pub fn assert_diag(&self, code: &str) -> &Self {
        assert!(
            !self.status.success(),
            "expected build to fail with {code}, but it succeeded.\nstderr:\n{}",
            self.stderr
        );
        assert!(
            self.stderr_contains(code),
            "expected {code} in stderr, got:\n{}",
            self.stderr
        );
        self
    }

    /// Read the artifact ELF/PAX/PE bytes off disk. Panics if no `-o` was set.
    pub fn artifact_bytes(&self) -> Vec<u8> {
        let path = self
            .artifact
            .as_ref()
            .expect("BuildOutcome::artifact_bytes called but no output path was set");
        std::fs::read(path).unwrap_or_else(|e| {
            panic!("read {}: {}", path.display(), e);
        })
    }
}

/// Build the given fixture with `--emit elf64` into a scratch dir, using
/// `cargo run --quiet -- build <fixture> --emit elf64 -o <scratch>/out.o`.
///
/// Mirrors the old per-file `cargo_run` shape byte-for-byte, then loads the
/// artifact path into the returned outcome.
pub fn run_build<P: AsRef<Path>>(fixture: P) -> BuildOutcome {
    let scratch = ScratchDir::new("build");
    let out_path = scratch.artifact("out.o");
    let opts = BuildOpts::new(fixture.as_ref())
        .emit(EmitFmt::Elf64)
        .output(&out_path);
    let cmd_out = spawn(&opts);
    BuildOutcome::from_output(cmd_out, Some(out_path), Some(scratch))
}

/// Build with full options control; the caller is responsible for `-o` if a
/// custom output location is desired.
pub fn run_build_with(opts: BuildOpts) -> BuildOutcome {
    let cmd_out = spawn(&opts);
    BuildOutcome::from_output(cmd_out, opts.output.clone(), None)
}

/// `paideia-as check <fixture>` — no `--emit`, no `-o`.
pub fn run_check<P: AsRef<Path>>(fixture: P) -> BuildOutcome {
    let opts = BuildOpts {
        input: fixture.as_ref().to_path_buf(),
        emit: EmitFmt::None,
        target: TargetTriplet::None,
        extra_args: Vec::new(),
        output: None,
        scratch_tag: "check",
    };
    let cmd_out = spawn_verb(&opts, "check");
    BuildOutcome::from_output(cmd_out, None, None)
}

/// Free-form CLI invocation (`paideia-as <args...>`), used by CLI-shape tests
/// that don't fit the `build`/`check` pattern.
pub fn run_cli(args: &[&str]) -> BuildOutcome {
    let out = cargo_run(args);
    BuildOutcome::from_output(out, None, None)
}

/// Low-level cargo-run helper preserved for the handful of tests that
/// need direct `Output` control (env tweaks, custom stdin, ...).
pub fn cargo_run(args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

fn spawn(opts: &BuildOpts) -> Output {
    spawn_verb(opts, "build")
}

fn spawn_verb(opts: &BuildOpts, verb: &str) -> Output {
    let mut argv: Vec<String> = Vec::new();
    argv.push(verb.to_string());
    argv.push(opts.input.to_string_lossy().into_owned());
    if let Some(target) = opts.target.args() {
        argv.extend(target.into_iter().map(str::to_string));
    }
    if let Some(emit) = opts.emit.args() {
        argv.extend(emit.into_iter().map(str::to_string));
    }
    for extra in &opts.extra_args {
        argv.push(extra.clone());
    }
    if let Some(out) = &opts.output {
        argv.push("-o".to_string());
        argv.push(out.to_string_lossy().into_owned());
    }

    let argv_ref: Vec<&str> = argv.iter().map(String::as_str).collect();
    cargo_run(&argv_ref)
}
