//! paideia-pq-sign CLI.

use std::io::{self, Write};
use std::process::ExitCode;

use paideia_pq_sign::Signer;
use rand_core::SeedableRng;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: paideia-pq-sign <subcommand> [args...]");
        eprintln!("  release <path>                          (deprecated: use hsm release)");
        eprintln!("  hsm init <path>                         (initialize soft-HSM at path)");
        eprintln!("  hsm release <hsm> <artifact>            (sign artifact with soft-HSM)");
        eprintln!("  hsm pkcs11 init --module <path> --slot <id> --pin <pin>");
        eprintln!("                                          (initialize PKCS#11 session)");
        return ExitCode::from(2);
    }

    match args[1].as_str() {
        "release" => {
            if args.len() < 3 {
                eprintln!("usage: paideia-pq-sign release <path>");
                return ExitCode::from(2);
            }
            run_release(&args[2])
        }
        "hsm" => {
            if args.len() < 3 {
                eprintln!("usage: paideia-pq-sign hsm <init|release|pkcs11> [args...]");
                return ExitCode::from(2);
            }
            match args[2].as_str() {
                "init" => {
                    if args.len() < 4 {
                        eprintln!("usage: paideia-pq-sign hsm init <path>");
                        return ExitCode::from(2);
                    }
                    run_hsm_init(&args[3])
                }
                "release" => {
                    if args.len() < 5 {
                        eprintln!("usage: paideia-pq-sign hsm release <hsm-path> <artifact-path>");
                        return ExitCode::from(2);
                    }
                    run_hsm_release(&args[3], &args[4])
                }
                "pkcs11" => {
                    if args.len() < 4 {
                        eprintln!("usage: paideia-pq-sign hsm pkcs11 <init> [args...]");
                        return ExitCode::from(2);
                    }
                    match args[3].as_str() {
                        "init" => run_pkcs11_init(&args[4..]),
                        _ => {
                            eprintln!("unknown pkcs11 subcommand: {}", args[3]);
                            ExitCode::from(2)
                        }
                    }
                }
                _ => {
                    eprintln!("unknown hsm subcommand: {}", args[2]);
                    ExitCode::from(2)
                }
            }
        }
        _ => {
            eprintln!("unknown subcommand: {}", args[1]);
            ExitCode::from(2)
        }
    }
}

