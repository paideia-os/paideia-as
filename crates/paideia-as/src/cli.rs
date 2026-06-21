//! Command-line argument parsing for `paideia-as`.
//!
//! Subcommands are split across sibling modules (`cmd_*.rs`); this file
//! only defines the clap `Cli` and `Cmd` enums.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
        #[arg(long = "emit", default_value = "placeholder")]
        emit: String,
        /// Phase-5 behaviour: warn on encoder failure and drop instruction instead of exiting.
        /// Default (Phase-6+): encoder failures abort the build with exit 2.
        #[arg(long)]
        encoder_warn: bool,
    },
    /// Type-check without emitting object files. Phase-1: lex + parse +
    /// lower; the type checker is a stub. Writes a SARIF sidecar
    /// alongside the input.
    Check {
        /// Path to the input `.pdx` file.
        input: PathBuf,
        /// Print the IR pretty-printed dump to stdout after lowering.
        #[arg(long)]
        dump_ir: bool,
    },
    /// Run linearity / effect / opt-pass linters.
    Lint { inputs: Vec<String> },
    /// Emit a specific format.
    Emit { format: String, inputs: Vec<String> },
    /// Print the unsafe-block audit catalog.
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
