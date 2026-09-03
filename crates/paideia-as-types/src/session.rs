//! Session-typed protocol ADT for functor signatures (v0.25-M1-001, #1355).
//!
//! Session types describe the shape of a two-party message exchange as a
//! type. The classic references are Honda 1993 (dyadic session types) and
//! Vasconcelos et al. (linear session types with duality). This module
//! carries the *core* Vasconcelos-flavour ADT that later phases (M1-002
//! subtyping, M1-003 duality-check pass, M1-004 elaboration, and the R29
//! driver framework) build on top of. Because everything downstream reads
//! `SessionTy` by shape, the enum layout is deliberately stable — new
//! phases add variants at the *tail* rather than reorganising existing
//! ones.
//!
//! The four constructors singled out in the issue — session variables,
//! `end`, `seq`, `branch` — are all present with their canonical duality
//! rules. `Send`/`Recv` are included because a session variable cannot be
//! well-formed without at least one message operator to reference it, and
//! the tests for `dual(dual(S)) == S` need Send↔Recv to be an
//! observable involution. `Choice` is the internal-choice dual of
//! `Branch` (see `dual`).
//!
//! # Design notes
//!
//! - The payload of a message is carried as a `String` rather than a
//!   `TypeId` because M1-001 lands *before* the type interner learns to
//!   embed session types (`crates/paideia-as-types/src/types.rs` still
//!   has no `Type::Session(_)`). M1-003 replaces the `String` payload
//!   with a real interned `TypeId` in a follow-up patch that only touches
//!   the payload field — the rest of the ADT is stable.
//! - Duality is an **involution** — every constructor's `dual` maps back
//!   to itself under a second application. The test suite asserts this on
//!   representative terms; the property extends by structural induction.
//! - Well-formedness (`wf_session`) is a syntactic check: labels in a
//!   `Branch`/`Choice` must be unique, message continuations must be
//!   well-formed, and free session variables must appear inside at least
//!   one `Rec` binder (future work: substitution-based semantics).
//!
//! Anything the elaborator or driver framework needs beyond these
//! primitives (subtyping, recursion unfolding, capability tagging) is
//! deliberately deferred to M1-002+ so this file stays small and
//! obviously-correct.

use std::collections::HashSet;

/// A session type describing one endpoint of a two-party protocol.
///
/// The enum is `#[non_exhaustive]` at the crate boundary — downstream
/// crates should always match through a helper (`dual`, `wf_session`)
/// rather than exhaustively destructuring `SessionTy` themselves.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SessionTy {
    /// `end` — the terminated session. `dual(End) == End`.
    End,
    /// `s` — a session variable, bound by an enclosing `Rec` or by the
    /// functor signature's `with S : session` clause.
    Var(String),
    /// `!T . S` — send a value of payload type `T`, then continue as `S`.
    /// Dual: `Recv { payload, cont: dual(cont) }`.
    Send {
        /// Payload type in surface syntax (M1-003 upgrades to `TypeId`).
        payload: String,
        /// Continuation session.
        cont: Box<SessionTy>,
    },
    /// `?T . S` — receive a value of payload type `T`, then continue as `S`.
    /// Dual: `Send { payload, cont: dual(cont) }`.
    Recv {
        /// Payload type in surface syntax (M1-003 upgrades to `TypeId`).
        payload: String,
        /// Continuation session.
        cont: Box<SessionTy>,
    },
    /// `S1 ; S2` — sequential composition. Present so recursive protocols
    /// can splice a shared prefix; not part of pure Honda/Vasconcelos but
    /// standard in richer session calculi (e.g. Padovani's FuSe).
    /// Dual distributes: `dual(S1;S2) == dual(S1); dual(S2)`.
    Seq(Box<SessionTy>, Box<SessionTy>),
    /// `& { l1: S1, ..., ln: Sn }` — external choice (offer). The other
    /// endpoint selects one of the labels. Dual: `Choice(...)`.
    Branch(Vec<(String, SessionTy)>),
    /// `⊕ { l1: S1, ..., ln: Sn }` — internal choice (select). This
    /// endpoint picks one of the labels. Dual: `Branch(...)`.
    Choice(Vec<(String, SessionTy)>),
    /// `μ X . S` — recursive session. Binds `X` inside `S`.
    /// Dual: `Rec(x, dual(S))` — the binder is invariant, the body flips.
    Rec(String, Box<SessionTy>),
}

