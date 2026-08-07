//! `paideia-as-klog-migrate` — CLI entry point.
//!
//! See `design/toolchain/klog-migration-helper.md` for the full spec.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use klog_migrate::{RenderOpts, compile_fail_pattern, migrate};
use similar::TextDiff;

/// Rewrite direct-UART patterns into structured klog calls in a `.pdx` file.
#[derive(Debug, Parser)]
#[command(
    version,
    about = "Rewrite direct-UART patterns into structured klog calls in a .pdx file",
    long_about = "Tokenises the input .pdx file with paideia-as-lexer, then rewrites every\n\
                  'lea rdi, [rip + <MSG>]; call uart_puts;' pattern into the 4-instruction\n\
                  klog_s1(LEVEL, SUBSYS, MSG) block. See design/toolchain/klog-migration-helper.md."
)]
struct Cli {
    /// Path to the .pdx file to migrate.
    file: PathBuf,

    /// Level literal for non-fail messages [default: 3 = LEVEL_INFO].
    #[arg(long, default_value_t = 3)]
    level: u32,

    /// Symbol name for the SUBSYS argument [default: SUBSYS_BOOT].
    #[arg(long, default_value = "SUBSYS_BOOT")]
    subsys: String,

    /// Level literal for messages whose symbol matches --fail-pattern [default: 1 = LEVEL_ERROR].
    #[arg(long, default_value_t = 1)]
    fail_level: u32,

    /// Regex tested against msg symbol name; a hit uses --fail-level.
    #[arg(long, default_value = r"(?i)(fail|err)")]
    fail_pattern: String,

    /// Overwrite the input file with the migrated source.
    #[arg(long, conflicts_with = "check")]
    in_place: bool,

    /// Exit 1 iff the file would change; do not write.
    #[arg(long, conflicts_with = "in_place")]
    check: bool,

    /// Print a unified diff to stderr in addition to the primary mode.
    #[arg(long)]
    diff: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let source = match std::fs::read_to_string(&cli.file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "paideia-as-klog-migrate: read {}: {e}",
                cli.file.display()
            );
            return ExitCode::from(2);
        }
    };

    let fail_regex = match compile_fail_pattern(&cli.fail_pattern) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("paideia-as-klog-migrate: invalid --fail-pattern: {e}");
            return ExitCode::from(2);
        }
    };

    let opts = RenderOpts {
        level: cli.level,
        fail_level: cli.fail_level,
        subsys: cli.subsys.clone(),
        fail_pattern: fail_regex,
    };

    let report = match migrate(&source, &opts) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("paideia-as-klog-migrate: migration failed: {e}");
            return ExitCode::from(2);
        }
    };
    let migrated = &report.source;
    let hits = report.rewritten;
    let skipped = report.skipped_by_annotation;

    // Surface any migration warnings before the primary-mode output.
    for w in &report.warnings {
        match w {
            klog_migrate::Warning::TrailingCommentDropped { line, msg_symbol } => {
                eprintln!(
                    "paideia-as-klog-migrate: {}:{}: dropped trailing comment while migrating `{}` — review by hand",
                    cli.file.display(),
                    line,
                    msg_symbol
                );
            }
        }
    }

    // Advisory report of skipped-by-annotation sites (#1275). Emitted in
    // every mode so operators see intentionally-preserved raw uart_puts
    // sites without having to grep for them.
    if skipped > 0 {
        eprintln!(
            "paideia-as-klog-migrate: {} skipped {} site{} by `// klog-migrate: skip` annotation",
            cli.file.display(),
            skipped,
            if skipped == 1 { "" } else { "s" }
        );
    }

    if cli.diff && migrated != &source {
        let diff = TextDiff::from_lines(&source, migrated);
        let out = diff
            .unified_diff()
            .context_radius(3)
            .header(
                &format!("{} (original)", cli.file.display()),
                &format!("{} (migrated)", cli.file.display()),
            )
            .to_string();
        eprint!("{out}");
    }

    if cli.check {
        if migrated == &source {
            eprintln!("paideia-as-klog-migrate: {} is clean", cli.file.display());
            return ExitCode::SUCCESS;
        } else {
            eprintln!(
                "paideia-as-klog-migrate: {} would rewrite {} site{}",
                cli.file.display(),
                hits,
                if hits == 1 { "" } else { "s" }
            );
            return ExitCode::FAILURE;
        }
    }

    if cli.in_place {
        if migrated == &source {
            eprintln!("paideia-as-klog-migrate: {} unchanged", cli.file.display());
            return ExitCode::SUCCESS;
        }
        if let Err(e) = std::fs::write(&cli.file, migrated) {
            eprintln!(
                "paideia-as-klog-migrate: write {}: {e}",
                cli.file.display()
            );
            return ExitCode::from(2);
        }
        eprintln!(
            "paideia-as-klog-migrate: {} rewrote {} site{}",
            cli.file.display(),
            hits,
            if hits == 1 { "" } else { "s" }
        );
        return ExitCode::SUCCESS;
    }

    // Default mode: print migrated source to stdout.
    print!("{migrated}");
    ExitCode::SUCCESS
}
