//! Parser for the `tests/build-emit/*.expected_bytes.txt` sidecar format.
//!
//! Format: hex bytes, one or many per line, whitespace-separated. Lines
//! starting with `;` are comments; blank lines are ignored. Historically
//! this parser was open-coded in every test that consumed a sidecar; the
//! copies were byte-identical.

use std::path::Path;

/// Parse the expected-bytes format from a string.
pub fn parse(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') {
            continue;
        }
        for hex in trimmed.split_whitespace() {
            if let Ok(b) = u8::from_str_radix(hex, 16) {
                out.push(b);
            }
        }
    }
    out
}

/// Convenience: read a sidecar and parse it in one step.
pub fn load(path: &Path) -> Vec<u8> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    parse(&text)
}
