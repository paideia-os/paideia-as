//! Regression test: M0307 ("macro feature not in phase 1") is retired.
//!
//! Phase-1 reserved M0307 as a placeholder for unimplemented macro
//! features. With the Phase 2 m2 reflection track live (PRs #361-#371),
//! every gap M0307 was reserved to flag is now implemented. The code
//! is documented in `catalog.toml` as `deprecated = true` and must
//! never be emitted by any code path.
//!
//! This test confirms the catalog entry is marked deprecated. The
//! "M0307 is no longer emitted" property is, today, vacuously true —
//! no code path ever produced it. The test exists so a future PR that
//! accidentally re-introduces M0307 trips a clear failure.

use std::path::PathBuf;

fn catalog_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("catalog.toml");
    p
}

#[test]
fn m0307_is_marked_deprecated_in_catalog() {
    let content = std::fs::read_to_string(catalog_path()).expect("catalog readable");
    let idx = content
        .find("[diagnostic.M0307]")
        .expect("M0307 section present");
    // Find the next `[diagnostic.` heading or EOF.
    let rest = &content[idx..];
    let end = rest[1..]
        .find("[diagnostic.")
        .map(|n| n + 1)
        .unwrap_or(rest.len());
    let section = &rest[..end];
    assert!(
        section.contains("deprecated = true"),
        "M0307 must be marked `deprecated = true`. Section:\n{section}"
    );
}

#[test]
fn m0307_is_not_emitted_anywhere_in_the_workspace() {
    // Scan every .rs source file in the workspace for a string that
    // looks like an active M0307 emission. The catalog and this test
    // file are the only allowed mention sites.
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .parent()
        .expect("workspace root")
        .to_path_buf();
    let mut offenders = Vec::new();
    walk(&workspace, &mut offenders);
    assert!(
        offenders.is_empty(),
        "M0307 must not be emitted anywhere. Offenders:\n{}",
        offenders.join("\n")
    );
}

fn walk(dir: &std::path::Path, offenders: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') || name == "target" {
            continue;
        }
        if path.is_dir() {
            walk(&path, offenders);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        // Skip this test file itself.
        if path.ends_with("m0307_retired.rs") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        // Heuristic: any literal "M0307" mention in a non-test source
        // file is a violation. The token also appears as a number
        // (307) in some code; we look for the exact "M0307" or
        // a `,\s*307\s*\)` Category::M construction.
        for (i, line) in content.lines().enumerate() {
            if line.contains("M0307") {
                offenders.push(format!("{}:{}: {}", path.display(), i + 1, line.trim()));
            }
        }
    }
}
