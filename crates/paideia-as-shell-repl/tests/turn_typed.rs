//! R225.M4 typed-turn corpus: eight fixtures pinning stage 2's new
//! HM Algorithm W wiring — [`paideia_as_shell_repl::type_stage`] — as
//! it appears through the outer [`paideia_as_shell_repl::eval_turn`]
//! surface.
//!
//! Each `assert!` names the fixture (`r225m4-typed-NN`) in its message
//! so the R220.M10 correlator attributes a regression to a fixture
//! without re-parsing the test name.
//!
//! # M4 deviations from the ticket's original test corpus
//!
//! The ticket's test 3 assumed a `LitBool` mapping to a shell-hm `Bool`
//! literal; shell-hm at R225.M3 defines only `Lit::Int` and `Lit::Str`
//! (see `paideia_as_shell_hm::expr::Lit`), so this fixture instead
//! covers the ticket's documented deviation path: bare `"true"` reaches
//! the type stage as `Cmd { name: Ident("true"), args: [] }`, defers
//! to command dispatch under the M4 skip rule (Ident-head nullary
//! command), and surfaces as a `cmd:` error under the empty registry.
//! `inferred_type` is `None` on that turn — the *documented* outcome
//! when shell-hm cannot model the literal.
//!
//! The ticket's test 4 assumed a bare `"foo"` would surface as a
//! `type:`-prefixed unbound-variable error. Under the M4 skip rule
//! that shape also reaches command dispatch (`Cmd { name: Ident("foo"),
//! args: [] }` is a nullary command invocation, not a variable
//! reference), so the fixture is reshaped to lift the unbound-variable
//! probe inside a lambda body — `{|x| y}` types the body under a
//! λ-context where `y` is a genuine `Var` reference; HM then reports
//! `UnboundVar("y")` and the turn aborts with the promised `type:`
//! prefix.

use paideia_as_shell_hm::{MonoType, TypeEnv};
use paideia_as_shell_repl::{eval_turn, ReplState, TurnResult};

/// `r225m4-typed-01`: a bare integer literal `"42"` reaches stage 2
/// as `Cmd { name: LitInt, args: [] }`; type_check unwraps the Cmd
/// pass-through (empty args, non-Ident head) and infers `Con("Int")`.
/// The turn's `inferred_type` is `Some(Int)` and the executor still
/// runs — the fallback `Value("cmd: non-name head")` from the M3 Cmd
/// arm is fine; this fixture pins the *type-stage* outcome, not the
/// executor's rendering.
#[test]
fn r225m4_typed_01_bare_int_literal_types_as_int() {
    const FP: &str = "r225m4-typed-01";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "42".to_owned());

    assert_eq!(
        turn.inferred_type,
        Some(MonoType::Con("Int".to_owned())),
        "{FP}: bare integer literal must type as Int, got: {:?}",
        turn.inferred_type
    );
}

/// `r225m4-typed-02`: a bare string literal `"\"hello\""` reaches
/// stage 2 as `Cmd { name: LitStr, args: [] }`; type_check unwraps
/// and infers `Con("Str")`.
#[test]
fn r225m4_typed_02_bare_string_literal_types_as_str() {
    const FP: &str = "r225m4-typed-02";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "\"hello\"".to_owned());

    assert_eq!(
        turn.inferred_type,
        Some(MonoType::Con("Str".to_owned())),
        "{FP}: bare string literal must type as Str, got: {:?}",
        turn.inferred_type
    );
}

/// `r225m4-typed-03` (deviation): the ticket's original test expected
/// `Some(Bool)` for `"true"`, but shell-hm at R225.M3 has no `Bool`
/// literal and the pipeline parser produces `Cmd { name: Ident("true"),
/// args: [] }` (bare `true` is an Ident at the AST level, not
/// `LitBool`). Under the M4 skip rule, an Ident-headed nullary Cmd
/// defers to the executor's Cmd dispatch — the type stage records
/// `inferred_type: None` and does NOT abort the turn; the empty
/// registry then rejects `true` as an unknown command with a `cmd:`
/// prefix. The fixture pins both the `inferred_type` outcome AND the
/// `cmd:` render so a regression in the skip rule (typing Ident heads
/// as unbound Var references) is caught even if the type stage still
/// produces `None`.
#[test]
fn r225m4_typed_03_bool_literal_deferred_to_cmd_dispatch() {
    const FP: &str = "r225m4-typed-03";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "true".to_owned());

    assert_eq!(
        turn.inferred_type, None,
        "{FP}: shell-hm has no Bool; the Ident-headed Cmd defers, so inferred_type must be None, got: {:?}",
        turn.inferred_type
    );
    match turn.result {
        TurnResult::Error(e) => assert!(
            e.starts_with("cmd:"),
            "{FP}: expected 'cmd:' prefix on empty-registry dispatch, got: {e:?}"
        ),
        TurnResult::Value(v) => panic!(
            "{FP}: expected cmd-dispatch error on 'true', got Value: {v}"
        ),
    }
}

