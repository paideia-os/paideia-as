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
use paideia_as_types::Type;

/// Resolve a shell type-name literal (`"Int"`, `"String"`, `"Bool"`,
/// …) into a monomorphic `paideia_as_types::Type` — the R222.M4
/// bridge from the `ArgSpec::type_name` / `FlagSpec::type_name`
/// placeholder strings to the elaborator's HM type substrate.
///
/// # Vocabulary
///
/// The five R222 light commands (`find`, `where`, `sort`, `head`,
/// `count`) exercise these type-name literals:
///
/// | shell literal     | HM type            | parses argv as |
/// |-------------------|--------------------|----------------|
/// | `Int`, `I64`      | `Type::SInt(64)`   | signed int     |
/// | `U32`             | `Type::UInt(32)`   | non-neg int    |
/// | `U64`             | `Type::UInt(64)`   | non-neg int    |
/// | `Bool`            | `Type::Bool`       | `true`/`false` |
/// | `String`, `Path`, | `Type::Str`        | raw string     |
/// |  `FileType`,      |                    |                |
/// |  `ByteSize`,      |                    |                |
/// |  `Lambda`,        |                    |                |
/// |  `FieldRef\|Lambda` |                  |                |
///
/// Unknown type-name literals resolve to `Type::Str` — the safe
/// fallback until the elaborator gains a shell-user-facing type
/// resolver (R225.M4). Callers who need "unresolved" telemetry
/// should compare on the original `type_name` string, which stays
/// on the spec.
///
/// The subset covered here matches what the R222.M4 `argparse` module
/// can actually parse a `Value` for (`Int` / `Str` / `Bool`); richer
/// types (`ByteSize`, `Lambda`, …) fall back to `Str` and are
/// interpreted downstream by the command's `execute` op.
///
/// # Why `Type` rather than `TypeScheme`
///
/// `paideia_as_types` exposes `Type` as a monomorphic enum; the
/// generalisation/instantiation shape of an HM `TypeScheme` has not
/// yet been surfaced publicly. ArgSpec/FlagSpec are always
/// monomorphic (no `forall a. …` bindings in a shell argument slot),
/// so `Type` is the exact fit. When the elaborator publishes a
/// public `TypeScheme` (R225.M4 close-out), this function's return
/// type widens without changing any existing caller — a
/// `Type` is a degenerate `TypeScheme` (`forall.` with an empty
/// binder).
pub fn resolve_type_name(type_name: &str) -> Type {
    match type_name {
        "Int" | "I64" => Type::SInt(64),
        "I8" => Type::SInt(8),
        "I16" => Type::SInt(16),
        "I32" => Type::SInt(32),
        "U8" => Type::UInt(8),
        "U16" => Type::UInt(16),
        "U32" => Type::UInt(32),
        "U64" => Type::UInt(64),
        "Bool" => Type::Bool,
        // Everything else — the shell-facing composite type names
        // (`FileType`, `ByteSize`, `Lambda`, `FieldRef|Lambda`, …)
        // and the plain `String` / `Path` bytes — surface as
        // `Type::Str`. The command's `execute` op refines from there.
        _ => Type::Str,
    }
}

/// A positional argument spec.
///
/// R222.M4 landed: `type_name` retains its `String` shape (the
/// on-the-wire representation the R222.M5 commands.toml loader
/// reads), and [`Self::resolve_type`] elaborates it into a real
/// [`paideia_as_types::Type`] via [`resolve_type_name`] at parse
/// time.
///
/// `required = false` means the argument may be omitted from the shell
/// line; the elaborator supplies `default` if present, else `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgSpec {
    /// Argument name (for `describe` output and error diagnostics).
    pub name: String,
    /// Type-name literal. Elaborated to a `paideia_as_types::Type` on
    /// demand via [`Self::resolve_type`] — the on-the-wire string
    /// stays so the R222.M5 commands.toml loader can round-trip it
    /// unchanged from disk to registry to shell.
    pub type_name: String,
    /// Whether the shell line MUST supply this argument.
    pub required: bool,
    /// Default value token, when `required = false`.
    pub default: Option<String>,
    /// Human-readable one-line description.
    pub help: String,
}

impl ArgSpec {
    /// Elaborate [`Self::type_name`] into the HM `Type` handle the
    /// R222.M4 argparse layer parses against.
    ///
    /// Delegates to [`resolve_type_name`]; kept as an inherent method
    /// so a caller with a spec in hand can write `spec.resolve_type()`
    /// without pulling the free function into scope.
    pub fn resolve_type(&self) -> Type {
        resolve_type_name(&self.type_name)
    }
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
    /// Type-name literal. See [`ArgSpec::type_name`] for the
    /// on-the-wire vs elaborated-`Type` split; elaborated via
    /// [`Self::resolve_type`].
    pub type_name: String,
    /// Default value literal (interpreted per `type_name`).
    pub default: Option<String>,
    /// Human-readable one-line description.
    pub help: String,
}

impl FlagSpec {
    /// Elaborate [`Self::type_name`] into an HM `Type`. See
    /// [`ArgSpec::resolve_type`] for the reasoning.
    pub fn resolve_type(&self) -> Type {
        resolve_type_name(&self.type_name)
    }

    /// Whether the parsed flag-name literal (stripped of the leading
    /// `--` or `-`) matches this spec's long-form name or its
    /// optional short-form single-char.
    ///
    /// The `parsed` argument is what [`crate::argparse`] hands over
    /// after stripping the leading dash(es) from an argv token; a
    /// caller with a raw `--foo` should strip first (see
    /// [`crate::argparse::parse_flags`] for the driver loop).
    pub fn matches_name(&self, parsed: &str) -> bool {
        if parsed == self.name {
            return true;
        }
        if let Some(short) = self.short {
            let mut chars = parsed.chars();
            let first = chars.next();
            if first == Some(short) && chars.next().is_none() {
                return true;
            }
        }
        false
    }
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
