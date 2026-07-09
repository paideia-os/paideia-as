//! Command-line argument parsing for `paideia-as`.
//!
//! Subcommands are split across sibling modules (`cmd_*.rs`); this file
//! only defines the clap `Cli` and `Cmd` enums.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Target triplet for `--target` shortcut.
#[derive(Copy, Clone, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum Target {
    /// UEFI x86-64 (PE32+/COFF output format)
    #[value(name = "uefi-x86_64")]
    UefiX86_64,
    /// ELF64 kernel object x86-64
    #[value(name = "elf-kernel-x86_64")]
    ElfKernelX86_64,
    /// ELF64 user object x86-64
    #[value(name = "elf-user-x86_64")]
    ElfUserX86_64,
    /// PAX (PaideiaOS Architectural Executable) x86-64
    #[value(name = "pax-x86_64")]
    PaxX86_64,
}

/// Top-level CLI shape for `paideia-as`.
#[derive(Parser)]
#[command(name = "paideia-as", version, about = "PaideiaOS custom assembler")]
pub struct Cli {
    /// Subcommand to run.
    #[command(subcommand)]
    pub command: Cmd,
}

/// Subcommand selection.
#[derive(Subcommand)]
pub enum Cmd {
    /// Compile `.pdx` files. Phase-1 form: writes a `<stem>.placeholder`
    /// next to the input; the real ELF/PAX/PE emitters arrive at
    /// deliverable 8.
    Build {
        /// Path to the input `.pdx` file.
        input: PathBuf,
        /// Output artifact path. When `--emit elf64` is passed, defaults
        /// to `<stem>.o` next to the input.
        #[arg(short = 'o', long = "output")]
        output: Option<PathBuf>,
        /// Output format. Phase-1 supports `placeholder` (default) and
        /// `elf64` (writes a parseable ELF64 object via
        /// paideia-as-emitter-elf).
        #[arg(long = "emit", conflicts_with = "target")]
        emit: Option<String>,
        /// Target triplet shortcut for output format.
        #[arg(long = "target", value_enum, conflicts_with = "emit")]
        target: Option<Target>,
        /// Optimization level (0=off, 1+=enabled). Level 1 runs peephole, instruction scheduling, etc.
        #[arg(short = 'O', long = "optimize", default_value = "0")]
        optimize: u32,
        /// Phase-5 behaviour: warn on encoder failure and drop instruction instead of exiting.
        /// Default (Phase-6+): encoder failures abort the build with exit 2.
        #[arg(long)]
        encoder_warn: bool,
        /// Write SARIF 2.1.0 diagnostic output to the specified file.
        #[arg(long = "sarif", value_name = "PATH")]
        sarif: Option<PathBuf>,
    },
    /// Type-check without emitting object files. Phase-1: lex + parse +
    /// lower; the type checker is a stub.
    Check {
        /// Path to the input `.pdx` file.
        input: PathBuf,
        /// Print the IR pretty-printed dump to stdout after lowering.
        #[arg(long)]
        dump_ir: bool,
        /// Write SARIF 2.1.0 diagnostic output to the specified file.
        #[arg(long = "sarif", value_name = "PATH")]
        sarif: Option<PathBuf>,
    },
    /// Run linearity / effect / opt-pass linters.
    #[command(hide = true)]
    Lint { inputs: Vec<String> },
    /// Emit a specific format.
    #[command(hide = true)]
    Emit { format: String, inputs: Vec<String> },
    /// Print the unsafe-block audit catalog.
    #[command(hide = true)]
    Audit { inputs: Vec<String> },
    /// Generate reference documentation from inline annotations.
    Doc { inputs: Vec<String> },
    /// Lex, parse, and pretty-print the AST for one `.pdx` file.
    DumpAst {
        /// Path to the input `.pdx` file.
        input: PathBuf,
    },
    /// Discover and run tests in `.pdx` files.
    Test {
        /// Paths to scan for tests. If not provided, scans `tests/` and `src/`.
        #[arg(value_name = "PATHS")]
        paths: Vec<PathBuf>,
        /// Filter tests by regex pattern.
        #[arg(long)]
        filter: Option<String>,
        /// List discovered tests without running them.
        #[arg(long)]
        list: bool,
    },
    /// Format source code.
    Fmt {
        /// Path to the input `.pdx` file. Omit to read from stdin.
        input: Option<PathBuf>,
        /// Read from stdin instead of a file.
        #[arg(long)]
        stdin: bool,
        /// Check mode: exit with status 1 if formatting would change the file.
        #[arg(long)]
        check: bool,
    },
}
