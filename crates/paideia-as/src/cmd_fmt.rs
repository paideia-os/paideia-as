//! `paideia-as fmt` — source code formatter.
//!
//! Delegates to paideia-fmt for all formatting logic. Supports:
//! - Reading from a file (default) or stdin (--stdin).
//! - Writing in-place to the file (default) or stdout (--stdin).
//! - Checking mode (--check): exits 1 if formatted differs from input.
//! - Diff mode (--diff): displays unified diff to stdout; exits 1 if formatted differs from input.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::process::ExitCode;

use paideia_fmt::{FormatOptions, format};
use similar::TextDiff;

/// Run `paideia-as fmt [--stdin] [--check] [--diff] [FILE]`.
pub fn run(file: Option<&Path>, check: bool, diff: bool, stdin: bool) -> ExitCode {
    let source = match read_source(file, stdin) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("paideia-as fmt: {e}");
            return ExitCode::from(2);
        }
    };

    let formatted = format(&source, &FormatOptions::default());

    if diff {
        let label = if stdin {
            "<stdin>".to_string()
        } else {
            file.map(|p| p.display().to_string())
                .unwrap_or_else(|| "<stdin>".to_string())
        };

        if let Some(diff_output) = emit_diff(&source, &formatted, &label, stdin) {
            print!("{}", diff_output);
        }

        if formatted != source {
            return ExitCode::from(1);
        }
        return ExitCode::SUCCESS;
    }

    if check {
        if formatted != source {
            return ExitCode::from(1);
        }
        return ExitCode::SUCCESS;
    }

    // Write the formatted output.
    if let Err(e) = write_output(&formatted, file, stdin) {
        eprintln!("paideia-as fmt: {e}");
        return ExitCode::from(2);
    }

    ExitCode::SUCCESS
}

fn read_source(file: Option<&Path>, stdin: bool) -> Result<String, String> {
    if stdin {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| format!("failed to read stdin: {e}"))?;
        Ok(buf)
    } else {
        let path = file.ok_or("no file provided (use --stdin to read from stdin)")?;
        fs::read_to_string(path).map_err(|e| format!("failed to read {}: {e}", path.display()))
    }
}

fn write_output(formatted: &str, file: Option<&Path>, stdin: bool) -> Result<(), String> {
    if stdin {
        std::io::stdout()
            .write_all(formatted.as_bytes())
            .map_err(|e| format!("failed to write stdout: {e}"))?;
    } else if let Some(path) = file {
        fs::write(path, formatted)
            .map_err(|e| format!("failed to write {}: {e}", path.display()))?;
    }
    Ok(())
}

fn emit_diff(source: &str, formatted: &str, label: &str, stdin: bool) -> Option<String> {
    if source == formatted {
        return None;
    }

    let label_clean = label.trim_start_matches('/');
    let header_old = if stdin {
        "<stdin>".to_string()
    } else {
        format!("a/{}", label_clean)
    };

    let header_new = if stdin {
        "<stdin> (formatted)".to_string()
    } else {
        format!("b/{}", label_clean)
    };

    let diff = TextDiff::from_lines(source, formatted);
    let unified = diff
        .unified_diff()
        .context_radius(3)
        .header(&header_old, &header_new)
        .to_string();

    Some(unified)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as TempWrite;
    use tempfile::NamedTempFile;

    #[test]
    fn cmd_fmt_check_returns_ok_on_already_formatted() {
        let input = "let x = 1\nlet y = 2\n";
        let result = format(input, &FormatOptions::default());
        // If already formatted, check mode should return 0 (same content).
        assert_eq!(result, input);
    }

    #[test]
    fn cmd_fmt_check_returns_nonzero_on_unformatted() {
        let input = "let x = 1  \nlet y = 2  ";
        let result = format(input, &FormatOptions::default());
        // If formatting changes content, check mode should detect it.
        assert_ne!(result, input);
    }

    #[test]
    fn cmd_fmt_writes_to_file_in_place() {
        let mut tmp = NamedTempFile::new().unwrap();
        let input = "let x = 1  \nlet y = 2  ";
        tmp.write_all(input.as_bytes()).unwrap();
        tmp.flush().unwrap();

        let path = tmp.path();
        let exit = run(Some(path), false, false, false);
        assert_eq!(exit, ExitCode::SUCCESS);

        let written = fs::read_to_string(path).unwrap();
        let expected = format(input, &FormatOptions::default());
        assert_eq!(written, expected);
    }

    #[test]
    fn diff_produces_nonempty_output_on_unformatted_file() {
        let input = "let x = 1  \nlet y = 2  ";
        let formatted = format(input, &FormatOptions::default());
        let diff_output = emit_diff(input, &formatted, "test.pdx", false);
        assert!(diff_output.is_some());
        let output = diff_output.unwrap();
        assert!(!output.is_empty());
        assert!(output.contains("@@"));
    }

    #[test]
    fn diff_returns_exit_1_on_change() {
        let mut tmp = NamedTempFile::new().unwrap();
        let input = "let x = 1  \nlet y = 2  ";
        tmp.write_all(input.as_bytes()).unwrap();
        tmp.flush().unwrap();

        let path = tmp.path();
        let exit = run(Some(path), false, true, false);
        assert_eq!(exit, ExitCode::from(1));
    }

    #[test]
    fn diff_returns_exit_0_on_no_change() {
        let mut tmp = NamedTempFile::new().unwrap();
        let input = "let x = 1\nlet y = 2\n";
        tmp.write_all(input.as_bytes()).unwrap();
        tmp.flush().unwrap();

        let path = tmp.path();
        let exit = run(Some(path), false, true, false);
        assert_eq!(exit, ExitCode::SUCCESS);
    }

    #[test]
    fn diff_does_not_modify_file() {
        let mut tmp = NamedTempFile::new().unwrap();
        let input = "let x = 1  \nlet y = 2  ";
        tmp.write_all(input.as_bytes()).unwrap();
        tmp.flush().unwrap();

        let path = tmp.path();
        let _ = run(Some(path), false, true, false);

        let written = fs::read_to_string(path).unwrap();
        assert_eq!(written, input);
    }
}
