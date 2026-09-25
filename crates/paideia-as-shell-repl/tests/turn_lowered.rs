//! R229.M2 lowering + typecheck corpus: eight fixtures pinning
//! [`paideia_as_shell_repl::lower::lower_datalog`] against a small
//! slice of the shell-ast surface (facts, rules, negated body goals,
//! non-DatalogBlock roots) and pinning the datalog branch of
//! [`paideia_as_shell_repl::eval_turn`] against the R226.M9 typed
//! query path.
//!
//! Each `assert!` names the fingerprint (`r229m2-turn-NN`) in its
//! message so the R220.M10 correlator attributes a regression to a
//! fixture without re-parsing the test name.

use paideia_as_shell_ast::parser as ast_parser;
use paideia_as_shell_ast::{Context, NodeSpan, SyntaxNode};
use paideia_as_shell_datalog::{BodyGoal, Term};
use paideia_as_shell_repl::{eval_turn, lower_datalog, LowerError, ReplState, TurnResult};

/// Small helper: parse `src` with the shell-ast parser and unwrap. The
/// M1 corpus already covers parse failure explicitly; the M2 lowering
/// fixtures always start from a valid `DatalogBlock`.
fn parse_ok(src: &str) -> SyntaxNode {
    ast_parser::parse(src).expect("shell-ast parser must accept M2 fixture input")
}

/// `r229m2-turn-01`: `datalog { p(a). q(b). }` lowers to a `Program`
/// with two facts, then trips the empty-registry schema check and
/// surfaces a `typecheck:` error via `eval_turn`.
#[test]
fn r229m2_turn_01_two_fact_block_typecheck_fails() {
    const FP: &str = "r229m2-turn-01";

    // Lowering succeeds — no typecheck involvement in `lower_datalog`.
    let node = parse_ok("datalog { p(a). q(b). }");
    let program = lower_datalog(&node).expect("lowering must succeed on well-formed block");
    assert_eq!(
        program.facts.len(),
        2,
        "{FP}: two-fact block must lower to two `Program::facts` entries"
    );
    assert!(
        program.rules.is_empty(),
        "{FP}: no rules in the fixture — Program::rules must be empty"
    );

    // End-to-end via `eval_turn`: typecheck fires with an empty
    // registry, so both `p/1` and `q/1` are `UnknownPredicate`. The
    // executor renders a single "typecheck:" error regardless of the
    // per-atom count.
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "datalog { p(a). q(b). }".to_owned());
    match turn.result {
        TurnResult::Error(e) => assert!(
            e.contains("typecheck"),
            "{FP}: expected 'typecheck' in error diagnostic, got: {e:?}"
        ),
        TurnResult::Value(v) => panic!("{FP}: expected Error, got Value: {v}"),
    }
}

/// `r229m2-turn-02`: `datalog { p(a). q(?X) => p(?X). }` lowers to a
/// `Program` with one fact and one rule; typecheck still fires.
#[test]
fn r229m2_turn_02_rule_lowers_and_typechecks() {
    const FP: &str = "r229m2-turn-02";

    let node = parse_ok("datalog { p(a). q(?X) => p(?X). }");
    let program = lower_datalog(&node).expect("lowering must succeed");
    assert_eq!(program.facts.len(), 1, "{FP}: one fact `p(a)` expected");
    assert_eq!(program.rules.len(), 1, "{FP}: one rule `q(?X) => p(?X)` expected");

    let rule = &program.rules[0];
    assert_eq!(rule.head.predicate, "q", "{FP}: rule head predicate must be `q`");
    assert_eq!(rule.body.len(), 1, "{FP}: rule body has one goal");
    match &rule.body[0] {
        BodyGoal::Positive(a) => assert_eq!(
            a.predicate, "p",
            "{FP}: rule body goal predicate must be `p`, got {}",
            a.predicate
        ),
        BodyGoal::Negative(_) => panic!("{FP}: rule body must be positive"),
    }

    // The head's argument slot is a Var (from `?X`).
    match &rule.head.terms[0] {
        Term::Var(name) => assert_eq!(name, "X", "{FP}: head var name must be `X`"),
        other => panic!("{FP}: head term must be Var, got {other:?}"),
    }

    // End-to-end: empty registry rejects `q/1` and `p/1`.
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "datalog { p(a). q(?X) => p(?X). }".to_owned());
    match turn.result {
        TurnResult::Error(e) => assert!(
            e.contains("typecheck"),
            "{FP}: expected 'typecheck' in error, got: {e:?}"
        ),
        TurnResult::Value(v) => panic!("{FP}: expected Error, got Value: {v}"),
    }
}

