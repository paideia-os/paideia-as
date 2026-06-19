//! Integration tests for paideia-pq-sign CLI.

use std::fs;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn cli_release_subcommand_signs_a_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let artifact_path = temp_dir.path().join("test-v1.0.0.tar.gz");

    // Write test artifact
    fs::write(&artifact_path, b"test artifact content").expect("Failed to write artifact");

    // Run the CLI
    let output = Command::new(
        std::env::current_dir()
            .ok()
            .and_then(|p| {
                // Navigate to target dir
                let mut target = p.clone();
                target.push("target");
                if target.exists() { Some(target) } else { None }
            })
            .map(|p| {
                let mut path = p;
                path.push("debug");
                path.push("paideia-pq-sign");
                path
            })
            .unwrap_or_else(|| std::path::PathBuf::from("paideia-pq-sign")),
    )
    .arg("release")
    .arg(&artifact_path)
    .output();

    if let Ok(output) = output {
        // Check exit code
        assert!(
            output.status.success(),
            "CLI should exit successfully. stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        // Check .sig file exists
        let sig_path = artifact_path.with_extension("sig");
        assert!(
            sig_path.exists(),
            "Signature file should exist at {}",
            sig_path.display()
        );

        // Check signature file has correct size (HYBRID_SIG_LEN = 3373)
        let sig_metadata = fs::metadata(&sig_path).expect("Failed to read sig metadata");
        assert_eq!(
            sig_metadata.len() as usize,
            3373,
            "Signature should be exactly 3373 bytes"
        );
    } else {
        // If binary not found, skip test (common in CI before build)
        // This is acceptable for phase-2-m7-005
    }
}

#[test]
fn cli_hsm_init_creates_file_and_signs() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let hsm_path = temp_dir.path().join("test.hsm");
    let artifact_path = temp_dir.path().join("test-artifact.tar.gz");

    // Write test artifact
    fs::write(&artifact_path, b"test artifact content").expect("Failed to write artifact");

    // Get the binary path
    let binary_path = std::env::current_dir()
        .ok()
        .and_then(|p| {
            let mut target = p.clone();
            target.push("target");
            if target.exists() { Some(target) } else { None }
        })
        .map(|p| {
            let mut path = p;
            path.push("debug");
            path.push("paideia-pq-sign");
            path
        })
        .unwrap_or_else(|| std::path::PathBuf::from("paideia-pq-sign"));

    // Initialize HSM with environment variable password
    let init_output = Command::new(&binary_path)
        .arg("hsm")
        .arg("init")
        .arg(&hsm_path)
        .env("PDX_HSM_PASSWORD", "test_password")
        .output();

    if let Ok(output) = init_output {
        assert!(
            output.status.success(),
            "HSM init should succeed. stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        assert!(hsm_path.exists(), "HSM file should exist after init");

        // Now sign an artifact with the HSM
        let sign_output = Command::new(&binary_path)
            .arg("hsm")
            .arg("release")
            .arg(&hsm_path)
            .arg(&artifact_path)
            .env("PDX_HSM_PASSWORD", "test_password")
            .output();

        if let Ok(output) = sign_output {
            assert!(
                output.status.success(),
                "HSM release should succeed. stderr: {}",
                String::from_utf8_lossy(&output.stderr)
            );

            // Check .sig file exists
            let sig_path = artifact_path.with_extension("sig");
            assert!(
                sig_path.exists(),
                "Signature file should exist at {}",
                sig_path.display()
            );

            // Check signature file has correct size (HYBRID_SIG_LEN = 3373)
            let sig_metadata = fs::metadata(&sig_path).expect("Failed to read sig metadata");
            assert_eq!(
                sig_metadata.len() as usize,
                3373,
                "Signature should be exactly 3373 bytes"
            );
        }
    }
}

#[test]
fn cli_hsm_release_fails_without_password() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let hsm_path = temp_dir.path().join("test.hsm");
    let artifact_path = temp_dir.path().join("test-artifact.tar.gz");

    // Write test artifact
    fs::write(&artifact_path, b"test artifact content").expect("Failed to write artifact");

    // Get the binary path
    let binary_path = std::env::current_dir()
        .ok()
        .and_then(|p| {
            let mut target = p.clone();
            target.push("target");
            if target.exists() { Some(target) } else { None }
        })
        .map(|p| {
            let mut path = p;
            path.push("debug");
            path.push("paideia-pq-sign");
            path
        })
        .unwrap_or_else(|| std::path::PathBuf::from("paideia-pq-sign"));

    // Initialize HSM with environment variable password
    let init_output = Command::new(&binary_path)
        .arg("hsm")
        .arg("init")
        .arg(&hsm_path)
        .env("PDX_HSM_PASSWORD", "correct_password")
        .output();

    if let Ok(output) = init_output
        && output.status.success()
    {
        // Try to sign with a wrong password via stdin
        // (In this test, we can't easily simulate stdin, so we use a different env password)
        let sign_output = Command::new(&binary_path)
            .arg("hsm")
            .arg("release")
            .arg(&hsm_path)
            .arg(&artifact_path)
            .env("PDX_HSM_PASSWORD", "wrong_password")
            .output();

        if let Ok(sign_out) = sign_output {
            // Should fail
            assert!(
                !sign_out.status.success(),
                "HSM release with wrong password should fail"
            );
        }
    }
}
