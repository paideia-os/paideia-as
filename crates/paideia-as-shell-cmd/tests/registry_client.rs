//! R222.M5 — `registry_client` test corpus.
//!
//! Ten fixtures fingerprinted `r222m5-reg-01`..`r222m5-reg-10`. Each
//! synthesises one or two `commands.toml` files under a per-test
//! `tempfile::TempDir` and exercises a single load-time property. No
//! test touches a persistent filesystem path — every fixture is
//! self-contained under the `TempDir` and cleaned up on drop.

use std::fs;
use std::path::PathBuf;

use paideia_as_shell_cmd::{
    load_from, seed_registry, CommandRegistry, CommandWeight, RegistryLoadError,
};

/// Materialise a manifest fixture under `dir` at `filename`.
fn write_toml(dir: &tempfile::TempDir, filename: &str, body: &str) -> PathBuf {
    let path = dir.path().join(filename);
    fs::write(&path, body).expect("write fixture");
    path
}

const R222M5_REG_01_SYSTEM: &str = r#"
[[commands]]
name = "find"
input_schema = "FileSchema@0.1"
output_schema = "FileSchema@0.1"
weight = "Heavy"

[[commands]]
name = "where"
input_schema = "FileSchema@0.1"
output_schema = "FileSchema@0.1"

[[commands]]
name = "count"
input_schema = "FileSchema@0.1"
"#;

/// Fixture 01 — system-only load of a three-entry manifest returns
/// exactly three [`paideia_as_shell_cmd::CommandSig`] values in order.
#[test]
fn r222m5_reg_01_system_only_three_entries() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sys = write_toml(&dir, "system.toml", R222M5_REG_01_SYSTEM);

    let loaded = load_from(&sys, None).expect("r222m5-reg-01: load");
    assert_eq!(loaded.len(), 3, "r222m5-reg-01: entry count");
    assert_eq!(loaded[0].name, "find", "r222m5-reg-01: order[0]");
    assert_eq!(loaded[1].name, "where", "r222m5-reg-01: order[1]");
    assert_eq!(loaded[2].name, "count", "r222m5-reg-01: order[2]");
}

/// Fixture 02 — a user manifest entry shadows a same-name system entry
/// in place: total count is unchanged and the user body wins.
#[test]
fn r222m5_reg_02_user_shadows_system() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sys = write_toml(
        &dir,
        "system.toml",
        r#"
[[commands]]
name = "find"
input_schema = "FileSchema@0.1"
weight = "Light"

[[commands]]
name = "where"
input_schema = "FileSchema@0.1"
weight = "Light"
"#,
    );
    let usr = write_toml(
        &dir,
        "user.toml",
        r#"
[[commands]]
name = "find"
input_schema = "UserFileSchema@0.1"
weight = "Heavy"
"#,
    );

    let loaded = load_from(&sys, Some(&usr)).expect("r222m5-reg-02: load");
    assert_eq!(loaded.len(), 2, "r222m5-reg-02: shadow keeps count");
    let find = loaded
        .iter()
        .find(|s| s.name == "find")
        .expect("r222m5-reg-02: find present");
    assert_eq!(
        find.input_schema.as_ref().map(|s| s.name.as_str()),
        Some("UserFileSchema@0.1"),
        "r222m5-reg-02: user schema wins",
    );
    assert_eq!(
        find.weight,
        CommandWeight::Heavy,
        "r222m5-reg-02: user weight wins",
    );
}

/// Fixture 03 — a user manifest entry whose name is not in the system
/// manifest appends: total = system + new.
#[test]
fn r222m5_reg_03_user_adds_new_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sys = write_toml(
        &dir,
        "system.toml",
        r#"
[[commands]]
name = "find"
[[commands]]
name = "where"
"#,
    );
    let usr = write_toml(
        &dir,
        "user.toml",
        r#"
[[commands]]
name = "grep"
weight = "Heavy"
"#,
    );

    let loaded = load_from(&sys, Some(&usr)).expect("r222m5-reg-03: load");
    assert_eq!(loaded.len(), 3, "r222m5-reg-03: system + new");
    assert!(
        loaded.iter().any(|s| s.name == "grep"),
        "r222m5-reg-03: grep appended",
    );
}

/// Fixture 04 — a missing system manifest surfaces
/// [`RegistryLoadError::IoError`].
#[test]
fn r222m5_reg_04_missing_system_is_io_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sys = dir.path().join("does-not-exist.toml");

    let err = load_from(&sys, None).expect_err("r222m5-reg-04: expected error");
    assert!(
        matches!(err, RegistryLoadError::IoError(_)),
        "r222m5-reg-04: got {err:?}",
    );
}

