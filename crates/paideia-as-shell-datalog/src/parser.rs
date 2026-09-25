//! R226.M1 Datalog parser: `&[Token]` → `Program` / `Query`.
//!
//! Consumes the `Context::Datalog` slice of a shell-lex token stream
//! (caller strips the `datalog { … }` wrapper — the parser itself
//! never sees `DatalogKw` / outermost `LBrace` / matching `RBrace`).
//!
//! # Grammar (M1 subset)
//!
//! ```text
//!   program   := ( clause )*
//!   clause    := atom '.'                       // fact
//!             |  atom '=>' body '.'             // rule
//!   body      := atom ( ',' atom )*
//!   atom      := IDENT '(' term ( ',' term )* ')'
//!   term      := QVAR                           // ?X       -> Term::Var
//!             |  INTERP                         // $x       -> Term::Bound
//!             |  IDENT                          // alice    -> Term::Const(Ident)
//!             |  STRING                         // "text"   -> Term::Const(Str)
//!             |  NUMBER                         // 42       -> Term::Const(Num)
//!
//!   query     := atom ( ',' atom )*             // no trailing '.'
//! ```
//!
//! `Newline` and `Semi` tokens are treated as whitespace at M1 (facts
//! and rules terminate on `.`). This matches the lexer's fixture-05
//! behaviour, which emits `Newline` inside the Datalog block for
//! multiline sources but expects the grammar to ignore them.
//!
//! # Error recovery
//!
//! On the first hard syntax error the parser returns
//! `Err(ParseError)` carrying the offending span. There is no
//! per-clause recovery — the caller (a REPL) is expected to re-run
//! parsing after the user fixes the block. R226.M9 will bring
//! error-recovery machinery when it wires the schema-registry type
//! diagnostics into the loop.

use crate::ast::{Aggregate, AggregateQuery, Atom, BodyGoal, Program, Query, Rule, Term, Value};
use paideia_as_shell_lex::{Span, Token, TokenKind};

/// Discriminated parse-failure modes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// The token stream ran out mid-clause. Carries the last-seen
    /// span for diagnostics.
    UnexpectedEof,
    /// A token appeared where the grammar demanded a different shape.
    /// `expected` is a short human phrase (e.g. "predicate name",
    /// "opening `(`", "`.` to terminate the clause") — kept as a
    /// `&'static str` because these are grammar-fixed and never
    /// contain user input.
    Expected {
        /// Short human description of what was expected.
        expected: &'static str,
        /// The kind that was found instead, shown in the error message.
        found: String,
    },
    /// A fact (head with no body) contained a logic variable, so it
    /// cannot serve as an EDB seed. E.g. `parent(?X, bob).` is not a
    /// valid fact. R226.M8 (assert/retract) will keep this rule; a
    /// non-ground assertion has no meaning without a body.
    NonGroundFact {
        /// The atom that failed the ground check.
        atom: String,
    },
    /// An atom carried zero arguments (`foo()`). At M1 the schema
    /// registry does not yet arbitrate zero-arity predicates; refuse
    /// them to avoid ambiguous fixpoint semantics until R226.M2 (in
    /// the published plan) attaches types.
    ZeroArityAtom {
        /// The bare predicate name that arrived with `()`.
        predicate: String,
    },
}

/// Parse-time diagnostic carrying the offending source span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// Byte range in the original source (from the lexer).
    pub span: Span,
    /// Specific failure mode.
    pub kind: ParseErrorKind,
}

/// Parse a Datalog block's contents into a `Program`.
///
/// The caller must supply only the tokens *inside* the `datalog { … }`
/// wrapper (or the tokens of a stand-alone `.dl` script). Passing the
/// wrapper tokens through would trip the "predicate name expected"
/// error on the first `LBrace`.
pub fn parse_block(tokens: &[Token]) -> Result<Program, ParseError> {
    let mut p = Parser::new(tokens);
    let mut program = Program::empty();
    p.skip_whitespace();
    while !p.at_end() {
        let clause = p.parse_clause()?;
        match clause {
            ClauseParse::Fact(atom) => {
                if !atom.is_ground() {
                    return Err(ParseError {
                        span: p.last_span(),
                        kind: ParseErrorKind::NonGroundFact {
                            atom: atom.to_string(),
                        },
                    });
                }
                program.facts.push(atom);
            }
            ClauseParse::Rule(rule) => program.rules.push(rule),
        }
        p.skip_whitespace();
    }
    Ok(program)
}

