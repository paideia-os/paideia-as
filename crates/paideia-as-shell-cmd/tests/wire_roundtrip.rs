//! R222.M8 — `CommandSig` wire-format round-trip corpus.
//!
//! For each of the five R222.M3 light commands (`find`, `where`,
//! `sort`, `head`, `count`) we build the concrete `CommandSig` via
//! its functor, serialise with [`paideia_as_shell_cmd::to_wire`],
//! deserialise back with [`paideia_as_shell_cmd::from_wire`], and
//! assert every descriptive field survives byte-identically. The
//! `execute` fn-ptr is compared separately: the restored side must be
//! the [`paideia_as_shell_cmd::wire::placeholder_execute`] stub, not
//! the original functor pointer — that's the contract R222.M8 signs.
//!
//! Fingerprints `r222m8-wire-01..05` cover the five commands;
//! `r222m8-wire-06` covers `CommandRegistry::describe`; `-07`/`-08`
//! cover the two error rejections (bad-magic + short-read).

mod common;
use common::assert_eq_tagged;

use paideia_as_shell_cmd::{
    commands::{count, find, head, sort, where_},
    from_wire, to_wire,
    wire::{placeholder_execute, WireError, WIRE_MAGIC, WIRE_VERSION},
    CommandRegistry, CommandSig, SchemasSig,
};

/// Assert two `CommandSig`s are equal in every field except `execute`,
/// and that the restored side's `execute` is the placeholder stub.
///
/// The stub-identity check catches a decoder that forgets to plant
/// the placeholder and instead defaults to some other fn-ptr — a
/// class of bug the spec is explicit about not tolerating.
fn assert_descriptive_eq(tag: &str, before: &CommandSig, after: &CommandSig) {
    assert_eq_tagged(tag, after.name.clone(), before.name.clone());
    assert_eq_tagged(tag, after.input_schema.clone(), before.input_schema.clone());
    assert_eq_tagged(
        tag,
        after.output_schema.clone(),
        before.output_schema.clone(),
    );
    assert_eq_tagged(tag, after.arguments.clone(), before.arguments.clone());
    assert_eq_tagged(tag, after.flags.clone(), before.flags.clone());
    assert_eq_tagged(tag, after.effects.clone(), before.effects.clone());
    assert_eq_tagged(
        tag,
        after.required_capabilities.clone(),
        before.required_capabilities.clone(),
    );

    // `execute` must be the placeholder stub, distinct from the
    // original functor's `execute`. `fn`-ptr equality on stable Rust
    // is well-defined for `fn(&InvocationCtx) -> ExecuteResult`.
    assert_eq_tagged(
        tag,
        after.execute as usize,
        placeholder_execute as usize,
    );
    // And distinct from the original: guards against a decoder that
    // (say) accidentally copied `before.execute` through.
    if (before.execute as usize) != (placeholder_execute as usize) {
        assert!(
            (after.execute as usize) != (before.execute as usize),
            "{tag}: restored execute must NOT equal the original fn-ptr"
        );
    }
}

/// Serialise-then-deserialise-then-reserialise: the twice-serialised
/// bytes must match the once-serialised bytes exactly. Catches a
/// decoder that silently drops or reorders anything.
fn assert_bytes_stable(tag: &str, before: &CommandSig) {
    let bytes1 = to_wire(before);
    let after = from_wire(&bytes1).expect("round-trip decode");
    let bytes2 = to_wire(&after);
    assert_eq_tagged(tag, bytes2, bytes1);
}

// ---------- 01..05: round-trip per command ----------

#[test]
fn r222m8_wire_01_find_roundtrip() {
    let tag = "r222m8-wire-01";
    let sig = find::functor(&SchemasSig::r220_seed());
    let bytes = to_wire(&sig);
    assert!(!bytes.is_empty(), "{tag}: wire bytes non-empty");
    let restored = from_wire(&bytes).expect("decode");
    assert_descriptive_eq(tag, &sig, &restored);
    assert_bytes_stable(tag, &sig);
}

#[test]
fn r222m8_wire_02_where_roundtrip() {
    let tag = "r222m8-wire-02";
    let sig = where_::functor(&SchemasSig::r220_seed());
    let bytes = to_wire(&sig);
    let restored = from_wire(&bytes).expect("decode");
    assert_descriptive_eq(tag, &sig, &restored);
    assert_bytes_stable(tag, &sig);
}

#[test]
fn r222m8_wire_03_sort_roundtrip() {
    let tag = "r222m8-wire-03";
    let sig = sort::functor(&SchemasSig::r220_seed());
    let bytes = to_wire(&sig);
    let restored = from_wire(&bytes).expect("decode");
    assert_descriptive_eq(tag, &sig, &restored);
    assert_bytes_stable(tag, &sig);
}

