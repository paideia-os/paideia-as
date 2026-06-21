//! `paideia-as doc <inputs...>` — extract and render documentation.
//!
//! Scans one or more `.pdx` source files, extracts documentation items
//! (let/fn/struct/enum/trait/impl/effect/capability/module with their
//! `///` doc-comments), and renders an HTML document with cross-references.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

/// Run `paideia-as doc <inputs...>`.
///
/// Processes each input file, extracts documentation, and writes a
/// combined HTML document to stdout (or a file if specified).
///
/// Returns an `ExitCode` so the CLI can propagate non-zero on errors.
pub fn run(inputs: Vec<String>) -> ExitCode {
    if inputs.is_empty() {
        eprintln!("paideia-as doc: no input files specified");
        return ExitCode::from(2);
    }

    let mut all_items = Vec::new();

    // Process each input file.
    for input_str in inputs {
        let path = PathBuf::from(&input_str);
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("paideia-as doc: cannot read {}: {e}", path.display());
                return ExitCode::from(2);
            }
        };

        let content = match String::from_utf8(bytes) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("paideia-as doc: {} is not valid UTF-8: {e}", path.display());
                return ExitCode::from(1);
            }
        };

        // Extract documentation from this file.
        let corpus = paideia_as_doc::extract(&content);
        all_items.extend(corpus.items);
    }

    // Render combined documentation to HTML.
    let combined = paideia_as_doc::DocCorpus { items: all_items };
    let html = paideia_as_doc::render_html(&combined);

    println!("{html}");

    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn doc_with_no_inputs_returns_error() {
        let result = run(vec![]);
        assert_eq!(result, ExitCode::from(2));
    }

    #[test]
    fn doc_with_single_file() {
        let tmpdir = TempDir::new().unwrap();
        let file_path = tmpdir.path().join("test.pdx");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "/// A test function").unwrap();
        writeln!(file, "fn test() {{}}").unwrap();
        drop(file);

        let result = run(vec![file_path.to_string_lossy().to_string()]);
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn doc_with_missing_file_returns_error() {
        let result = run(vec!["/nonexistent/file.pdx".to_string()]);
        assert_eq!(result, ExitCode::from(2));
    }
}
