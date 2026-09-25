//! R221.M6 source-span-provenance fixtures (10).
//!
//! Fingerprint `r221m6-span-NN`. Asserts:
//! * `original` spans on ASCII input equal `nfc` spans (identity map).
//! * `original` spans on non-NFC input are widened to enclose the
//!   user's actual bytes.
//! * `NodeSpan::context` stamped correctly per sub-language.
//! * Every child node's span is inside its parent's.
//! * Diagnostics carry both coordinate systems.

use paideia_as_shell_ast::{Context, NfcMap, SyntaxNode, parse, parse_with_map};

fn root_span(node: &SyntaxNode) -> paideia_as_shell_ast::NodeSpan {
    node.span()
}

#[test]
fn r221m6_span_01_ascii_identity_span() {
    let src = "ls";
    let n = parse(src).unwrap();
    let s = root_span(&n);
    assert_eq!(s.original, s.nfc, "ASCII must give identity NFC map");
    assert_eq!(s.original, (0, 2));
}

#[test]
fn r221m6_span_02_ascii_pipeline_covers_full() {
    let src = "ls | wc";
    let n = parse(src).unwrap();
    let s = root_span(&n);
    assert_eq!(s.original, (0, 7));
}

#[test]
fn r221m6_span_03_context_pipeline_at_root() {
    let n = parse("ls").unwrap();
    assert_eq!(n.span().context, Context::Pipeline);
}

#[test]
fn r221m6_span_04_context_datalog_inside_block() {
    let n = parse("datalog { p(x). }").unwrap();
    if let SyntaxNode::DatalogBlock { items, .. } = &n {
        assert_eq!(items[0].span().context, Context::Datalog);
    } else if let SyntaxNode::Cmd { name, .. } = &n {
        if let SyntaxNode::DatalogBlock { items, .. } = name.as_ref() {
            assert_eq!(items[0].span().context, Context::Datalog);
        }
    }
}

#[test]
fn r221m6_span_05_context_lambda_inside_block() {
    let n = parse("{ |x| x }").unwrap();
    let inner_ctx = match &n {
        SyntaxNode::Lambda { body, .. } => body.span().context,
        SyntaxNode::Cmd { name, .. } => match name.as_ref() {
            SyntaxNode::Lambda { body, .. } => body.span().context,
            _ => panic!("expected lambda"),
        },
        _ => panic!("expected lambda"),
    };
    assert_eq!(inner_ctx, Context::Lambda);
}

#[test]
fn r221m6_span_06_child_span_inside_parent() {
    // For `ls foo`, the first-arg span must be inside the Cmd span.
    let n = parse("ls foo").unwrap();
    if let SyntaxNode::Cmd { args, span, .. } = &n {
        let a = args[0].span();
        assert!(a.original.0 >= span.original.0);
        assert!(a.original.1 <= span.original.1);
    }
}

#[test]
fn r221m6_span_07_non_ascii_original_span_widens() {
    // é as decomposed (NFD): "e" + U+0301.  4 bytes.  After NFC: "é"
    // = 2 bytes.  A one-token span in NFC (2 bytes) should widen to
    // the full 4-byte original.
    let src = "e\u{0301}"; // "é" as e + combining acute
    let (nfc, map) = NfcMap::build(src);
    assert!(nfc.len() < src.len(), "NFC should compress");
    let orig = map.to_original((0, nfc.len()));
    assert_eq!(orig, (0, src.len()));
}

#[test]
fn r221m6_span_08_ascii_map_is_identity() {
    let (_, map) = NfcMap::build("abc");
    assert!(map.is_identity());
    assert_eq!(map.to_original((1, 2)), (1, 2));
}

#[test]
fn r221m6_span_09_diagnostic_carries_both_coordinates() {
    // Unmatched brace: parser produces error with both spans.
    let err = parse("{ |x| x").unwrap_err();
    assert_eq!(
        err.original_span, err.nfc_span,
        "ASCII input: original == nfc"
    );
}

#[test]
fn r221m6_span_10_pretty_source_reparses() {
    // Ensures spans on a parsed AST can be used to reconstruct the
    // canonical source: parse the pretty output and confirm span shape.
    use paideia_as_shell_ast::pretty_print;
    let n = parse("ls foo bar").unwrap();
    let pp = pretty_print(&n);
    let (nfc, map) = NfcMap::build(&pp);
    let n2 = parse_with_map(&nfc, &map).unwrap();
    assert_eq!(n2.span().nfc, (0, pp.len()));
}
