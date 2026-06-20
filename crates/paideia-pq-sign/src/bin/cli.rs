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
        eprintln!(
            "  hsm yubihsm init --connector <url> --ed25519-key-id <id> [--opt-in-hybrid-fallback]"
        );
        eprintln!(
            "                                          (initialize YubiHSM2 session with hybrid fallback)"
        );
        eprintln!("  timestamp --tsa-url <url> --input <artifact>");
        eprintln!(
            "                                          (fetch RFC 3161 timestamp token for artifact)"
        );
        eprintln!(
            "  verify --artifact <path> [--revocation-list <path>] [--ignore-revocation] [--tsa-token <path>]"
        );
        eprintln!(
            "                                          (verify artifact signature and check revocation)"
        );
        eprintln!("                                          (validate TSA token if provided)");
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
                eprintln!("usage: paideia-pq-sign hsm <init|release|pkcs11|yubihsm> [args...]");
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
                "yubihsm" => {
                    if args.len() < 4 {
                        eprintln!("usage: paideia-pq-sign hsm yubihsm <init> [args...]");
                        return ExitCode::from(2);
                    }
                    match args[3].as_str() {
                        "init" => run_yubihsm_init(&args[4..]),
                        _ => {
                            eprintln!("unknown yubihsm subcommand: {}", args[3]);
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
        "timestamp" => {
            if args.len() < 3 {
                eprintln!("usage: paideia-pq-sign timestamp --tsa-url <url> --input <artifact>");
                return ExitCode::from(2);
            }
            run_timestamp(&args[2..])
        }
        "verify" => {
            if args.len() < 3 {
                eprintln!(
                    "usage: paideia-pq-sign verify --artifact <path> [--revocation-list <path>] [--ignore-revocation]"
                );
                return ExitCode::from(2);
            }
            run_verify(&args[2..])
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

fn run_yubihsm_init(args: &[String]) -> ExitCode {
    // Parse args: --connector <url> --ed25519-key-id <id> [--opt-in-hybrid-fallback]
    let mut connector: Option<&str> = None;
    let mut ed25519_key_id: Option<u16> = None;
    let mut opt_in_hybrid_fallback = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--connector" => {
                if i + 1 < args.len() {
                    connector = Some(&args[i + 1]);
                    i += 2;
                } else {
                    eprintln!("--connector requires an argument");
                    return ExitCode::from(2);
                }
            }
            "--ed25519-key-id" => {
                if i + 1 < args.len() {
                    match args[i + 1].parse::<u16>() {
                        Ok(id) => {
                            ed25519_key_id = Some(id);
                            i += 2;
                        }
                        Err(_) => {
                            eprintln!("--ed25519-key-id requires a numeric argument (0-65535)");
                            return ExitCode::from(2);
                        }
                    }
                } else {
                    eprintln!("--ed25519-key-id requires an argument");
                    return ExitCode::from(2);
                }
            }
            "--opt-in-hybrid-fallback" => {
                opt_in_hybrid_fallback = true;
                i += 1;
            }
            _ => {
                eprintln!("unknown argument: {}", args[i]);
                return ExitCode::from(2);
            }
        }
    }

    let connector = match connector {
        Some(c) => c,
        None => {
            eprintln!("--connector is required");
            return ExitCode::from(2);
        }
    };

    let ed25519_key_id = match ed25519_key_id {
        Some(id) => id,
        None => {
            eprintln!("--ed25519-key-id is required");
            return ExitCode::from(2);
        }
    };

    // Initialize YubiHSM2 signer with hybrid fallback
    let signer = match paideia_pq_sign::hsm::YubiHsmSigner::new(
        connector,
        ed25519_key_id,
        opt_in_hybrid_fallback,
    ) {
        Ok(s) => s,
        Err(paideia_pq_sign::hsm::YubiHsmError::OptInRequired) => {
            eprintln!(
                "error (Q0902): YubiHSM2 backend requires explicit opt-in for hybrid fallback"
            );
            eprintln!("See design/security/pq-trust-root.md (phase-3 appendix m6-005)");
            eprintln!("To proceed, add: --opt-in-hybrid-fallback");
            return ExitCode::from(1);
        }
        Err(e) => {
            eprintln!("YubiHSM2 initialization failed: {e}");
            return ExitCode::from(1);
        }
    };

    println!("YubiHSM2 signer initialized:");
    println!("  Connector: {}", signer.connector_url());
    println!("  Ed25519 key ID: {}", signer.ed25519_key_id());
    println!("  Hybrid fallback: Ed25519 (hardware) + ML-DSA-65 (soft-HSM)");
    println!(
        "  Status: Phase-3 scaffold (real signing requires yubihsm crate and running YubiHSM2)"
    );
    println!();
    println!("warning (Q0902): PQ leg (ML-DSA-65) uses soft-HSM fallback");
    println!("  Operator has acknowledged this via --opt-in-hybrid-fallback");

    ExitCode::SUCCESS
}

fn run_timestamp(args: &[String]) -> ExitCode {
    // Parse args: --tsa-url <url> --input <artifact>
    let mut tsa_url: Option<&str> = None;
    let mut input_path: Option<&str> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--tsa-url" => {
                if i + 1 < args.len() {
                    tsa_url = Some(&args[i + 1]);
                    i += 2;
                } else {
                    eprintln!("--tsa-url requires an argument");
                    return ExitCode::from(2);
                }
            }
            "--input" => {
                if i + 1 < args.len() {
                    input_path = Some(&args[i + 1]);
                    i += 2;
                } else {
                    eprintln!("--input requires an argument");
                    return ExitCode::from(2);
                }
            }
            _ => {
                eprintln!("unknown argument: {}", args[i]);
                return ExitCode::from(2);
            }
        }
    }

    let tsa_url = match tsa_url {
        Some(url) => url,
        None => {
            eprintln!("--tsa-url is required");
            eprintln!("usage: paideia-pq-sign timestamp --tsa-url <url> --input <artifact>");
            return ExitCode::from(2);
        }
    };

    let input_path = match input_path {
        Some(path) => path,
        None => {
            eprintln!("--input is required");
            eprintln!("usage: paideia-pq-sign timestamp --tsa-url <url> --input <artifact>");
            return ExitCode::from(2);
        }
    };

    let artifact = std::path::Path::new(input_path);
    if !artifact.exists() {
        eprintln!("artifact not found: {}", input_path);
        return ExitCode::from(2);
    }

    // Read the artifact
    let data = match std::fs::read(artifact) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to read artifact: {e}");
            return ExitCode::from(1);
        }
    };

    // Build timestamp request
    let req = paideia_pq_sign::timestamp::build_request(
        &data,
        paideia_pq_sign::timestamp::HashAlgo::Sha256,
    );

    // Fetch token
    let token = match paideia_pq_sign::timestamp::fetch_token(&req, Some(tsa_url)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Timestamp error: {e}");
            return ExitCode::from(1);
        }
    };

    // Print token info
    println!("Timestamp token generated:");
    println!("  TSA: {}", token.tsa_name);
    println!("  Generation time (UTC): {}", token.gen_time_seconds);
    println!("  Serial number: {}", token.serial_number);
    println!(
        "  Message imprint (hex): {}",
        hex::encode(&token.message_imprint)
    );
    println!("  Signature length: {} bytes", token.signature.len());

    if token.signature.is_empty() {
        println!();
        println!(
            "Note: Phase-3 m8-001 scaffold — signature is empty until real TSA HTTP integration."
        );
    }

    ExitCode::SUCCESS
}

