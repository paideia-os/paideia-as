//! R222.M8 — `CommandSig` wire format for `describe` queries.
//!
//! # Why we serialise a functor's return
//!
//! `describe find` is a REPL introspection call: the shell asks the
//! command registry "what does `find` look like?" and gets back a
//! byte string it can render locally (help text, argument tables,
//! effect / capability display), pipe over a supervisor RPC to a
//! remote registry, or hand to a documentation tool. The bytes are
//! the *descriptive* part of the functor's return — everything in
//! [`CommandSig`] **except** the `execute` fn-ptr. `execute` is a
//! process-local address; it has no wire meaning and re-hydrating
//! one on the far side would smuggle an unbounded set of assumptions
//! about the remote's address space into the caller.
//!
//! # Why a hand-rolled length-prefixed format (interim)
//!
//! The eventual on-wire representation is a FlatBuffers table
//! (`design/terminal/wire-format.md` §2 lists CommandSig alongside
//! SchemaHandle among the tables generated from the shared `.fbs`).
//! This crate does not yet pull in `flatbuffers` — R222.M8 is a
//! host-side landing on the way to R222.M5's supervisor RPC — so we
//! ship a tight hand-rolled shape whose semantics are the same:
//! little-endian scalars, `u32` length prefixes for byte strings and
//! collections, and no padding. When R222.M5 bundles the generated
//! FlatBuffers `.rs` in, this module's callers see a single
//! `describe → Vec<u8>` boundary and pick up the schema-evolution
//! story FlatBuffers gives for free — the current struct-level shape
//! is deliberately picked to be a one-to-one carrier of what the
//! future table will hold, so the migration is a `to_wire` /
//! `from_wire` body swap and nothing else.
//!
//! # Byte layout (v1)
//!
//! All multi-byte scalars are little-endian.
//!
//! ```text
//! u16 magic       = 0xCC01   (CommandSig v1 marker)
//! u16 version     = 1
//! str name                    (u32 len + UTF-8 bytes)
//! opt<SchemaRef> input_schema (u8 tag; if 1 then SchemaRef)
//! opt<SchemaRef> output_schema (same)
//! u32 arg_count
//!   for each arg:
//!     str  name
//!     str  type_name
//!     u8   required (0|1)
//!     opt<str> default (u8 tag; if 1 then str)
//!     str  help
//! u32 flag_count
//!   for each flag:
//!     str  name
//!     u8   has_short (0|1); if 1 then u8 ASCII short-form byte
//!     str  type_name
//!     opt<str> default (u8 tag; if 1 then str)
//!     str  help
//! u32 effect_count
//!   for each effect:  str name
//! u32 cap_count
//!   for each cap:     str name
//!
//! SchemaRef  ::= u64 fingerprint (little-endian) + str name
//! ```
//!
//! # Design notes
//!
//! * **Option tags are `u8` (0 or 1), not `u8` presence + a raw payload
//!   with a sentinel.** The scope brief writes "u8 has_short + optional
//!   u8 short" for the flag short-form and "length-prefixed default"
//!   without an explicit tag for Option<String>. Round-tripping
//!   `default = None` vs `default = Some("")` byte-identically is
//!   REQUIRED by the R222.M8 acceptance ("assert every field except
//!   `execute` is bit-identical"), so an explicit `u8` tag is the
//!   correct read of the spec — a bare length prefix would silently
//!   collapse the two into the empty string. Same reasoning for
//!   optional `SchemaRef`.
//!
//! * **Short flags are one ASCII byte.** Every FlagSpec short in the
//!   R222.M3 reference commands is ASCII (`-t`, `-r`, `-v`, `-d`),
//!   and shell short-forms have never conventionally been non-ASCII.
//!   `to_wire` refuses to serialise a non-ASCII short (assertion in
//!   debug, error via `InvalidShortChar` symmetry on decode); when
//!   the FlatBuffers port lands, the field grows to a `u32`
//!   codepoint and this restriction lifts by construction.
//!
//! * **`execute` is a placeholder on decode.** [`from_wire`] returns a
//!   [`CommandSig`] whose `execute` field is
//!   [`placeholder_execute`] — a fn-ptr that panics on invocation.
//!   Callers that got a `CommandSig` off the wire are describing it,
//!   not invoking it; if the far side wanted to invoke the command,
//!   it would resolve the name against its own local registry and
//!   get an executable functor there.
//!
//! * **Bit-identity across a round trip.** [`to_wire`] emits fields
//!   in a fixed order and never inserts padding; [`from_wire`]
//!   consumes them in the same order and rebuilds the `Vec<String>`
//!   fields in appearance order. The test corpus asserts full field
//!   equality (`sig_before == sig_after`) modulo the `execute`
//!   fn-ptr, which the round-trip test compares by asserting the
//!   restored `execute` is the placeholder rather than the original.

