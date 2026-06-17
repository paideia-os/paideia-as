//! paideia-as — PaideiaOS custom assembler (CLI entry point)
//!
//! Phase-1 skeleton; subcommands are stubs to be filled in over phases 1–2.
//! Design: https://github.com/paideia-os/paideia-os/tree/main/design/toolchain

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "paideia-as", version, about = "PaideiaOS custom assembler")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Compile .pdx files to object files (ELF / PE-COFF / PAX-fragment)
    Build { inputs: Vec<String> },
    /// Type-check without emitting object files
    Check { inputs: Vec<String> },
    /// Run linearity / effect / opt-pass linters
    Lint { inputs: Vec<String> },
    /// Emit a specific format
    Emit { format: String, inputs: Vec<String> },
    /// Print the unsafe-block audit catalog
    Audit { inputs: Vec<String> },
    /// Generate reference documentation from inline annotations
    Doc { inputs: Vec<String> },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Build { .. } => eprintln!("paideia-as build: stub (phase 1)"),
        Cmd::Check { .. } => eprintln!("paideia-as check: stub (phase 1)"),
        Cmd::Lint  { .. } => eprintln!("paideia-as lint: stub (phase 1)"),
        Cmd::Emit  { .. } => eprintln!("paideia-as emit: stub (phase 1)"),
        Cmd::Audit { .. } => eprintln!("paideia-as audit: stub (phase 1)"),
        Cmd::Doc   { .. } => eprintln!("paideia-as doc: stub (phase 1)"),
    }
}
