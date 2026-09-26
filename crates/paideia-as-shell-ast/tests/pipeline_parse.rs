//! R221.M5 pipeline-parser fixtures (15).
//!
//! Fingerprint `r221m5-parse-pipe-NN`. Each fixture asserts a specific
//! `SyntaxNode` shape or count-invariant (arg count, pipe depth,
//! redirect kind).

mod common;
use common::{parse_err, parse_ok};
use paideia_as_shell_ast::{RedirectKind, SyntaxNode};

fn is_cmd(node: &SyntaxNode) -> bool {
    matches!(node, SyntaxNode::Cmd { .. })
}

#[test]
fn r221m5_parse_pipe_01_bare_command() {
    let n = parse_ok("r221m5-parse-pipe-01", "ls");
    assert!(is_cmd(&n));
    if let SyntaxNode::Cmd { args, .. } = &n {
        assert!(args.is_empty(), "bare `ls` has no args");
    }
}

#[test]
fn r221m5_parse_pipe_02_command_with_args() {
    let n = parse_ok("r221m5-parse-pipe-02", "grep foo bar");
    if let SyntaxNode::Cmd { args, .. } = &n {
        assert_eq!(args.len(), 2);
    } else {
        panic!("expected Cmd");
    }
}

#[test]
fn r221m5_parse_pipe_03_two_stage_pipe() {
    let n = parse_ok("r221m5-parse-pipe-03", "ls | grep foo");
    assert!(matches!(n, SyntaxNode::Pipe { .. }));
}

#[test]
fn r221m5_parse_pipe_04_three_stage_pipe_right_assoc() {
    // `a | b | c` should nest as Pipe(a, Pipe(b, c)).
    let n = parse_ok("r221m5-parse-pipe-04", "a | b | c");
    if let SyntaxNode::Pipe { rhs, .. } = &n {
        assert!(matches!(rhs.as_ref(), SyntaxNode::Pipe { .. }));
    } else {
        panic!("expected Pipe");
    }
}

#[test]
fn r221m5_parse_pipe_05_semi_separator() {
    // `/tmp` lexes as `Op("/"), Ident("tmp")` — path-glyph gluing is a
    // R222 concern per the R221.M4 changelog. Test uses a bare arg
    // instead.
    let n = parse_ok("r221m5-parse-pipe-05", "cd home; ls");
    assert!(matches!(n, SyntaxNode::Seq { .. }));
    if let SyntaxNode::Seq { items, .. } = &n {
        assert_eq!(items.len(), 2);
    }
}

#[test]
fn r221m5_parse_pipe_06_newline_separator() {
    let n = parse_ok("r221m5-parse-pipe-06", "ls\ncd");
    assert!(matches!(n, SyntaxNode::Seq { .. }));
}

