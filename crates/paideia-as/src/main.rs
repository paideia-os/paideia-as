//! paideia-as — PaideiaOS custom assembler (CLI entry point).
//!
//! Subcommand wiring; the substance lives in `cli.rs` (clap defs) and
//! `cmd_*.rs` (one file per subcommand).
//! Design: https://github.com/paideia-os/paideia-os/tree/main/design/toolchain

#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod cli;
mod cmd_build;
mod cmd_check;
mod cmd_doc;
mod cmd_dump_ast;
mod cmd_fmt;
mod cmd_test;
mod det;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::{Cli, Cmd};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Build {
            input,
            output,
            emit,
            encoder_warn,
        } => cmd_build::run(&input, output.as_deref(), &emit, encoder_warn),
        Cmd::Check { input, dump_ir } => cmd_check::run(&input, dump_ir),
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
        Cmd::Doc { inputs } => cmd_doc::run(inputs),
        Cmd::DumpAst { input } => cmd_dump_ast::run(&input),
        Cmd::Test {
            paths,
            filter,
            list,
        } => cmd_test::run(paths, filter, list),
        Cmd::Fmt {
            input,
            stdin,
            check,
        } => cmd_fmt::run(input.as_deref(), check, stdin),
    }
}
