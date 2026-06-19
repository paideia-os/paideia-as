//! paideia-fmt CLI: reads from --stdin OR file paths.

use std::io::{Read, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--stdin") {
        let mut buf = String::new();
        if std::io::stdin().read_to_string(&mut buf).is_err() {
            return ExitCode::from(1);
        }
        let opts = paideia_fmt::FormatOptions::default();
        let out = paideia_fmt::format(&buf, &opts);
        if std::io::stdout().write_all(out.as_bytes()).is_err() {
            return ExitCode::from(1);
        }
    } else {
        eprintln!("usage: paideia-fmt --stdin (file paths not yet supported)");
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}
