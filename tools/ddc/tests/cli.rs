use std::io::Write;
use std::process::Command;

#[test]
fn cli_diff_identical_files_exits_zero() {
    let tmp = tempfile::tempdir().unwrap();
    let file_a = tmp.path().join("a");
    let file_b = tmp.path().join("b");
    let allowlist = tmp.path().join("allowlist.toml");
    let content = b"identical content";

    let mut f = std::fs::File::create(&file_a).unwrap();
    f.write_all(content).unwrap();
    let mut f = std::fs::File::create(&file_b).unwrap();
    f.write_all(content).unwrap();
    let mut f = std::fs::File::create(&allowlist).unwrap();
    f.write_all(b"# Empty allowlist\n[[rules]]\nname = \"placeholder\"\nstart = 999999\nend = 999999\nreason = \"placeholder rule\"\n").unwrap();

    let output = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "ddc-diff",
            "--",
            file_a.to_str().unwrap(),
            file_b.to_str().unwrap(),
            allowlist.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
}

#[test]
fn cli_diff_divergent_files_exits_nonzero() {
    let tmp = tempfile::tempdir().unwrap();
    let file_a = tmp.path().join("a");
    let file_b = tmp.path().join("b");
    let allowlist = tmp.path().join("allowlist.toml");

    let mut f = std::fs::File::create(&file_a).unwrap();
    f.write_all(b"content_a").unwrap();
    let mut f = std::fs::File::create(&file_b).unwrap();
    f.write_all(b"content_b").unwrap();
    let mut f = std::fs::File::create(&allowlist).unwrap();
    f.write_all(b"# Empty allowlist\n[[rules]]\nname = \"placeholder\"\nstart = 999999\nend = 999999\nreason = \"placeholder rule\"\n").unwrap();

    let output = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "ddc-diff",
            "--",
            file_a.to_str().unwrap(),
            file_b.to_str().unwrap(),
            allowlist.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
}
