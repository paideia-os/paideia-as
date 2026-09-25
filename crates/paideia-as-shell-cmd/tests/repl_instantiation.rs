//! R222.M3 — functor instantiation at the REPL prompt.
//!
//! Canary fixtures for the SH-D5 dispatch path: raw prompt line →
//! registry lookup → functor instantiation against session schemas →
//! elaborated argv → invokable [`Invocation`]. Fingerprint
//! `r222-m3-cmd-NN`.

mod common;
use common::assert_eq_tagged;
use paideia_as_shell_cmd::{
    dispatch_line, CommandRegistry, DispatchError, SchemasSig,
};

/// Fixture 01 — the headline canary: `find .` resolves, instantiates
/// against the R220 seed, and produces an `Invocation` whose sig is
/// the `find` CommandSig with `FileSchema@0.1` output.
#[test]
fn r222_m3_cmd_01_find_dot() {
    let reg = CommandRegistry::with_light_commands();
    let schemas = SchemasSig::r220_seed();
    let inv = dispatch_line(&reg, &schemas, "find .", "r222-m3-cmd-01")
        .expect("r222-m3-cmd-01: dispatch");
    assert_eq_tagged("r222-m3-cmd-01", inv.sig.name.as_str(), "find");
    assert_eq_tagged(
        "r222-m3-cmd-01",
        inv.sig
            .output_schema
            .as_ref()
            .map(|s| s.name.clone()),
        Some("FileSchema@0.1".to_owned()),
    );
    assert_eq_tagged("r222-m3-cmd-01", inv.argv.clone(), vec![".".to_owned()]);
    assert_eq_tagged("r222-m3-cmd-01", inv.fingerprint.as_str(), "r222-m3-cmd-01");

    // Execute the stub — scalar 1 means "argv was non-empty; a real
    // FS walk would emit at least one record" per the find stub.
    let res = inv.execute();
    assert_eq_tagged("r222-m3-cmd-01", res.scalar, 1);
    assert_eq_tagged("r222-m3-cmd-01", res.fingerprint.as_str(), "r222-m3-cmd-01");
}

/// Fixture 02 — `where { |r| r.size > 10 }` (whitespace-elaborated;
/// the lambda literal itself is not parsed at R222.M3 — the elaborator
/// receives the raw argv and the R223.M3 filter combinator will
/// evaluate it). Proves the whole `where` functor path resolves and
/// keeps its schema-preserving shape.
#[test]
fn r222_m3_cmd_02_where_predicate() {
    let reg = CommandRegistry::with_light_commands();
    let schemas = SchemasSig::r220_seed();
    let inv = dispatch_line(
        &reg,
        &schemas,
        "where predicate_stub",
        "r222-m3-cmd-02",
    )
    .expect("r222-m3-cmd-02: dispatch");
    assert_eq_tagged("r222-m3-cmd-02", inv.sig.name.as_str(), "where");
    assert_eq_tagged(
        "r222-m3-cmd-02",
        inv.sig.input_schema.clone(),
        inv.sig.output_schema.clone(),
    );
    assert_eq_tagged("r222-m3-cmd-02", inv.argv.len(), 1);
}

/// Fixture 03 — `sort key --desc`. Two positional args? No — `sort`
/// has one required positional (`key`); `--desc` looks like a flag
/// but R222.M3's argv splitter is flag-blind (that's R222.M4). Here
/// we prove the raw argv passes through and the sig still has both
/// flags declared.
#[test]
fn r222_m3_cmd_03_sort_key_desc() {
    let reg = CommandRegistry::with_light_commands();
    let schemas = SchemasSig::r220_seed();
    let inv = dispatch_line(&reg, &schemas, "sort size --desc", "r222-m3-cmd-03")
        .expect("r222-m3-cmd-03: dispatch");
    assert_eq_tagged("r222-m3-cmd-03", inv.sig.name.as_str(), "sort");
    // R222.M3 sees TWO argv entries (`size`, `--desc`) — the flag
    // pass-through is R222.M4. We prove the raw shape here.
    assert_eq_tagged("r222-m3-cmd-03", inv.argv.len(), 2);
    assert_eq_tagged("r222-m3-cmd-03", inv.argv[1].as_str(), "--desc");
    assert!(inv.sig.flag("desc").is_some());
}

