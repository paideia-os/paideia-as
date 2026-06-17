use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::ops::RangeInclusive;
use std::str::FromStr;
use thiserror::Error;

/// Diagnostic category letter defining the issue domain.
///
/// Each category corresponds to a letter and has a reserved numeric range.
/// See the category table in `design/toolchain/diagnostics.md`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Category {
    /// Encoding, lexer (range 0001–0099).
    E,
    /// Parser — grammar, syntax (range 0100–0299).
    P,
    /// Module system, imports, functors (range 0300–0499).
    M,
    /// Type system — HM + dependent + record (range 0500–0899).
    T,
    /// Substructural lattice — linearity, drops (range 0900–1099).
    S,
    /// Effect system — rows, handlers, closures (range 1100–1299).
    F,
    /// Capability discipline — kinds, rights, derivation (range 1300–1499).
    C,
    /// Optimization passes (range 1500–1599).
    O,
    /// Unsafe-block discipline (range 1600–1699).
    U,
    /// Binary emission — ELF, PE/COFF, PAX (range 1700–1799).
    B,
    /// DWARF / debug info (range 1800–1899).
    D,
    /// Linter / style (range 2000–2999).
    L,
    /// Workspace, build-graph (range 3000–3099).
    W,
    /// Runtime checks — LAM verification, capability runtime (range 3100–3199).
    R,
    /// Catch-all / experimental (range 9000–9099).
    Z,
}

impl Category {
    /// Returns the single uppercase ASCII letter for this category.
    #[must_use]
    pub fn letter(self) -> char {
        match self {
            Self::E => 'E',
            Self::P => 'P',
            Self::M => 'M',
            Self::T => 'T',
            Self::S => 'S',
            Self::F => 'F',
            Self::C => 'C',
            Self::O => 'O',
            Self::U => 'U',
            Self::B => 'B',
            Self::D => 'D',
            Self::L => 'L',
            Self::W => 'W',
            Self::R => 'R',
            Self::Z => 'Z',
        }
    }

    /// Attempts to parse a category from a single uppercase ASCII letter.
    ///
    /// Returns `None` if the character is not a recognized category letter.
    #[must_use]
    pub fn from_letter(c: char) -> Option<Self> {
        match c {
            'E' => Some(Self::E),
            'P' => Some(Self::P),
            'M' => Some(Self::M),
            'T' => Some(Self::T),
            'S' => Some(Self::S),
            'F' => Some(Self::F),
            'C' => Some(Self::C),
            'O' => Some(Self::O),
            'U' => Some(Self::U),
            'B' => Some(Self::B),
            'D' => Some(Self::D),
            'L' => Some(Self::L),
            'W' => Some(Self::W),
            'R' => Some(Self::R),
            'Z' => Some(Self::Z),
            _ => None,
        }
    }

    /// Returns the valid numeric range for diagnostic codes in this category.
    #[must_use]
    pub fn range(self) -> RangeInclusive<u16> {
        match self {
            Self::E => 1..=99,
            Self::P => 100..=299,
            Self::M => 300..=499,
            Self::T => 500..=899,
            Self::S => 900..=1099,
            Self::F => 1100..=1299,
            Self::C => 1300..=1499,
            Self::O => 1500..=1599,
            Self::U => 1600..=1699,
            Self::B => 1700..=1799,
            Self::D => 1800..=1899,
            Self::L => 2000..=2999,
            Self::W => 3000..=3099,
            Self::R => 3100..=3199,
            Self::Z => 9000..=9099,
        }
    }
}

/// Severity level for a diagnostic code.
///
/// Severity is represented by a single ASCII digit (0–4) in the canonical string form.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Severity {
    /// Error severity (digit 0).
    Error = 0,
    /// Warning severity (digit 1).
    Warning = 1,
    /// Note severity (digit 2).
    Note = 2,
    /// Hint severity (digit 3).
    Hint = 3,
    /// Lint severity (digit 4).
    Lint = 4,
}

