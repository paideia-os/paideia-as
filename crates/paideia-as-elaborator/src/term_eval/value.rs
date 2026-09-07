//! Runtime value + environment for the typed-term evaluator.
//!
//! Split out from the single-file `term_eval.rs` in the phase-2 God-file
//! refactor (issue #1412 under umbrella #1405). Contains:
//! - Fuel + stack-depth defaults for macro evaluation.
//! - The `Value` enum — the small closed universe of values macro bodies
//!   produce.
//! - The `Env` structure — per-call binding table plus fuel + depth
//!   accounting.
//! - `EvalResult`, the shared `Result<Value, Diagnostic>` alias.

use std::collections::HashMap;

use paideia_as_ast::reflect::TermHead;
use paideia_as_ast::Term;
use paideia_as_diagnostics::Diagnostic;

/// Default fuel budget for evaluation: enough for complex macro bodies.
pub const DEFAULT_FUEL: u64 = 65_536;

/// Default maximum stack depth for evaluation.
pub const DEFAULT_STACK_DEPTH: u32 = 256;

/// Runtime value produced by evaluating a macro body.
///
/// Phase-2-m5 minimum: integer, bool, term-head, term, list of values.
/// More variants (Closure, Record, ...) arrive as macro bodies need them.
#[derive(Clone, Debug)]
pub enum Value<'a> {
    /// 64-bit signed integer.
    Int(i64),
    /// Boolean value.
    Bool(bool),
    /// A term head discriminant (Lambda, Literal, Quote, etc.).
    Head(TermHead),
    /// An AST term handle.
    Term(Term<'a>),
    /// A list (vector) of values.
    List(Vec<Value<'a>>),
    /// Unit value — placeholder for evaluator errors.
    Unit,
}

impl<'a> PartialEq for Value<'a> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Head(a), Value::Head(b)) => a == b,
            (Value::Term(a), Value::Term(b)) => a.id() == b.id(),
            (Value::List(a), Value::List(b)) => a == b,
            (Value::Unit, Value::Unit) => true,
            _ => false,
        }
    }
}

impl<'a> Value<'a> {
    /// Convert a value to a string for diagnostic messages.
    pub(super) fn display(&self) -> String {
        match self {
            Value::Int(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Head(h) => format!("{:?}", h),
            Value::Term(_) => "<term>".to_string(),
            Value::List(vals) => {
                let items: Vec<String> = vals.iter().map(|v| v.display()).collect();
                format!("[{}]", items.join(", "))
            }
            Value::Unit => "()".to_string(),
        }
    }
}

/// Result of evaluating one expression node.
pub type EvalResult<'a> = Result<Value<'a>, Diagnostic>;

/// Per-call environment binding names to values.
///
/// Also tracks fuel (remaining evaluation steps) and stack depth (current recursion depth)
/// to detect infinite loops and unbounded recursion in reflective macro bodies.
#[derive(Clone, Debug)]
pub struct Env<'a> {
    scope: HashMap<String, Value<'a>>,
    /// Remaining fuel — decremented on every eval step. Hitting 0 emits M0311.
    pub fuel: u64,
    /// Current evaluator stack depth. Incremented on each recursive eval entry; decremented on return.
    pub depth: u32,
    /// Cap on `depth`. Default 256.
    pub max_depth: u32,
}

impl<'a> Env<'a> {
    /// Construct a new empty environment with default fuel and depth limits.
    #[must_use]
    pub fn new() -> Self {
        Self::with_limits(DEFAULT_FUEL, DEFAULT_STACK_DEPTH)
    }

    /// Construct a new environment with custom fuel and depth limits.
    #[must_use]
    pub fn with_limits(fuel: u64, max_depth: u32) -> Self {
        Self {
            scope: HashMap::new(),
            fuel,
            depth: 0,
            max_depth,
        }
    }

    /// Bind a name to a value in the environment.
    pub fn bind(&mut self, name: impl Into<String>, value: Value<'a>) {
        self.scope.insert(name.into(), value);
    }

    /// Look up a name in the environment.
    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<Value<'a>> {
        self.scope.get(name).cloned()
    }
}

impl<'a> Default for Env<'a> {
    fn default() -> Self {
        Self::new()
    }
}
