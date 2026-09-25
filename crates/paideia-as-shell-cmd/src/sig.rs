//! R222.M2 — `CommandSig`: the functor-return signature.
//!
//! Per `design/terminal/semantic-shell.md` §6.1:
//!
//! ```text
//! signature CommandSig =
//!   name : String
//!   input_schema : Option<Schema>
//!   output_schema : Option<Schema>
//!   arguments : List<ArgSpec>
//!   flags : List<FlagSpec>
//!   effects : EffectRow
//!   required_capabilities : CapSpec
//!
//!   op execute : (input : Stream<InputRecord>,
//!                 args : ArgValues,
//!                 env : CapabilityEnvironment)
//!               -> Stream<OutputRecord>
//!               !{effects_declared_above}
//! ```
//!
//! # Field-by-field mapping
//!
//! * `name` — the command's shell name (`"find"`, `"where"`, …). The
//!   key the [`CommandRegistry`] uses for lookup. Byte-eq at the map
//!   boundary today; R222.M5 will move to R220.M4's `Str::eq`
//!   (NFC-normalised) on the substrate side.
//!
//! * `input_schema` / `output_schema` — [`SchemaRef`] handles the
//!   command's `execute` reads from and writes to. `None` on either
//!   side is a source or sink command respectively.
//!
//! * `arguments` — positional arguments per [`ArgSpec`]. Order-
//!   significant; the R222.M3 elaborator binds argv positions to spec
//!   positions by index.
//!
//! * `flags` — named flags per [`FlagSpec`]. Order-insignificant;
//!   the elaborator looks them up by name (long) or short-form.
//!
//! * `effects` — [`EffectRow`] naming the effects the command's
//!   `execute` may perform. Kept as a `Vec<String>` interim; the
//!   R220.M8 effect-row inference at call sites will let us tighten
//!   this to a real effect-row handle once the elaborator surfaces
//!   one publicly.
//!
//! * `required_capabilities` — [`CapSpec`] the supervisor mints for
//!   this command at spawn time; must be a subset of the invoker's
//!   session env (SH-D6 §7.2).
//!
//! * `execute` — the op. Rust interim: `fn(&InvocationCtx) ->
//!   ExecuteResult`. When R223.M1 lands the pipeline stage integration,
//!   the signature widens to accept a `Stream<InputRecord>` handle
//!   and returns a `Stream<OutputRecord>`; the field-name stays
//!   `execute` under the port.

use crate::schema::SchemaRef;

/// A positional argument spec (R222.M2 shape; R222.M4 will grow the
/// `type_name` string into a real `paideia-as-types::Type` handle).
///
/// `required = false` means the argument may be omitted from the shell
/// line; the elaborator supplies `default` if present, else `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgSpec {
    /// Argument name (for `describe` output and error diagnostics).
    pub name: String,
    /// Type name — placeholder until R222.M4 (`Type` handle).
    pub type_name: String,
    /// Whether the shell line MUST supply this argument.
    pub required: bool,
    /// Default value token, when `required = false`.
    pub default: Option<String>,
    /// Human-readable one-line description.
    pub help: String,
}

/// A named flag spec.
///
/// Long form is `--name`; the optional short form is `-<char>`. The
/// elaborator accepts either. `type_name = "Bool"` marks a switch
/// flag (no value token); anything else expects a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlagSpec {
    /// Long-form name (without the leading `--`).
    pub name: String,
    /// Short-form single-char, when present.
    pub short: Option<char>,
    /// Type name — placeholder until R222.M4 (`Type` handle).
    pub type_name: String,
    /// Default value literal (interpreted per `type_name`).
    pub default: Option<String>,
    /// Human-readable one-line description.
    pub help: String,
}

/// R220.M8-shaped placeholder — a list of effect names.
///
/// A Rust `Vec<String>` is the interim; the moment the elaborator
/// exposes a public effect-row handle (R220.M8 close-out plus the
/// R225.M4 effect-row unification landing), this shape moves to that
/// handle and existing consumers gain principal-type inference.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EffectRow {
    /// Effect names (e.g. `["fs_read", "fs_enumerate"]`).
    pub effects: Vec<String>,
}

impl EffectRow {
    /// Construct from a slice of effect-name literals.
    pub fn of(effects: &[&str]) -> Self {
        Self {
            effects: effects.iter().map(|e| (*e).to_owned()).collect(),
        }
    }