/// `r225m4-typed-04` (deviation): an unbound variable `"foo"` at
/// pipeline top-level reaches stage 2 as an Ident-headed Cmd and thus
/// defers to command dispatch under the M4 skip rule (see the module
/// doc). To exercise the real HM `UnboundVar` path, this fixture uses
/// a lambda body `{|x| y}` where `y` is a genuine `Var` reference in
/// lambda context; HM sees `Lam("x", Var("y"))`, `y` is not in the
/// (empty) `state.type_env`, `infer` returns `InferError::UnboundVar`,
/// and the turn aborts with the promised `type:` prefix.
#[test]
fn r225m4_typed_04_unbound_var_in_lambda_body_aborts_turn() {
    const FP: &str = "r225m4-typed-04";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "{|x| y}".to_owned());

    assert_eq!(
        turn.inferred_type, None,
        "{FP}: abort at stage 2 must leave inferred_type = None, got: {:?}",
        turn.inferred_type
    );
    match turn.result {
        TurnResult::Error(e) => {
            assert!(
                e.starts_with("type:"),
                "{FP}: HM abort must render with 'type:' prefix, got: {e:?}"
            );
            let lc = e.to_ascii_lowercase();
            assert!(
                lc.contains("unbound"),
                "{FP}: diagnostic must mention 'unbound', got: {e:?}"
            );
            // The offending name is rendered inside backticks (see
            // `TypeStageError::Display`); grep for it there to avoid a
            // false positive on the letter 'y' inside 'type'.
            assert!(
                e.contains("`y`"),
                "{FP}: diagnostic must name the offending var as `y` in backticks, got: {e:?}"
            );
        }
        TurnResult::Value(v) => panic!(
            "{FP}: expected type: error on unbound var, got Value: {v}"
        ),
    }
}

/// `r225m4-typed-05`: an identity lambda `"{|x| x}"` types as
/// `∀a. a -> a`. Stage 2 lowers to `Lam("x", Var("x"))`, HM infers a
/// fresh `Var(a) -> Var(a)`. We assert only the outer shape (an
/// `Arrow` with structurally identical argument and result types) so a
/// change to the fresh-var counter does not break the fixture.
#[test]
fn r225m4_typed_05_identity_lambda_types_as_arrow_a_a() {
    const FP: &str = "r225m4-typed-05";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "{|x| x}".to_owned());

    match turn.inferred_type {
        Some(MonoType::Arrow(ref a, ref b)) => {
            assert_eq!(
                a, b,
                "{FP}: identity lambda's Arrow must have equal arg/result, got: {a:?} vs {b:?}"
            );
            assert!(
                matches!(a.as_ref(), MonoType::Var(_)),
                "{FP}: identity lambda's arg type must be a fresh Var, got: {a:?}"
            );
        }
        other => panic!(
            "{FP}: identity lambda must type as an Arrow, got: {other:?}"
        ),
    }
}

/// `r225m4-typed-06`: a datalog block `"datalog { p(a). }"` is skipped
/// by stage 2 up front (its own R226.M9 typed pass runs in stage 4),
/// so `inferred_type` is `None` and the turn's `result` is whatever
/// the datalog branch produces (a `typecheck:` error under the empty
/// registry, per R229.M2).
#[test]
fn r225m4_typed_06_datalog_block_skips_type_stage() {
    const FP: &str = "r225m4-typed-06";
    let mut state = ReplState::new();
    let turn = eval_turn(&mut state, "datalog { p(a). }".to_owned());

    assert_eq!(
        turn.inferred_type, None,
        "{FP}: DatalogBlock must be skipped by stage 2, got: {:?}",
        turn.inferred_type
    );
    // The datalog branch still runs — the M4 wiring does not alter
    // its output. We only check that we did NOT render a `type:`
    // error (i.e., stage 2 did not misroute the block into HM).
    match &turn.result {
        TurnResult::Error(e) => assert!(
            !e.starts_with("type:"),
            "{FP}: DatalogBlock must not surface a 'type:' error, got: {e:?}"
        ),
        TurnResult::Value(_) => {}
    }
}

