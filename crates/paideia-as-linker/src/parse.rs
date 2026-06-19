//! Parse phase of the paideia-link linker.
//!
//! Reads each input PAX file, validates its header magic + version,
//! and produces a `ParsedPax` containing the header + section table +
//! per-section content slices (borrowed from the input bytes).

use paideia_as_emitter_pax::{PAX_HEADER_SIZE, PaxHeader, Section, SectionTable};
use std::path::PathBuf;

/// One parsed PAX input.
#[derive(Debug, Clone)]
pub struct ParsedPax {
    /// Path to the parsed PAX file.
    pub path: PathBuf,
    /// Parsed PAX header from the file.
    pub header: PaxHeader,
    /// Parsed section table from the file.
    pub section_table: SectionTable,
    /// All raw bytes of the input. Section content is a sub-slice of this.
    pub bytes: Vec<u8>,
}

impl ParsedPax {
    /// Return the content bytes of `section`, sliced out of `bytes`.
    /// Returns an empty slice for BSS / no-content sections.
    pub fn section_content<'a>(&'a self, section: &Section) -> &'a [u8] {
        if section.content_size == 0 {
            return &[];
        }
        let start = section.content_offset as usize;
        let end = start + section.content_size as usize;
        &self.bytes[start..end.min(self.bytes.len())]
    }
}