/// Fixture 05 — a missing user manifest with the system file present is
/// silently accepted; the returned list equals the system-only load.
#[test]
fn r222m5_reg_05_missing_user_is_ok() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sys = write_toml(&dir, "system.toml", R222M5_REG_01_SYSTEM);
    let missing_user = dir.path().join("no-user-here.toml");

    let loaded = load_from(&sys, Some(&missing_user)).expect("r222m5-reg-05: load");
    assert_eq!(
        loaded.len(),
        3,
        "r222m5-reg-05: missing user leaves system list",
    );
}

/// Fixture 06 — a malformed TOML manifest surfaces
/// [`RegistryLoadError::TomlParse`] and carries the offending path.
#[test]
fn r222m5_reg_06_malformed_toml_carries_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sys = write_toml(
        &dir,
        "system.toml",
        r#"this is = not = valid toml [[[["#,
    );

    let err = load_from(&sys, None).expect_err("r222m5-reg-06: expected error");
    match err {
        RegistryLoadError::TomlParse { path, reason } => {
            assert_eq!(path, sys, "r222m5-reg-06: path in payload");
            assert!(
                !reason.is_empty(),
                "r222m5-reg-06: parser reason non-empty",
            );
        }
        other => panic!("r222m5-reg-06: wrong variant: {other:?}"),
    }
}

/// Fixture 07 — two entries with the same name inside a single manifest
/// surface [`RegistryLoadError::DuplicateInSameFile`].
#[test]
fn r222m5_reg_07_same_file_duplicate_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sys = write_toml(
        &dir,
        "system.toml",
        r#"
[[commands]]
name = "find"

[[commands]]
name = "find"
"#,
    );

    let err = load_from(&sys, None).expect_err("r222m5-reg-07: expected error");
    match err {
        RegistryLoadError::DuplicateInSameFile { name, path } => {
            assert_eq!(name, "find", "r222m5-reg-07: duplicate name");
            assert_eq!(path, sys, "r222m5-reg-07: duplicate path");
        }
        other => panic!("r222m5-reg-07: wrong variant: {other:?}"),
    }
}

/// Fixture 08 — an entry with `weight` omitted lands with
/// `CommandWeight::Light` (the safe default per `CommandWeight::default`).
#[test]
fn r222m5_reg_08_missing_weight_is_light() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sys = write_toml(
        &dir,
        "system.toml",
        r#"
[[commands]]
name = "head"
"#,
    );

    let loaded = load_from(&sys, None).expect("r222m5-reg-08: load");
    assert_eq!(loaded.len(), 1);
    assert_eq!(
        loaded[0].weight,
        CommandWeight::Light,
        "r222m5-reg-08: default weight",
    );
}

/// Fixture 09 — an entry with `weight = "Heavy"` lands with
/// `CommandWeight::Heavy`.
#[test]
fn r222m5_reg_09_heavy_weight_parses() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sys = write_toml(
        &dir,
        "system.toml",
        r#"
[[commands]]
name = "grep"
weight = "Heavy"
"#,
    );

    let loaded = load_from(&sys, None).expect("r222m5-reg-09: load");
    assert_eq!(loaded.len(), 1);
    assert_eq!(
        loaded[0].weight,
        CommandWeight::Heavy,
        "r222m5-reg-09: heavy weight",
    );
}

/// Fixture 10 — `seed_registry` populates a `CommandRegistry` with the
/// loaded sigs; the returned count matches the load, and every loaded
/// name resolves via `CommandRegistry::resolve_sig`.
#[test]
fn r222m5_reg_10_seed_registry_populates() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sys = write_toml(&dir, "system.toml", R222M5_REG_01_SYSTEM);
    let loaded = load_from(&sys, None).expect("r222m5-reg-10: load");

    let mut reg = CommandRegistry::new();
    let inserted = seed_registry(&loaded, &mut reg);
    assert_eq!(
        inserted,
        loaded.len(),
        "r222m5-reg-10: seed_registry count matches",
    );
    assert_eq!(
        reg.sig_len(),
        loaded.len(),
        "r222m5-reg-10: registry sig_len matches",
    );
    for sig in &loaded {
        let hit = reg
            .resolve_sig(&sig.name)
            .unwrap_or_else(|| panic!("r222m5-reg-10: {} not resolvable", sig.name));
        assert_eq!(hit.name, sig.name, "r222m5-reg-10: name round-trip");
    }
}
