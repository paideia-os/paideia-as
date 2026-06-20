//! ddc-diff CLI: compare two binaries via the differ + allowlist.

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: ddc-diff <path-a> <path-b> <allowlist-toml>");
        return ExitCode::from(2);
    }

    let allowlist = match ddc::allowlist::Allowlist::load(Path::new(&args[3])) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("allowlist load error: {e}");
            return ExitCode::from(2);
        }
    };

    let report = match ddc::diff_files(Path::new(&args[1]), Path::new(&args[2]), &allowlist) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("diff error: {e}");
            return ExitCode::from(2);
        }
    };

    let json = serde_json::to_string_pretty(&report).expect("serialize");
    println!("{json}");

    if report.match_modulo_allowlist {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE // 1 = divergences not covered by allowlist
    }
}
