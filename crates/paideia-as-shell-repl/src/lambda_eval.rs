//! Lambda-fragment evaluator for R229.M5 — a small tree-walking
//! interpreter over the shell-ast's lambda variants, invoked by the
//! `SyntaxNode::Lambda` arm of [`crate::turn::execute`].
//!
//! # Position in the R229 pipeline
//!
//! R229.M2 wired the Datalog branch, R229.M3 the `Cmd` dispatcher, and
//! R229.M4 the `Pipe` value threader. Each of those runs over an
//! existing sub-language substrate — the seminaïve fixpoint, the
//! `CmdDispatchRegistry`, the pipeline runner. Lambdas have no such
//! substrate yet: the shell-hm crate types them (R225.M4 wired the
//! checker into stage 2 of the turn) but nothing executes them. M5
//! lands the first executor — a tree walker that produces a
//! [`Value`] on the same surface `TurnResult::Value` prints.
//!
//! # Why a tree walker, not a JIT
//!
//! The R229 milestone doc mentions "the lambda JIT" for a later
//! milestone. M5 keeps the surface honest — evaluate lambdas — without
//! committing to code-gen. The walker is *deliberately* small: eleven
//! `SyntaxNode` arms map to eleven-ish match cases, one closure struct,
//! one error enum. When the JIT lands, this module becomes the
//! interpreter fallback (portable, always available) and the JIT
//! becomes an optimization on hot lambdas — the [`Value`] surface a
//! caller sees does not change.
//!
//! # Scope of the M5 subset
//!
//! Recognised shapes (walk rules in [`eval_lambda`]):
//!
//! * Literals: [`SyntaxNode::LitInt`], [`SyntaxNode::LitStr`],
//!   [`SyntaxNode::LitBool`] → [`Value::Int`] / [`Value::Str`] /
//!   [`Value::Bool`].
//! * References: [`SyntaxNode::Var`] and [`SyntaxNode::Ident`] both
//!   look up the name in `env`; unbound → [`LambdaError::UnboundVar`].
//! * [`SyntaxNode::Group`] passes through to its `inner`.
//! * [`SyntaxNode::Lambda`] with non-empty `params` produces
//!   [`Value::Fn`] capturing the current `env`. The empty-`params` case
//!   is treated as a thunk and its body is evaluated in `env` right
//!   away — the lambda parser's `{ body }` no-header form (see
//!   `parser/lambda.rs`) is the only source of empty-`params` lambdas,
//!   and a user typing `{ 42 }` wants `42`, not `<closure>`.
//! * [`SyntaxNode::Cmd`] used as application: `name` evaluates to a
//!   [`Value::Fn`], each `arg` evaluates in the *caller's* env, then
//!   the closure's captured env is extended with the parameter
//!   bindings and its body evaluated. Arity mismatch surfaces as
//!   [`LambdaError::ArityMismatch`]; a non-function head surfaces as
//!   [`LambdaError::TypeError`]. The `Cmd` node is what the shell-ast
//!   lambda parser produces for `f arg` inside a lambda body (see
//!   `parser/lambda.rs::parse_postfix`), so this arm is the primary
//!   application surface at M5. An explicit [`SyntaxNode::App`] is
//!   handled the same way for the small set of programmatic callers
//!   that build it directly.
//! * [`SyntaxNode::BinOp`] over `Int` operands: `+`, `-`, `*`, `/`
//!   arithmetic; `==`, `!=`, `<`, `<=`, `>`, `>=` comparisons produce
//!   `Bool`; `+` over `Str` operands concatenates. Any other operator
//!   / operand combination is [`LambdaError::TypeError`]. Division by
//!   zero is also `TypeError` (a JIT will lower this to a hardware
//!   trap; the walker rejects it up front so the user sees a message
//!   rather than a panic).
//! * [`SyntaxNode::UnaryOp`]: `-` negates `Int`, `not` negates `Bool`.
//! * [`SyntaxNode::Let`]: evaluate `value` in the current env, extend
//!   with `name → val`, evaluate `body` under the extension. Kept for
//!   the programmatic path — the R221.M5 lambda parser has no `let`
//!   production yet, but the AST variant exists and the M5 walker
//!   handles it so a follow-on parser milestone can wire it up
//!   without touching this file.
//!
//! Every other shape (Match, RecordExpr, FieldAccess, the pipeline /
//! datalog variants) is [`LambdaError::NotSupported`]. `NotSupported`
//! is a hard failure at M5 — the caller in `turn::execute` renders it
//! as a `lambda:` error rather than silently proceeding, because the
//! Lambda arm has no other path (unlike stage 2's `UnsupportedNode`
//! which is a soft skip because the executor's other arms may still
//! run).
//!
//! # Not in this module
//!
//! * Type checking. Stage 2 (`type_stage::type_check`) runs before we
//!   see the node and either produces a `MonoType` or a soft
//!   `UnsupportedNode` fallback. M5 does *not* re-check types — it
//!   surfaces runtime type mismatches as [`LambdaError::TypeError`]
//!   with a rendered `{op} not supported on {lhs:?} and {rhs:?}` shape,
//!   which is enough for the user to see what happened without a
//!   second checker crossing paths with HM.
//! * Side effects on `ReplState`. The walker is a pure function of
//!   `(node, env)`. The caller in `turn.rs` snapshots
//!   `state.value_env` before calling and does not update it — R229.M5
//!   does not persist top-level bindings across turns; that lands with
//!   the follow-on milestone that wires `let` at the REPL surface.

