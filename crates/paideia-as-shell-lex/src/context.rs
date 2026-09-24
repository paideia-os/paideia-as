//! Context enum + stack for the R221.M4 shell lexer.
//!
//! Kept in its own module because R221.M5's unified AST needs the
//! `Context` type as a field on every `SyntaxNode` (so nodes remember
//! which sub-language grammar they parsed under) and importing the
//! whole lexer just to name the type would create a spurious
//! parser-→-lexer coupling.

/// The three lexical contexts SH-D1 enumerates. See
/// `design/terminal/semantic-shell.md` §2.1 for the source-of-truth
/// table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Context {
    /// Top-level shell context. Stages separated by `|`; arguments
    /// space-separated; values are typed records. Newline or `;` ends a
    /// pipeline expression.
    Pipeline,
    /// Datalog block, entered via `datalog { … }`. Comma-separated
    /// atoms; `?var` is a logic variable; `$expr` interpolates a
    /// pipeline value; `.` terminates a fact; `=>` separates head from
    /// body in a rule.
    Datalog,
    /// Lambda body, entered via `{ |args| body }` or `\args -> body`.
    /// HM-typed expression; can call pipeline commands, embed Datalog,
    /// do arithmetic.
    Lambda,
}

/// LIFO stack of open contexts. Never empty by construction: the
/// bottom is always [`Context::Pipeline`], so the current context is
/// always well-defined even at end-of-input.
///
/// Kept as a `Vec` (not a fixed-capacity `SmallVec`) because the R229
/// REPL genuinely admits arbitrary nesting — a user script may build a
/// lambda that inspects a Datalog result that filters a pipeline —
/// and the extra allocation cost is negligible next to the tokenizer's
/// own work.
#[derive(Clone, Debug)]
pub struct ContextStack {
    inner: Vec<Context>,
}

impl ContextStack {
    /// A fresh stack sitting at top-level `Pipeline`.
    pub fn new() -> Self {
        Self {
            inner: vec![Context::Pipeline],
        }
    }

    /// The currently-active context. Never returns `None` by
    /// construction (the bottom is always `Pipeline`).
    #[inline]
    pub fn current(&self) -> Context {
        *self.inner.last().expect("stack invariant: never empty")
    }

    /// Push a new context. Called when the lexer emits an opening
    /// `{` that starts a `datalog { … }` or a `{ |args| body }` block.
    #[inline]
    pub fn push(&mut self, ctx: Context) {
        self.inner.push(ctx);
    }

    /// Pop the top context. Returns the popped `Context` on success,
    /// or `None` if the stack was already at its bottom (`Pipeline`);
    /// the caller then emits an `UnmatchedRBrace` diagnostic and keeps
    /// tokenizing — an unmatched `}` at the top level does not corrupt
    /// the state machine.
    #[inline]
    pub fn pop(&mut self) -> Option<Context> {
        if self.inner.len() <= 1 {
            None
        } else {
            self.inner.pop()
        }
    }

    /// Current depth (1 at rest — the base `Pipeline`; 2 immediately
    /// inside a top-level `datalog { … }` or `{ |a| … }`; etc.). Used
    /// by the mixed-fixture tests to assert 1..=3 nesting.
    #[inline]
    pub fn depth(&self) -> usize {
        self.inner.len()
    }
}

impl Default for ContextStack {
    fn default() -> Self {
        Self::new()
    }
}