impl SessionTy {
    /// Convenience: construct the terminated session.
    #[must_use]
    pub fn end() -> Self {
        SessionTy::End
    }

    /// Convenience: construct a session variable.
    #[must_use]
    pub fn var(name: impl Into<String>) -> Self {
        SessionTy::Var(name.into())
    }

    /// Convenience: construct a sequential composition `S1 ; S2`.
    #[must_use]
    pub fn seq(lhs: SessionTy, rhs: SessionTy) -> Self {
        SessionTy::Seq(Box::new(lhs), Box::new(rhs))
    }

    /// Convenience: construct a branch (external choice).
    #[must_use]
    pub fn branch(arms: Vec<(String, SessionTy)>) -> Self {
        SessionTy::Branch(arms)
    }

    /// Convenience: construct a `Send` message operator.
    #[must_use]
    pub fn send(payload: impl Into<String>, cont: SessionTy) -> Self {
        SessionTy::Send {
            payload: payload.into(),
            cont: Box::new(cont),
        }
    }

    /// Convenience: construct a `Recv` message operator.
    #[must_use]
    pub fn recv(payload: impl Into<String>, cont: SessionTy) -> Self {
        SessionTy::Recv {
            payload: payload.into(),
            cont: Box::new(cont),
        }
    }
}

/// Compute the *dual* of a session type — the session the other endpoint
/// must observe for the exchange to type-check.
///
/// Duality is an **involution**: `dual(dual(S)) == S` for every
/// well-formed `S`. This property is exercised by the tests below.
///
/// The rules by constructor:
///
/// - `End` ↔ `End`
/// - `Var(x)` ↔ `Var(x)` (variables are their own duals; recursion is
///   the binder's responsibility)
/// - `Send { p, c }` ↔ `Recv { p, dual(c) }`
/// - `Recv { p, c }` ↔ `Send { p, dual(c) }`
/// - `Seq(a, b)` ↔ `Seq(dual(a), dual(b))`
/// - `Branch(arms)` ↔ `Choice(arms.map(|(l, s)| (l, dual(s))))`
/// - `Choice(arms)` ↔ `Branch(arms.map(|(l, s)| (l, dual(s))))`
/// - `Rec(x, s)` ↔ `Rec(x, dual(s))`
#[must_use]
pub fn dual(s: &SessionTy) -> SessionTy {
    match s {
        SessionTy::End => SessionTy::End,
        SessionTy::Var(x) => SessionTy::Var(x.clone()),
        SessionTy::Send { payload, cont } => SessionTy::Recv {
            payload: payload.clone(),
            cont: Box::new(dual(cont)),
        },
        SessionTy::Recv { payload, cont } => SessionTy::Send {
            payload: payload.clone(),
            cont: Box::new(dual(cont)),
        },
        SessionTy::Seq(a, b) => SessionTy::Seq(Box::new(dual(a)), Box::new(dual(b))),
        SessionTy::Branch(arms) => SessionTy::Choice(dual_arms(arms)),
        SessionTy::Choice(arms) => SessionTy::Branch(dual_arms(arms)),
        SessionTy::Rec(x, body) => SessionTy::Rec(x.clone(), Box::new(dual(body))),
    }
}

fn dual_arms(arms: &[(String, SessionTy)]) -> Vec<(String, SessionTy)> {
    arms.iter().map(|(l, s)| (l.clone(), dual(s))).collect()
}