/// Parse a stand-alone R226.M6 aggregation query.
///
/// # Grammar
///
/// ```text
///   aggregate_query := agg_expr [ group_by_clause ] where_clause
///   agg_expr        := AGG_IDENT '(' QVAR ')'
///   AGG_IDENT       ∈ { count, sum, min, max, avg }
///   group_by_clause := 'group' 'by' QVAR ( ',' QVAR )*
///   where_clause    := 'where' body_goal ( ',' body_goal )*
///   body_goal       := atom                              // positive
///                   |  'not' atom                        // negative
/// ```
///
/// `group`, `by`, `where` arrive from the lexer as
/// [`TokenKind::Ident`] (only `and` / `or` / `not` are folded to
/// [`TokenKind::Op`]); the parser matches on the identifier text.
///
/// # Body separator
///
/// The lexer has no `:` token, so we spell the body separator as
/// `where` — SQL-familiar and unambiguous with Datalog's `=>` rule
/// separator (which stays reserved for rule definitions inside a
/// `datalog { … }` block).
///
/// # Examples
///
/// ```text
///   count(?x) where person(?x)
///   count(?c) group by ?p where child(?p, ?c)
///   sum(?amount) group by ?dept, ?quarter where sale(?dept, ?quarter, ?amount)
///   avg(?a) where val(?x, ?a), not excluded(?x)
/// ```
///
/// # Errors
///
/// Every failure mode routes through [`ParseError`] with a
/// [`ParseErrorKind::Expected`] payload naming what the grammar
/// wanted — a misspelled aggregate name, a missing `where`, an
/// unexpected token in the body — each carries the offending span
/// for editor overlays.
pub fn parse_aggregate_query(tokens: &[Token]) -> Result<AggregateQuery, ParseError> {
    let mut p = Parser::new(tokens);
    p.skip_whitespace();

    // Aggregate name — must be one of the five reserved identifiers.
    let agg = match p.advance() {
        Some(tok) => match &tok.kind {
            TokenKind::Ident(name) => match name.as_str() {
                "count" => Aggregate::Count,
                "sum" => Aggregate::Sum,
                "min" => Aggregate::Min,
                "max" => Aggregate::Max,
                "avg" => Aggregate::Avg,
                other => {
                    return Err(ParseError {
                        span: tok.span,
                        kind: ParseErrorKind::Expected {
                            expected: "aggregate name (count, sum, min, max, avg)",
                            found: format!("Ident({other:?})"),
                        },
                    });
                }
            },
            _ => {
                return Err(ParseError {
                    span: tok.span,
                    kind: ParseErrorKind::Expected {
                        expected: "aggregate name (count, sum, min, max, avg)",
                        found: format!("{:?}", tok.kind),
                    },
                });
            }
        },
        None => {
            return Err(ParseError {
                span: p.last_span(),
                kind: ParseErrorKind::UnexpectedEof,
            });
        }
    };

    // '(' target_var ')'
    match p.peek_kind() {
        Some(TokenKind::LParen) => {
            p.advance();
        }
        _ => return Err(p.expected("`(` after aggregate name")),
    }
    let target_var = match p.advance() {
        Some(tok) => match &tok.kind {
            TokenKind::QVar(name) => name.clone(),
            _ => {
                return Err(ParseError {
                    span: tok.span,
                    kind: ParseErrorKind::Expected {
                        expected: "logic variable (?var) as aggregate target",
                        found: format!("{:?}", tok.kind),
                    },
                });
            }
        },
        None => {
            return Err(ParseError {
                span: p.last_span(),
                kind: ParseErrorKind::UnexpectedEof,
            });
        }
    };
    match p.peek_kind() {
        Some(TokenKind::RParen) => {
            p.advance();
        }
        _ => return Err(p.expected("`)` to close aggregate target")),
    }

    // Optional `group by ?v (, ?v)*`
    p.skip_whitespace();
    let mut group_by: Vec<String> = Vec::new();
    if matches!(
        p.peek_kind(),
        Some(TokenKind::Ident(s)) if s == "group"
    ) {
        p.advance();
        p.skip_whitespace();
        match p.peek_kind() {
            Some(TokenKind::Ident(s)) if s == "by" => {
                p.advance();
            }
            _ => return Err(p.expected("`by` after `group`")),
        }
        p.skip_whitespace();
        // First group-by var.
        match p.advance() {
            Some(tok) => match &tok.kind {
                TokenKind::QVar(name) => group_by.push(name.clone()),
                _ => {
                    return Err(ParseError {
                        span: tok.span,
                        kind: ParseErrorKind::Expected {
                            expected: "logic variable (?var) after `group by`",
                            found: format!("{:?}", tok.kind),
                        },
                    });
                }
            },
            None => {
                return Err(ParseError {
                    span: p.last_span(),
                    kind: ParseErrorKind::UnexpectedEof,
                });
            }
        }
        // Additional `, ?var` pairs.
        loop {
            p.skip_whitespace();
            match p.peek_kind() {
                Some(TokenKind::Comma) => {
                    p.advance();
                    p.skip_whitespace();
                    match p.advance() {
                        Some(tok) => match &tok.kind {
                            TokenKind::QVar(name) => group_by.push(name.clone()),
                            _ => {
                                return Err(ParseError {
                                    span: tok.span,
                                    kind: ParseErrorKind::Expected {
                                        expected: "logic variable (?var) after `,` in group-by list",
                                        found: format!("{:?}", tok.kind),
                                    },
                                });
                            }
                        },
                        None => {
                            return Err(ParseError {
                                span: p.last_span(),
                                kind: ParseErrorKind::UnexpectedEof,
                            });
                        }
                    }
                }
                _ => break,
            }
        }
    }

    // Required `where` body_goal (',' body_goal)*
    p.skip_whitespace();
    match p.peek_kind() {
        Some(TokenKind::Ident(s)) if s == "where" => {
            p.advance();
        }
        _ => return Err(p.expected("`where` to introduce aggregate body")),
    }
    p.skip_whitespace();
    let mut goals: Vec<BodyGoal> = Vec::new();
    goals.push(p.parse_body_goal()?);
    loop {
        p.skip_whitespace();
        if p.at_end() {
            break;
        }
        match p.peek_kind() {
            Some(TokenKind::Comma) => {
                p.advance();
                p.skip_whitespace();
                goals.push(p.parse_body_goal()?);
            }
            Some(TokenKind::Dot) => {
                // Tolerate a trailing `.` as `parse_query` does — a
                // REPL user pasting a rule-shaped tail should not be
                // surprised.
                p.advance();
                p.skip_whitespace();
                if !p.at_end() {
                    return Err(p.expected("end of aggregate query after `.`"));
                }
                break;
            }
            _ => return Err(p.expected("`,` between body goals or end of aggregate query")),
        }
    }

    Ok(AggregateQuery {
        agg,
        target_var,
        goals,
        group_by,
    })
}

