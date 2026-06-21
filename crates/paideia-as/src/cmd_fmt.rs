//! `paideia-as fmt` — source code formatter.
//!
//! Delegates to paideia-fmt for all formatting logic. Supports:
//! - Reading from a file (default) or stdin (--stdin).
//! - Writing in-place to the file (default) or stdout (--stdin).
//! - Checking mode (--check): exits 1 if formatted differs from input.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::process::ExitCode;

use paideia_fmt::{FormatOptions, format};

/// Run `paideia-as fmt [--stdin] [--check] [FILE]`.
pub fn run(file: Option<&Path>, check: bool, stdin: bool) -> ExitCode {
    let source = match read_source(file, stdin) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("paideia-as fmt: {e}");
            return ExitCode::from(2);
        }
    };

    let formatted = format(&source, &FormatOptions::default());

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
        let exit = run(Some(path), false, false);
        assert_eq!(exit, ExitCode::SUCCESS);

        let written = fs::read_to_string(path).unwrap();
        let expected = format(input, &FormatOptions::default());
        assert_eq!(written, expected);
    }
}