/// Fixture 04 — `head 5`. Exercises the optional-argument path: `n`
/// is not required, so an empty argv would ALSO dispatch — but here
/// argv[0] = "5" carries through, and the head stub returns 5.
#[test]
fn r222_m3_cmd_04_head_five() {
    let reg = CommandRegistry::with_light_commands();
    let schemas = SchemasSig::r220_seed();
    let inv = dispatch_line(&reg, &schemas, "head 5", "r222-m3-cmd-04")
        .expect("r222-m3-cmd-04: dispatch");
    assert_eq_tagged("r222-m3-cmd-04", inv.sig.name.as_str(), "head");
    let res = inv.execute();
    assert_eq_tagged("r222-m3-cmd-04", res.scalar, 5);

    // Also prove empty argv dispatches (n defaults to 10).
    let inv2 = dispatch_line(&reg, &schemas, "head", "r222-m3-cmd-04b")
        .expect("r222-m3-cmd-04b: dispatch");
    let res2 = inv2.execute();
    assert_eq_tagged("r222-m3-cmd-04b", res2.scalar, 10);
}

/// Fixture 05 — `count` sink; also exercises negative paths:
/// unknown command + `find` with no args (missing required `path`).
#[test]
fn r222_m3_cmd_05_count_and_negatives() {
    let reg = CommandRegistry::with_light_commands();
    let schemas = SchemasSig::r220_seed();

    // Happy path — `count` with no argv (its only args are flags).
    let inv = dispatch_line(&reg, &schemas, "count", "r222-m3-cmd-05")
        .expect("r222-m3-cmd-05: dispatch");
    assert_eq_tagged("r222-m3-cmd-05", inv.sig.name.as_str(), "count");
    assert!(inv.sig.output_schema.is_none());

    // Negative 1 — empty line.
    let e_empty = dispatch_line(&reg, &schemas, "", "r222-m3-cmd-05e1").unwrap_err();
    assert_eq_tagged("r222-m3-cmd-05e1", e_empty, DispatchError::EmptyLine);

    // Negative 2 — unknown command.
    let e_unk = dispatch_line(&reg, &schemas, "notacmd .", "r222-m3-cmd-05e2")
        .unwrap_err();
    assert_eq_tagged(
        "r222-m3-cmd-05e2",
        e_unk,
        DispatchError::UnknownCommand("notacmd".to_owned()),
    );

    // Negative 3 — `find` with no path arg.
    let e_missing = dispatch_line(&reg, &schemas, "find", "r222-m3-cmd-05e3")
        .unwrap_err();
    assert_eq_tagged(
        "r222-m3-cmd-05e3",
        e_missing,
        DispatchError::MissingRequiredArg {
            command: "find".to_owned(),
            arg_name: "path".to_owned(),
        },
    );
}

/// Fixture 06 — registry contents. Proves the five-command seed
/// exposes exactly the light-command set the R222.M3 landing wires
/// (`find`, `where`, `sort`, `head`, `count`). A drift alarm for
/// R222.M6 (which will move `find` and `grep` out to heavy dispatch).
#[test]
fn r222_m3_cmd_06_registry_light_set() {
    let reg = CommandRegistry::with_light_commands();
    assert_eq_tagged("r222-m3-cmd-06", reg.len(), 5);
    let mut names: Vec<&str> = reg.names().collect();
    names.sort();
    assert_eq_tagged(
        "r222-m3-cmd-06",
        names,
        vec!["count", "find", "head", "sort", "where"],
    );
}
