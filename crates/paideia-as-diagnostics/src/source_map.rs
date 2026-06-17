use crate::span::FileId;
use std::path::{Path, PathBuf};

/// A line and column position within a source file.
///
/// Both line and column are 1-based. Column is a 1-based character index
/// (counting Unicode scalar values), not a byte offset.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct LineCol {
    /// 1-based line number.
    pub line: u32,
    /// 1-based character index within the line.
    pub col: u32,
}

impl LineCol {
    /// Creates a new line/column pair (both 1-based).
    #[must_use]
    pub fn new(line: u32, col: u32) -> Self {
        Self { line, col }
    }
}

/// Maps source file paths and contents to byte positions and vice versa.
///
/// Maintains aligned vectors of file paths and contents, assigning stable
/// `FileId` values sequentially as files are added.
pub struct SourceMap {
    paths: Vec<PathBuf>,
    contents: Vec<String>,
}

impl SourceMap {
    /// Creates an empty source map.
    #[must_use]
    pub fn new() -> Self {
        Self {
            paths: Vec::new(),
            contents: Vec::new(),
        }
    }

    /// Adds a file to the source map and returns its `FileId`.
    ///
    /// Returns `FileId::new((self.paths.len() + 1) as u32).unwrap()`.
    pub fn add_file(&mut self, path: PathBuf, content: String) -> FileId {
        self.paths.push(path);
        self.contents.push(content);
        FileId::new(self.paths.len() as u32).unwrap()
    }

    /// Returns the file path for the given `FileId`, or panics if the ID is invalid.
    #[must_use]
    pub fn path(&self, id: FileId) -> &Path {
        &self.paths[id.get() as usize - 1]
    }

    /// Returns the file contents for the given `FileId`, or panics if the ID is invalid.
    #[must_use]
    pub fn content(&self, id: FileId) -> &str {
        &self.contents[id.get() as usize - 1]
    }

    /// Converts a byte offset within a file to a line/column position.
    ///
    /// Returns `None` if:
    /// - The `FileId` is out of range, or
    /// - The byte offset exceeds the file contents length.
    ///
    /// If the byte offset lands in the middle of a multi-byte UTF-8 sequence,
    /// it is rounded down to the nearest character boundary.
    ///
    /// Edge cases:
    /// - Empty content: `byte_to_line_col(id, 0)` returns `Some(LineCol { 1, 1 })`.
    ///   `byte_to_line_col(id, 1)` returns `None`.
    /// - End-of-file: `byte == content.len()` is valid and returns the position
    ///   after the last character.
    pub fn byte_to_line_col(&self, id: FileId, byte: u32) -> Option<LineCol> {
        let idx = id.get() as usize - 1;
        if idx >= self.contents.len() {
            return None;
        }

        let content = &self.contents[idx];
        if byte > content.len() as u32 {
            return None;
        }

        let byte = byte as usize;

        // Round down to a valid character boundary.
        let mut byte = byte;
        while byte > 0 && !content.is_char_boundary(byte) {
            byte -= 1;
        }

        // Count lines and track the byte offset of the start of the current line.
        let mut line_count = 1u32;
        let mut line_start_byte = 0usize;
        let mut current_byte = 0usize;

        for ch in content[..byte].chars() {
            if ch == '\n' {
                line_count += 1;
                line_start_byte = current_byte + ch.len_utf8();
            }
            current_byte += ch.len_utf8();
        }

        // Count characters from the line start to the target byte.
        let chars_in_line = content[line_start_byte..byte].chars().count() as u32;

        Some(LineCol {
            line: line_count,
            col: chars_in_line + 1,
        })
    }
}

