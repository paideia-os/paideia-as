//! Reference `CommandSig` functor implementations for the five light
//! commands the R222.M3 canary exercises.
//!
//! Every functor here has the shape
//!
//! ```text
//! pub fn functor(schemas: &SchemasSig) -> CommandSig
//! ```
//!
//! matching [`CommandFunctor`]. The `schemas` argument is the session's
//! [`crate::SchemasSig`]; each functor pulls the schemas it consumes
//! by canonical name (`FileSchema@0.1`, etc.), returning a
//! [`crate::CommandSig`] with the concrete schema references bound
//! into `input_schema` / `output_schema`. If the required schema is
//! not present in the session, the functor still constructs a
//! `CommandSig` — with a `SchemaRef::of_name` fabricated on the fly
//! (matching the eventual ML functor semantics: the signature is
//! satisfied by *any* schema of the right shape; the daemon-side
//! registry decides whether that shape is available at wire time).
//!
//! # The five reference commands
//!
//! * [`find`] — walks the filesystem, emits `FileSchema@0.1` records.
//!   Source command (`input = None`).
//! * [`where_`] — filters an input stream by a predicate. Named
//!   `where_` in Rust because `where` is a keyword; the shell name is
//!   still `"where"`.
//! * [`sort`] — reorders an input stream by a key.
//! * [`head`] — keeps the first N records of an input stream.
//! * [`count`] — counts input records. Sink command (`output = None`).
//!
//! # Why these five
//!
//! The R222.M6 dispatch table (light vs heavy) classifies these five
//! as **light** — small, fast, no major state, execute as in-process
//! function calls (per semantic-shell.md §6.2). Using them as the
//! R222.M3 canary set proves the functor shape end-to-end WITHOUT
//! also stubbing the substrate-process-spawn path that heavy
//! commands (`grep`, `compile`, `vim`) need. That process seam is
//! R222.M6's own landing.

pub mod count;
pub mod find;
pub mod head;
pub mod sort;
pub mod where_;

use crate::schema::SchemasSig;
use crate::sig::CommandSig;

/// The Rust-side type of a paideia-as `functor (Schemas : SchemasSig)
/// -> struct : CommandSig` — a plain function pointer taking the
/// session's schemas and returning a fully-bound signature.
///
/// `fn` rather than `Box<dyn Fn>` on purpose — see [`crate::sig::ExecuteFn`]
/// for the same reasoning (interior fn-ptr identity + fat-ptr port
/// compatibility on the R222.M5 substrate side).
pub type CommandFunctor = fn(&SchemasSig) -> CommandSig;
