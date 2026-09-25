//! R221.M5 lambda-parser fixtures (15).
//!
//! Fingerprint `r221m5-parse-lam-NN`.

mod common;
use common::{parse_err, parse_ok};
use paideia_as_shell_ast::SyntaxNode;

fn unwrap_lambda(node: &SyntaxNode) -> (&Vec<String>, &SyntaxNode) {
    match node {
        SyntaxNode::Lambda { params, body, .. } => (params, body.as_ref()),
        SyntaxNode::Cmd { name, .. } => match name.as_ref() {
            SyntaxNode::Lambda { params, body, .. } => (params, body.as_ref()),
            _ => panic!("not a lambda: {node:?}"),
        },
        _ => panic!("not a lambda: {node:?}"),
    }
}

#[test]
fn r221m5_parse_lam_01_identity() {
    let n = parse_ok("r221m5-parse-lam-01", "{ |x| x }");
    let (params, body) = unwrap_lambda(&n);
    assert_eq!(params, &vec!["x".to_string()]);
    assert!(matches!(body, SyntaxNode::Var { .. }));
}

#[test]
fn r221m5_parse_lam_02_two_params() {
    let n = parse_ok("r221m5-parse-lam-02", "{ |x y| x }");
    let (params, _) = unwrap_lambda(&n);
    assert_eq!(params.len(), 2);
}

#[test]
fn r221m5_parse_lam_03_addition_body() {
    let n = parse_ok("r221m5-parse-lam-03", "{ |x| x + 1 }");
    let (_, body) = unwrap_lambda(&n);
    assert!(matches!(body, SyntaxNode::BinOp { .. }));
}

#[test]
fn r221m5_parse_lam_04_multiplication_precedence() {
    // 2 + 3 * 4 → 2 + (3 * 4)
    let n = parse_ok("r221m5-parse-lam-04", "{ |x| 2 + 3 * 4 }");
    let (_, body) = unwrap_lambda(&n);
    if let SyntaxNode::BinOp { op, rhs, .. } = body {
        assert_eq!(op, "+");
        assert!(matches!(rhs.as_ref(), SyntaxNode::BinOp { .. }));
    } else {
        panic!("expected BinOp at top");
    }
}

#[test]
fn r221m5_parse_lam_05_comparison() {
    let n = parse_ok("r221m5-parse-lam-05", "{ |x| x > 10 }");
    let (_, body) = unwrap_lambda(&n);
    if let SyntaxNode::BinOp { op, .. } = body {
        assert_eq!(op, ">");
    }
}

#[test]
fn r221m5_parse_lam_06_field_access() {
    let n = parse_ok("r221m5-parse-lam-06", "{ |f| f.size }");
    let (_, body) = unwrap_lambda(&n);
    assert!(matches!(body, SyntaxNode::FieldAccess { .. }));
}

#[test]
fn r221m5_parse_lam_07_negation() {
    let n = parse_ok("r221m5-parse-lam-07", "{ |x| not x }");
    let (_, body) = unwrap_lambda(&n);
    if let SyntaxNode::UnaryOp { op, .. } = body {
        assert_eq!(op, "not");
    } else {
        panic!("expected UnaryOp");
    }
}

#[test]
fn r221m5_parse_lam_08_bool_literal() {
    let n = parse_ok("r221m5-parse-lam-08", "{ |x| true }");
    let (_, body) = unwrap_lambda(&n);
    assert!(matches!(body, SyntaxNode::LitBool { value: true, .. }));
}

#[test]
fn r221m5_parse_lam_09_bool_literal_false() {
    let n = parse_ok("r221m5-parse-lam-09", "{ |x| false }");
    let (_, body) = unwrap_lambda(&n);
    assert!(matches!(body, SyntaxNode::LitBool { value: false, .. }));
}

#[test]
fn r221m5_parse_lam_10_paren_group() {
    let n = parse_ok("r221m5-parse-lam-10", "{ |x| (x + 1) * 2 }");
    let (_, body) = unwrap_lambda(&n);
    if let SyntaxNode::BinOp { op, lhs, .. } = body {
        assert_eq!(op, "*");
        assert!(matches!(lhs.as_ref(), SyntaxNode::Group { .. }));
    }
}

#[test]
fn r221m5_parse_lam_11_reject_missing_close_brace() {
    parse_err("r221m5-parse-lam-11", "{ |x| x");
}

#[test]
fn r221m5_parse_lam_12_reject_missing_open_pipe() {
    parse_err("r221m5-parse-lam-12", "{ x| x }");
}

#[test]
fn r221m5_parse_lam_13_and_or_ops() {
    let n = parse_ok("r221m5-parse-lam-13", "{ |x| x and true }");
    let (_, body) = unwrap_lambda(&n);
    if let SyntaxNode::BinOp { op, .. } = body {
        assert_eq!(op, "and");
    } else {
        panic!("expected BinOp");
    }
}

#[test]
fn r221m5_parse_lam_14_zero_param_lambda() {
    let n = parse_ok("r221m5-parse-lam-14", "{ 42 }");
    let (params, body) = unwrap_lambda(&n);
    assert!(params.is_empty());
    assert!(matches!(body, SyntaxNode::LitInt { value: 42, .. }));
}

#[test]
fn r221m5_parse_lam_15_nested_field_chain() {
    let n = parse_ok("r221m5-parse-lam-15", "{ |x| x.a.b }");
    let (_, body) = unwrap_lambda(&n);
    if let SyntaxNode::FieldAccess { base, .. } = body {
        assert!(matches!(base.as_ref(), SyntaxNode::FieldAccess { .. }));
    } else {
        panic!("expected chained FieldAccess");
    }
}
