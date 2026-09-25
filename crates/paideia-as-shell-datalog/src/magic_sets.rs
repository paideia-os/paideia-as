//! R226.M3 magic-set rewriting — **stub for M1/M2 landing**.
//!
//! Landed as an empty module (rather than a nonexistent one) so that
//! R226.M3's landing is API-additive: downstream crates can already
//! import `paideia_as_shell_datalog::magic_sets::rewrite` in their
//! type signatures and switch over the moment the real implementation
//! lands.
//!
//! # What R226.M3 will bring
//!
//! Beeri & Ramakrishnan 1991: given a `Program` and a `Query` that
//! binds some argument positions of a goal, rewrite the program so
//! only the tuples relevant to that goal are computed. The seminaïve
//! evaluator then runs against the rewritten program and delivers a
//! 10x–100x speedup on goal-directed queries against a 10⁶-node FS
//! graph — the R5 risk mitigation the plan `semantic-shell-
//! language-plan.md` §Risks calls out.
//!
//! # Signature stub
//!
//! Not `pub fn rewrite(...)` yet — an unimplemented stub would be a
//! compile-time trap for a caller that upgrades on the M3 API. The
//! module is intentionally empty: any code that names it today is a
//! deliberate placeholder, and the M3 landing populates it with the
//! real `pub fn rewrite(program: &Program, query: &Query) -> Program`.
