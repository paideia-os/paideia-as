//! R222.M2 — `CommandSig` reference-implementation fixture corpus.
//!
//! Five fixtures — one per light command. Each instantiates the
//! command's functor against a seeded [`SchemasSig`] and asserts the
//! returned [`CommandSig`] matches SH-D5's field-by-field shape:
//! `name` / `input_schema` / `output_schema` / `arguments` / `flags`
//! / `effects` / `required_capabilities` / `execute`. Fingerprint
//! `r222-m2-cmd-NN`.

mod common;
use common::assert_eq_tagged;
use paideia_as_shell_cmd::{
    commands::{count, find, head, sort, where_},
    InvocationCtx, SchemasSig,
};

/// Fixture 01 — `find` functor.
#[test]
fn r222_m2_cmd_01_find() {
    let sig = find::functor(&SchemasSig::r220_seed());
    assert_eq_tagged("r222-m2-cmd-01", sig.name.as_str(), "find");
    assert!(sig.input_schema.is_none(), "r222-m2-cmd-01: find is a source command");
    assert_eq_tagged(
        "r222-m2-cmd-01",
        sig.output_schema.as_ref().map(|s| s.name.clone()),
        Some("FileSchema@0.1".to_owned()),
    );
    assert_eq_tagged("r222-m2-cmd-01", sig.arguments.len(), 1);
    assert_eq_tagged("r222-m2-cmd-01", sig.arguments[0].name.as_str(), "path");
    assert!(sig.arguments[0].required, "r222-m2-cmd-01: path is required");
    assert!(sig.flag("recursive").is_some());
    assert!(sig.effects.effects.contains(&"fs_read".to_owned()));
    assert!(sig
        .required_capabilities
        .caps
        .contains(&"fs_read_under_path".to_owned()));
}

/// Fixture 02 — `where` functor.
#[test]
fn r222_m2_cmd_02_where() {
    let sig = where_::functor(&SchemasSig::r220_seed());
    assert_eq_tagged("r222-m2-cmd-02", sig.name.as_str(), "where");
    assert_eq_tagged(
        "r222-m2-cmd-02",
        sig.input_schema.clone(),
        sig.output_schema.clone(),
    );
    assert_eq_tagged("r222-m2-cmd-02", sig.arguments.len(), 1);
    assert_eq_tagged(
        "r222-m2-cmd-02",
        sig.arguments[0].name.as_str(),
        "predicate",
    );
    assert!(sig.effects.effects.is_empty(), "r222-m2-cmd-02: pure");
    assert!(sig.required_capabilities.caps.is_empty());
}

/// Fixture 03 — `sort` functor.
#[test]
fn r222_m2_cmd_03_sort() {
    let sig = sort::functor(&SchemasSig::r220_seed());
    assert_eq_tagged("r222-m2-cmd-03", sig.name.as_str(), "sort");
    assert_eq_tagged(
        "r222-m2-cmd-03",
        sig.input_schema.clone(),
        sig.output_schema.clone(),
    );
    assert!(sig.flag("desc").is_some(), "r222-m2-cmd-03: --desc flag");
    assert!(sig.flag("stable").is_some(), "r222-m2-cmd-03: --stable flag");
    assert!(sig.effects.effects.is_empty());
}

/// Fixture 04 — `head` functor.
#[test]
fn r222_m2_cmd_04_head() {
    let sig = head::functor(&SchemasSig::r220_seed());
    assert_eq_tagged("r222-m2-cmd-04", sig.name.as_str(), "head");
    assert_eq_tagged(
        "r222-m2-cmd-04",
        sig.input_schema.clone(),
        sig.output_schema.clone(),
    );
    assert_eq_tagged("r222-m2-cmd-04", sig.arguments.len(), 1);
    let arg = &sig.arguments[0];
    assert_eq_tagged("r222-m2-cmd-04", arg.name.as_str(), "n");
    assert!(!arg.required, "r222-m2-cmd-04: `n` is optional (default 10)");
    assert_eq_tagged("r222-m2-cmd-04", arg.default.clone(), Some("10".to_owned()));

    // Execute stub: passing n = 25 returns scalar 25.
    let ctx = InvocationCtx {
        argv: vec!["25".to_owned()],
        fingerprint: "r222-m2-cmd-04".to_owned(),
    };
    let res = (sig.execute)(&ctx);
    assert_eq_tagged("r222-m2-cmd-04", res.scalar, 25);
}

/// Fixture 05 — `count` functor. Sink command: `output = None`.
#[test]
fn r222_m2_cmd_05_count() {
    let sig = count::functor(&SchemasSig::r220_seed());
    assert_eq_tagged("r222-m2-cmd-05", sig.name.as_str(), "count");
    assert!(sig.input_schema.is_some(), "r222-m2-cmd-05: count reads records");
    assert!(sig.output_schema.is_none(), "r222-m2-cmd-05: count is a sink");
    assert!(sig.arguments.is_empty());
    assert!(sig.flag("unique").is_some());
}