use std::collections::HashMap;
use std::fmt;

use paideia_as_shell_ast::SyntaxNode;

/// A runtime value produced by [`eval_lambda`].
///
/// The variants intentionally mirror the primitive types the M5 walker
/// can build — `Int` / `Str` / `Bool` come from literal or binop
/// evaluation, `Unit` is reserved for a future `()` literal (no source
/// syntax produces it at M5, but the caller in `turn.rs` renders it
/// as `()` so the field is not dead weight), and `Fn` wraps a lambda
/// as a first-class value.
///
/// Deliberately not `Debug`-derived-and-forgotten: the caller renders
/// via pattern-matching (see the Lambda arm in [`crate::turn::execute`])
/// so the `Debug` shape stays for developer diagnostics only. `PartialEq`
/// is derived so tests can compare `Value::Int(42) == v` directly; a
/// closure comparison compares the params vec, the body AST, and the
/// captured env structurally — good enough for the tests that need to
/// distinguish "the same closure" from "a fresh one".
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// A 64-bit signed integer. Matches [`SyntaxNode::LitInt`]'s
    /// carrier type; no widening.
    Int(i64),
    /// A UTF-8 string. Matches [`SyntaxNode::LitStr`]'s carrier.
    Str(String),
    /// A boolean, produced by [`SyntaxNode::LitBool`] or by any
    /// comparison-shaped `BinOp`.
    Bool(bool),
    /// The trivial value. Reserved for a future `()` literal / the
    /// result of a side-effect-only body; no M5 source shape produces
    /// it, but the caller renders it as `"()"` so we can grow into it
    /// without a second renderer touch.
    Unit,
    /// A first-class function value: parameters, body AST, and the
    /// environment captured at the [`SyntaxNode::Lambda`] site.
    Fn(Closure),
}

/// A first-class function value.
///
/// Owns its body AST outright (a `SyntaxNode`, not a reference) so a
/// closure produced by turn N can outlive the caller frame it was
/// evaluated in. Captured bindings are cloned on close-over — the
/// walker never lets a closure alias the caller's env, which keeps
/// mutation-through-alias impossible even before we grow reference
/// types.
#[derive(Clone, Debug, PartialEq)]
pub struct Closure {
    /// Parameter names in declaration order — same order as the
    /// arguments a caller must supply.
    pub params: Vec<String>,
    /// The lambda body AST. Cloned out of the source `SyntaxNode::Lambda`
    /// so the closure is self-contained (no borrow back into the caller's
    /// AST arena).
    pub body: SyntaxNode,
    /// Bindings visible at the lambda site, cloned on capture. Applied
    /// as the base for the parameter extension when the closure is
    /// called.
    pub captured: HashMap<String, Value>,
}

/// Failure modes for [`eval_lambda`].
///
/// Kept as an enum (not a `String` newtype) so the caller in
/// [`crate::turn::execute`] renders each shape in a way that a future
/// diagnostics layer can machine-address — the R229.M5 caller collapses
/// them into `format!("lambda: {}", err)` today, but leaving the enum
/// carriers structured means a later milestone can lift each into a
/// span-carrying diagnostic without re-parsing the string.
#[derive(Clone, Debug, PartialEq)]
pub enum LambdaError {
    /// The env does not bind the referenced name. Payload is the
    /// name as typed.
    UnboundVar(String),
    /// A call site supplied a wrong argument count for the applied
    /// closure. Both counts named so a diagnostic can point out
    /// which side has more.
    ArityMismatch {
        /// Number of parameters the closure declares.
        expected: usize,
        /// Number of arguments the caller supplied.
        actual: usize,
    },
    /// A runtime type mismatch — `1 + "s"`, applying a non-function,
    /// dividing by zero, etc. Payload is a rendered explanation.
    TypeError(String),
    /// The AST shape is outside the M5 subset (Match, RecordExpr, a
    /// pipeline-only variant reached through synthetic construction,
    /// …). Payload is a short variant tag or reason.
    NotSupported(String),
}

