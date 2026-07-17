//! Lexical helpers: identifier validation + integer-literal parsing.
//! Split out of `cmd_build.rs` (2026-07-08).

/// Check if a string is a valid identifier (PA-R17-003).
///
/// A valid identifier must start with a letter or underscore, followed by
/// letters, digits, or underscores.
pub(super) fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let first = s.chars().next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// #1181: operator lexemes the pre-emit call_sites populator lets through
/// so the elaborator can identify the operator by string at emit time.
/// Delegates to central registry in paideia-as-ir (#1230).
pub(super) fn is_known_operator(s: &str) -> bool {
    paideia_as_ir::is_operator(s)
}

/// Parse an integer literal from text, supporting decimal, hex, binary, and octal formats.
///
/// Formats:
/// - Decimal: `42`, `-42`
/// - Hexadecimal: `0x2A`, `0X2a`
/// - Binary: `0b101010`, `0B101010`
/// - Octal: `0o52`, `0O52`
///
/// Returns `Ok(value)` on success, `Err(())` on parse failure.
pub(super) fn parse_integer_literal(text: &str) -> Result<i64, ()> {
    let text = text.trim();
    if text.is_empty() {
        return Err(());
    }

    // Handle negative numbers
    let (is_negative, text) = if text.starts_with('-') {
        (true, &text[1..])
    } else if text.starts_with('+') {
        (false, &text[1..])
    } else {
        (false, text)
    };

    // Determine the base and skip the prefix
    let (base, digits) = if text.starts_with("0x") || text.starts_with("0X") {
        (16, &text[2..])
    } else if text.starts_with("0b") || text.starts_with("0B") {
        (2, &text[2..])
    } else if text.starts_with("0o") || text.starts_with("0O") {
        (8, &text[2..])
    } else {
        (10, text)
    };

    // Strip integer type suffixes (u8/u16/u32/u64/i8/i16/i32/i64/usize/isize) if present.
    // Longer suffixes must be checked first so "u64" doesn't accidentally match after "u128".
    let digits = {
        let mut d = digits;
        for suffix in ["usize", "isize", "u128", "i128", "u64", "i64", "u32", "i32", "u16", "i16", "u8", "i8"] {
            if let Some(stripped) = d.strip_suffix(suffix) {
                d = stripped;
                break;
            }
        }
        d
    };

    // Remove underscores (allowed in numeric literals)
    let digits: String = digits.chars().filter(|c| *c != '_').collect();

    // For non-decimal bases, top-bit-set values overflow i64 but are valid u64.
    // Parse as u64 (bit-preserving) to allow values > i64::MAX, then negate if negative.
    let result = u64::from_str_radix(&digits, base)
        .map(|v| v as i64)
        .map_err(|_| ())?;

    if is_negative {
        Ok(result.wrapping_neg())
    } else {
        Ok(result)
    }
}