/// `r229m2-turn-03`: a synthetic `SyntaxNode` — the shell-ast parser
/// today does not emit `NotAtom` inside a rule body (the datalog
/// parser's rule loop only accepts positive atoms), so we construct
/// the AST directly to pin the lowering contract: a `NotAtom { inner:
/// Atom }` body element becomes `BodyGoal::Negative(atom)`.
#[test]
fn r229m2_turn_03_notatom_in_rule_body_lowers_to_negative() {
    const FP: &str = "r229m2-turn-03";
    let span = NodeSpan::synthetic(Context::Datalog);

    let pos_body = SyntaxNode::Atom {
        pred: "p".into(),
        args: vec![SyntaxNode::QVar { name: "X".into(), span }],
        span,
    };
    let neg_inner = SyntaxNode::Atom {
        pred: "r".into(),
        args: vec![SyntaxNode::QVar { name: "X".into(), span }],
        span,
    };
    let neg_body = SyntaxNode::NotAtom {
        inner: Box::new(neg_inner),
        span,
    };
    let head = SyntaxNode::Atom {
        pred: "q".into(),
        args: vec![SyntaxNode::QVar { name: "X".into(), span }],
        span,
    };
    let rule = SyntaxNode::Rule {
        head: Box::new(head),
        body: vec![pos_body, neg_body],
        span,
    };
    let block = SyntaxNode::DatalogBlock {
        items: vec![rule],
        span,
    };

    let program = lower_datalog(&block).expect("synthetic rule must lower");
    assert_eq!(program.rules.len(), 1, "{FP}: exactly one rule");
    let body = &program.rules[0].body;
    assert_eq!(body.len(), 2, "{FP}: two body goals");
    assert!(
        body[0].is_positive(),
        "{FP}: first body goal (`p(?X)`) must be positive"
    );
    assert!(
        body[1].is_negated(),
        "{FP}: second body goal (`not r(?X)`) must be negative"
    );
    assert_eq!(
        body[1].atom().predicate,
        "r",
        "{FP}: negated atom's predicate must be `r`"
    );
}

/// `r229m2-turn-04`: `lower_datalog` refuses a root that is not a
/// `DatalogBlock`. Uses a bare `Ident` node for the input.
#[test]
fn r229m2_turn_04_non_block_root_rejected() {
    const FP: &str = "r229m2-turn-04";
    let span = NodeSpan::synthetic(Context::Pipeline);
    let node = SyntaxNode::Ident {
        name: "foo".into(),
        span,
    };
    match lower_datalog(&node) {
        Err(LowerError::UnsupportedNode { kind }) => assert_eq!(
            kind, "ident",
            "{FP}: `UnsupportedNode.kind` must name the offending variant"
        ),
        Err(other) => panic!("{FP}: expected UnsupportedNode, got: {other:?}"),
        Ok(_) => panic!("{FP}: non-block root must not lower to a Program"),
    }
}

/// `r229m2-turn-05`: end-to-end via `eval_turn` — a plain
/// `datalog { … }` fact input produces `TurnResult::Error("typecheck: …")`
/// with the current empty-registry executor. Pins the render contract
/// M2 promised.
#[test]
fn r229m2_turn_05_eval_turn_typecheck_render() {
    const FP: &str = "r229m2-turn-05";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "datalog { widget(green). }".to_owned());
    match turn.result {
        TurnResult::Error(e) => {
            assert!(
                e.starts_with("typecheck:"),
                "{FP}: prefix must be 'typecheck:', got: {e:?}"
            );
            assert!(
                e.contains("error"),
                "{FP}: must include the word 'error', got: {e:?}"
            );
        }
        TurnResult::Value(v) => panic!("{FP}: expected typecheck Error, got Value: {v}"),
    }
}

