//! R222.M4 — ArgSpec + FlagSpec HM-typed parse fixture corpus.
//!
//! 20 fixtures, fingerprint `r222m4-argspec-NN`:
//!
//!   * 01..10 — well-typed accept cases (parse yields the expected
//!     `Value`).
//!   * 11..20 — ill-typed reject cases (parse yields the expected
//!     error variant).
//!
//! Each `assert_eq_tagged` / `assert!` passes the fingerprint tag as
//! the diagnostic prefix so a failure names the exact canary.
//!
//! The corpus covers, per the R222.M4 issue:
//!
//!   * positional Int arg parse (01)
//!   * positional String arg parse (02)
//!   * default value applied when argv is short (03)
//!   * `--flag=value` joined form (04)
//!   * `--flag value` split form (05)
//!   * short `-x` flag with value (06)
//!   * Bool switch flag bare (07)
//!   * mixed positional args (08)
//!   * flag with default when omitted from flag-argv (09)
//!   * `parse_argv` with all-optional specs + empty argv (10)
//!   * missing required arg (11)
//!   * extra positional (12)
//!   * argv type mismatch: Int expected, got string (13)
//!   * unknown flag (14)
//!   * flag type mismatch: Int expected, got string (15)
//!   * non-Bool flag missing its value (16)
//!   * unknown short-form flag (17)
//!   * malformed value in `--flag=<junk>` for Int (18)
//!   * `ArgSpec::parse_from` on empty argv → Missing (19)
//!   * Bool flag with non-bool value → TypeMismatch (20)

mod common;
use common::assert_eq_tagged;

use paideia_as_shell_cmd::{
    argparse::{parse_argv, parse_flags, ArgParseError, FlagParseError, Value},
    ArgSpec, FlagSpec,
};

// -------------------------------------------------------------------
// Fixture builders
// -------------------------------------------------------------------

fn arg(name: &str, type_name: &str, required: bool, default: Option<&str>) -> ArgSpec {
    ArgSpec {
        name: name.to_owned(),
        type_name: type_name.to_owned(),
        required,
        default: default.map(str::to_owned),
        help: format!("test arg {name}"),
    }
}

fn flag(name: &str, short: Option<char>, type_name: &str, default: Option<&str>) -> FlagSpec {
    FlagSpec {
        name: name.to_owned(),
        short,
        type_name: type_name.to_owned(),
        default: default.map(str::to_owned),
        help: format!("test flag {name}"),
    }
}

fn s(x: &str) -> String {
    x.to_owned()
}

// -------------------------------------------------------------------
// 01..10 — well-typed accept
// -------------------------------------------------------------------

/// 01 — positional `Int` arg parses to `Value::Int(42)`.
#[test]
fn r222m4_argspec_01_positional_int_arg() {
    let spec = arg("n", "Int", true, None);
    let got = spec.parse_from(&[s("42")]).expect("r222m4-argspec-01: parse ok");
    assert_eq_tagged("r222m4-argspec-01", got, Value::Int(42));
}

/// 02 — positional `String` arg parses to `Value::Str`.
#[test]
fn r222m4_argspec_02_positional_string_arg() {
    let spec = arg("path", "String", true, None);
    let got = spec
        .parse_from(&[s("/home/snunez")])
        .expect("r222m4-argspec-02: parse ok");
    assert_eq_tagged("r222m4-argspec-02", got, Value::Str("/home/snunez".to_owned()));
}

/// 03 — default value applied when argv is short of specs.
#[test]
fn r222m4_argspec_03_default_applied() {
    let specs = [arg("n", "Int", false, Some("10"))];
    let got = parse_argv(&specs, &[]).expect("r222m4-argspec-03: parse ok");
    assert_eq_tagged("r222m4-argspec-03", got, vec![Value::Int(10)]);
}

/// 04 — `--flag=value` joined form.
#[test]
fn r222m4_argspec_04_flag_joined_form() {
    let specs = [flag("count", None, "Int", None)];
    let got = parse_flags(&specs, &[s("--count=7")]).expect("r222m4-argspec-04: parse ok");
    assert_eq_tagged("r222m4-argspec-04", got.get("count").cloned(), Some(Value::Int(7)));
}

/// 05 — `--flag value` split form.
#[test]
fn r222m4_argspec_05_flag_split_form() {
    let specs = [flag("count", None, "Int", None)];
    let got =
        parse_flags(&specs, &[s("--count"), s("13")]).expect("r222m4-argspec-05: parse ok");
    assert_eq_tagged("r222m4-argspec-05", got.get("count").cloned(), Some(Value::Int(13)));
}

