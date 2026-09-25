//! paideia-as-shell-cmd — R222.M1..M3 command-module functor surface.
//!
//! # SH-D5 in one paragraph
//!
//! Every semantic-shell command is a **functor** parameterised by the
//! schemas and services it consumes:
//!
//! ```text
//! module Find : CommandSig = functor (Schemas : SchemasSig) -> struct
//!   let name = "find"
//!   let input_schema = None
//!   let output_schema = Some FileSchema
//!   ...
//! end
//! ```
//!
//! This crate is the Rust-side lowering of that shape used by the
//! host-side shell during bring-up (per
//! `design/terminal/semantic-shell-materialization-plan.md`): a
//! [`CommandFunctor`] is a `fn(&SchemasSig) -> CommandSig`, and the
//! [`CommandRegistry`] is the `HashMap<String, CommandFunctor>` the
//! REPL prompt consults on every turn. The interim `String`-keyed
//! table exactly matches the shape R220.M7's `HashMap<Str,
//! ClosureFatPtr>` will lower to on the paideia-as substrate when
//! R222.M5 (commands.toml loader + supervisor RPC) lands — the map
//! stays byte-compatible under the port.
//!
//! # Milestone breakdown
//!
//! * **R222.M1** — [`SchemasSig`] and the [`SchemaRef`] /
//!   [`SchemaFingerprint`] key shape it composes over. Fingerprint
//!   semantics track `design/terminal/schema-registry.md` §3
//!   (FNV-1a-64 over the schema's NUL-terminated name string; BLAKE3
//!   migration is a follow-up when the paideia-as intrinsic lands).
//!
//! * **R222.M2** — [`CommandSig`] with `name` / `input_schema` /
//!   `output_schema` / `arguments` / `flags` / `effects` /
//!   `required_capabilities` / `execute`. Reference implementations
//!   live under [`commands`]: `Find`, `Where`, `Sort`, `Head`,
//!   `Count` (the five light commands the R222.M6 dispatch table
//!   will classify as in-process).
//!
//! * **R222.M3** — [`dispatch`] resolves a raw prompt line against
//!   the registry, instantiates the functor with the session's
//!   `SchemasSig`, elaborates argv, and returns an [`Invocation`]
//!   the caller invokes `execute` on. Per-turn fingerprint tag
//!   follows the R220.M10 `@fingerprint("...")` shape.
//!
//! * **R222.M4** — [`argparse`] parses raw argv + flag-argv against
//!   the spec's [`ArgSpec`] / [`FlagSpec`] with HM type-check via
//!   [`sig::resolve_type_name`] into `paideia_as_types::Type`.
//!   Returns [`argparse::Value`] (`Int | Str | Bool`) plus structured
//!   [`argparse::ArgParseError`] / [`argparse::FlagParseError`] with
//!   argv byte-spans for a downstream diagnostics layer.
//!
//! # What R222.M1..M4 deliberately does NOT do
//!
//! * **On-disk registry manifest** — R222.M5 loads
//!   `/system/shell/commands.toml` and per-user overrides at
//!   `/users/<u>/shell/commands.toml`. R222.M3 seeds the registry
//!   in-code via [`CommandRegistry::with_light_commands`].
//! * **Light vs heavy dispatch** — R222.M6. Every command in this
//!   landing is light (in-process function call). Heavy commands
//!   (`grep`, `compile`, `vim`) will register a functor whose
//!   `execute` spawns a substrate process via the osarch process
//!   seam; the shape of `CommandSig` does not change.
//! * **Pipeline stage integration** — R223.M1. `dispatch::from_line`
//!   here splits on whitespace only. When the R221.M5 unified AST
//!   parser lands, a `dispatch::from_tokens(&[Token]) ->
//!   Result<Invocation, _>` sibling appears and this crate picks up
//!   a dependency on `paideia-as-shell-lex`.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod argparse;
pub mod commands;
pub mod dispatch;
pub mod fingerprint;
pub mod registry;
pub mod schema;
pub mod sig;
pub mod wire;

pub use argparse::{parse_argv, parse_flags, ArgParseError, FlagParseError, Value};
pub use commands::CommandFunctor;
pub use dispatch::{dispatch as dispatch_line, DispatchError, Invocation};
pub use fingerprint::fnv1a_64;
pub use registry::CommandRegistry;
pub use schema::{SchemaFingerprint, SchemaRef, SchemasSig};
pub use sig::{
    resolve_type_name, ArgSpec, CapSpec, CommandSig, EffectRow, ExecuteResult, FlagSpec,
    InvocationCtx,
};
pub use wire::{from_wire, to_wire, WireError, WIRE_MAGIC, WIRE_VERSION};
