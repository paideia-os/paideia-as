//! paideia-pq-sign CLI.

use std::process::ExitCode;

use paideia_pq_sign::Signer;
use rand_core::SeedableRng;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: paideia-pq-sign release <version-or-path>");
        return ExitCode::from(2);
    }
    match args[1].as_str() {
        "release" => {
            if args.len() < 3 {
                eprintln!("usage: paideia-pq-sign release <version-or-path>");
                return ExitCode::from(2);
            }
            run_release(&args[2])
        }
        _ => {
            eprintln!("unknown subcommand: {}", args[1]);
            ExitCode::from(2)
        }
    }
}

fn run_release(version_or_path: &str) -> ExitCode {
    // Phase-2-m7-005 minimum: treat the arg as a path. If it's a v-style
    // version string (starts with "v" and resolves to no path), document
    // that the caller is expected to package the tarball first and pass
    // the path.
    let path = std::path::Path::new(version_or_path);
    if !path.exists() {
        eprintln!("artifact not found: {}", version_or_path);
        return ExitCode::from(2);
    }

    // Phase-2-m7-005 stand-in: use a deterministic test keypair from a
    // fixed seed. Real release signing uses an HSM-backed key (m7-006).
    let mut rng = rand_chacha::ChaCha20Rng::from_seed([7u8; 32]);
    let (_pk, sk) = paideia_pq_sign::Hybrid::keygen(&mut rng);

    let sig = match paideia_pq_sign::release::sign_release_artifact(&sk, path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("sign error: {e}");
            return ExitCode::from(1);
        }
    };

    if let Err(e) = paideia_pq_sign::release::write_detached_signature(path, &sig) {
        eprintln!("write error: {e}");
        return ExitCode::from(1);
    }

    println!("Signed: {}.sig", path.display());
    ExitCode::SUCCESS
}
