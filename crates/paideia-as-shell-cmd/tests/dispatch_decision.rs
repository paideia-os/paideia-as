//! R222.M6 — `decide_dispatch` fixture corpus.
//!
//! Ten fixtures fingerprinted `r222m6-disp-01`..`r222m6-disp-10`
//! cover:
//!
//! * 01–05 — the five R222.M3 reference commands. `find` classifies
//!   as heavy (spawn); `where`, `sort`, `head`, `count` classify as
//!   light (in-process).
//! * 06 — every in-process target carries a non-zero `functor_id`
//!   (fnv1a_64 over a non-empty name never hits the offset basis for
//!   these commands — a smoke against a decoder that forgot to hash).
//! * 07 — every spawn target's `argv[0]` equals the command name.
//! * 08 — round-trip via the registry: `resolve → functor(schemas)
//!   → decide_dispatch` produces the same target as calling
//!   `decide_dispatch` on the functor's return directly.
//! * 09 — synthetic Heavy command with a custom name: `binary_path`
//!   is `/bin/<name>` verbatim.
//! * 10 — synthetic Light command with a custom name: `functor_id`
//!   matches `fnv1a_64(name)`.

mod common;
use common::assert_eq_tagged;

use paideia_as_shell_cmd::{
    commands::{count, find, head, sort, where_},
    decide_dispatch,
    fingerprint::fnv1a_64,
    wire::placeholder_execute,
    CapSpec, CommandRegistry, CommandSig, CommandWeight, DispatchTarget, EffectRow, SchemasSig,
};

/// Fixture 01 — `find` → spawn `/bin/find` with argv `["find"]`.
#[test]
fn r222m6_disp_01_find_spawn() {
    let tag = "r222m6-disp-01";
    let sig = find::functor(&SchemasSig::r220_seed());
    let target = decide_dispatch(&sig);
    assert_eq_tagged(
        tag,
        target,
        DispatchTarget::Spawn {
            binary_path: "/bin/find".to_owned(),
            argv: vec!["find".to_owned()],
        },
    );
}

/// Fixture 02 — `where` → in-process with `functor_id = fnv1a_64("where")`.
#[test]
fn r222m6_disp_02_where_inprocess() {
    let tag = "r222m6-disp-02";
    let sig = where_::functor(&SchemasSig::r220_seed());
    let target = decide_dispatch(&sig);
    assert_eq_tagged(
        tag,
        target,
        DispatchTarget::InProcess {
            functor_id: fnv1a_64(b"where"),
        },
    );
}

/// Fixture 03 — `sort` → in-process.
#[test]
fn r222m6_disp_03_sort_inprocess() {
    let tag = "r222m6-disp-03";
    let sig = sort::functor(&SchemasSig::r220_seed());
    let target = decide_dispatch(&sig);
    assert_eq_tagged(
        tag,
        target,
        DispatchTarget::InProcess {
            functor_id: fnv1a_64(b"sort"),
        },
    );
}

/// Fixture 04 — `head` → in-process.
#[test]
fn r222m6_disp_04_head_inprocess() {
    let tag = "r222m6-disp-04";
    let sig = head::functor(&SchemasSig::r220_seed());
    let target = decide_dispatch(&sig);
    assert_eq_tagged(
        tag,
        target,
        DispatchTarget::InProcess {
            functor_id: fnv1a_64(b"head"),
        },
    );
}

/// Fixture 05 — `count` → in-process.
#[test]
fn r222m6_disp_05_count_inprocess() {
    let tag = "r222m6-disp-05";
    let sig = count::functor(&SchemasSig::r220_seed());
    let target = decide_dispatch(&sig);
    assert_eq_tagged(
        tag,
        target,
        DispatchTarget::InProcess {
            functor_id: fnv1a_64(b"count"),
        },
    );
}

/// Fixture 06 — every in-process target carries a non-zero functor_id.
///
/// A decoder that returns 0 unconditionally (e.g. because the hash
/// helper is stubbed) would pass 02–05 only for a name whose fnv1a_64
/// happens to be 0 (mathematically none of these do). Explicitly
/// asserting non-zero catches that class of bug head-on.
#[test]
fn r222m6_disp_06_inprocess_id_nonzero() {
    let tag = "r222m6-disp-06";
    let schemas = SchemasSig::r220_seed();
    for functor in [
        where_::functor as fn(&SchemasSig) -> CommandSig,
        sort::functor,
        head::functor,
        count::functor,
    ] {
        let sig = functor(&schemas);
        let target = decide_dispatch(&sig);
        match target {
            DispatchTarget::InProcess { functor_id } => {
                assert!(
                    functor_id != 0,
                    "{tag}: functor_id must be non-zero for `{}`",
                    sig.name
                );
            }
            other => panic!("{tag}: expected InProcess for `{}`, got {other:?}", sig.name),
        }
    }
}

