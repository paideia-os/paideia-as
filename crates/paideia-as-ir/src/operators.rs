//! Central operator registry for paideia-as.
//!
//! Issue #1230: Centralized KNOWN_OPERATORS list to prevent silent miscompiles
//! from divergent operator-lexeme lists across emit sites.

/// All recognized operators, sorted for binary_search.
/// Covers: bitwise, arithmetic, unary, and comparison operators.
pub const KNOWN_OPERATORS: &[&str] = &[
    "!",     // 0: unary NOT
    "!=",    // 1: not-equal
    "%",     // 2: modulo
    "&",     // 3: bitwise AND
    "*",     // 4: multiplication
    "+",     // 5: addition
    "-",     // 6: subtraction
    "/",     // 7: division
    "<",     // 8: less-than
    "<<",    // 9: left-shift
    "<=",    // 10: less-equal
    "==",    // 11: equal
    ">",     // 12: greater-than
    ">=",    // 13: greater-equal
    ">>",    // 14: right-shift
    "^",     // 15: bitwise XOR
    "|",     // 16: bitwise OR
    "~",     // 17: bitwise NOT (unary)
];

/// Binary operators (take two operands).
pub const BINARY_OPERATORS: &[&str] = &[
    "!=", "%", "&", "*", "+", "-", "/", "<", "<<", "<=", "==", ">", ">=", ">>", "^", "|",
];

/// Unary operators (take one operand).
pub const UNARY_OPERATORS: &[&str] = &["!", "~"];

/// Check if a string is a known operator.
#[inline]
pub fn is_operator(s: &str) -> bool {
    KNOWN_OPERATORS.binary_search(&s).is_ok()
}

/// Check if a string is a known binary operator.
#[inline]
pub fn is_binary_op(s: &str) -> bool {
    BINARY_OPERATORS.binary_search(&s).is_ok()
}

/// Check if a string is a known unary operator.
#[inline]
pub fn is_unary_op(s: &str) -> bool {
    UNARY_OPERATORS.binary_search(&s).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// C1: Verify KNOWN_OPERATORS is sorted (required for binary_search).
    #[test]
    fn known_operators_sorted_for_binary_search() {
        for i in 0..KNOWN_OPERATORS.len() - 1 {
            assert!(
                KNOWN_OPERATORS[i] < KNOWN_OPERATORS[i + 1],
                "KNOWN_OPERATORS[{}] = {} should be < KNOWN_OPERATORS[{}] = {}",
                i,
                KNOWN_OPERATORS[i],
                i + 1,
                KNOWN_OPERATORS[i + 1]
            );
        }
    }

    /// C2: Verify binary and unary operators partition KNOWN_OPERATORS.
    #[test]
    fn binary_and_unary_partition_covers_known() {
        let mut binary_set = std::collections::HashSet::new();
        for &op in BINARY_OPERATORS {
            binary_set.insert(op);
        }

        let mut unary_set = std::collections::HashSet::new();
        for &op in UNARY_OPERATORS {
            unary_set.insert(op);
        }

        // Intersection should be empty
        for &op in UNARY_OPERATORS {
            assert!(
                !binary_set.contains(op),
                "Operator {} appears in both BINARY_OPERATORS and UNARY_OPERATORS",
                op
            );
        }

        // Union should equal KNOWN_OPERATORS
        let mut union = binary_set.clone();
        for &op in UNARY_OPERATORS {
            union.insert(op);
        }

        let known_set: std::collections::HashSet<_> = KNOWN_OPERATORS.iter().copied().collect();

        assert_eq!(
            union, known_set,
            "BINARY_OPERATORS ∪ UNARY_OPERATORS != KNOWN_OPERATORS"
        );
    }
}