fn run_release(version_or_path: &str) -> ExitCode {
    // Phase-2-m7-005 stand-in: use a deterministic test keypair from a
    // fixed seed. Real release signing uses an HSM-backed key (m7-006).
    // DEPRECATED: use `paideia-pq-sign hsm release` for real deployments.
    let path = std::path::Path::new(version_or_path);
    if !path.exists() {
        eprintln!("artifact not found: {}", version_or_path);
        return ExitCode::from(2);
    }

    eprintln!("WARNING: using legacy deterministic keypair. Use 'hsm release' for development.");

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

fn run_hsm_init(hsm_path: &str) -> ExitCode {
    let path = std::path::Path::new(hsm_path);

    // Prompt for password (try env var first for CI)
    let password = match std::env::var("PDX_HSM_PASSWORD") {
        Ok(pwd) => pwd.into_bytes(),
        Err(_) => {
            eprint!("Enter password for HSM: ");
            io::stdout().flush().ok();

            let mut pwd = String::new();
            if io::stdin().read_line(&mut pwd).is_err() {
                eprintln!("Failed to read password");
                return ExitCode::from(1);
            }
            pwd.trim_end().as_bytes().to_vec()
        }
    };

    if password.is_empty() {
        eprintln!("Password cannot be empty");
        return ExitCode::from(1);
    }

    let mut rng = rand_core::OsRng;
    let hsm = paideia_pq_sign::soft_hsm::SoftHsmFile::generate(&mut rng, &password);

    if let Err(e) = hsm.save(path) {
        eprintln!("Failed to save HSM: {e}");
        return ExitCode::from(1);
    }

    println!("Soft-HSM initialized at: {}", path.display());
    println!("Public key (hex):");
    println!("{}", hex::encode(hsm.public_key.to_bytes()));
    ExitCode::SUCCESS
}

fn run_hsm_release(hsm_path: &str, artifact_path: &str) -> ExitCode {
    let hsm_file = std::path::Path::new(hsm_path);
    let artifact = std::path::Path::new(artifact_path);

    if !hsm_file.exists() {
        eprintln!("HSM file not found: {}", hsm_path);
        return ExitCode::from(2);
    }

    if !artifact.exists() {
        eprintln!("Artifact not found: {}", artifact_path);
        return ExitCode::from(2);
    }

    // Load HSM file
    let hsm = match paideia_pq_sign::soft_hsm::SoftHsmFile::load(hsm_file) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Failed to load HSM: {e}");
            return ExitCode::from(1);
        }
    };

    // Prompt for password
    let password = match std::env::var("PDX_HSM_PASSWORD") {
        Ok(pwd) => pwd.into_bytes(),
        Err(_) => {
            eprint!("Enter HSM password: ");
            io::stdout().flush().ok();

            let mut pwd = String::new();
            if io::stdin().read_line(&mut pwd).is_err() {
                eprintln!("Failed to read password");
                return ExitCode::from(1);
            }
            pwd.trim_end().as_bytes().to_vec()
        }
    };

    // Unlock HSM
    let secret_key = match hsm.unlock(&password) {
        Some(sk) => sk,
        None => {
            eprintln!("Failed to unlock HSM: wrong password or corrupted file");
            return ExitCode::from(1);
        }
    };

    // Sign artifact
    let sig = match paideia_pq_sign::release::sign_release_artifact(&secret_key, artifact) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to sign artifact: {e}");
            return ExitCode::from(1);
        }
    };

    // Write signature
    if let Err(e) = paideia_pq_sign::release::write_detached_signature(artifact, &sig) {
        eprintln!("Failed to write signature: {e}");
        return ExitCode::from(1);
    }

    println!("Signed: {}.sig", artifact.display());
    ExitCode::SUCCESS
}

fn run_pkcs11_init(args: &[String]) -> ExitCode {
    // Parse args: --module <path> --slot <id> --pin <pin>
    let mut module_path: Option<&str> = None;
    let mut slot_id: Option<u64> = None;
    let mut pin: Option<&str> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--module" => {
                if i + 1 < args.len() {
                    module_path = Some(&args[i + 1]);
                    i += 2;
                } else {
                    eprintln!("--module requires an argument");
                    return ExitCode::from(2);
                }
            }
            "--slot" => {
                if i + 1 < args.len() {
                    match args[i + 1].parse::<u64>() {
                        Ok(id) => {
                            slot_id = Some(id);
                            i += 2;
                        }
                        Err(_) => {
                            eprintln!("--slot requires a numeric argument");
                            return ExitCode::from(2);
                        }
                    }
                } else {
                    eprintln!("--slot requires an argument");
                    return ExitCode::from(2);
                }
            }
            "--pin" => {
                if i + 1 < args.len() {
                    pin = Some(&args[i + 1]);
                    i += 2;
                } else {
                    eprintln!("--pin requires an argument");
                    return ExitCode::from(2);
                }
            }
            _ => {
                eprintln!("unknown argument: {}", args[i]);
                return ExitCode::from(2);
            }
        }
    }

    let module_path = match module_path {
        Some(p) => p,
        None => {
            eprintln!("--module is required");
            return ExitCode::from(2);
        }
    };

    let slot_id = match slot_id {
        Some(id) => id,
        None => {
            eprintln!("--slot is required");
            return ExitCode::from(2);
        }
    };

    let pin = match pin {
        Some(p) => p,
        None => {
            eprintln!("--pin is required");
            return ExitCode::from(2);
        }
    };

    // Initialize PKCS#11 signer
    let signer = match paideia_pq_sign::hsm::Pkcs11Signer::new(module_path, slot_id, pin) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("PKCS#11 initialization failed: {e}");
            return ExitCode::from(1);
        }
    };

    println!("PKCS#11 signer initialized:");
    println!("  Module: {}", signer.module_path());
    println!("  Slot: {}", signer.slot_id());
    println!("  Status: Phase-3 scaffold (real signing requires SoftHSM2 or hardware HSM)");

    ExitCode::SUCCESS
}