/// Errors that may occur during parse phase.
#[derive(Debug, Clone)]
pub enum ParseError {
    /// I/O error reading file.
    Io(String),
    /// Magic number does not match PAX_MAGIC.
    NotPax,
    /// Format version is not supported (currently only 1 is supported).
    UnsupportedVersion(u16),
    /// Input bytes are shorter than PAX_HEADER_SIZE.
    TruncatedHeader,
    /// Input bytes are shorter than required for section table.
    TruncatedSectionTable,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Io(msg) => write!(f, "I/O error: {}", msg),
            ParseError::NotPax => write!(f, "Not a valid PAX file (bad magic)"),
            ParseError::UnsupportedVersion(v) => write!(f, "Unsupported PAX version: {}", v),
            ParseError::TruncatedHeader => write!(f, "File too short for PAX header"),
            ParseError::TruncatedSectionTable => write!(f, "File too short for section table"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parse a single .pax file.
pub fn parse_pax(path: PathBuf) -> Result<ParsedPax, ParseError> {
    let bytes = std::fs::read(&path).map_err(|e| ParseError::Io(e.to_string()))?;

    // Validate header size
    if bytes.len() < PAX_HEADER_SIZE {
        return Err(ParseError::TruncatedHeader);
    }

    // Parse header
    let header = PaxHeader::from_bytes(&bytes).ok_or(ParseError::NotPax)?;

    // Check format version
    if header.format_version != paideia_as_emitter_pax::PAX_FORMAT_VERSION {
        return Err(ParseError::UnsupportedVersion(header.format_version));
    }

    // Parse section table
    let section_table_start = header.section_table_offset as usize;
    let section_count = header.section_count;

    if section_count == 0 {
        let section_table = SectionTable::new();
        return Ok(ParsedPax {
            path,
            header,
            section_table,
            bytes,
        });
    }

    let section_table_bytes = &bytes[section_table_start..];
    let section_table = SectionTable::from_bytes(section_table_bytes, section_count)
        .ok_or(ParseError::TruncatedSectionTable)?;

    Ok(ParsedPax {
        path,
        header,
        section_table,
        bytes,
    })
}

/// Parse multiple .pax inputs in order. Stops at the first error.
pub fn parse_inputs(paths: &[PathBuf]) -> Result<Vec<ParsedPax>, ParseError> {
    let mut result = Vec::with_capacity(paths.len());
    for path in paths {
        result.push(parse_pax(path.clone())?);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_emitter_pax::Architecture;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn build_minimal_pax_bytes() -> Vec<u8> {
        // Build a minimal valid PAX in-memory.
        // Header (96 bytes) + empty section table (0 sections)
        let mut header = PaxHeader::new(Architecture::X86_64);
        header.section_table_offset = PAX_HEADER_SIZE as u64;
        header.section_count = 0;
        header.blake3_content_hash = *b"0123456789ABCDEF0123456789ABCDEF";

        let mut bytes = vec![];
        bytes.extend_from_slice(&header.to_bytes());

        bytes
    }

    fn write_tempfile(content: &[u8]) -> std::io::Result<PathBuf> {
        let counter = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let temp_dir = std::env::temp_dir();
        let filename = format!("paideia_as_linker_test_{}.pax", counter);
        let path = temp_dir.join(&filename);

        let mut file = std::fs::File::create(&path)?;
        file.write_all(content)?;

        Ok(path)
    }

    #[test]
    fn parse_minimal_pax_succeeds() {
        let bytes = build_minimal_pax_bytes();
        let path = write_tempfile(&bytes).expect("failed to write tempfile");

        let result = parse_pax(path.clone());
        assert!(result.is_ok(), "parse should succeed for minimal valid PAX");

        let parsed = result.unwrap();
        assert_eq!(parsed.path, path);
        assert_eq!(parsed.header.magic, *b"PAX\0");
        assert_eq!(parsed.header.format_version, 1);
        assert_eq!(parsed.section_table.len(), 0);
    }

    #[test]
    fn parse_non_pax_returns_not_pax() {
        let bytes = vec![0u8; 96];
        let path = write_tempfile(&bytes).expect("failed to write tempfile");

        let result = parse_pax(path);
        assert!(matches!(result, Err(ParseError::NotPax)));
    }

    #[test]
    fn parse_truncated_header_errors() {
        let bytes = vec![0u8; 50];
        let path = write_tempfile(&bytes).expect("failed to write tempfile");

        let result = parse_pax(path);
        assert!(matches!(result, Err(ParseError::TruncatedHeader)));
    }

    #[test]
    fn parse_unsupported_version_errors() {
        let mut header = PaxHeader::new(Architecture::X86_64);
        header.format_version = 99;
        header.section_table_offset = PAX_HEADER_SIZE as u64;
        header.section_count = 0;

        let bytes: Vec<u8> = header.to_bytes().to_vec();
        let path = write_tempfile(&bytes).expect("failed to write tempfile");

        let result = parse_pax(path);
        assert!(matches!(result, Err(ParseError::UnsupportedVersion(99))));
    }

    #[test]
    fn parse_2_inputs_returns_both() {
        let bytes1 = build_minimal_pax_bytes();
        let bytes2 = build_minimal_pax_bytes();

        let path1 = write_tempfile(&bytes1).expect("failed to write tempfile");
        let path2 = write_tempfile(&bytes2).expect("failed to write tempfile");

        let paths = vec![path1.clone(), path2.clone()];
        let result = parse_inputs(&paths);

        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].path, path1);
        assert_eq!(parsed[1].path, path2);
    }

    #[test]
    fn parsed_pax_section_content_returns_correct_slice() {
        // Create a PAX with one section containing 32 bytes.
        let section_content = vec![0xABu8; 32];
        let mut header = PaxHeader::new(Architecture::X86_64);
        header.section_table_offset = PAX_HEADER_SIZE as u64;
        header.section_count = 1;
        header.blake3_content_hash = *b"0123456789ABCDEF0123456789ABCDEF";

        let section_table_offset = PAX_HEADER_SIZE as u64;
        let section_content_offset = section_table_offset + 64; // After header and one section descriptor

        let section = Section::code(section_content_offset, section_content.len() as u64);

        let mut bytes = vec![];
        bytes.extend_from_slice(&header.to_bytes());

        let mut section_table = SectionTable::new();
        section_table.push(section.clone());
        bytes.extend_from_slice(&section_table.to_bytes());
        bytes.extend_from_slice(&section_content);

        let path = write_tempfile(&bytes).expect("failed to write tempfile");
        let parsed = parse_pax(path).expect("parse should succeed");

        let retrieved_content = parsed.section_content(&parsed.section_table.sections[0]);
        assert_eq!(retrieved_content, section_content.as_slice());
    }

    #[test]
    fn parsed_pax_round_trip_via_bytes() {
        let bytes = build_minimal_pax_bytes();
        let path = write_tempfile(&bytes).expect("failed to write tempfile");

        let parsed = parse_pax(path).expect("parse should succeed");

        // Re-serialize header and section table
        let mut reserialized = vec![];
        reserialized.extend_from_slice(&parsed.header.to_bytes());
        reserialized.extend_from_slice(&parsed.section_table.to_bytes());

        // Should match the original input bytes
        assert_eq!(reserialized, bytes);
    }

    #[test]
    fn snapshot_2_input_link_descriptor() {
        let bytes1 = build_minimal_pax_bytes();
        let bytes2 = build_minimal_pax_bytes();

        let path1 = write_tempfile(&bytes1).expect("failed to write tempfile");
        let path2 = write_tempfile(&bytes2).expect("failed to write tempfile");

        let paths = vec![path1.clone(), path2.clone()];
        let parsed_list = parse_inputs(&paths).expect("parse_inputs should succeed");

        // Verify first input
        let p1 = &parsed_list[0];
        assert_eq!(p1.path, path1);
        assert_eq!(p1.header.format_version, 1);
        assert_eq!(p1.section_table.len(), 0);

        // Verify second input
        let p2 = &parsed_list[1];
        assert_eq!(p2.path, path2);
        assert_eq!(p2.header.format_version, 1);
        assert_eq!(p2.section_table.len(), 0);
    }
}
