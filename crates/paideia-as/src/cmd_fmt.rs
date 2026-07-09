//! `paideia-as fmt` — source code formatter.
//!
//! Delegates to paideia-fmt for all formatting logic. Supports:
//! - Reading from one or more files (default) or stdin (--stdin).
//! - Writing in-place to each file (default) or stdout (--stdin).
//! - Checking mode (--check): exits 1 if any file's formatted output differs from input.
//! - Diff mode (--diff): displays a unified diff per changed file to stdout;
//!   exits 1 if any file's formatted output differs.
//!
//! Multi-file semantics (#1126):
//! - All files are processed. A per-file read/write failure emits a diagnostic
//!   to stderr and yields exit code 2 at end-of-run, but does not stop processing.
//! - --check and --diff exit code is 1 if ANY file would change (2 if any I/O error).

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use paideia_fmt::{FormatOptions, format};
use similar::TextDiff;

/// Run `paideia-as fmt [--stdin] [--check] [--diff] [FILES..]`.
pub fn run(files: &[PathBuf], check: bool, diff: bool, stdin: bool) -> ExitCode {
    if stdin {
        return run_stdin(check, diff);
    }

    if files.is_empty() {
        eprintln!("paideia-as fmt: no input files (use --stdin to read from stdin)");
        return ExitCode::from(2);
    }

    let mut any_change = false;
    let mut any_error = false;

    for file in files {
        match run_one_file(file, check, diff) {
            FileOutcome::Unchanged => {}
            FileOutcome::Changed => any_change = true,
            FileOutcome::Error => any_error = true,
        }
    }

    if any_error {
        ExitCode::from(2)
    } else if any_change && (check || diff) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

enum FileOutcome {
    Unchanged,
    Changed,
    Error,
}

fn run_one_file(file: &Path, check: bool, diff: bool) -> FileOutcome {
    let source = match fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("paideia-as fmt: failed to read {}: {e}", file.display());
            return FileOutcome::Error;
        }
    };

    let formatted = format(&source, &FormatOptions::default());
    let changed = source != formatted;

    if diff {
        if let Some(diff_output) = emit_diff(&source, &formatted, &file.display().to_string(), false) {
            print!("{}", diff_output);
        }
        return if changed { FileOutcome::Changed } else { FileOutcome::Unchanged };
    }

    if check {
        return if changed { FileOutcome::Changed } else { FileOutcome::Unchanged };
    }

    if changed {
        if let Err(e) = fs::write(file, &formatted) {
            eprintln!("paideia-as fmt: failed to write {}: {e}", file.display());
            return FileOutcome::Error;
        }
    }
    FileOutcome::Unchanged
}