/// Parse a stand-alone query — comma-separated goal atoms, no trailing
/// dot. `alice`/`bob` and other identifiers are `Term::Const(Ident)`;
/// `?X` is `Term::Var`.
pub fn parse_query(tokens: &[Token]) -> Result<Query, ParseError> {
    let mut p = Parser::new(tokens);
    p.skip_whitespace();
    let mut goals = Vec::new();
    goals.push(p.parse_atom()?);
    loop {
        p.skip_whitespace();
        if p.at_end() {
            break;
        }
        match p.peek_kind() {
            Some(TokenKind::Comma) => {
                p.advance();
                p.skip_whitespace();
                goals.push(p.parse_atom()?);
            }
            Some(TokenKind::Dot) => {
                // Tolerate a trailing `.` on queries: some REPL users
                // will paste a rule-shaped input, and refusing the dot
                // would be surprising. Silently drop it.
                p.advance();
                p.skip_whitespace();
                if !p.at_end() {
                    return Err(p.expected("end of query after `.`"));
                }
                break;
            }
            _ => return Err(p.expected("`,` between goals or end of query")),
        }
    }
    Ok(Query { goals })
}

// --------------------------------------------------------------------
// Internal parser state
// --------------------------------------------------------------------

/// One parsed clause: either a fact (no body) or a rule (head + body).
enum ClauseParse {
    Fact(Atom),
    Rule(Rule),
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, pos: 0 }
    }

    #[inline]
    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    #[inline]
    fn peek_kind(&self) -> Option<&TokenKind> {
        self.tokens.get(self.pos).map(|t| &t.kind)
    }

    #[inline]
    fn advance(&mut self) -> Option<&Token> {
        let t = self.tokens.get(self.pos);
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    /// Span of the most-recently consumed token, or of the token at the
    /// current cursor if nothing has been consumed yet. Used to attach
    /// a source location to an error.
    fn last_span(&self) -> Span {
        if self.pos == 0 {
            self.tokens
                .first()
                .map(|t| t.span)
                .unwrap_or(Span::new(0, 0))
        } else {
            self.tokens
                .get(self.pos - 1)
                .map(|t| t.span)
                .unwrap_or(Span::new(0, 0))
        }
    }

    /// Skip `Newline` and `Semi` tokens — both are pure whitespace at
    /// the M1 grammar level (see module doc).
    fn skip_whitespace(&mut self) {
        while let Some(k) = self.peek_kind() {
            match k {
                TokenKind::Newline | TokenKind::Semi => {
                    self.pos += 1;
                }
                _ => break,
            }
        }
    }

    /// Convenience: fabricate an `Expected{…}` error at the current
    /// cursor, describing what the grammar wanted.
    fn expected(&self, expected: &'static str) -> ParseError {
        let (span, found) = match self.tokens.get(self.pos) {
            Some(t) => (t.span, format!("{:?}", t.kind)),
            None => (self.last_span(), "end of input".to_owned()),
        };
        ParseError {
            span,
            kind: ParseErrorKind::Expected { expected, found },
        }
    }

    fn parse_clause(&mut self) -> Result<ClauseParse, ParseError> {
        let head = self.parse_atom()?;
        self.skip_whitespace();
        match self.peek_kind() {
            Some(TokenKind::Dot) => {
                self.advance();
                Ok(ClauseParse::Fact(head))
            }
            Some(TokenKind::FatArrow) => {
                self.advance();
                self.skip_whitespace();
                let mut body = Vec::new();
                body.push(self.parse_body_goal()?);
                loop {
                    self.skip_whitespace();
                    match self.peek_kind() {
                        Some(TokenKind::Comma) => {
                            self.advance();
                            self.skip_whitespace();
                            body.push(self.parse_body_goal()?);
                        }
                        Some(TokenKind::Dot) => {
                            self.advance();
                            break;
                        }
                        _ => {
                            return Err(
                                self.expected("`,` between body atoms or `.` to terminate rule")
                            )
                        }
                    }
                }
                Ok(ClauseParse::Rule(Rule { head, body }))
            }
            _ => Err(self.expected("`.` (fact) or `=>` (rule)")),
        }
    }

    /// Parse a single body conjunct: either a plain atom (positive
    /// goal) or `not <atom>` (negative goal). The `not` keyword arrives
    /// from the lexer as `TokenKind::Op("not")` (see the shell-lex
    /// `finish_ident` classifier — keyword-operators `and`/`or`/`not`
    /// all fold to `Op`). We tolerate whitespace between `not` and the
    /// atom so multi-line rule bodies stay readable.
    ///
    /// R226.M5 grammar addition; a lone `not` followed by anything
    /// other than a predicate name is a parse error (same as an atom
    /// missing its opening `(`). Repeated `not not p(...)` is refused
    /// deliberately — double negation elimination is a semantic
    /// transformation, not a lexical one, and no shell corpus needs it.
    fn parse_body_goal(&mut self) -> Result<BodyGoal, ParseError> {
        self.skip_whitespace();
        let negated = matches!(
            self.peek_kind(),
            Some(TokenKind::Op(s)) if s.as_str() == "not"
        );
        if negated {
            self.advance();
            self.skip_whitespace();
            // Refuse double negation: `not not p(...)` — see docstring.
            if matches!(
                self.peek_kind(),
                Some(TokenKind::Op(s)) if s.as_str() == "not"
            ) {
                return Err(self.expected("predicate name after `not` (double negation disallowed)"));
            }
        }
        let atom = self.parse_atom()?;
        Ok(if negated {
            BodyGoal::Negative(atom)
        } else {
            BodyGoal::Positive(atom)
        })
    }

    fn parse_atom(&mut self) -> Result<Atom, ParseError> {
        let name = match self.advance() {
            Some(t) => match &t.kind {
                TokenKind::Ident(s) => s.clone(),
                _ => {
                    return Err(ParseError {
                        span: t.span,
                        kind: ParseErrorKind::Expected {
                            expected: "predicate name (identifier)",
                            found: format!("{:?}", t.kind),
                        },
                    })
                }
            },
            None => {
                return Err(ParseError {
                    span: self.last_span(),
                    kind: ParseErrorKind::UnexpectedEof,
                })
            }
        };
        // Opening `(`
        match self.peek_kind() {
            Some(TokenKind::LParen) => {
                self.advance();
            }
            _ => return Err(self.expected("`(` after predicate name")),
        }
        // Term list — at least one.
        let mut terms = Vec::new();
        // Guard against `foo()` — see ZeroArityAtom docs.
        match self.peek_kind() {
            Some(TokenKind::RParen) => {
                let span = self.last_span();
                self.advance();
                return Err(ParseError {
                    span,
                    kind: ParseErrorKind::ZeroArityAtom { predicate: name },
                });
            }
            _ => {}
        }
        terms.push(self.parse_term()?);
        loop {
            match self.peek_kind() {
                Some(TokenKind::Comma) => {
                    self.advance();
                    terms.push(self.parse_term()?);
                }
                Some(TokenKind::RParen) => {
                    self.advance();
                    break;
                }
                _ => return Err(self.expected("`,` between terms or `)` to close atom")),
            }
        }
        Ok(Atom::new(name, terms))
    }

    fn parse_term(&mut self) -> Result<Term, ParseError> {
        let tok = match self.advance() {
            Some(t) => t,
            None => {
                return Err(ParseError {
                    span: self.last_span(),
                    kind: ParseErrorKind::UnexpectedEof,
                })
            }
        };
        match &tok.kind {
            TokenKind::QVar(name) => Ok(Term::Var(name.clone())),
            TokenKind::InterpVar(name) => Ok(Term::Bound(name.clone())),
            TokenKind::Ident(name) => Ok(Term::Const(Value::Ident(name.clone()))),
            TokenKind::Str(s) => Ok(Term::Const(Value::Str(s.clone()))),
            TokenKind::Number(raw) => {
                // R226.M6-followup (issue #1460): floats are folded
                // in. The lexer already emits `3.14` as one `Number`
                // token (it consumes an optional `.digit+` fractional
                // part, see `paideia_as_shell_lex::lexer::consume_number`),
                // so the split here is purely on presence of `.`:
                //   * has `.` → parse as `f64` → `Value::Float`.
                //   * otherwise → parse as `i64` → `Value::Num`.
                // A suffix or `_` separator still falls through to the
                // Err arm — the underscore support and typed suffixes
                // are R226.M9 territory once the schema registry
                // pins numeric column types.
                if raw.contains('.') {
                    match raw.parse::<f64>() {
                        Ok(x) => Ok(Term::Const(Value::Float(x))),
                        Err(_) => Err(ParseError {
                            span: tok.span,
                            kind: ParseErrorKind::Expected {
                                expected: "float literal",
                                found: format!("Number({raw:?})"),
                            },
                        }),
                    }
                } else {
                    match raw.parse::<i64>() {
                        Ok(n) => Ok(Term::Const(Value::Num(n))),
                        Err(_) => Err(ParseError {
                            span: tok.span,
                            kind: ParseErrorKind::Expected {
                                expected: "integer literal",
                                found: format!("Number({raw:?})"),
                            },
                        }),
                    }
                }
            }
            _ => Err(ParseError {
                span: tok.span,
                kind: ParseErrorKind::Expected {
                    expected: "term (?var, $bound, identifier, string, or number)",
                    found: format!("{:?}", tok.kind),
                },
            }),
        }
    }
}
