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
mod cmd_common;
mod cmd_doc;
mod cmd_dump_ast;
mod cmd_fmt;
mod cmd_test;
mod det;
mod resolve_var_operands;

use std::process::ExitCode;

use clap::{Parser, error::ErrorKind};

use crate::cli::{Cli, Cmd, OUTPUT_SELECTION_GUIDANCE};

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            let kind = err.kind();
            // Detect if it's a missing-required error on the `build` subcommand.
            // The error text will include "paideia-as build" if it's the build command.
            let is_build_missing = kind == ErrorKind::MissingRequiredArgument
                && err.to_string().contains("paideia-as build");

            // Print the standard clap error to stderr (for usage errors, clap prints to stderr).
            let _ = err.print();

            // If it's a missing output selection on the `build` command, append guidance.
            if is_build_missing {
                eprintln!();
                eprintln!("{}", OUTPUT_SELECTION_GUIDANCE);
            }

            std::process::exit(err.exit_code());
        }
    };

    match cli.command {
        Cmd::Build {
            input,
            output,
            emit,
            target,
            optimize,
            encoder_warn,
            sarif,
        } => cmd_build::run(&input, output.as_deref(), emit.as_deref(), target, optimize, encoder_warn, sarif.as_deref()),
        Cmd::Check { input, dump_ir, sarif } => cmd_check::run(&input, dump_ir, sarif.as_deref()),
        Cmd::Lint { .. } => {
            eprintln!("paideia-as lint: not yet implemented");
            ExitCode::from(2)
        }
        Cmd::Emit { .. } => {
            eprintln!("paideia-as emit: not yet implemented");
            ExitCode::from(2)
        }
        Cmd::Audit { .. } => {
            eprintln!("paideia-as audit: not yet implemented");
            ExitCode::from(2)
        }
        Cmd::Doc { inputs } => cmd_doc::run(inputs),
        Cmd::DumpAst { input } => cmd_dump_ast::run(&input),
        Cmd::Test {
            paths,
            filter,
            list,
            format,
        } => cmd_test::run(paths, filter, list, format),
        Cmd::Fmt {
            input,
            stdin,
            check,
            diff,
        } => cmd_fmt::run(input.as_deref(), check, diff, stdin),
    }
}