/// 06 — short `-x=value` form.
#[test]
fn r222m4_argspec_06_short_flag_with_value() {
    let specs = [flag("count", Some('c'), "Int", None)];
    let got = parse_flags(&specs, &[s("-c=99")]).expect("r222m4-argspec-06: parse ok");
    assert_eq_tagged("r222m4-argspec-06", got.get("count").cloned(), Some(Value::Int(99)));
}

/// 07 — bare Bool switch (`--verbose`).
#[test]
fn r222m4_argspec_07_bool_switch() {
    let specs = [flag("verbose", Some('v'), "Bool", None)];
    let got = parse_flags(&specs, &[s("--verbose")]).expect("r222m4-argspec-07: parse ok");
    assert_eq_tagged(
        "r222m4-argspec-07",
        got.get("verbose").cloned(),
        Some(Value::Bool(true)),
    );
}

/// 08 — mixed positional args: [String, Int].
#[test]
fn r222m4_argspec_08_mixed_positional_args() {
    let specs = [
        arg("path", "String", true, None),
        arg("limit", "Int", true, None),
    ];
    let got = parse_argv(&specs, &[s("/tmp"), s("100")]).expect("r222m4-argspec-08: parse ok");
    assert_eq_tagged(
        "r222m4-argspec-08",
        got,
        vec![Value::Str("/tmp".to_owned()), Value::Int(100)],
    );
}

/// 09 — `FlagSpec::parse_from` on a `--flag=value` literal against a
/// spec with a default (default is only consulted by higher-level
/// callers; the joined form always wins).
#[test]
fn r222m4_argspec_09_flag_parse_from_joined() {
    let spec = flag("count", None, "Int", Some("1"));
    let (name, val) = spec
        .parse_from("--count=5")
        .expect("r222m4-argspec-09: parse ok");
    assert_eq_tagged("r222m4-argspec-09", name, "count".to_owned());
    assert_eq_tagged("r222m4-argspec-09", val, Value::Int(5));
}

/// 10 — `parse_argv` with a single optional spec + empty argv drops
/// the slot silently and yields an empty result vector.
#[test]
fn r222m4_argspec_10_optional_no_default_empty_argv() {
    let specs = [arg("query", "String", false, None)];
    let got = parse_argv(&specs, &[]).expect("r222m4-argspec-10: parse ok");
    assert_eq_tagged("r222m4-argspec-10", got, Vec::<Value>::new());
}

// -------------------------------------------------------------------
// 11..20 — ill-typed reject
// -------------------------------------------------------------------

/// 11 — required arg with no default AND empty argv → `Missing`.
#[test]
fn r222m4_argspec_11_missing_required() {
    let specs = [arg("path", "String", true, None)];
    let err = parse_argv(&specs, &[]).expect_err("r222m4-argspec-11: expected Missing");
    match err {
        ArgParseError::Missing { name, .. } => {
            assert_eq_tagged("r222m4-argspec-11", name, "path".to_owned())
        }
        other => panic!("r222m4-argspec-11: expected Missing, got {other:?}"),
    }
}

/// 12 — argv longer than specs → `ExtraPositional`.
#[test]
fn r222m4_argspec_12_extra_positional() {
    let specs = [arg("n", "Int", true, None)];
    let err = parse_argv(&specs, &[s("1"), s("2")])
        .expect_err("r222m4-argspec-12: expected ExtraPositional");
    assert!(
        matches!(err, ArgParseError::ExtraPositional { .. }),
        "r222m4-argspec-12: expected ExtraPositional, got {err:?}"
    );
}

/// 13 — Int arg, got non-numeric string → `TypeMismatch`.
#[test]
fn r222m4_argspec_13_arg_type_mismatch_int_got_string() {
    let specs = [arg("n", "Int", true, None)];
    let err = parse_argv(&specs, &[s("banana")])
        .expect_err("r222m4-argspec-13: expected TypeMismatch");
    match err {
        ArgParseError::TypeMismatch { name, expected, got, .. } => {
            assert_eq_tagged("r222m4-argspec-13", name, "n".to_owned());
            assert_eq_tagged("r222m4-argspec-13", expected, "Int".to_owned());
            assert_eq_tagged("r222m4-argspec-13", got, "String".to_owned());
        }
        other => panic!("r222m4-argspec-13: expected TypeMismatch, got {other:?}"),
    }
}

/// 14 — flag literal not matching any spec → `Unknown`.
#[test]
fn r222m4_argspec_14_unknown_flag() {
    let specs = [flag("count", None, "Int", None)];
    let err = parse_flags(&specs, &[s("--nonsuch=1")])
        .expect_err("r222m4-argspec-14: expected Unknown");
    match err {
        FlagParseError::Unknown { name, .. } => {
            assert_eq_tagged("r222m4-argspec-14", name, "nonsuch".to_owned())
        }
        other => panic!("r222m4-argspec-14: expected Unknown, got {other:?}"),
    }
}