impl Default for SourceMap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_file_returns_sequential_ids() {
        let mut sm = SourceMap::new();
        let id1 = sm.add_file(PathBuf::from("file1.pdx"), "content1".into());
        let id2 = sm.add_file(PathBuf::from("file2.pdx"), "content2".into());
        assert_eq!(id1.get(), 1);
        assert_eq!(id2.get(), 2);
    }

    #[test]
    fn path_content_accessors() {
        let mut sm = SourceMap::new();
        let id = sm.add_file(PathBuf::from("test.pdx"), "hello".into());
        assert_eq!(sm.path(id), Path::new("test.pdx"));
        assert_eq!(sm.content(id), "hello");
    }

    #[test]
    fn byte_to_line_col_ascii() {
        let mut sm = SourceMap::new();
        let id = sm.add_file(PathBuf::from("test.pdx"), "abc\ndef\nghi".into());

        // "abc\ndef\nghi"
        // Bytes:  0123456789...
        // Line 1: "abc" (bytes 0-2)
        // "\n" at byte 3
        // Line 2: "def" (bytes 4-6), byte 5 is 'e'
        assert_eq!(
            sm.byte_to_line_col(id, 5),
            Some(LineCol { line: 2, col: 2 })
        );
    }

    #[test]
    fn byte_to_line_col_utf8_kanji() {
        let mut sm = SourceMap::new();
        let content = "a家b\nc";
        let id = sm.add_file(PathBuf::from("test.pdx"), content.into());

        // "a家b\nc"
        // 'a' is 1 byte at 0
        // '家' is 3 bytes at 1..4
        // 'b' is 1 byte at 4
        // '\n' is 1 byte at 5
        // 'c' is 1 byte at 6

        assert_eq!(
            sm.byte_to_line_col(id, 0),
            Some(LineCol { line: 1, col: 1 })
        );
        assert_eq!(
            sm.byte_to_line_col(id, 1),
            Some(LineCol { line: 1, col: 2 })
        );
        assert_eq!(
            sm.byte_to_line_col(id, 4),
            Some(LineCol { line: 1, col: 3 })
        );
        assert_eq!(
            sm.byte_to_line_col(id, 6),
            Some(LineCol { line: 2, col: 1 })
        );
    }

    #[test]
    fn byte_to_line_col_mid_codepoint_rounds_down() {
        let mut sm = SourceMap::new();
        let content = "a家b";
        let id = sm.add_file(PathBuf::from("test.pdx"), content.into());

        // "a家b"
        // Byte 2 is in the middle of '家' (which spans bytes 1-3).
        // Should round down to byte 1, which is LineCol { 1, 2 }.
        assert_eq!(
            sm.byte_to_line_col(id, 2),
            Some(LineCol { line: 1, col: 2 })
        );
    }

    #[test]
    fn byte_to_line_col_unknown_file_returns_none() {
        let sm = SourceMap::new();
        let id = FileId::new(999).unwrap();
        assert_eq!(sm.byte_to_line_col(id, 0), None);
    }

    #[test]
    fn byte_to_line_col_out_of_range_returns_none() {
        let mut sm = SourceMap::new();
        let id = sm.add_file(PathBuf::from("test.pdx"), "hello".into());
        assert_eq!(sm.byte_to_line_col(id, 10), None);
    }

    #[test]
    fn byte_to_line_col_end_of_file() {
        let mut sm = SourceMap::new();
        let id = sm.add_file(PathBuf::from("test.pdx"), "hello".into());
        assert_eq!(
            sm.byte_to_line_col(id, 5),
            Some(LineCol { line: 1, col: 6 })
        );
    }

    #[test]
    fn byte_to_line_col_empty_content() {
        let mut sm = SourceMap::new();
        let id = sm.add_file(PathBuf::from("test.pdx"), "".into());
        assert_eq!(
            sm.byte_to_line_col(id, 0),
            Some(LineCol { line: 1, col: 1 })
        );
        assert_eq!(sm.byte_to_line_col(id, 1), None);
    }

    #[test]
    fn round_trip_span_to_linecol() {
        use crate::span::Span;

        let mut sm = SourceMap::new();
        let content = "a家b";
        let id = sm.add_file(PathBuf::from("test.pdx"), content.into());

        // '家' starts at byte 1 and is 3 bytes long.
        let span = Span::new(id, 1, 3);
        let line_col = sm.byte_to_line_col(id, span.byte_start());
        assert_eq!(line_col, Some(LineCol { line: 1, col: 2 }));
    }
}