/// `r229m2-turn-06`: pipeline branch renders the pipe stage tag.
/// R229.M4 lands real value threading; against an empty registry
/// `ls | wc` halts at stage 0 with `UnknownCommand("ls")`, so the turn
/// surfaces an Error rendered `pipe: pipeline halted at stage 0
/// (unknown command: ls)` — the M2/M3 stub Value shape is gone. The
/// fingerprint stays `r229m2-turn-06` because the property under
/// test — Pipe routes through the pipe arm and carries the `pipe:`
/// stage prefix — is preserved.
#[test]
fn r229m2_turn_06_pipeline_stub_unchanged() {
    const FP: &str = "r229m2-turn-06";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "ls | wc".to_owned());
    match turn.result {
        TurnResult::Error(e) => {
            assert!(
                e.starts_with("pipe:"),
                "{FP}: pipe halt must carry the `pipe:` stage tag, got: {e:?}"
            );
            assert!(
                e.contains("halted at stage 0"),
                "{FP}: empty registry halts at stage 0, got: {e:?}"
            );
        }
        TurnResult::Value(v) => panic!(
            "{FP}: empty registry must halt the pipeline, got Value: {v}"
        ),
    }
}

/// `r229m2-turn-07`: R229.M2 did not touch the Lambda arm, so under M2
/// this asserted the `lambda: <not yet implemented>` stub. R229.M5
/// lands the executor: `{ |x| x }` now evaluates to a closure and the
/// arm renders `<closure>`. The fixture id stays because the fact
/// under test — the Datalog / Cmd / Pipe wiring of M2 does not
/// perturb the Lambda arm's dispatch — is unchanged.
#[test]
fn r229m2_turn_07_lambda_stub_unchanged() {
    const FP: &str = "r229m2-turn-07";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "{ |x| x }".to_owned());
    match turn.result {
        TurnResult::Value(v) => assert_eq!(
            v, "<closure>",
            "{FP}: identity lambda must render `<closure>` under M5, got: {v:?}"
        ),
        TurnResult::Error(e) => panic!("{FP}: lambda should not error, got: {e}"),
    }
}

/// `r229m2-turn-08`: ten-turn state stress. A mix of pipeline stubs,
/// lambda stubs, and datalog blocks (each of the latter surfacing as a
/// `typecheck:` error under the empty-registry executor) does not
/// panic, and `turn_counter` reaches exactly 10.
#[test]
fn r229m2_turn_08_ten_turn_state_stress() {
    const FP: &str = "r229m2-turn-08";
    let mut state = ReplState::new();
    let inputs: [&str; 10] = [
        "ls",
        "datalog { a(x). }",
        "{ |x| x }",
        "ls | wc",
        "datalog { }",
        "ls -al",
        "datalog { b(y). c(?Z) => b(?Z). }",
        "{ |a| a }",
        "cat foo",
        "datalog { d(1). }",
    ];
    for (i, src) in inputs.iter().enumerate() {
        let turn = eval_turn(&mut state, (*src).to_owned());
        // Fingerprint id is the pre-increment counter value.
        assert_eq!(
            turn.fingerprint,
            format!("repl.turn.{:016x}", i),
            "{FP}: iteration {i} fingerprint mismatch on input {src:?}"
        );
        // Neither Value nor Error is a panic — the executor must always
        // return a rendered outcome.
        match turn.result {
            TurnResult::Value(_) | TurnResult::Error(_) => {}
        }
    }
    assert_eq!(
        state.turn_counter, 10,
        "{FP}: ten turns must advance the counter to 10"
    );
}