/// `r225m4-typed-07`: a Cmd with a registered nullary sig — reached
/// as `Cmd { name: Ident("ls"), args: [] }` — defers to command
/// dispatch under the M4 skip rule (Ident-headed nullary Cmd is an
/// `UnsupportedNode`, so the type stage records `inferred_type: None`
/// without aborting). The executor then dispatches through the M3
/// happy path. Documents the ticket's "Cmd shape isn't supported →
/// inferred_type = None" outcome.
#[test]
fn r225m4_typed_07_registered_cmd_defers_and_dispatches() {
    const FP: &str = "r225m4-typed-07";

    // Build the registry via `with_command` + a nullary CommandSig
    // (borrowed from the R229.M3 fixture shape — see turn_cmd.rs). The
    // M4 test only cares that the Cmd branch runs, so we do not need
    // to spell every CommandSig field here beyond what the dispatcher
    // consults for a nullary "ls".
    use paideia_as_shell_cmd::{
        CapSpec, CommandSig, CommandWeight, EffectRow, ExecuteResult, InvocationCtx,
    };
    fn noop_execute(ctx: &InvocationCtx) -> ExecuteResult {
        ExecuteResult::new(0, &ctx.fingerprint)
    }
    let ls_sig = CommandSig {
        name: "ls".to_owned(),
        input_schema: None,
        output_schema: None,
        arguments: vec![],
        flags: vec![],
        effects: EffectRow::pure(),
        required_capabilities: CapSpec::none(),
        execute: noop_execute,
        weight: CommandWeight::Light,
    };
    let mut state = ReplState::default().with_command("ls", ls_sig);

    let turn = eval_turn(&mut state, "ls".to_owned());
    assert_eq!(
        turn.inferred_type, None,
        "{FP}: Ident-headed Cmd defers to dispatch; inferred_type must be None, got: {:?}",
        turn.inferred_type
    );
    match turn.result {
        TurnResult::Value(v) => assert!(
            v.starts_with("cmd: ls ok"),
            "{FP}: registered nullary command must reach the happy path, got: {v:?}"
        ),
        TurnResult::Error(e) => panic!(
            "{FP}: expected happy-path dispatch on registered 'ls', got Error: {e}"
        ),
    }
}

/// `r225m4-typed-08`: `state.type_env` is threaded through a turn but
/// R225.M4 never mutates it — this fixture pins that invariant. A
/// fresh `ReplState::new()` starts with `TypeEnv::default()`; after
/// running a turn that would touch stage 2 (`"42"` — a real HM
/// success), the env is still equal to the empty default.
#[test]
fn r225m4_typed_08_type_env_survives_across_turns() {
    const FP: &str = "r225m4-typed-08";
    let mut state = ReplState::new();
    assert_eq!(
        state.type_env,
        TypeEnv::default(),
        "{FP}: fresh ReplState must have the default TypeEnv"
    );

    // First turn: a real HM path (bare `42` → `Int`).
    let t1 = eval_turn(&mut state, "42".to_owned());
    assert_eq!(
        t1.inferred_type,
        Some(MonoType::Con("Int".to_owned())),
        "{FP}: turn 1 must type '42' as Int; type_env otherwise not exercised"
    );
    assert_eq!(
        state.type_env,
        TypeEnv::default(),
        "{FP}: type_env must survive turn 1 unchanged (R225.M4 never mutates it)"
    );

    // Second turn: a deferring shape (Ident-headed nullary Cmd). Also
    // must not touch the env.
    let _t2 = eval_turn(&mut state, "ls".to_owned());
    assert_eq!(
        state.type_env,
        TypeEnv::default(),
        "{FP}: type_env must survive turn 2 unchanged"
    );

    // Third turn: an actual HM failure (unbound var in a lambda). Also
    // must not touch the env — the abort short-circuits at stage 2,
    // before any elaboration that might carry side effects.
    let _t3 = eval_turn(&mut state, "{|x| y}".to_owned());
    assert_eq!(
        state.type_env,
        TypeEnv::default(),
        "{FP}: type_env must survive an aborting stage 2 unchanged"
    );

    assert_eq!(
        state.turn_counter, 3,
        "{FP}: three turns must advance turn_counter to 3"
    );
}
