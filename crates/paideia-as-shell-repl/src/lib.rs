//! paideia-as-shell-repl — R229.M1 REPL turn pipeline skeleton.
//!
//! # In one paragraph
//!
//! One REPL "turn" is a single string of source the user typed
//! (a pipeline, a `datalog { … }` block, a lambda, whatever). This
//! crate owns the four-stage pipeline that a turn walks — parse,
//! typecheck, elaborate, execute — plus the `ReplState` (turn counter
//! + persistent [`SessionEdb`](paideia_as_shell_datalog::SessionEdb))
//! that outlives the individual turn.
//!
//! R229.M1 lands the skeleton only: parse is real (calls into
//! `paideia-as-shell-ast`), typecheck and elaborate are identity
//! stubs, and execute dispatches by `SyntaxNode` variant — with only
//! the Datalog branch actually running (through `Evaluator`), the
//! others returning stub `TurnResult::Value` strings. Subsequent
//! milestones (M2 HM wiring, M3 command dispatch, M4 lambda JIT)
//! grow each stage without moving the outer pipeline or the
//! [`ReplState`] shape.
//!
//! # Stages
//!
//! ```text
//!   source ── parse ──▶ SyntaxNode ── typecheck (stub) ──▶
//!             elaborate (stub) ──▶ execute (variant dispatch) ──▶
//!             TurnResult
//! ```
//!
//! # State
//!
//! [`ReplState`] holds the turn counter (monotone across the session)
//! and the [`SessionEdb`](paideia_as_shell_datalog::SessionEdb)
//! (assertions accumulated by prior turns; the Datalog evaluator
//! reads it as an overlay on top of any block-local facts). A driver
//! constructs one `ReplState` per session and threads a `&mut` of it
//! through every `eval_turn` call.
//!
//! # Fingerprints
//!
//! Every [`ReplTurn`] carries a `fingerprint` of the form
//! `repl.turn.{id:016x}` where `id` is the *pre-increment* turn
//! counter — so the first turn is `repl.turn.0000000000000000`, the
//! second `repl.turn.0000000000000001`, and so on. Test fixtures
//! (`tests/turn_pipeline.rs`, tagged `r229m1-turn-01`..`10`) pin the
//! format and monotonicity so R229.M7's replay harness can align
//! recorded transcripts to a new session's turns without re-parsing.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod cmd_dispatch;
pub mod lambda_eval;
pub mod lower;
pub mod pipeline;
pub mod turn;
pub mod type_stage;

pub use cmd_dispatch::{execute_cmd, CmdDispatchRegistry, CmdError};
pub use lambda_eval::{eval_lambda, Closure, LambdaError, Value};
pub use lower::{lower_datalog, LowerError};
pub use pipeline::{execute_pipeline, PipelineResult};
pub use turn::{eval_turn, ReplState, ReplTurn, TurnResult};
pub use type_stage::{type_check, TypeStageError};