/// Error type for diagnostic code parsing and construction.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum CodeParseError {
    /// The category letter was not recognized.
    #[error("unknown category letter '{letter}'")]
    UnknownCategory {
        /// The unrecognized letter.
        letter: char,
    },

    /// The numeric part is outside the category's valid range.
    #[error("diagnostic code {number} outside valid range for category {category:?}")]
    OutOfRange {
        /// The category with the invalid range.
        category: Category,
        /// The number outside the valid range.
        number: u16,
    },

    /// The input format does not match the expected pattern.
    #[error("invalid diagnostic code format: {reason}")]
    BadFormat {
        /// Description of the format error.
        reason: &'static str,
    },
}

/// Stable diagnostic identifier.
///
/// The wire form is `<Letter><NNNN>` (5 characters), e.g. `E0001`, per
/// `design/toolchain/diagnostics.md` §1 (decision DI-D1). Severity is
/// **not** part of the wire form — it is metadata stored in the catalog
/// (`catalog.toml`, §3) and on the in-memory `DiagnosticCode` for
/// convenience. `FromStr` defaults severity to `Severity::Error`.
///
/// All fields are private; construct via [`DiagnosticCode::new`] or
/// [`DiagnosticCode::from_str`].
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct DiagnosticCode {
    category: Category,
    severity: Severity,
    number: u16,
}

impl DiagnosticCode {
    /// Constructs a new diagnostic code with the given category, severity, and number.
    ///
    /// Returns `Err` if the number is outside the valid range for the category.
    pub fn new(
        category: Category,
        severity: Severity,
        number: u16,
    ) -> Result<Self, CodeParseError> {
        let range = category.range();
        if !range.contains(&number) {
            return Err(CodeParseError::OutOfRange { category, number });
        }
        Ok(Self {
            category,
            severity,
            number,
        })
    }

    /// Returns the category of this diagnostic code.
    #[must_use]
    pub fn category(self) -> Category {
        self.category
    }

    /// Returns the severity level of this diagnostic code.
    #[must_use]
    pub fn severity(self) -> Severity {
        self.severity
    }

    /// Returns the numeric part of this diagnostic code.
    #[must_use]
    pub fn number(self) -> u16 {
        self.number
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{:04}", self.category.letter(), self.number)
    }
}

impl FromStr for DiagnosticCode {
    type Err = CodeParseError;

    /// Parse a diagnostic code from its wire form `<Letter><1..=4 digits>`.
    ///
    /// The wire form does not carry severity — that lives in the catalog
    /// (`catalog.toml`, see `diagnostics.md` §3). `FromStr` therefore yields
    /// a code with `Severity::Error`; callers that have catalog metadata
    /// should re-construct via [`DiagnosticCode::new`] with the catalog
    /// severity.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = s.as_bytes();

        // Step 1: require len >= 2 (letter + at least 1 number digit).
        if bytes.len() < 2 {
            return Err(CodeParseError::BadFormat {
                reason: "too short",
            });
        }

        // Step 2: Parse category from first byte.
        let cat_byte = bytes[0];
        if !cat_byte.is_ascii_uppercase() {
            return Err(CodeParseError::BadFormat {
                reason: "first char must be uppercase letter",
            });
        }
        let cat_char = cat_byte as char;
        let category = Category::from_letter(cat_char)
            .ok_or(CodeParseError::UnknownCategory { letter: cat_char })?;

        // Step 3: Parse remaining bytes as the number (1–4 digits).
        let number_bytes = &bytes[1..];
        if number_bytes.len() > 4 {
            return Err(CodeParseError::BadFormat {
                reason: "too many digits",
            });
        }
        if !number_bytes.iter().all(|b| b.is_ascii_digit()) {
            return Err(CodeParseError::BadFormat {
                reason: "non-digit in number",
            });
        }

        let number_str = std::str::from_utf8(number_bytes).unwrap();
        let number = number_str
            .parse::<u16>()
            .map_err(|_| CodeParseError::BadFormat {
                reason: "number too large",
            })?;

        // Step 4: Validate number is in category range.
        let range = category.range();
        if !range.contains(&number) {
            return Err(CodeParseError::OutOfRange { category, number });
        }