fn run_stdin(check: bool, diff: bool) -> ExitCode {
    let mut source = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut source) {
        eprintln!("paideia-as fmt: failed to read stdin: {e}");
        return ExitCode::from(2);
    }

    let formatted = format(&source, &FormatOptions::default());
    let changed = source != formatted;

    if diff {
        if let Some(diff_output) = emit_diff(&source, &formatted, "<stdin>", true) {
            print!("{}", diff_output);
        }
        return if changed { ExitCode::from(1) } else { ExitCode::SUCCESS };
    }

    if check {
        return if changed { ExitCode::from(1) } else { ExitCode::SUCCESS };
    }

    if let Err(e) = std::io::stdout().write_all(formatted.as_bytes()) {
        eprintln!("paideia-as fmt: failed to write stdout: {e}");
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
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
        assert_eq!(result, input);
    }

    #[test]
    fn cmd_fmt_check_returns_nonzero_on_unformatted() {
        let input = "let x = 1  \nlet y = 2  ";
        let result = format(input, &FormatOptions::default());
        assert_ne!(result, input);
    }

    #[test]
    fn cmd_fmt_writes_to_file_in_place() {
        let mut tmp = NamedTempFile::new().unwrap();
        let input = "let x = 1  \nlet y = 2  ";
        tmp.write_all(input.as_bytes()).unwrap();
        tmp.flush().unwrap();

        let path = tmp.path().to_path_buf();
        let exit = run(&[path.clone()], false, false, false);
        assert_eq!(exit, ExitCode::SUCCESS);

        let written = fs::read_to_string(&path).unwrap();
        let expected = format(input, &FormatOptions::default());
        assert_eq!(written, expected);
    }

    #[test]
    fn cmd_fmt_writes_to_multiple_files_in_place() {
        let inputs: Vec<(NamedTempFile, &str)> = vec![
            (NamedTempFile::new().unwrap(), "let a = 1  \n"),
            (NamedTempFile::new().unwrap(), "let b = 2  \n"),
            (NamedTempFile::new().unwrap(), "let c = 3  \n"),
        ];
        let mut paths = Vec::new();
        for (tmp, content) in &inputs {
            let mut f = tmp.reopen().unwrap();
            f.write_all(content.as_bytes()).unwrap();
            f.flush().unwrap();
            paths.push(tmp.path().to_path_buf());
        }

        let exit = run(&paths, false, false, false);
        assert_eq!(exit, ExitCode::SUCCESS);

        for (path, (_, source)) in paths.iter().zip(inputs.iter()) {
            let written = fs::read_to_string(path).unwrap();
            let expected = format(source, &FormatOptions::default());
            assert_eq!(written, expected, "content mismatch in {}", path.display());
        }
    }

    #[test]
    fn cmd_fmt_check_multi_file_exits_1_if_any_changed() {
        let mut good = NamedTempFile::new().unwrap();
        good.write_all(b"let x = 1\nlet y = 2\n").unwrap();
        good.flush().unwrap();

        let mut bad = NamedTempFile::new().unwrap();
        bad.write_all(b"let z = 3  \n").unwrap();
        bad.flush().unwrap();

        let exit = run(&[good.path().to_path_buf(), bad.path().to_path_buf()], true, false, false);
        assert_eq!(exit, ExitCode::from(1));

        // Both files unmodified — check doesn't write.
        let good_after = fs::read_to_string(good.path()).unwrap();
        assert_eq!(good_after, "let x = 1\nlet y = 2\n");
    }

    #[test]
    fn cmd_fmt_check_multi_file_exits_0_if_all_clean() {
        let mut a = NamedTempFile::new().unwrap();
        a.write_all(b"let x = 1\nlet y = 2\n").unwrap();
        a.flush().unwrap();

        let mut b = NamedTempFile::new().unwrap();
        b.write_all(b"let p = 5\nlet q = 6\n").unwrap();
        b.flush().unwrap();

        let exit = run(&[a.path().to_path_buf(), b.path().to_path_buf()], true, false, false);
        assert_eq!(exit, ExitCode::SUCCESS);
    }

    #[test]
    fn cmd_fmt_diff_multi_file_prints_per_file_headers() {
        let mut a = NamedTempFile::new().unwrap();
        a.write_all(b"let x = 1  \n").unwrap();
        a.flush().unwrap();

        let mut b = NamedTempFile::new().unwrap();
        b.write_all(b"let y = 2  \n").unwrap();
        b.flush().unwrap();

        // Exercise the run path — output goes to stdout so we can't capture
        // it easily in a unit test, but assert exit code semantics.
        let exit = run(&[a.path().to_path_buf(), b.path().to_path_buf()], false, true, false);
        assert_eq!(exit, ExitCode::from(1));

        // Diff mode must not modify files.
        let a_after = fs::read_to_string(a.path()).unwrap();
        assert_eq!(a_after, "let x = 1  \n");
    }

    #[test]
    fn cmd_fmt_no_inputs_no_stdin_exits_2() {
        let exit = run(&[], false, false, false);
        assert_eq!(exit, ExitCode::from(2));
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

        let path = tmp.path().to_path_buf();
        let exit = run(&[path], false, true, false);
        assert_eq!(exit, ExitCode::from(1));
    }

    #[test]
    fn diff_returns_exit_0_on_no_change() {
        let mut tmp = NamedTempFile::new().unwrap();
        let input = "let x = 1\nlet y = 2\n";
        tmp.write_all(input.as_bytes()).unwrap();
        tmp.flush().unwrap();

        let path = tmp.path().to_path_buf();
        let exit = run(&[path], false, true, false);
        assert_eq!(exit, ExitCode::SUCCESS);
    }

    #[test]
    fn diff_does_not_modify_file() {
        let mut tmp = NamedTempFile::new().unwrap();
        let input = "let x = 1  \nlet y = 2  ";
        tmp.write_all(input.as_bytes()).unwrap();
        tmp.flush().unwrap();

        let path = tmp.path().to_path_buf();
        let _ = run(&[path.clone()], false, true, false);

        let written = fs::read_to_string(&path).unwrap();
        assert_eq!(written, input);
    }
}