/// Fixture 07 — every spawn target's argv[0] equals the command name.
#[test]
fn r222m6_disp_07_spawn_argv0_is_name() {
    let tag = "r222m6-disp-07";
    let sig = find::functor(&SchemasSig::r220_seed());
    let target = decide_dispatch(&sig);
    match target {
        DispatchTarget::Spawn { binary_path, argv } => {
            assert!(!argv.is_empty(), "{tag}: argv must not be empty");
            assert_eq_tagged(tag, argv[0].clone(), sig.name.clone());
            // And the binary_path uses the same name.
            assert_eq_tagged(tag, binary_path, format!("/bin/{}", sig.name));
        }
        other => panic!("{tag}: expected Spawn, got {other:?}"),
    }
}

/// Fixture 08 — round-trip via the registry.
///
/// Simulates the R229 REPL path: resolve the name against the
/// registry, instantiate the functor against the session schemas,
/// then decide. The target must match the one `decide_dispatch`
/// returns on a directly-instantiated sig.
#[test]
fn r222m6_disp_08_registry_roundtrip() {
    let tag = "r222m6-disp-08";
    let reg = CommandRegistry::with_light_commands();
    let schemas = SchemasSig::r220_seed();

    // find → Spawn (heavy)
    let sig_find_direct = find::functor(&schemas);
    let sig_find_via_registry = reg.resolve("find").expect("`find` registered")(&schemas);
    assert_eq_tagged(
        tag,
        decide_dispatch(&sig_find_via_registry),
        decide_dispatch(&sig_find_direct),
    );
    assert!(matches!(
        decide_dispatch(&sig_find_via_registry),
        DispatchTarget::Spawn { .. }
    ));

    // where → InProcess (light)
    let sig_where_direct = where_::functor(&schemas);
    let sig_where_via_registry = reg.resolve("where").expect("`where` registered")(&schemas);
    assert_eq_tagged(
        tag,
        decide_dispatch(&sig_where_via_registry),
        decide_dispatch(&sig_where_direct),
    );
    assert!(matches!(
        decide_dispatch(&sig_where_via_registry),
        DispatchTarget::InProcess { .. }
    ));

    // And an unknown name resolves to None — the dispatcher never
    // reaches `decide_dispatch` for it, but we assert the shape here
    // so the round-trip covers the negative case too.
    assert!(reg.resolve("no-such-command").is_none());
}

/// Fixture 09 — synthetic Heavy command with a custom name.
///
/// Uses [`placeholder_execute`] as the fn-ptr — a public no-op-shape
/// stub the wire module already exports. Weight = Heavy → target's
/// `binary_path` is `/bin/<name>` for the exact custom name.
#[test]
fn r222m6_disp_09_synthetic_heavy_custom_name() {
    let tag = "r222m6-disp-09";
    let sig = synthetic_sig("my-compile-driver", CommandWeight::Heavy);
    let target = decide_dispatch(&sig);
    assert_eq_tagged(
        tag,
        target,
        DispatchTarget::Spawn {
            binary_path: "/bin/my-compile-driver".to_owned(),
            argv: vec!["my-compile-driver".to_owned()],
        },
    );
}

/// Fixture 10 — synthetic Light command with a custom name.
///
/// Weight = Light → `functor_id = fnv1a_64(name_bytes)`.
#[test]
fn r222m6_disp_10_synthetic_light_custom_name() {
    let tag = "r222m6-disp-10";
    let name = "custom-projection";
    let sig = synthetic_sig(name, CommandWeight::Light);
    let target = decide_dispatch(&sig);
    assert_eq_tagged(
        tag,
        target,
        DispatchTarget::InProcess {
            functor_id: fnv1a_64(name.as_bytes()),
        },
    );
}

/// Build a minimal [`CommandSig`] with the given name and weight,
/// planting the wire-side [`placeholder_execute`] as `execute`. The
/// sig is deliberately bare — the dispatch decision has no dependency
/// on argument / flag / effect / cap shape, and threading real fixtures
/// through this helper would obscure that.
fn synthetic_sig(name: &str, weight: CommandWeight) -> CommandSig {
    CommandSig {
        name: name.to_owned(),
        input_schema: None,
        output_schema: None,
        arguments: vec![],
        flags: vec![],
        effects: EffectRow::pure(),
        required_capabilities: CapSpec::none(),
        execute: placeholder_execute,
        weight,
    }
}
