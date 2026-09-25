//! R221.M5 Datalog-parser fixtures (15).
//!
//! Fingerprint `r221m5-parse-dlg-NN`.

mod common;
use common::{parse_err, parse_ok};
use paideia_as_shell_ast::SyntaxNode;

fn block(node: &SyntaxNode) -> &Vec<SyntaxNode> {
    match node {
        SyntaxNode::DatalogBlock { items, .. } => items,
        SyntaxNode::Cmd { name, .. } => match name.as_ref() {
            SyntaxNode::DatalogBlock { items, .. } => items,
            _ => panic!("not a datalog block"),
        },
        _ => panic!("not a datalog block: {node:?}"),
    }
}

#[test]
fn r221m5_parse_dlg_01_empty_block() {
    let n = parse_ok("r221m5-parse-dlg-01", "datalog { }");
    assert!(block(&n).is_empty());
}

#[test]
fn r221m5_parse_dlg_02_single_fact() {
    let n = parse_ok("r221m5-parse-dlg-02", "datalog { parent(alice, bob). }");
    let items = block(&n);
    assert_eq!(items.len(), 1);
    if let SyntaxNode::Atom { pred, args, .. } = &items[0] {
        assert_eq!(pred, "parent");
        assert_eq!(args.len(), 2);
    } else {
        panic!("expected Atom, got {:?}", items[0]);
    }
}

#[test]
fn r221m5_parse_dlg_03_zero_arg_fact() {
    let n = parse_ok("r221m5-parse-dlg-03", "datalog { started. }");
    let items = block(&n);
    if let SyntaxNode::Atom { pred, args, .. } = &items[0] {
        assert_eq!(pred, "started");
        assert!(args.is_empty());
    } else {
        panic!("expected zero-arg Atom");
    }
}

#[test]
fn r221m5_parse_dlg_04_rule_with_body() {
    let n = parse_ok(
        "r221m5-parse-dlg-04",
        "datalog { ancestor(?x, ?y) => parent(?x, ?y). }",
    );
    let items = block(&n);
    assert!(matches!(&items[0], SyntaxNode::Rule { .. }));
    if let SyntaxNode::Rule { body, .. } = &items[0] {
        assert_eq!(body.len(), 1);
    }
}

#[test]
fn r221m5_parse_dlg_05_rule_multi_body() {
    let n = parse_ok(
        "r221m5-parse-dlg-05",
        "datalog { ancestor(?x, ?z) => parent(?x, ?y), ancestor(?y, ?z). }",
    );
    let items = block(&n);
    if let SyntaxNode::Rule { body, .. } = &items[0] {
        assert_eq!(body.len(), 2);
    } else {
        panic!("expected Rule");
    }
}

#[test]
fn r221m5_parse_dlg_06_qvar_term() {
    let n = parse_ok("r221m5-parse-dlg-06", "datalog { p(?x). }");
    if let SyntaxNode::Atom { args, .. } = &block(&n)[0] {
        assert!(matches!(&args[0], SyntaxNode::QVar { .. }));
    }
}

#[test]
fn r221m5_parse_dlg_07_interp_var() {
    let n = parse_ok("r221m5-parse-dlg-07", "datalog { p($it). }");
    if let SyntaxNode::Atom { args, .. } = &block(&n)[0] {
        assert!(matches!(&args[0], SyntaxNode::InterpVar { .. }));
    }
}

#[test]
fn r221m5_parse_dlg_08_string_term() {
    let n = parse_ok("r221m5-parse-dlg-08", r#"datalog { tag(?x, "red"). }"#);
    if let SyntaxNode::Atom { args, .. } = &block(&n)[0] {
        assert!(matches!(&args[1], SyntaxNode::LitStr { .. }));
    }
}

#[test]
fn r221m5_parse_dlg_09_number_term() {
    let n = parse_ok("r221m5-parse-dlg-09", "datalog { age(alice, 30). }");
    if let SyntaxNode::Atom { args, .. } = &block(&n)[0] {
        assert!(matches!(&args[1], SyntaxNode::LitInt { value: 30, .. }));
    }
}

#[test]
fn r221m5_parse_dlg_10_negated_atom() {
    let n = parse_ok("r221m5-parse-dlg-10", "datalog { not banned(?x). }");
    assert!(matches!(&block(&n)[0], SyntaxNode::NotAtom { .. }));
}

#[test]
fn r221m5_parse_dlg_11_multiple_facts() {
    let n = parse_ok(
        "r221m5-parse-dlg-11",
        "datalog { parent(a, b). parent(b, c). parent(c, d). }",
    );
    assert_eq!(block(&n).len(), 3);
}

#[test]
fn r221m5_parse_dlg_12_reject_missing_close_brace() {
    parse_err("r221m5-parse-dlg-12", "datalog { p(x).");
}

#[test]
fn r221m5_parse_dlg_13_reject_missing_close_paren() {
    parse_err("r221m5-parse-dlg-13", "datalog { p(x }");
}

#[test]
fn r221m5_parse_dlg_14_facts_span_context_datalog() {
    let n = parse_ok("r221m5-parse-dlg-14", "datalog { p(x). }");
    let items = block(&n);
    let s = items[0].span();
    assert_eq!(s.context, paideia_as_shell_ast::Context::Datalog);
}

#[test]
fn r221m5_parse_dlg_15_multiline_block() {
    let n = parse_ok(
        "r221m5-parse-dlg-15",
        "datalog {\n  p(a).\n  q(b).\n}",
    );
    assert_eq!(block(&n).len(), 2);
}