use crate::schema::{SchemaFingerprint, SchemaRef};
use crate::sig::{
    ArgSpec, CapSpec, CommandSig, EffectRow, ExecuteResult, FlagSpec, InvocationCtx,
};

/// Wire-format magic prefix for a `CommandSig` v1 payload.
///
/// Encoded little-endian at byte 0 of every `to_wire` output;
/// `from_wire` refuses anything else with [`WireError::BadMagic`].
/// The value is arbitrary but deliberately chosen NOT to collide
/// with the ASCII printable range so a hex dump of a wire payload
/// is visually distinct from an accidental UTF-8 blob.
pub const WIRE_MAGIC: u16 = 0xCC01;

/// Wire-format version for this module's `to_wire` output.
///
/// Bumped when the layout changes in a way that is not
/// backwards-readable. `from_wire` refuses any other value with
/// [`WireError::BadVersion`]; when a v2 lands, this module gains a
/// dispatch on the version byte rather than a silent read of the v1
/// layout under a newer tag.
pub const WIRE_VERSION: u16 = 1;

/// What can go wrong deserialising a `CommandSig` off the wire.
///
/// Kept as a plain enum rather than a `miette::Diagnostic` because
/// R222.M8 has no source-span to point at — a wire buffer is bytes,
/// not a `.pdx` file. When R222.M5 wires the supervisor RPC and a
/// bad payload arrives from a remote, the caller wraps this into
/// whatever diagnostic shape that RPC uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WireError {
    /// First `u16` did not match [`WIRE_MAGIC`]. Either the buffer is
    /// not a `CommandSig` payload at all, or it belongs to an
    /// unrelated wire format the caller mis-routed here.
    BadMagic,
    /// Second `u16` matched [`WIRE_MAGIC`] but not [`WIRE_VERSION`].
    /// The buffer is a `CommandSig` payload from an incompatible
    /// version of this module.
    BadVersion,
    /// The buffer ended before the parser had read all the bytes the
    /// layout demanded. `needed` is the byte count the current field
    /// still wanted; `got` is what was left in the buffer.
    ShortRead {
        /// Bytes the parser wanted to read for the current field.
        needed: usize,
        /// Bytes actually available in the remainder of the buffer.
        got: usize,
    },
    /// A length-prefixed string field held bytes that are not valid
    /// UTF-8. The Rust-side `String` shape refuses to hold non-UTF-8
    /// bytes, so this is a decode-time rejection rather than a
    /// silent lossy conversion.
    InvalidUtf8,
    /// A `u8` value used as an Option tag was neither 0 nor 1. The
    /// wire is corrupt (or a future version has repurposed the byte
    /// without bumping the version — a bug on the encoder side).
    InvalidOptionTag,
    /// A short-form flag byte was not ASCII (`>= 0x80`). The v1 wire
    /// restricts shorts to ASCII single characters.
    InvalidShortChar,
    /// A `u8` value used as a bool was neither 0 nor 1.
    InvalidBool,
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadMagic => f.write_str("wire: bad magic (not a CommandSig payload)"),
            Self::BadVersion => f.write_str("wire: unsupported CommandSig version"),
            Self::ShortRead { needed, got } => {
                write!(f, "wire: short read (needed {needed} bytes, got {got})")
            }
            Self::InvalidUtf8 => f.write_str("wire: invalid UTF-8 in string field"),
            Self::InvalidOptionTag => f.write_str("wire: invalid Option tag byte"),
            Self::InvalidShortChar => f.write_str("wire: non-ASCII short-form flag byte"),
            Self::InvalidBool => f.write_str("wire: invalid bool byte"),
        }
    }
}

impl std::error::Error for WireError {}

// ---------------------------------------------------------------------------
// Encode
// ---------------------------------------------------------------------------

