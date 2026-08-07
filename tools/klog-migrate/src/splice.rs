//! Source-buffer splice logic.
//!
//! Applies a slice of [`Replacement`]s to a source buffer. Overlapping
//! replacements are a bug in the scanner and produce [`SpliceError::Overlap`].

/// A single byte-range replacement in a source buffer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Replacement {
    /// Byte offset of the first character to replace.
    pub byte_start: usize,
    /// Byte offset one past the last character to replace.
    pub byte_end: usize,
    /// Text to splice in place of `byte_start..byte_end`.
    pub new_text: String,
}

/// Errors that can arise while applying replacements.
#[derive(Debug, Eq, PartialEq)]
pub enum SpliceError {
    /// Two replacements overlap in the source. Fields identify the pair.
    Overlap {
        /// Byte end of the earlier replacement.
        earlier_end: usize,
        /// Byte start of the later replacement.
        later_start: usize,
    },
    /// A replacement's `byte_start > byte_end`.
    InvertedRange {
        /// The offending start.
        byte_start: usize,
        /// The offending end.
        byte_end: usize,
    },
    /// A replacement's end offset is past the source length.
    OutOfBounds {
        /// The offending end.
        byte_end: usize,
        /// The source's byte length.
        source_len: usize,
    },
}

impl std::fmt::Display for SpliceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Overlap {
                earlier_end,
                later_start,
            } => write!(
                f,
                "overlapping replacements: earlier ends at {earlier_end}, later starts at {later_start}"
            ),
            Self::InvertedRange {
                byte_start,
                byte_end,
            } => write!(
                f,
                "inverted replacement range: start={byte_start} > end={byte_end}"
            ),
            Self::OutOfBounds {
                byte_end,
                source_len,
            } => write!(
                f,
                "replacement end {byte_end} past source length {source_len}"
            ),
        }
    }
}

impl std::error::Error for SpliceError {}

/// Apply a slice of replacements to a source buffer.
///
/// Replacements need not be sorted; this function sorts them internally
/// (stable) by `byte_start` and applies them **right-to-left** so earlier
/// byte offsets remain valid across length deltas.
///
/// # Errors
///
/// - [`SpliceError::Overlap`] if any two replacements overlap in the source.
/// - [`SpliceError::InvertedRange`] if `byte_start > byte_end`.
/// - [`SpliceError::OutOfBounds`] if `byte_end > source.len()`.
pub fn apply_replacements(source: &str, reps: &[Replacement]) -> Result<String, SpliceError> {
    if reps.is_empty() {
        return Ok(source.to_owned());
    }

    let mut sorted: Vec<&Replacement> = reps.iter().collect();
    sorted.sort_by_key(|r| r.byte_start);

    let source_len = source.len();
    for r in &sorted {
        if r.byte_start > r.byte_end {
            return Err(SpliceError::InvertedRange {
                byte_start: r.byte_start,
                byte_end: r.byte_end,
            });
        }
        if r.byte_end > source_len {
            return Err(SpliceError::OutOfBounds {
                byte_end: r.byte_end,
                source_len,
            });
        }
    }
    for pair in sorted.windows(2) {
        if pair[0].byte_end > pair[1].byte_start {
            return Err(SpliceError::Overlap {
                earlier_end: pair[0].byte_end,
                later_start: pair[1].byte_start,
            });
        }
    }

    // Apply right-to-left over the original byte offsets.
    let mut out = String::with_capacity(source_len);
    out.push_str(source);
    for r in sorted.iter().rev() {
        out.replace_range(r.byte_start..r.byte_end, &r.new_text);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_replacements_returns_source() {
        let out = apply_replacements("abc", &[]).unwrap();
        assert_eq!(out, "abc");
    }

    #[test]
    fn single_replacement_replaces() {
        let reps = vec![Replacement {
            byte_start: 0,
            byte_end: 3,
            new_text: "XYZ".to_owned(),
        }];
        let out = apply_replacements("abc", &reps).unwrap();
        assert_eq!(out, "XYZ");
    }

    #[test]
    fn two_replacements_apply_right_to_left() {
        // "abcdef" -> replace 0..1 with "AA", 3..4 with "DDDD"
        // Left-to-right would fail to compute offsets; right-to-left works.
        let reps = vec![
            Replacement {
                byte_start: 0,
                byte_end: 1,
                new_text: "AA".to_owned(),
            },
            Replacement {
                byte_start: 3,
                byte_end: 4,
                new_text: "DDDD".to_owned(),
            },
        ];
        let out = apply_replacements("abcdef", &reps).unwrap();
        assert_eq!(out, "AAbcDDDDef");
    }

    #[test]
    fn reps_get_sorted_before_apply() {
        // Same as above but passed out of order.
        let reps = vec![
            Replacement {
                byte_start: 3,
                byte_end: 4,
                new_text: "DDDD".to_owned(),
            },
            Replacement {
                byte_start: 0,
                byte_end: 1,
                new_text: "AA".to_owned(),
            },
        ];
        let out = apply_replacements("abcdef", &reps).unwrap();
        assert_eq!(out, "AAbcDDDDef");
    }

    #[test]
    fn overlap_is_error() {
        let reps = vec![
            Replacement {
                byte_start: 0,
                byte_end: 3,
                new_text: "X".to_owned(),
            },
            Replacement {
                byte_start: 2,
                byte_end: 4,
                new_text: "Y".to_owned(),
            },
        ];
        assert!(matches!(
            apply_replacements("abcdef", &reps).unwrap_err(),
            SpliceError::Overlap { .. }
        ));
    }

    #[test]
    fn inverted_range_is_error() {
        let reps = vec![Replacement {
            byte_start: 3,
            byte_end: 1,
            new_text: "".to_owned(),
        }];
        assert!(matches!(
            apply_replacements("abcdef", &reps).unwrap_err(),
            SpliceError::InvertedRange { .. }
        ));
    }

    #[test]
    fn out_of_bounds_is_error() {
        let reps = vec![Replacement {
            byte_start: 0,
            byte_end: 99,
            new_text: "".to_owned(),
        }];
        assert!(matches!(
            apply_replacements("abcdef", &reps).unwrap_err(),
            SpliceError::OutOfBounds { .. }
        ));
    }

    #[test]
    fn adjacent_non_overlap_is_fine() {
        let reps = vec![
            Replacement {
                byte_start: 0,
                byte_end: 3,
                new_text: "X".to_owned(),
            },
            Replacement {
                byte_start: 3,
                byte_end: 6,
                new_text: "Y".to_owned(),
            },
        ];
        let out = apply_replacements("abcdef", &reps).unwrap();
        assert_eq!(out, "XY");
    }
}