impl fmt::Display for LambdaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnboundVar(name) => write!(f, "unbound variable `{name}`"),
            Self::ArityMismatch { expected, actual } => write!(
                f,
                "arity mismatch: expected {expected} argument(s), got {actual}"
            ),
            Self::TypeError(msg) => write!(f, "type error: {msg}"),
            Self::NotSupported(what) => write!(f, "unsupported node: {what}"),
        }
    }
}

impl std::error::Error for LambdaError {}

/// Evaluate a lambda-context [`SyntaxNode`] under `env` to a [`Value`].
///
/// See the module doc for the full list of recognised shapes. The walker
/// is a plain recursive function — no explicit continuation stack,
/// no arena — because the M5 subset is small and the tests exercise
/// depths of at most three or four nested nodes. When the R229.M6
/// diagnostics layer or a JIT arrives, this function stays as the
/// portable fallback and the specialised paths sit alongside it.
pub fn eval_lambda(
    node: &SyntaxNode,
    env: &HashMap<String, Value>,
) -> Result<Value, LambdaError> {
    match node {
        SyntaxNode::LitInt { value, .. } => Ok(Value::Int(*value)),
        SyntaxNode::LitStr { value, .. } => Ok(Value::Str(value.clone())),
        SyntaxNode::LitBool { value, .. } => Ok(Value::Bool(*value)),

        // Both name-shaped variants resolve the same way: the AST
        // splits `Var` (Lambda context) from `Ident` (Pipeline /
        // Datalog context) as a colorer hint, but at eval time a bare
        // name means the same thing.
        SyntaxNode::Var { name, .. } | SyntaxNode::Ident { name, .. } => env
            .get(name)
            .cloned()
            .ok_or_else(|| LambdaError::UnboundVar(name.clone())),

        SyntaxNode::Group { inner, .. } => eval_lambda(inner, env),

        SyntaxNode::Lambda { params, body, .. } => {
            if params.is_empty() {
                // No-header thunk form `{ body }` — evaluate the body
                // directly. The parser produces this when the user types
                // a bare expression in braces (see
                // `parser/lambda.rs::parse_lambda_block`); auto-forcing
                // here matches what a user typing `{ 42 }` expects.
                eval_lambda(body, env)
            } else {
                Ok(Value::Fn(Closure {
                    params: params.clone(),
                    body: (**body).clone(),
                    captured: env.clone(),
                }))
            }
        }

        // `Cmd { name, args }` inside a lambda is application. The
        // lambda parser's `parse_postfix` produces this shape for
        // `ident { |a| body }` calls; the M5 walker treats it as the
        // primary application surface.
        SyntaxNode::Cmd { name, args, .. } => apply(name, args, env),

        // `App { func, args }` is handled identically. The R221.M5
        // parser does not emit this variant at present, but the AST
        // provides it and programmatic callers (tests, a future
        // elaboration pass) will build it directly.
        SyntaxNode::App { func, args, .. } => apply(func, args, env),

        SyntaxNode::BinOp { op, lhs, rhs, .. } => {
            let l = eval_lambda(lhs, env)?;
            let r = eval_lambda(rhs, env)?;
            eval_binop(op, l, r)
        }

        SyntaxNode::UnaryOp { op, inner, .. } => {
            let v = eval_lambda(inner, env)?;
            eval_unaryop(op, v)
        }

        SyntaxNode::Let { name, value, body, .. } => {
            let val = eval_lambda(value, env)?;
            let mut extended = env.clone();
            extended.insert(name.clone(), val);
            eval_lambda(body, &extended)
        }

        // Everything else: name what did not run so the caller's
        // rendered `lambda: unsupported node: <tag>` message is
        // specific.
        other => Err(LambdaError::NotSupported(variant_tag(other).to_owned())),
    }
}

/// Apply a callable node to a list of argument nodes under `env`.
///
/// Factored out of the [`eval_lambda`] `Cmd` and `App` arms because
/// both are structurally identical — the split at the AST level is a
/// syntactic hint, not a semantic one. The head is evaluated first
/// (so a computed function value like `(if b then f else g) x` behaves
/// as expected once the parser grows conditional expressions), then
/// each argument in source order under the *caller's* env (not the
/// closure's captured env — captures are frozen at close-over time,
/// and evaluating args in the captured env would give surprising
/// results for a closure that is passed around).
fn apply(
    head: &SyntaxNode,
    args: &[SyntaxNode],
    env: &HashMap<String, Value>,
) -> Result<Value, LambdaError> {
    let func_val = eval_lambda(head, env)?;
    let closure = match func_val {
        Value::Fn(c) => c,
        other => {
            return Err(LambdaError::TypeError(format!(
                "cannot apply non-function value: {other:?}"
            )));
        }
    };
    if closure.params.len() != args.len() {
        return Err(LambdaError::ArityMismatch {
            expected: closure.params.len(),
            actual: args.len(),
        });
    }
    let mut call_env = closure.captured.clone();
    for (param, arg_node) in closure.params.iter().zip(args.iter()) {
        let arg_val = eval_lambda(arg_node, env)?;
        call_env.insert(param.clone(), arg_val);
    }
    eval_lambda(&closure.body, &call_env)
}

