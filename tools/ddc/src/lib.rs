//! ddc — diverse-double-compilation differ.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod allowlist;

use std::path::Path;

/// A byte-level divergence between two builds.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Divergence {
    /// Byte offset where the divergence occurs.
    pub offset: u64,
    /// Byte value from build A.
    pub byte_a: u8,
    /// Byte value from build B.
    pub byte_b: u8,
    /// Whether this divergence is covered by the allowlist.
    pub allowlisted: bool,
    /// The allowlist rule that covered it, if any.
    pub allowlist_rule: Option<String>,
}

/// The result of a comparison between two binaries.
#[derive(Clone, Debug, serde::Serialize)]
pub struct DiffReport {
    /// Path to binary A.
    pub path_a: String,
    /// Path to binary B.
    pub path_b: String,
    /// Size of binary A.
    pub size_a: u64,
    /// Size of binary B.
    pub size_b: u64,
    /// All divergences found.
    pub divergences: Vec<Divergence>,
    /// Count of divergences covered by allowlist.
    pub allowlisted_count: u64,
    /// Count of divergences not covered by allowlist.
    pub unallowlisted_count: u64,
    /// Whether binaries match modulo the allowlist.
    pub match_modulo_allowlist: bool,
}

/// Compare two files byte-by-byte; check each divergence against the
/// allowlist; produce a DiffReport.
pub fn diff_files(
    path_a: &Path,
    path_b: &Path,
    allowlist: &allowlist::Allowlist,
) -> std::io::Result<DiffReport> {
    let a = std::fs::read(path_a)?;
    let b = std::fs::read(path_b)?;
    let size_a = a.len() as u64;
    let size_b = b.len() as u64;
    let min_len = a.len().min(b.len());

    let mut divergences = Vec::new();
    let mut allowlisted_count = 0u64;
    let mut unallowlisted_count = 0u64;

    for i in 0..min_len {
        if a[i] != b[i] {
            let (allowlisted, rule) = allowlist.check(i as u64);
            divergences.push(Divergence {
                offset: i as u64,
                byte_a: a[i],
                byte_b: b[i],
                allowlisted,
                allowlist_rule: rule,
            });
            if allowlisted {
                allowlisted_count += 1;
            } else {
                unallowlisted_count += 1;
            }
        }
    }

    if size_a != size_b {
        // Size mismatch is a structural divergence — not covered by
        // allowlist (offsets only).
        for i in min_len..(size_a.max(size_b) as usize) {
            let byte_a = if i < a.len() { a[i] } else { 0 };
            let byte_b = if i < b.len() { b[i] } else { 0 };
            divergences.push(Divergence {
                offset: i as u64,
                byte_a,
                byte_b,
                allowlisted: false,
                allowlist_rule: None,
            });
            unallowlisted_count += 1;
        }
    }

    let match_modulo_allowlist = unallowlisted_count == 0;

    Ok(DiffReport {
        path_a: path_a.display().to_string(),
        path_b: path_b.display().to_string(),
        size_a,
        size_b,
        divergences,
        allowlisted_count,
        unallowlisted_count,
        match_modulo_allowlist,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn diff_identical_files_returns_no_divergences() {
        let tmp = tempfile::tempdir().unwrap();
        let file_a = tmp.path().join("a");
        let file_b = tmp.path().join("b");
        let content = b"identical content";

        let mut f = std::fs::File::create(&file_a).unwrap();
        f.write_all(content).unwrap();
        let mut f = std::fs::File::create(&file_b).unwrap();
        f.write_all(content).unwrap();

        let allowlist = allowlist::Allowlist::default();
        let report = diff_files(&file_a, &file_b, &allowlist).unwrap();

        assert!(report.divergences.is_empty());
        assert_eq!(report.allowlisted_count, 0);
        assert_eq!(report.unallowlisted_count, 0);
        assert!(report.match_modulo_allowlist);
    }

    #[test]
    fn diff_different_files_returns_divergences() {
        let tmp = tempfile::tempdir().unwrap();
        let file_a = tmp.path().join("a");
        let file_b = tmp.path().join("b");

        let mut f = std::fs::File::create(&file_a).unwrap();
        f.write_all(b"content_a").unwrap();
        let mut f = std::fs::File::create(&file_b).unwrap();
        f.write_all(b"content_b").unwrap();

        let allowlist = allowlist::Allowlist::default();
        let report = diff_files(&file_a, &file_b, &allowlist).unwrap();

        assert!(!report.divergences.is_empty());
        // "content_a" vs "content_b": they differ only at index 7 ('a' vs 'b')
        assert_eq!(report.unallowlisted_count, 1);
        assert!(!report.match_modulo_allowlist);
    }

    #[test]
    fn diff_size_mismatch_records_extra_bytes_as_divergences() {
        let tmp = tempfile::tempdir().unwrap();
        let file_a = tmp.path().join("a");
        let file_b = tmp.path().join("b");

        let mut f = std::fs::File::create(&file_a).unwrap();
        f.write_all(b"short").unwrap();
        let mut f = std::fs::File::create(&file_b).unwrap();
        f.write_all(b"much longer content").unwrap();

        let allowlist = allowlist::Allowlist::default();
        let report = diff_files(&file_a, &file_b, &allowlist).unwrap();

        assert_eq!(report.size_a, 5);
        assert_eq!(report.size_b, 19);
        assert!(!report.divergences.is_empty());
        // Extra bytes from file_b are treated as divergences
        assert!(!report.match_modulo_allowlist);
    }

    #[test]
    fn diff_with_allowlist_marks_covered_divergences() {
        let tmp = tempfile::tempdir().unwrap();
        let file_a = tmp.path().join("a");
        let file_b = tmp.path().join("b");

        let mut f = std::fs::File::create(&file_a).unwrap();
        f.write_all(b"test").unwrap();
        let mut f = std::fs::File::create(&file_b).unwrap();
        f.write_all(b"tast").unwrap(); // differs at byte 1: 'e' (101) vs 'a' (97)

        let allowlist = allowlist::Allowlist {
            rules: vec![allowlist::AllowlistRule {
                name: "test-rule".to_string(),
                start: 1,
                end: 1,
                reason: "test divergence".to_string(),
            }],
        };

        let report = diff_files(&file_a, &file_b, &allowlist).unwrap();

        assert_eq!(report.divergences.len(), 1);
        assert_eq!(report.allowlisted_count, 1);
        assert_eq!(report.unallowlisted_count, 0);
        assert!(report.match_modulo_allowlist);
        assert!(report.divergences[0].allowlisted);
        assert_eq!(report.divergences[0].offset, 1);
        assert_eq!(report.divergences[0].byte_a, b'e');
        assert_eq!(report.divergences[0].byte_b, b'a');
    }
}