/// Serialise a [`CommandSig`] to its wire-format byte string.
///
/// The output starts with [`WIRE_MAGIC`] and [`WIRE_VERSION`]; every
/// subsequent field is emitted per the layout documented at
/// module-level. The `execute` fn-ptr is deliberately NOT written —
/// see the module docs.
pub fn to_wire(sig: &CommandSig) -> Vec<u8> {
    // Rough pre-size: header (4B) + name + two schema refs + a few
    // hundred bytes of arg/flag/effect/cap payload. 256 is a decent
    // ballpark that avoids reallocating for the R222.M3 five light
    // commands (measured: each fits in under 512 bytes).
    let mut buf: Vec<u8> = Vec::with_capacity(256);

    write_u16(&mut buf, WIRE_MAGIC);
    write_u16(&mut buf, WIRE_VERSION);

    write_str(&mut buf, &sig.name);
    write_opt_schema(&mut buf, sig.input_schema.as_ref());
    write_opt_schema(&mut buf, sig.output_schema.as_ref());

    write_u32(&mut buf, sig.arguments.len() as u32);
    for a in &sig.arguments {
        write_str(&mut buf, &a.name);
        write_str(&mut buf, &a.type_name);
        buf.push(a.required as u8);
        write_opt_str(&mut buf, a.default.as_deref());
        write_str(&mut buf, &a.help);
    }

    write_u32(&mut buf, sig.flags.len() as u32);
    for f in &sig.flags {
        write_str(&mut buf, &f.name);
        match f.short {
            Some(c) => {
                let cp = c as u32;
                // v1 wire: ASCII only. Debug-assert here so a
                // functor that grows a non-ASCII short (which the
                // reference commands never do) is caught in tests
                // before it reaches an unsuspecting decoder.
                debug_assert!(cp < 0x80, "wire v1: short-form flag must be ASCII");
                buf.push(1);
                buf.push(cp as u8);
            }
            None => buf.push(0),
        }
        write_str(&mut buf, &f.type_name);
        write_opt_str(&mut buf, f.default.as_deref());
        write_str(&mut buf, &f.help);
    }

    write_u32(&mut buf, sig.effects.effects.len() as u32);
    for e in &sig.effects.effects {
        write_str(&mut buf, e);
    }

    write_u32(&mut buf, sig.required_capabilities.caps.len() as u32);
    for c in &sig.required_capabilities.caps {
        write_str(&mut buf, c);
    }

    buf
}

#[inline]
fn write_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
}

#[inline]
fn write_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

#[inline]
fn write_u64(buf: &mut Vec<u8>, v: u64) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_str(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    write_u32(buf, bytes.len() as u32);
    buf.extend_from_slice(bytes);
}

fn write_opt_str(buf: &mut Vec<u8>, s: Option<&str>) {
    match s {
        None => buf.push(0),
        Some(v) => {
            buf.push(1);
            write_str(buf, v);
        }
    }
}

fn write_schema(buf: &mut Vec<u8>, s: &SchemaRef) {
    write_u64(buf, s.fingerprint.0);
    write_str(buf, &s.name);
}

fn write_opt_schema(buf: &mut Vec<u8>, s: Option<&SchemaRef>) {
    match s {
        None => buf.push(0),
        Some(v) => {
            buf.push(1);
            write_schema(buf, v);
        }
    }
}

// ---------------------------------------------------------------------------
// Decode
// ---------------------------------------------------------------------------

