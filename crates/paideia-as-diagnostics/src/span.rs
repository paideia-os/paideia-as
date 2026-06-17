use core::num::NonZeroU32;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A stable file identifier assigned by the source map.
///
/// Uniquely identifies a source file within a diagnostic context.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug, Serialize, Deserialize)]
pub struct FileId(NonZeroU32);

impl FileId {
    /// Creates a new `FileId` from a 1-based file number, or `None` if the number is 0.
    #[must_use]
    pub fn new(n: u32) -> Option<Self> {
        NonZeroU32::new(n).map(Self)
    }

    /// Returns the underlying 1-based file number.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl fmt::Display for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0.get())
    }
}

/// A contiguous byte range within a single source file.
///
/// Spans record the file identifier and the byte position (start and length).
/// All byte offsets are 0-based; byte_len is the count of bytes.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct Span {
    file: FileId,
    byte_start: u32,
    byte_len: u32,
}

impl Span {
    /// Creates a new span with the given file, byte start, and byte length.
    #[must_use]
    pub fn new(file: FileId, byte_start: u32, byte_len: u32) -> Self {
        Self {
            file,
            byte_start,
            byte_len,
        }
    }

    /// Returns the file identifier for this span.
    #[must_use]
    pub fn file(self) -> FileId {
        self.file
    }

    /// Returns the starting byte offset (0-based) of this span.
    #[must_use]
    pub fn byte_start(self) -> u32 {
        self.byte_start
    }

    /// Returns the byte length of this span.
    #[must_use]
    pub fn byte_len(self) -> u32 {
        self.byte_len
    }

    /// Returns the ending byte offset (exclusive) of this span.
    ///
    /// May panic on overflow if `byte_start + byte_len > u32::MAX`.
    /// This is acceptable because source files are capped at u32::MAX bytes (~4 GiB);
    /// the lexer reports E-category errors on overflow (see PR-8).
    #[must_use]
    pub fn byte_end(self) -> u32 {
        self.byte_start + self.byte_len
    }

    /// Computes the minimal span containing both this span and another.
    ///
    /// # Panics
    ///
    /// Panics if `self.file() != other.file()`. The panic message will be:
    /// `"Span::merge: cross-file merge from {self.file} to {other.file}"`.
    #[must_use]
    pub fn merge(self, other: Span) -> Span {
        if self.file != other.file {
            panic!(
                "Span::merge: cross-file merge from {} to {}",
                self.file, other.file
            );
        }
        let start = self.byte_start.min(other.byte_start);
        let end = self.byte_end().max(other.byte_end());
        Span::new(self.file, start, end - start)
    }
}

use core::mem::size_of;
use static_assertions::const_assert_eq;

const_assert_eq!(size_of::<Span>(), 12);
const_assert_eq!(size_of::<Option<FileId>>(), 4);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_id_zero_rejected() {
        assert!(FileId::new(0).is_none());
    }

    #[test]
    fn file_id_one_accepted() {
        assert_eq!(FileId::new(1).unwrap().get(), 1);
    }

    #[test]
    fn file_id_display() {
        assert_eq!(format!("{}", FileId::new(7).unwrap()), "#7");
    }

    #[test]
    fn span_size_is_12() {
        assert_eq!(size_of::<Span>(), 12);
    }

    #[test]
    fn span_is_copy() {
        fn _assert<T: Copy>() {}
        _assert::<Span>();
    }

    #[test]
    fn merge_idempotent() {
        let f = FileId::new(1).unwrap();
        let a = Span::new(f, 10, 5);
        assert_eq!(a.merge(a), a);
    }

    #[test]
    fn merge_commutative() {
        let f = FileId::new(1).unwrap();
        let a = Span::new(f, 10, 5);
        let b = Span::new(f, 8, 10);
        assert_eq!(a.merge(b), b.merge(a));
    }

    #[test]
    fn merge_disjoint_covers_gap() {
        let f = FileId::new(1).unwrap();
        let a = Span::new(f, 0, 3);
        let b = Span::new(f, 7, 5);
        let merged = a.merge(b);
        assert_eq!(merged.byte_start(), 0);
        assert_eq!(merged.byte_len(), 12);
    }

    #[test]
    #[should_panic(expected = "cross-file merge")]
    fn merge_cross_file_panics() {
        let f1 = FileId::new(1).unwrap();
        let f2 = FileId::new(2).unwrap();
        let a = Span::new(f1, 0, 3);
        let b = Span::new(f2, 0, 3);
        let _ = a.merge(b);
    }

    #[test]
    fn byte_end_simple() {
        let f = FileId::new(1).unwrap();
        let s = Span::new(f, 10, 5);
        assert_eq!(s.byte_end(), 15);
    }
}
