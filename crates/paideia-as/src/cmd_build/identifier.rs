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

/// Check if a string is a valid qualified identifier — a `::`-separated
/// path where each segment is a plain identifier.
///
/// Issue #1290: the pre-emit `call_sites` populator previously accepted only
/// `is_valid_identifier`, so a lambda body like `fn (a, b) -> Foo::bar(a, b)`
/// left `arena.call_sites().get(body_id)` empty. Downstream, the App-body arm
/// in `visit_lambda` fell through to the operator-fallback for (Var, Var)
/// args and dispatched into `emit_var_assign_expr_to_rax`, which fires T0540
/// on a non-var_assign shape.
///
/// Accepting qualified names here routes them through `emit_function_call`,
/// which knows how to resolve `TraitName::method` via `stdlib_lowering`.
pub(super) fn is_valid_qualified_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    // Every `::`-separated segment must itself be a plain identifier.
    let mut has_segments = false;
    for seg in s.split("::") {
        if !is_valid_identifier(seg) {
            return false;
        }
        has_segments = true;
    }
    has_segments
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

#[cfg(test)]
mod tests {
    use super::*;

    // Issue #1290: `is_valid_qualified_identifier` must accept `Foo::bar` shapes
    // so the pre-emit `call_sites` populator records qualified callees, letting
    // `visit_lambda` route them through `emit_function_call` instead of the
    // (Var, Var) operator fallback that fires T0540.

    #[test]
    fn qualified_identifier_accepts_two_segments_1290() {
        assert!(is_valid_qualified_identifier("PerCpuOps::write_u64"));
        assert!(is_valid_qualified_identifier("Foo::bar"));
    }

    #[test]
    fn qualified_identifier_accepts_three_segments_1290() {
        assert!(is_valid_qualified_identifier("Outer::Inner::method"));
    }

    #[test]
    fn qualified_identifier_still_accepts_bare_ident_1290() {
        // A single segment IS a valid path; the caller uses this check as a
        // qualified-name FALLBACK when the plain-ident check has already passed,
        // but the function itself should treat a bare ident as legitimate.
        assert!(is_valid_qualified_identifier("foo"));
        assert!(is_valid_qualified_identifier("_foo"));
    }

    #[test]
    fn qualified_identifier_rejects_empty_segments() {
        assert!(!is_valid_qualified_identifier(""));
        assert!(!is_valid_qualified_identifier("Foo::"));
        assert!(!is_valid_qualified_identifier("::bar"));
        assert!(!is_valid_qualified_identifier("Foo::::bar"));
    }

    #[test]
    fn qualified_identifier_rejects_leading_digit_segment() {
        assert!(!is_valid_qualified_identifier("Foo::1bar"));
    }

    #[test]
    fn qualified_identifier_rejects_operator_chars() {
        assert!(!is_valid_qualified_identifier("Foo::+"));
        assert!(!is_valid_qualified_identifier("Foo.bar"));
    }
}
