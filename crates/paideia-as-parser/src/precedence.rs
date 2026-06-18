//! Operator precedence and binding-power tables for Pratt parsing.
//!
//! This module implements §7 (operator precedence) of the syntax reference.
//! Each operator is classified by tier, associativity, and binding power (bp).
//!
//! **Binding-power encoding:**
//! - Each infix operator has a `(left_bp, right_bp)` pair.
//! - Left-associative: `right_bp = left_bp + 1` (loop stops on equal-priority rhs).
//! - Right-associative: `right_bp = left_bp - 1` (loop continues on equal-priority rhs).
//! - Prefix operators have only a right-bp (left-bp is implicit 0).
//! - Postfix operators have only a left-bp (right-bp is implicit 0).
//!
//! **Pratt loop protocol:**
//! - `parse_expr_bp(min_bp)` continues while `op.left_bp >= min_bp`.
//! - After consuming an infix op, recurse with `parse_expr_bp(op.right_bp)`.
//! - This ensures correct precedence and associativity behavior.

use paideia_as_lexer::TokenKind;

/// Binding-power pair for infix operators: (left, right).
///
/// The loop in `parse_expr_bp` continues when `left >= min_bp`,
/// and recurses with `min_bp = right` for the operand.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct InfixBp {
    /// Left binding power: loop continues while `infix_bp.left >= min_bp`.
    pub left: u8,
    /// Right binding power: passed as `min_bp` to the next recursion.
    pub right: u8,
}

/// Look up the infix binding power for an operator token.
///
/// Returns `None` if `kind` is not an infix operator (e.g., a prefix or postfix op).
///
/// The binding power tiers map to §7 of the syntax reference:
/// - Tier 13 (assignment, right-assoc): { left: 21, right: 20 }
/// - Tier 12 (range, non-assoc): { left: 22, right: 22 } (range chaining blocked by parser)
/// - Tier 11 (||): { left: 30, right: 31 }
/// - Tier 10 (&&): { left: 40, right: 41 }
/// - Tier 9 (comparison): { left: 50, right: 51 }
/// - Tier 8 (bit-or |): { left: 60, right: 61 }
/// - Tier 7 (bit-xor ^): { left: 70, right: 71 }
/// - Tier 6 (bit-and &): { left: 80, right: 81 }
/// - Tier 5 (shifts <<, >>): { left: 90, right: 91 }
/// - Tier 4 (additive +, -): { left: 100, right: 101 }
/// - Tier 3 (multiplicative *, /, %): { left: 110, right: 111 }
pub fn infix_bp(kind: TokenKind) -> Option<InfixBp> {
    use TokenKind::*;
    Some(match kind {
        // Tier 13: Assignment (right-associative)
        Assign => InfixBp {
            left: 21,
            right: 20,
        },

        // Tier 11: Logical OR
        OrOr => InfixBp {
            left: 30,
            right: 31,
        },

        // Tier 10: Logical AND
        AndAnd => InfixBp {
            left: 40,
            right: 41,
        },

        // Tier 9: Comparison operators (left-associative)
        Eq | Neq | Lt | Gt | Le | Ge => InfixBp {
            left: 50,
            right: 51,
        },

        // Tier 8: Bitwise OR (left-associative)
        Pipe => InfixBp {
            left: 60,
            right: 61,
        },

        // Tier 7: Bitwise XOR (left-associative)
        Caret => InfixBp {
            left: 70,
            right: 71,
        },

        // Tier 6: Bitwise AND (left-associative)
        Amp => InfixBp {
            left: 80,
            right: 81,
        },

        // Tier 5: Shifts (left-associative)
        Shl | Shr => InfixBp {
            left: 90,
            right: 91,
        },

        // Tier 4: Additive (left-associative)
        Plus | Minus => InfixBp {
            left: 100,
            right: 101,
        },

        // Tier 3: Multiplicative (left-associative)
        Star | Slash | Percent => InfixBp {
            left: 110,
            right: 111,
        },

        _ => return None,
    })
}

/// Look up the prefix right-binding-power for a prefix operator.
///
/// Prefix operators have no left-bp (implicitly 0 for the initial parse_expr_bp call).
/// The returned value is the right-bp used for the operand.
///
/// Tier 2 (prefix operators): right-bp = 130.
/// This ensures prefix operators bind tighter than all infix operators.
pub fn prefix_bp(kind: TokenKind) -> Option<u8> {
    use TokenKind::*;
    match kind {
        // Tier 2: Prefix operators (!,  ~, -, &, * as unary)
        Bang | Minus | Amp | Star => Some(130),
        _ => None,
    }
}

/// Look up the postfix left-binding-power for a postfix operator.
///
/// Postfix operators have no right-bp (implicitly 0).
/// The returned value is the left-bp used for the loop condition.
///
/// Tier 1 (postfix operators): left-bp = 140.
/// This ensures postfix operators bind tightest of all.
pub fn postfix_bp(kind: TokenKind) -> Option<u8> {
    use TokenKind::*;
    match kind {
        // Tier 1: Postfix operators (?, field access ., indexing [, calls ())
        // Phase-1: only handle `?`; calls and indexing deferred to later PRs.
        Question => Some(140),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infix_bp_multiplicative_tighter_than_additive() {
        let add_bp = infix_bp(TokenKind::Plus).expect("Plus is infix");
        let mul_bp = infix_bp(TokenKind::Star).expect("Star is infix");
        assert!(mul_bp.left > add_bp.left, "* should bind tighter than +");
    }

    #[test]
    fn infix_bp_assignment_looser_than_logical_or() {
        let assign_bp = infix_bp(TokenKind::Assign).expect("Assign is infix");
        let or_bp = infix_bp(TokenKind::OrOr).expect("OrOr is infix");
        assert!(assign_bp.left < or_bp.left, "= should bind looser than ||");
    }

    #[test]
    fn prefix_bp_tighter_than_all_infix() {
        let prefix = prefix_bp(TokenKind::Bang).expect("Bang is prefix");
        let loosest_infix = infix_bp(TokenKind::Assign).expect("Assign is infix").left;
        assert!(
            prefix > loosest_infix,
            "prefix should bind tighter than all infix"
        );
    }

    #[test]
    fn postfix_bp_tightest() {
        let postfix = postfix_bp(TokenKind::Question).expect("Question is postfix");
        let prefix = prefix_bp(TokenKind::Bang).expect("Bang is prefix");
        assert!(postfix > prefix, "postfix should bind tightest");
    }

    #[test]
    fn left_assoc_has_right_greater() {
        let add = infix_bp(TokenKind::Plus).expect("Plus is infix");
        assert_eq!(add.right, add.left + 1, "+ is left-associative");
    }

    #[test]
    fn right_assoc_has_right_less() {
        let assign = infix_bp(TokenKind::Assign).expect("Assign is infix");
        assert_eq!(assign.right, assign.left - 1, "= is right-associative");
    }
}
