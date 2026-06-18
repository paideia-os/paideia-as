//! paideia-as — PaideiaOS custom assembler (CLI entry point).
//!
//! Subcommand wiring; the substance lives in `cli.rs` (clap defs) and
//! `cmd_*.rs` (one file per subcommand).
//! Design: https://github.com/paideia-os/paideia-os/tree/main/design/toolchain

#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod cli;
mod cmd_dump_ast;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::{Cli, Cmd};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Build { .. } => {
            eprintln!("paideia-as build: stub (phase 1)");
            ExitCode::SUCCESS
        }
        Cmd::Check { .. } => {
            eprintln!("paideia-as check: stub (phase 1)");
            ExitCode::SUCCESS
        }
        Cmd::Lint { .. } => {
            eprintln!("paideia-as lint: stub (phase 1)");
            ExitCode::SUCCESS
        }
        Cmd::Emit { .. } => {
            eprintln!("paideia-as emit: stub (phase 1)");
            ExitCode::SUCCESS
        }
        Cmd::Audit { .. } => {
            eprintln!("paideia-as audit: stub (phase 1)");
            ExitCode::SUCCESS
        }
        Cmd::Doc { .. } => {
            eprintln!("paideia-as doc: stub (phase 1)");
            ExitCode::SUCCESS
        }
        Cmd::DumpAst { input } => cmd_dump_ast::run(&input),
    }
}