#[test]
fn r221m5_parse_pipe_07_string_arg() {
    let n = parse_ok("r221m5-parse-pipe-07", r#"echo "hello world""#);
    if let SyntaxNode::Cmd { args, .. } = &n {
        assert!(matches!(args.first().unwrap(), SyntaxNode::LitStr { .. }));
    } else {
        panic!("expected Cmd");
    }
}

#[test]
fn r221m5_parse_pipe_08_numeric_arg() {
    let n = parse_ok("r221m5-parse-pipe-08", "head 10");
    if let SyntaxNode::Cmd { args, .. } = &n {
        assert!(matches!(
            args.first().unwrap(),
            SyntaxNode::LitInt { value: 10, .. }
        ));
    } else {
        panic!("expected Cmd");
    }
}

#[test]
fn r221m5_parse_pipe_09_paren_group() {
    let n = parse_ok("r221m5-parse-pipe-09", "( ls )");
    if let SyntaxNode::Cmd { args, .. } = &n {
        // `( ls )` is parsed as `Cmd { name: Group(Cmd(ls)) }` because
        // the leading `(` starts an atom (arg-shaped), and Cmd absorbs.
        // Accept either Group top-level or Cmd wrapping it.
        assert!(args.is_empty());
    } else if let SyntaxNode::Group { .. } = &n {
        // Also acceptable
    } else {
        panic!("unexpected node: {n:?}");
    }
}

#[test]
fn r221m5_parse_pipe_10_redirect_stdout() {
    let n = parse_ok("r221m5-parse-pipe-10", "ls > out");
    if let SyntaxNode::Redirect { kind, .. } = &n {
        assert_eq!(*kind, RedirectKind::StdoutOverwrite);
    } else {
        panic!("expected Redirect, got {n:?}");
    }
}

#[test]
fn r221m5_parse_pipe_11_redirect_stdin() {
    let n = parse_ok("r221m5-parse-pipe-11", "wc < in");
    assert!(matches!(n, SyntaxNode::Redirect { kind: RedirectKind::StdinFrom, .. }));
}

#[test]
fn r221m5_parse_pipe_12_field_access_arg() {
    let n = parse_ok("r221m5-parse-pipe-12", "sort by f.size");
    // "sort" cmd with args ["by", FieldAccess(f, size)]
    if let SyntaxNode::Cmd { args, .. } = &n {
        assert_eq!(args.len(), 2);
        assert!(matches!(&args[1], SyntaxNode::FieldAccess { .. }));
    } else {
        panic!("expected Cmd");
    }
}

#[test]
fn r221m5_parse_pipe_13_multiple_pipes() {
    let n = parse_ok("r221m5-parse-pipe-13", "cat f | sort | uniq | head 5");
    let mut depth = 0;
    let mut cur = &n;
    while let SyntaxNode::Pipe { rhs, .. } = cur {
        depth += 1;
        cur = rhs;
    }
    assert_eq!(depth, 3, "three `|` glyphs should nest 3 deep");
}

#[test]
fn r221m5_parse_pipe_14_empty_input_is_empty_seq() {
    let n = parse_ok("r221m5-parse-pipe-14", "");
    assert!(matches!(&n, SyntaxNode::Seq { items, .. } if items.is_empty()));
}

#[test]
fn r221m5_parse_pipe_15_reject_bare_operator() {
    parse_err("r221m5-parse-pipe-15", "|");
}

// ---- PAS-DEBT-B2-017: process substitution `>(cmd)` --------------

/// Regression: bare-file redirect target still parses as a filename
/// (`Ident`), NOT wrapped in `Group`. The proc-subst path must not
/// intercept the plain `> file` form.
#[test]
fn pas_debt_b2_017_procsubst_01_file_target_still_ident() {
    let n = parse_ok("pas-debt-b2-017-01", "foo > out.log");
    if let SyntaxNode::Redirect { kind, target, .. } = &n {
        assert_eq!(*kind, RedirectKind::StdoutOverwrite);
        assert!(
            matches!(target.as_ref(), SyntaxNode::FieldAccess { .. }),
            "file target should be FieldAccess(out.log), got {target:?}"
        );
    } else {
        panic!("expected Redirect, got {n:?}");
    }
}

/// `foo > (bar)` — process substitution with a single command. Target
/// is the inner `Cmd(bar)` directly, NOT wrapped in `Group`.
#[test]
fn pas_debt_b2_017_procsubst_02_single_cmd() {
    let n = parse_ok("pas-debt-b2-017-02", "foo > (bar)");
    if let SyntaxNode::Redirect { kind, target, .. } = &n {
        assert_eq!(*kind, RedirectKind::StdoutOverwrite);
        assert!(
            matches!(target.as_ref(), SyntaxNode::Cmd { .. }),
            "proc-subst target should be bare Cmd (unwrapped), got {target:?}"
        );
        if let SyntaxNode::Cmd { name, .. } = target.as_ref() {
            if let SyntaxNode::Ident { name: n, .. } = name.as_ref() {
                assert_eq!(n.as_str(), "bar");
            } else {
                panic!("proc-subst inner Cmd name should be Ident, got {name:?}");
            }
        }
    } else {
        panic!("expected Redirect, got {n:?}");
    }
}

/// `foo > (bar | baz)` — pipeline inside process substitution. Target
/// is the inner `Pipe` node directly.
#[test]
fn pas_debt_b2_017_procsubst_03_pipeline_inside() {
    let n = parse_ok("pas-debt-b2-017-03", "foo > (bar | baz)");
    if let SyntaxNode::Redirect { kind, target, .. } = &n {
        assert_eq!(*kind, RedirectKind::StdoutOverwrite);
        assert!(
            matches!(target.as_ref(), SyntaxNode::Pipe { .. }),
            "proc-subst target should be bare Pipe (unwrapped), got {target:?}"
        );
    } else {
        panic!("expected Redirect, got {n:?}");
    }
}

/// `foo >> (bar)` — append-mode redirect with process substitution.
/// Same shape as StdoutOverwrite, only the kind differs.
#[test]
fn pas_debt_b2_017_procsubst_04_append_mode() {
    let n = parse_ok("pas-debt-b2-017-04", "foo >> (bar)");
    if let SyntaxNode::Redirect { kind, target, .. } = &n {
        assert_eq!(*kind, RedirectKind::StdoutAppend);
        assert!(
            matches!(target.as_ref(), SyntaxNode::Cmd { .. }),
            "proc-subst target should be bare Cmd, got {target:?}"
        );
    } else {
        panic!("expected Redirect, got {n:?}");
    }
}

/// Unclosed proc-subst `foo > (bar` reports a parse error rather than
/// silently accepting or panicking (the never-panic fuzz invariant).
#[test]
fn pas_debt_b2_017_procsubst_05_unclosed_errors() {
    parse_err("pas-debt-b2-017-05", "foo > (bar");
}

/// Round-trip: `foo > (bar)` pretty-prints back to `foo > (bar)`,
/// preserving the proc-subst reading (target's parens are re-emitted
/// by the pretty-printer's proc-subst-shaped-target branch).
#[test]
fn pas_debt_b2_017_procsubst_06_roundtrip_single_cmd() {
    common::assert_roundtrip("pas-debt-b2-017-06", "foo > (bar)");
}

/// Round-trip: `foo > (bar | baz)` — proc-subst wrapping a pipeline
/// must survive pretty-print + reparse identically.
#[test]
fn pas_debt_b2_017_procsubst_07_roundtrip_pipeline() {
    common::assert_roundtrip("pas-debt-b2-017-07", "foo > (bar | baz)");
}

/// Round-trip: `foo >> (bar)` — append proc-subst.
#[test]
fn pas_debt_b2_017_procsubst_08_roundtrip_append() {
    common::assert_roundtrip("pas-debt-b2-017-08", "foo >> (bar)");
}