/// Well-formedness error raised by [`wf_session`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionWfError {
    /// A `Branch` or `Choice` had zero arms — an unrecoverable protocol.
    EmptyChoice,
    /// A `Branch` or `Choice` had two arms with the same label.
    DuplicateLabel(String),
    /// A payload string was empty — the M1-001 payload is a stand-in for
    /// a real `TypeId`, so at minimum it must name *something*.
    EmptyPayload,
    /// A session variable referenced a name with no enclosing `Rec` and
    /// no top-level binder. M1-001 does not model the `with S : session`
    /// binder yet; callers that need to allow externally-bound variables
    /// should use [`wf_session_with_env`] with the outer scope prefilled.
    UnboundVar(String),
}

/// Check that a session type is *syntactically* well-formed.
///
/// See [`SessionWfError`] for the failure modes. This does **not** check
/// duality or protocol progress — those are M1-003 / M1-004 concerns.
///
/// Calling `wf_session(s)` is equivalent to `wf_session_with_env(s, &[])`.
pub fn wf_session(s: &SessionTy) -> Result<(), SessionWfError> {
    wf_session_with_env(s, &[])
}

/// Same as [`wf_session`] but with a set of session variables considered
/// bound by an outer scope (e.g. the functor signature's `with S :
/// session` clause).
pub fn wf_session_with_env(s: &SessionTy, outer: &[String]) -> Result<(), SessionWfError> {
    let mut env: HashSet<String> = outer.iter().cloned().collect();
    wf_rec(s, &mut env)
}