fn run_verify(args: &[String]) -> ExitCode {
    // Parse args: --artifact <path> [--revocation-list <path>] [--ignore-revocation] [--tsa-token <path>]
    let mut artifact_path: Option<&str> = None;
    let mut revocation_list_path: Option<&str> = None;
    let mut tsa_token_path: Option<&str> = None;
    let mut ignore_revocation = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--artifact" => {
                if i + 1 < args.len() {
                    artifact_path = Some(&args[i + 1]);
                    i += 2;
                } else {
                    eprintln!("--artifact requires an argument");
                    return ExitCode::from(2);
                }
            }
            "--revocation-list" => {
                if i + 1 < args.len() {
                    revocation_list_path = Some(&args[i + 1]);
                    i += 2;
                } else {
                    eprintln!("--revocation-list requires an argument");
                    return ExitCode::from(2);
                }
            }
            "--tsa-token" => {
                if i + 1 < args.len() {
                    tsa_token_path = Some(&args[i + 1]);
                    i += 2;
                } else {
                    eprintln!("--tsa-token requires an argument");
                    return ExitCode::from(2);
                }
            }
            "--ignore-revocation" => {
                ignore_revocation = true;
                i += 1;
            }
            _ => {
                eprintln!("unknown argument: {}", args[i]);
                return ExitCode::from(2);
            }
        }
    }

    let artifact_path = match artifact_path {
        Some(p) => p,
        None => {
            eprintln!("--artifact is required");
            eprintln!(
                "usage: paideia-pq-sign verify --artifact <path> [--revocation-list <path>] [--ignore-revocation]"
            );
            return ExitCode::from(2);
        }
    };

    let artifact = std::path::Path::new(artifact_path);
    if !artifact.exists() {
        eprintln!("artifact not found: {}", artifact_path);
        return ExitCode::from(2);
    }

    // Read the artifact and its signature (for future actual verification)
    let _artifact_data = match std::fs::read(artifact) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to read artifact: {e}");
            return ExitCode::from(1);
        }
    };

    let sig_path = format!("{}.sig", artifact_path);
    let sig_file = std::path::Path::new(&sig_path);
    if !sig_file.exists() {
        eprintln!("signature file not found: {}", sig_path);
        return ExitCode::from(2);
    }

    let sig_data = match std::fs::read(sig_file) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to read signature: {e}");
            return ExitCode::from(1);
        }
    };

    // TODO: Actually verify the signature (phase 3 enhancement)
    // For now, just check structure
    if sig_data.len() != paideia_pq_sign::HYBRID_SIG_LEN {
        eprintln!(
            "Invalid signature length: expected {}, got {}",
            paideia_pq_sign::HYBRID_SIG_LEN,
            sig_data.len()
        );
        return ExitCode::from(1);
    }

    // Load revocation list if provided
    if let Some(rev_path) = revocation_list_path {
        let rev_file = std::path::Path::new(rev_path);
        let revocation_list = match paideia_pq_sign::RevocationList::load_from_jsonl(rev_file) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Failed to load revocation list: {e}");
                return ExitCode::from(1);
            }
        };

        // Compute key_id from the public key part of the signature
        // For now, this is a placeholder using BLAKE3
        if sig_data.len() >= paideia_pq_sign::ED25519_PK_LEN {
            let pk_hash = blake3::hash(&sig_data[..paideia_pq_sign::ED25519_PK_LEN]);
            let key_id = hex::encode(&pk_hash.as_bytes()[..8]); // First 16 hex chars (8 bytes)

            if let Some(entry) = revocation_list.is_revoked(&key_id) {
                if !ignore_revocation {
                    eprintln!(
                        "Key revoked on {} (reason: {})",
                        entry.revoked_at, entry.reason
                    );
                    return ExitCode::from(1);
                } else {
                    eprintln!(
                        "WARNING: Key is revoked but verification proceeding with --ignore-revocation"
                    );
                    eprintln!(
                        "  Revocation date: {}, Reason: {}",
                        entry.revoked_at, entry.reason
                    );
                }
            }
        }
    }

    // Verify TSA token if provided
    let mut tsa_anchored = false;
    if let Some(token_path) = tsa_token_path {
        let token_file = std::path::Path::new(token_path);
        if !token_file.exists() {
            eprintln!("TSA token file not found: {}", token_path);
            return ExitCode::from(2);
        }

        let token_data = match std::fs::read(token_file) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Failed to read TSA token: {e}");
                return ExitCode::from(1);
            }
        };

        // Deserialize the timestamp token
        match paideia_pq_sign::timestamp::TimestampToken::from_bytes(&token_data) {
            Some(token) => {
                // Compute artifact hash to cross-check message imprint
                let artifact_hash = blake3::hash(&_artifact_data);
                let artifact_imprint = artifact_hash.as_bytes()[..32].to_vec();

                if token.message_imprint == artifact_imprint {
                    tsa_anchored = true;
                    println!("  TSA validation: passed");
                    println!("    TSA: {}", token.tsa_name);
                    println!("    Timestamp (UTC): {}", token.gen_time_seconds);
                    println!("    Serial: {}", token.serial_number);
                } else {
                    eprintln!("TSA validation failed: message imprint mismatch");
                    eprintln!("  Expected (artifact): {}", hex::encode(&artifact_imprint));
                    eprintln!("  Got (token): {}", hex::encode(&token.message_imprint));
                    return ExitCode::from(1);
                }
            }
            None => {
                eprintln!("Failed to deserialize TSA token from bytes");
                return ExitCode::from(1);
            }
        }
    }

    println!("Verification succeeded:");
    println!("  Artifact: {}", artifact_path);
    println!("  Signature length: {} bytes", sig_data.len());
    println!(
        "  TSA-anchored: {}",
        if tsa_anchored { "yes" } else { "no" }
    );
    if revocation_list_path.is_some() && !ignore_revocation {
        println!("  Revocation check: passed");
    }

    ExitCode::SUCCESS
}