        Ok(Self {
            category,
            severity: Severity::Error,
            number,
        })
    }
}

impl Serialize for DiagnosticCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for DiagnosticCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        DiagnosticCode::from_str(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test 1: Display canonical form (the issue's literal acceptance criterion).
    #[test]
    fn display_canonical() {
        let code = DiagnosticCode::new(Category::E, Severity::Error, 1).unwrap();
        assert_eq!(code.to_string(), "E0001");
    }

    /// Test 2: Display other categories (severity is NOT in the wire form).
    #[test]
    fn display_other_categories() {
        let code = DiagnosticCode::new(Category::T, Severity::Error, 501).unwrap();
        assert_eq!(code.to_string(), "T0501");

        let code = DiagnosticCode::new(Category::S, Severity::Error, 900).unwrap();
        assert_eq!(code.to_string(), "S0900");

        let code = DiagnosticCode::new(Category::Z, Severity::Error, 9099).unwrap();
        assert_eq!(code.to_string(), "Z9099");

        // Severity::Warning here is in-memory metadata; it must NOT appear in Display.
        let code = DiagnosticCode::new(Category::L, Severity::Warning, 2042).unwrap();
        assert_eq!(code.to_string(), "L2042");
    }

    /// Test 3: Parse canonical 5-char form.
    #[test]
    fn parse_canonical() {
        let code = "E0001".parse::<DiagnosticCode>().unwrap();
        assert_eq!(code.category(), Category::E);
        assert_eq!(code.severity(), Severity::Error);
        assert_eq!(code.number(), 1);

        let code = "P0250".parse::<DiagnosticCode>().unwrap();
        assert_eq!(code.category(), Category::P);
        assert_eq!(code.number(), 250);
    }

    /// Test 4: Parse shorthand from `diagnostics.md` §1
    /// ("E1 is acceptable shorthand for E0001").
    #[test]
    fn parse_shorthand() {
        let code = "E1".parse::<DiagnosticCode>().unwrap();
        assert_eq!(code.category(), Category::E);
        assert_eq!(code.number(), 1);
        assert_eq!(code.to_string(), "E0001"); // canonicalizes on Display

        // 2- and 3-digit shorthand also canonicalize.
        let code = "E12".parse::<DiagnosticCode>().unwrap();
        assert_eq!(code.to_string(), "E0012");

        let code = "P100".parse::<DiagnosticCode>().unwrap();
        assert_eq!(code.category(), Category::P);
        assert_eq!(code.number(), 100);
        assert_eq!(code.to_string(), "P0100");
    }

    /// Test 4b: shorthand parses syntactically but is still range-checked.
    #[test]
    fn parse_shorthand_out_of_range() {
        // L starts at 2000; "L42" parses to number 42, which is below the
        // range and must be rejected at the range-check step.
        let err = "L42".parse::<DiagnosticCode>().unwrap_err();
        assert_eq!(
            err,
            CodeParseError::OutOfRange {
                category: Category::L,
                number: 42
            }
        );
    }

    /// Test 5: Reject unknown category.
    #[test]
    fn parse_rejects_unknown_category() {
        let err = "X0001".parse::<DiagnosticCode>().unwrap_err();
        assert_eq!(err, CodeParseError::UnknownCategory { letter: 'X' });
    }

    /// Test 6: Reject out-of-range numbers.
    #[test]
    fn parse_rejects_out_of_range() {
        let err = "E0500".parse::<DiagnosticCode>().unwrap_err();
        assert_eq!(
            err,
            CodeParseError::OutOfRange {
                category: Category::E,
                number: 500
            }
        );

        let err = "L1500".parse::<DiagnosticCode>().unwrap_err();
        assert_eq!(
            err,
            CodeParseError::OutOfRange {
                category: Category::L,
                number: 1500
            }
        );
    }

    /// Test 7: Reject gap ranges (1900–1999, 3200–8999, 9100+).
    #[test]
    fn parse_rejects_gaps() {
        let err = "D1950".parse::<DiagnosticCode>().unwrap_err();
        assert_eq!(
            err,
            CodeParseError::OutOfRange {
                category: Category::D,
                number: 1950
            }
        );
    }

    /// Test 8: Reject input too short (letter alone).
    #[test]
    fn parse_rejects_too_short() {
        let err = "E".parse::<DiagnosticCode>().unwrap_err();
        assert_eq!(
            err,
            CodeParseError::BadFormat {
                reason: "too short"
            }
        );
    }

    /// Test 9: Reject lowercase category letter.
    #[test]
    fn parse_rejects_lowercase() {
        let err = "e0001".parse::<DiagnosticCode>().unwrap_err();
        assert_eq!(
            err,
            CodeParseError::BadFormat {
                reason: "first char must be uppercase letter"
            }
        );
    }

    /// Test 10: Reject number with too many digits.
    #[test]
    fn parse_rejects_overlong() {
        let err = "E00001".parse::<DiagnosticCode>().unwrap_err();
        assert_eq!(
            err,
            CodeParseError::BadFormat {
                reason: "too many digits"
            }
        );
    }

    /// Test 11: FromStr always returns `Severity::Error` because severity is
    /// not in the wire form — callers must obtain it from the catalog and
    /// re-construct via `DiagnosticCode::new`.
    #[test]
    fn parsed_severity_is_always_error() {
        // The wire form contains no severity; FromStr defaults to Error.
        let parsed = "L2042".parse::<DiagnosticCode>().unwrap();
        assert_eq!(parsed.severity(), Severity::Error);
        // The Display output of the parsed code is unaffected by severity.
        assert_eq!(parsed.to_string(), "L2042");

        // Re-constructing with a different severity preserves it in memory
        // but the Display form is still the same 5-char wire string.
        let with_warn =
            DiagnosticCode::new(parsed.category(), Severity::Warning, parsed.number()).unwrap();
        assert_eq!(with_warn.severity(), Severity::Warning);
        assert_eq!(with_warn.to_string(), "L2042");
    }

    /// Test 12: Constructor rejects out-of-range
    #[test]
    fn new_rejects_out_of_range() {
        let err = DiagnosticCode::new(Category::E, Severity::Error, 5000).unwrap_err();
        assert_eq!(
            err,
            CodeParseError::OutOfRange {
                category: Category::E,
                number: 5000
            }
        );
    }

    /// Test 13: Category range invariants
    #[test]
    fn category_range_invariants() {
        let categories = [
            Category::E,
            Category::P,
            Category::M,
            Category::T,
            Category::S,
            Category::F,
            Category::C,
            Category::O,
            Category::U,
            Category::B,
            Category::D,
            Category::L,
            Category::W,
            Category::R,
            Category::Z,
        ];

        for cat in categories {
            let range = cat.range();
            assert!(!range.is_empty(), "Range for {:?} is empty", cat);
            assert_eq!(
                Category::from_letter(cat.letter()),
                Some(cat),
                "from_letter(letter()) round-trip failed for {:?}",
                cat
            );
        }
    }

    /// Test 14: serde_json round-trip. Severity is not in the wire form, so
    /// `decoded` carries `Severity::Error` regardless of the original value.
    #[test]
    fn serde_round_trip() {
        let code = DiagnosticCode::new(Category::E, Severity::Error, 1).unwrap();
        let json = serde_json::to_string(&code).unwrap();
        assert_eq!(json, r#""E0001""#);

        let decoded: DiagnosticCode = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, code);

        // A code constructed with a non-Error severity still serializes to its
        // 5-char wire form; on decode, severity defaults to Error.
        let code_lint = DiagnosticCode::new(Category::L, Severity::Lint, 2500).unwrap();
        let json_lint = serde_json::to_string(&code_lint).unwrap();
        assert_eq!(json_lint, r#""L2500""#);

        let decoded_lint: DiagnosticCode = serde_json::from_str(&json_lint).unwrap();
        assert_eq!(decoded_lint.category(), Category::L);
        assert_eq!(decoded_lint.number(), 2500);
        assert_eq!(decoded_lint.severity(), Severity::Error);
    }
}