/// 15 — Int flag, got non-numeric value → `TypeMismatch`.
#[test]
fn r222m4_argspec_15_flag_type_mismatch_int_got_string() {
    let specs = [flag("count", None, "Int", None)];
    let err = parse_flags(&specs, &[s("--count=banana")])
        .expect_err("r222m4-argspec-15: expected TypeMismatch");
    match err {
        FlagParseError::TypeMismatch { name, expected, got, .. } => {
            assert_eq_tagged("r222m4-argspec-15", name, "count".to_owned());
            assert_eq_tagged("r222m4-argspec-15", expected, "Int".to_owned());
            assert_eq_tagged("r222m4-argspec-15", got, "String".to_owned());
        }
        other => panic!("r222m4-argspec-15: expected TypeMismatch, got {other:?}"),
    }
}

/// 16 — non-Bool flag supplied bare with no following value →
/// `MissingValue`. (`FlagSpec::parse_from` — single literal, no
/// lookahead is possible.)
#[test]
fn r222m4_argspec_16_flag_missing_value_bare() {
    let spec = flag("count", None, "Int", None);
    let err = spec
        .parse_from("--count")
        .expect_err("r222m4-argspec-16: expected MissingValue");
    match err {
        FlagParseError::MissingValue { name, .. } => {
            assert_eq_tagged("r222m4-argspec-16", name, "count".to_owned())
        }
        other => panic!("r222m4-argspec-16: expected MissingValue, got {other:?}"),
    }
}

/// 17 — short-form flag not registered on any spec → `Unknown`.
#[test]
fn r222m4_argspec_17_unknown_short_flag() {
    let specs = [flag("verbose", Some('v'), "Bool", None)];
    let err = parse_flags(&specs, &[s("-q")])
        .expect_err("r222m4-argspec-17: expected Unknown");
    match err {
        FlagParseError::Unknown { name, .. } => {
            assert_eq_tagged("r222m4-argspec-17", name, "q".to_owned())
        }
        other => panic!("r222m4-argspec-17: expected Unknown, got {other:?}"),
    }
}

/// 18 — joined-form Int flag with a garbled value → `TypeMismatch`.
/// Distinct from 15 because the garbled value contains dashes /
/// dots that a naïve regex might accept.
#[test]
fn r222m4_argspec_18_flag_int_malformed_value() {
    let specs = [flag("count", None, "Int", None)];
    let err = parse_flags(&specs, &[s("--count=3.14.15")])
        .expect_err("r222m4-argspec-18: expected TypeMismatch");
    match err {
        FlagParseError::TypeMismatch { name, expected, .. } => {
            assert_eq_tagged("r222m4-argspec-18", name, "count".to_owned());
            assert_eq_tagged("r222m4-argspec-18", expected, "Int".to_owned());
        }
        other => panic!("r222m4-argspec-18: expected TypeMismatch, got {other:?}"),
    }
}

/// 19 — `ArgSpec::parse_from` on empty argv with a required-no-default
/// spec → `Missing`.
#[test]
fn r222m4_argspec_19_argspec_parse_from_empty_missing() {
    let spec = arg("path", "String", true, None);
    let err = spec
        .parse_from(&[])
        .expect_err("r222m4-argspec-19: expected Missing");
    match err {
        ArgParseError::Missing { name, .. } => {
            assert_eq_tagged("r222m4-argspec-19", name, "path".to_owned())
        }
        other => panic!("r222m4-argspec-19: expected Missing, got {other:?}"),
    }
}

/// 20 — Bool flag with a non-bool value → `TypeMismatch`. (`--verbose=maybe`
/// is not `true`/`false`.)
#[test]
fn r222m4_argspec_20_bool_flag_bad_value() {
    let specs = [flag("verbose", Some('v'), "Bool", None)];
    let err = parse_flags(&specs, &[s("--verbose=maybe")])
        .expect_err("r222m4-argspec-20: expected TypeMismatch");
    match err {
        FlagParseError::TypeMismatch { name, expected, got, .. } => {
            assert_eq_tagged("r222m4-argspec-20", name, "verbose".to_owned());
            assert_eq_tagged("r222m4-argspec-20", expected, "Bool".to_owned());
            assert_eq_tagged("r222m4-argspec-20", got, "String".to_owned());
        }
        other => panic!("r222m4-argspec-20: expected TypeMismatch, got {other:?}"),
    }
}