#[test]
fn r222m8_wire_04_head_roundtrip() {
    let tag = "r222m8-wire-04";
    let sig = head::functor(&SchemasSig::r220_seed());
    let bytes = to_wire(&sig);
    let restored = from_wire(&bytes).expect("decode");
    assert_descriptive_eq(tag, &sig, &restored);
    assert_bytes_stable(tag, &sig);

    // Extra: `head` has `default = Some("10")` on its `n` arg — the
    // canary Option<String> case. Assert Some("10") did NOT collapse
    // to None or to Some("").
    let n_arg = &restored.arguments[0];
    assert_eq_tagged(tag, n_arg.default.clone(), Some("10".to_owned()));
}

#[test]
fn r222m8_wire_05_count_roundtrip() {
    let tag = "r222m8-wire-05";
    let sig = count::functor(&SchemasSig::r220_seed());
    let bytes = to_wire(&sig);
    let restored = from_wire(&bytes).expect("decode");
    assert_descriptive_eq(tag, &sig, &restored);
    assert_bytes_stable(tag, &sig);

    // Sink command: output_schema must be None on both sides.
    assert!(restored.output_schema.is_none(), "{tag}: sink");
}

// ---------- 06: describe() returns non-empty bytes ----------

#[test]
fn r222m8_wire_06_describe_find_non_empty() {
    let tag = "r222m8-wire-06";
    let reg = CommandRegistry::with_light_commands();
    let bytes = reg.describe("find").expect("`find` is registered");
    assert!(!bytes.is_empty(), "{tag}: describe(find) returned bytes");

    // And the header is the R222.M8 wire header we expect.
    assert!(bytes.len() >= 4, "{tag}: at least a header's worth");
    let magic = u16::from_le_bytes([bytes[0], bytes[1]]);
    let version = u16::from_le_bytes([bytes[2], bytes[3]]);
    assert_eq_tagged(tag, magic, WIRE_MAGIC);
    assert_eq_tagged(tag, version, WIRE_VERSION);

    // Absent command returns None.
    assert!(reg.describe("no-such-command").is_none());

    // Full-loop sanity: the bytes describe() returned decode back to
    // a CommandSig whose name is "find".
    let decoded = from_wire(&bytes).expect("describe payload decodes");
    assert_eq_tagged(tag, decoded.name.as_str(), "find");
}

// ---------- 07: bad-magic rejection ----------

#[test]
fn r222m8_wire_07_bad_magic() {
    let tag = "r222m8-wire-07";

    // Craft a header with the wrong magic (0xDEAD) but the right
    // version byte layout. Nothing after the header matters — the
    // decoder must fail before reading further.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0xDEADu16.to_le_bytes());
    bytes.extend_from_slice(&WIRE_VERSION.to_le_bytes());
    // Filler so the buffer is not itself a ShortRead on the magic.
    bytes.extend_from_slice(&[0u8; 32]);

    let err = from_wire(&bytes).expect_err("bad magic must be rejected");
    assert_eq_tagged(tag, err, WireError::BadMagic);
}

// ---------- 08: short-read rejection ----------

#[test]
fn r222m8_wire_08_short_read() {
    let tag = "r222m8-wire-08";

    // Start from a real payload, then truncate it. Truncating past the
    // header but before the `name` length is fully consumable exercises
    // the exact ShortRead path the u32 reader emits.
    let sig = find::functor(&SchemasSig::r220_seed());
    let full = to_wire(&sig);
    assert!(full.len() > 5, "sanity: full payload extends past header");

    // Truncate to just 5 bytes — enough for magic (2) + version (2) +
    // one byte of the name's u32 length prefix. Reading the u32
    // length must trip ShortRead { needed: 4, got: 1 }.
    let truncated = &full[..5];
    let err = from_wire(truncated).expect_err("truncated must be rejected");
    match err {
        WireError::ShortRead { needed, got } => {
            assert_eq_tagged(tag, needed, 4usize);
            assert_eq_tagged(tag, got, 1usize);
        }
        other => panic!("{tag}: expected ShortRead, got {other:?}"),
    }

    // Also cover the header-level short read: 0 bytes → ShortRead on
    // the magic u16.
    let err0 = from_wire(&[]).expect_err("empty buffer must be rejected");
    match err0 {
        WireError::ShortRead { needed, got } => {
            assert_eq_tagged(tag, needed, 2usize);
            assert_eq_tagged(tag, got, 0usize);
        }
        other => panic!("{tag}: expected ShortRead on empty, got {other:?}"),
    }
}