    /// Empty effect row (pure commands like `head`, `count`).
    pub fn pure() -> Self {
        Self::default()
    }
}

/// Set of capability names the command needs.
///
/// The supervisor mints child capabilities bounded by this set at
/// spawn time (SH-D6 §7.2). An empty `caps` set is a pure command
/// (no capability at all) — `head`, `count`, `where`, `sort` on
/// already-streamed records.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapSpec {
    /// Capability names (e.g. `["fs_read_under_path"]`).
    pub caps: Vec<String>,
}

impl CapSpec {
    /// Construct from a slice of capability-name literals.
    pub fn of(caps: &[&str]) -> Self {
        Self {
            caps: caps.iter().map(|c| (*c).to_owned()).collect(),
        }
    }

    /// Empty cap set.
    pub fn none() -> Self {
        Self::default()
    }
}

/// Runtime context handed to a command's `execute` op.
///
/// R222.M3 interim shape: raw argv + the per-turn fingerprint tag the
/// dispatcher stamped. R223.M1 will add `input: Stream<Record>` and
/// `env: CapabilityEnvironment`; those fields do NOT exist yet
/// because their upstream types (`Stream`, `CapabilityEnvironment`)
/// have not landed on the host side. Adding them empty here would
/// invite consumers to encode assumptions about a shape that is not
/// finished.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvocationCtx {
    /// Argv the shell line elaborated to, excluding the command name.
    pub argv: Vec<String>,
    /// Per-turn fingerprint (R220.M10 shape, e.g. `"r222-m3-cmd-01"`).
    pub fingerprint: String,
}

/// Result of an `execute` op.
///
/// R222.M3 interim: an integer scalar plus the per-turn fingerprint.
/// The scalar's meaning is command-specific — `count` returns the
/// count, `find` returns the number of records emitted, etc. R223.M1
/// replaces this with `Stream<OutputRecord>` once the pipeline stage
/// glue exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecuteResult {
    /// Command-specific scalar output.
    pub scalar: u64,
    /// Per-turn fingerprint echoed back from the ctx.
    pub fingerprint: String,
}

impl ExecuteResult {
    /// Construct — the common shape a stub `execute` returns.
    pub fn new(scalar: u64, fingerprint: impl Into<String>) -> Self {
        Self {
            scalar,
            fingerprint: fingerprint.into(),
        }
    }
}

/// The Rust-side lowering of a command's `execute` op.
///
/// A `fn` pointer rather than a `Box<dyn Fn>` — the R222.M5 substrate
/// port will store these in the same `HashMap<Str, ClosureFatPtr>`
/// R220.M7 landed, and a bare `fn` maps cleanly to that fat-ptr's
/// `code_ptr` (env_ptr = 0 for capture-less top-level functions).
/// When a command's execute needs closure state (e.g. a version of
/// `find` that captured a pre-parsed regex), the field-type widens to
/// a `Box<dyn Fn>` and we lose fn-ptr identity — but at R222.M3 every
/// reference command is capture-less.
pub type ExecuteFn = fn(&InvocationCtx) -> ExecuteResult;

/// R222.M2 — `CommandSig`.
///
/// Every field is `pub` so a functor implementation can build the
/// struct with a struct literal (matching how the eventual `.pdx`
/// port will read: `let name = "find" let input_schema = None ...`).
#[derive(Clone, Debug, PartialEq)]
pub struct CommandSig {
    /// Shell name.
    pub name: String,
    /// Input stream schema (`None` for source commands).
    pub input_schema: Option<SchemaRef>,
    /// Output stream schema (`None` for sink commands).
    pub output_schema: Option<SchemaRef>,
    /// Positional arguments.
    pub arguments: Vec<ArgSpec>,
    /// Named flags.
    pub flags: Vec<FlagSpec>,
    /// Effects the `execute` op may perform.
    pub effects: EffectRow,
    /// Capabilities the supervisor mints for the command.
    pub required_capabilities: CapSpec,
    /// The `execute` op.
    pub execute: ExecuteFn,
}

impl CommandSig {
    /// Look up an [`ArgSpec`] by name.
    pub fn arg(&self, name: &str) -> Option<&ArgSpec> {
        self.arguments.iter().find(|a| a.name == name)
    }

    /// Look up a [`FlagSpec`] by long-form name.
    pub fn flag(&self, name: &str) -> Option<&FlagSpec> {
        self.flags.iter().find(|f| f.name == name)
    }
}
