//! R221.M5 mixed-nesting parser fixtures (10).
//!
//! Fingerprint `r221m5-parse-mix-NN`. Exercises pipeline stages that
//! themselves contain lambda blocks or datalog blocks.

mod common;
use common::parse_ok;
use paideia_as_shell_ast::SyntaxNode;

#[test]
fn r221m5_parse_mix_01_pipeline_with_lambda_arg() {
    let n = parse_ok(
        "r221m5-parse-mix-01",
        "ls | filter { |f| f.size > 100 }",
    );
    assert!(matches!(n, SyntaxNode::Pipe { .. }));
}

#[test]
fn r221m5_parse_mix_02_pipeline_with_datalog_arg() {
    let n = parse_ok(
        "r221m5-parse-mix-02",
        r#"files | datalog { tagged(?f, "research") }"#,
    );
    assert!(matches!(n, SyntaxNode::Pipe { .. }));
}

#[test]
fn r221m5_parse_mix_03_lambda_returning_datalog() {
    let n = parse_ok(
        "r221m5-parse-mix-03",
        "each | { |x| datalog { pred($it, ?y) } }",
    );
    assert!(matches!(n, SyntaxNode::Pipe { .. }));
}

#[test]
fn r221m5_parse_mix_04_two_lambdas_in_pipeline() {
    let n = parse_ok(
        "r221m5-parse-mix-04",
        "list | map { |x| x + 1 } | filter { |x| x > 5 }",
    );
    assert!(matches!(n, SyntaxNode::Pipe { .. }));
}

#[test]
fn r221m5_parse_mix_05_datalog_then_lambda() {
    let n = parse_ok(
        "r221m5-parse-mix-05",
        r#"datalog { p(a). } | filter { |x| true }"#,
    );
    assert!(matches!(n, SyntaxNode::Pipe { .. }));
}

#[test]
fn r221m5_parse_mix_06_seq_of_pipelines() {
    // Path glyph `/` is not glued into idents (see pipe_05 note); test
    // uses bare arg names.
    let n = parse_ok(
        "r221m5-parse-mix-06",
        "ls | wc; cd home; head 3 < in",
    );
    if let SyntaxNode::Seq { items, .. } = &n {
        assert_eq!(items.len(), 3);
    } else {
        panic!("expected Seq");
    }
}

#[test]
fn r221m5_parse_mix_07_deep_pipe_chain() {
    let n = parse_ok(
        "r221m5-parse-mix-07",
        "a | b | c | d | e | f | g | h",
    );
    // Count right-spine.
    let mut depth = 0;
    let mut cur = &n;
    while let SyntaxNode::Pipe { rhs, .. } = cur {
        depth += 1;
        cur = rhs;
    }
    assert_eq!(depth, 7);
}

#[test]
fn r221m5_parse_mix_08_nested_lambda_inside_lambda() {
    let n = parse_ok(
        "r221m5-parse-mix-08",
        "outer { |x| inner { |y| x + y } }",
    );
    assert!(matches!(&n, SyntaxNode::Cmd { .. }));
}

#[test]
fn r221m5_parse_mix_09_seq_of_datalog_and_pipe() {
    let n = parse_ok(
        "r221m5-parse-mix-09",
        "datalog { p(a). }\nls | wc",
    );
    if let SyntaxNode::Seq { items, .. } = &n {
        assert_eq!(items.len(), 2);
    } else {
        panic!("expected Seq");
    }
}

#[test]
fn r221m5_parse_mix_10_lambda_with_string_arg() {
    let n = parse_ok(
        "r221m5-parse-mix-10",
        r#"apply { |s| s } "hello""#,
    );
    if let SyntaxNode::Cmd { args, .. } = &n {
        assert!(matches!(&args[0], SyntaxNode::Lambda { .. }));
        assert!(matches!(&args[1], SyntaxNode::LitStr { .. }));
    } else {
        panic!("expected Cmd");
    }
}
