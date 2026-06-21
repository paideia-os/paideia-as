//! Verify the mini-lexer fixture parses cleanly via paideia-as check.

#[test]
fn mini_lexer_pdx_parses_cleanly() {
    let pdx = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/mini_lexer.pdx");
    assert!(pdx.exists(), "mini_lexer.pdx must exist");
}

#[test]
#[ignore = "needs paideia-as binary; run with --ignored after cargo build --release -p paideia-as"]
fn mini_lexer_pdx_parses_via_paideia_as_check() {
    let bin = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/release/paideia-as");
    if !bin.exists() {
        eprintln!("paideia-as not built; skipping");
        return;
    }
    let pdx = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdx/mini_lexer.pdx");
    let out = std::process::Command::new(&bin)
        .args(["check", &pdx.to_string_lossy()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
