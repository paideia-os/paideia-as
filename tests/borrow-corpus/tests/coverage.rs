//! Regression: every S09xx borrow-check code has ≥2 reject fixtures.

use std::fs;

const BORROW_CODES: &[&str] = &["S0906", "S0907", "S0908", "S0909"];

#[test]
fn every_borrow_code_has_at_least_two_reject_fixtures() {
    let corpus_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpus");
    for &code in BORROW_CODES {
        let mut count = 0;
        for entry in fs::read_dir(&corpus_dir).expect("read corpus") {
            let entry = entry.expect("entry");
            if !entry.file_name().to_string_lossy().starts_with("r_") {
                continue;
            }
            // Read .expect file (paired sibling) and check it lists `code`.
            let pdx_path = entry.path();
            let mut expect = pdx_path.clone();
            expect.set_extension("pdx.expect");
            if !expect.exists() {
                let mut alt = pdx_path.clone();
                alt.set_extension("expect");
                if alt.exists() {
                    expect = alt;
                }
            }
            if let Ok(content) = fs::read_to_string(&expect) {
                if content.contains(code) {
                    count += 1;
                }
            }
        }
        assert!(
            count >= 2,
            "code {} has only {} reject fixtures; need ≥2",
            code,
            count
        );
    }
}