/// Evaluate a binary operator over two already-reduced [`Value`]s.
///
/// Arithmetic and comparison arms are `Int × Int` only; `+` also
/// concatenates `Str × Str`. Anything else — mixing types, applying
/// `<` to strings, dividing by zero — surfaces as
/// [`LambdaError::TypeError`] with a rendered explanation. Kept in
/// its own function so a follow-on milestone that lifts these to a
/// span-carrying diagnostic has one place to touch.
fn eval_binop(op: &str, lhs: Value, rhs: Value) -> Result<Value, LambdaError> {
    match (op, &lhs, &rhs) {
        // Arithmetic (Int × Int → Int)
        ("+", Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_add(*b))),
        ("-", Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_sub(*b))),
        ("*", Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_mul(*b))),
        ("/", Value::Int(_a), Value::Int(0)) => {
            Err(LambdaError::TypeError("division by zero".to_owned()))
        }
        ("/", Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_div(*b))),

        // Comparisons (Int × Int → Bool)
        ("==", Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a == b)),
        ("!=", Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a != b)),
        ("<", Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
        ("<=", Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
        (">", Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
        (">=", Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),

        // Bool comparisons (Bool × Bool → Bool for eq/ne only)
        ("==", Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a == b)),
        ("!=", Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a != b)),

        // String concatenation
        ("+", Value::Str(a), Value::Str(b)) => Ok(Value::Str(format!("{a}{b}"))),

        // String equality
        ("==", Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a == b)),
        ("!=", Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a != b)),

        _ => Err(LambdaError::TypeError(format!(
            "binop `{op}` not supported on {lhs:?} and {rhs:?}"
        ))),
    }
}

/// Evaluate a unary operator over an already-reduced [`Value`].
///
/// Only `-` (Int negation, using `wrapping_neg` to keep `i64::MIN`
/// from panicking — the walker never traps) and `not` (Bool negation)
/// are supported. Any other combination is a [`LambdaError::TypeError`].
fn eval_unaryop(op: &str, inner: Value) -> Result<Value, LambdaError> {
    match (op, &inner) {
        ("-", Value::Int(n)) => Ok(Value::Int(n.wrapping_neg())),
        ("not", Value::Bool(b)) => Ok(Value::Bool(!b)),
        _ => Err(LambdaError::TypeError(format!(
            "unaryop `{op}` not supported on {inner:?}"
        ))),
    }
}

/// Short human name for a `SyntaxNode` variant. Used only by the
/// [`LambdaError::NotSupported`] payload so the message names the
/// shape without dumping the full subtree. Kept private (mirrors
/// `crate::turn::variant_name` and `crate::type_stage::variant_tag`
/// so a change to one does not silently propagate — the three callers
/// have subtly different rendering contracts).
fn variant_tag(n: &SyntaxNode) -> &'static str {
    match n {
        SyntaxNode::Cmd { .. } => "cmd",
        SyntaxNode::Pipe { .. } => "pipe",
        SyntaxNode::Seq { .. } => "seq",
        SyntaxNode::Redirect { .. } => "redirect",
        SyntaxNode::Background { .. } => "background",
        SyntaxNode::Group { .. } => "group",
        SyntaxNode::DatalogBlock { .. } => "datalog-block",
        SyntaxNode::Atom { .. } => "atom",
        SyntaxNode::Rule { .. } => "rule",
        SyntaxNode::QVar { .. } => "qvar",
        SyntaxNode::InterpVar { .. } => "interp-var",
        SyntaxNode::NotAtom { .. } => "not-atom",
        SyntaxNode::Lambda { .. } => "lambda",
        SyntaxNode::App { .. } => "app",
        SyntaxNode::Var { .. } => "var",
        SyntaxNode::Let { .. } => "let",
        SyntaxNode::Match { .. } => "match",
        SyntaxNode::BinOp { .. } => "binop",
        SyntaxNode::UnaryOp { .. } => "unaryop",
        SyntaxNode::FieldAccess { .. } => "field-access",
        SyntaxNode::RecordExpr { .. } => "record",
        SyntaxNode::LitStr { .. } => "lit-str",
        SyntaxNode::LitInt { .. } => "lit-int",
        SyntaxNode::LitBool { .. } => "lit-bool",
        SyntaxNode::Ident { .. } => "ident",
    }
}