/// Deserialise a wire-format byte string back into a [`CommandSig`].
///
/// The returned `CommandSig`'s `execute` is [`placeholder_execute`] —
/// a fn-ptr that panics on invocation. See the module docs for why.
pub fn from_wire(bytes: &[u8]) -> Result<CommandSig, WireError> {
    let mut r = Reader::new(bytes);

    let magic = r.read_u16()?;
    if magic != WIRE_MAGIC {
        return Err(WireError::BadMagic);
    }
    let version = r.read_u16()?;
    if version != WIRE_VERSION {
        return Err(WireError::BadVersion);
    }

    let name = r.read_str()?;
    let input_schema = r.read_opt_schema()?;
    let output_schema = r.read_opt_schema()?;

    let n_args = r.read_u32()? as usize;
    let mut arguments = Vec::with_capacity(n_args);
    for _ in 0..n_args {
        let name = r.read_str()?;
        let type_name = r.read_str()?;
        let required = r.read_bool()?;
        let default = r.read_opt_str()?;
        let help = r.read_str()?;
        arguments.push(ArgSpec {
            name,
            type_name,
            required,
            default,
            help,
        });
    }

    let n_flags = r.read_u32()? as usize;
    let mut flags = Vec::with_capacity(n_flags);
    for _ in 0..n_flags {
        let name = r.read_str()?;
        let has_short = r.read_u8()?;
        let short = match has_short {
            0 => None,
            1 => {
                let b = r.read_u8()?;
                if b >= 0x80 {
                    return Err(WireError::InvalidShortChar);
                }
                Some(b as char)
            }
            _ => return Err(WireError::InvalidOptionTag),
        };
        let type_name = r.read_str()?;
        let default = r.read_opt_str()?;
        let help = r.read_str()?;
        flags.push(FlagSpec {
            name,
            short,
            type_name,
            default,
            help,
        });
    }

    let n_effects = r.read_u32()? as usize;
    let mut effect_names = Vec::with_capacity(n_effects);
    for _ in 0..n_effects {
        effect_names.push(r.read_str()?);
    }
    let effects = EffectRow {
        effects: effect_names,
    };

    let n_caps = r.read_u32()? as usize;
    let mut cap_names = Vec::with_capacity(n_caps);
    for _ in 0..n_caps {
        cap_names.push(r.read_str()?);
    }
    let required_capabilities = CapSpec { caps: cap_names };

    Ok(CommandSig {
        name,
        input_schema,
        output_schema,
        arguments,
        flags,
        effects,
        required_capabilities,
        execute: placeholder_execute,
    })
}

/// The `execute` fn-ptr planted on a wire-restored [`CommandSig`].
///
/// Panics unconditionally: a `CommandSig` that arrived off the wire
/// carries only the descriptive part of the functor's return, not the
/// executable body. Callers that need to invoke a command resolve it
/// against their own local registry.
pub fn placeholder_execute(_ctx: &InvocationCtx) -> ExecuteResult {
    panic!("wire-restored CommandSig has no executable body")
}

/// Byte-cursor reader with short-read tracking.
///
/// Kept a private struct so the read helpers can share the position
/// state without threading a `&mut usize` through every call site.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    #[inline]
    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    fn ensure(&self, needed: usize) -> Result<(), WireError> {
        if self.remaining() < needed {
            return Err(WireError::ShortRead {
                needed,
                got: self.remaining(),
            });
        }
        Ok(())
    }

    fn read_u8(&mut self) -> Result<u8, WireError> {
        self.ensure(1)?;
        let v = self.buf[self.pos];
        self.pos += 1;
        Ok(v)
    }

    fn read_u16(&mut self) -> Result<u16, WireError> {
        self.ensure(2)?;
        let v = u16::from_le_bytes([self.buf[self.pos], self.buf[self.pos + 1]]);
        self.pos += 2;
        Ok(v)
    }

    fn read_u32(&mut self) -> Result<u32, WireError> {
        self.ensure(4)?;
        let mut arr = [0u8; 4];
        arr.copy_from_slice(&self.buf[self.pos..self.pos + 4]);
        self.pos += 4;
        Ok(u32::from_le_bytes(arr))
    }

    fn read_u64(&mut self) -> Result<u64, WireError> {
        self.ensure(8)?;
        let mut arr = [0u8; 8];
        arr.copy_from_slice(&self.buf[self.pos..self.pos + 8]);
        self.pos += 8;
        Ok(u64::from_le_bytes(arr))
    }

    fn read_bool(&mut self) -> Result<bool, WireError> {
        match self.read_u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(WireError::InvalidBool),
        }
    }

    fn read_str(&mut self) -> Result<String, WireError> {
        let len = self.read_u32()? as usize;
        self.ensure(len)?;
        let bytes = &self.buf[self.pos..self.pos + len];
        let s = std::str::from_utf8(bytes)
            .map_err(|_| WireError::InvalidUtf8)?
            .to_owned();
        self.pos += len;
        Ok(s)
    }

    fn read_opt_str(&mut self) -> Result<Option<String>, WireError> {
        match self.read_u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.read_str()?)),
            _ => Err(WireError::InvalidOptionTag),
        }
    }

    fn read_schema(&mut self) -> Result<SchemaRef, WireError> {
        let fingerprint = SchemaFingerprint(self.read_u64()?);
        let name = self.read_str()?;
        Ok(SchemaRef { name, fingerprint })
    }

    fn read_opt_schema(&mut self) -> Result<Option<SchemaRef>, WireError> {
        match self.read_u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.read_schema()?)),
            _ => Err(WireError::InvalidOptionTag),
        }
    }
}