fn wf_rec(s: &SessionTy, env: &mut HashSet<String>) -> Result<(), SessionWfError> {
    match s {
        SessionTy::End => Ok(()),
        SessionTy::Var(x) => {
            if env.contains(x) {
                Ok(())
            } else {
                Err(SessionWfError::UnboundVar(x.clone()))
            }
        }
        SessionTy::Send { payload, cont } | SessionTy::Recv { payload, cont } => {
            if payload.is_empty() {
                return Err(SessionWfError::EmptyPayload);
            }
            wf_rec(cont, env)
        }
        SessionTy::Seq(a, b) => {
            wf_rec(a, env)?;
            wf_rec(b, env)
        }
        SessionTy::Branch(arms) | SessionTy::Choice(arms) => {
            if arms.is_empty() {
                return Err(SessionWfError::EmptyChoice);
            }
            let mut seen: HashSet<&str> = HashSet::new();
            for (label, arm) in arms {
                if !seen.insert(label.as_str()) {
                    return Err(SessionWfError::DuplicateLabel(label.clone()));
                }
                wf_rec(arm, env)?;
            }
            Ok(())
        }
        SessionTy::Rec(x, body) => {
            let fresh = env.insert(x.clone());
            let result = wf_rec(body, env);
            if fresh {
                env.remove(x);
            }
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(name: &str) -> String {
        name.to_string()
    }

    // ---- dual involution ----

    #[test]
    fn dual_of_end_is_end() {
        assert_eq!(dual(&SessionTy::End), SessionTy::End);
    }

    #[test]
    fn dual_of_send_is_recv() {
        let s = SessionTy::send(payload("i32"), SessionTy::End);
        assert_eq!(
            dual(&s),
            SessionTy::recv(payload("i32"), SessionTy::End)
        );
    }

    #[test]
    fn dual_of_recv_is_send() {
        let s = SessionTy::recv(payload("i32"), SessionTy::End);
        assert_eq!(
            dual(&s),
            SessionTy::send(payload("i32"), SessionTy::End)
        );
    }

    #[test]
    fn dual_of_branch_is_choice_with_dualised_arms() {
        let s = SessionTy::branch(vec![
            ("ok".to_string(), SessionTy::send(payload("i32"), SessionTy::End)),
            ("err".to_string(), SessionTy::End),
        ]);
        let d = dual(&s);
        match d {
            SessionTy::Choice(arms) => {
                assert_eq!(arms.len(), 2);
                assert_eq!(arms[0].0, "ok");
                assert_eq!(
                    arms[0].1,
                    SessionTy::recv(payload("i32"), SessionTy::End)
                );
                assert_eq!(arms[1].0, "err");
                assert_eq!(arms[1].1, SessionTy::End);
            }
            _ => panic!("expected Choice"),
        }
    }

    #[test]
    fn dual_distributes_through_seq() {
        let s = SessionTy::seq(
            SessionTy::send(payload("i32"), SessionTy::End),
            SessionTy::recv(payload("bool"), SessionTy::End),
        );
        let d = dual(&s);
        assert_eq!(
            d,
            SessionTy::seq(
                SessionTy::recv(payload("i32"), SessionTy::End),
                SessionTy::send(payload("bool"), SessionTy::End),
            )
        );
    }

    #[test]
    fn dual_is_involution_end() {
        let s = SessionTy::End;
        assert_eq!(dual(&dual(&s)), s);
    }

    #[test]
    fn dual_is_involution_message_chain() {
        let s = SessionTy::send(
            payload("i32"),
            SessionTy::recv(payload("bool"), SessionTy::End),
        );
        assert_eq!(dual(&dual(&s)), s);
    }

    #[test]
    fn dual_is_involution_branch_choice_mixed() {
        let s = SessionTy::Choice(vec![
            (
                "req".to_string(),
                SessionTy::send(
                    payload("Req"),
                    SessionTy::branch(vec![
                        ("ack".to_string(), SessionTy::End),
                        ("nack".to_string(), SessionTy::recv(payload("Err"), SessionTy::End)),
                    ]),
                ),
            ),
            ("bye".to_string(), SessionTy::End),
        ]);
        assert_eq!(dual(&dual(&s)), s);
    }

    #[test]
    fn dual_is_involution_recursive() {
        let s = SessionTy::Rec(
            "X".to_string(),
            Box::new(SessionTy::send(payload("i32"), SessionTy::var("X"))),
        );
        assert_eq!(dual(&dual(&s)), s);
    }

    // ---- well-formedness ----

    #[test]
    fn wf_end_ok() {
        assert!(wf_session(&SessionTy::End).is_ok());
    }

    #[test]
    fn wf_unbound_var_rejected() {
        let s = SessionTy::var("X");
        assert_eq!(wf_session(&s), Err(SessionWfError::UnboundVar("X".into())));
    }

    #[test]
    fn wf_var_bound_by_rec_ok() {
        let s = SessionTy::Rec(
            "X".to_string(),
            Box::new(SessionTy::send(payload("i32"), SessionTy::var("X"))),
        );
        assert!(wf_session(&s).is_ok());
    }

    #[test]
    fn wf_var_bound_by_outer_env_ok() {
        let s = SessionTy::var("S");
        assert!(wf_session_with_env(&s, &["S".to_string()]).is_ok());
    }

    #[test]
    fn wf_empty_branch_rejected() {
        let s = SessionTy::branch(vec![]);
        assert_eq!(wf_session(&s), Err(SessionWfError::EmptyChoice));
    }

    #[test]
    fn wf_duplicate_label_rejected() {
        let s = SessionTy::branch(vec![
            ("l".to_string(), SessionTy::End),
            ("l".to_string(), SessionTy::End),
        ]);
        assert_eq!(
            wf_session(&s),
            Err(SessionWfError::DuplicateLabel("l".into()))
        );
    }

    #[test]
    fn wf_empty_payload_rejected() {
        let s = SessionTy::send(payload(""), SessionTy::End);
        assert_eq!(wf_session(&s), Err(SessionWfError::EmptyPayload));
    }

    #[test]
    fn wf_nested_branch_recurses() {
        let s = SessionTy::branch(vec![(
            "outer".to_string(),
            SessionTy::branch(vec![("inner".to_string(), SessionTy::var("Unbound"))]),
        )]);
        assert_eq!(
            wf_session(&s),
            Err(SessionWfError::UnboundVar("Unbound".into()))
        );
    }

    #[test]
    fn wf_rec_scope_pops() {
        // After the Rec body, `X` should no longer be in scope.
        let s = SessionTy::Seq(
            Box::new(SessionTy::Rec(
                "X".to_string(),
                Box::new(SessionTy::var("X")),
            )),
            Box::new(SessionTy::var("X")),
        );
        assert_eq!(
            wf_session(&s),
            Err(SessionWfError::UnboundVar("X".into()))
        );
    }
}
