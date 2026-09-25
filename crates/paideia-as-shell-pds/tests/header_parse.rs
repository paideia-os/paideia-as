//! R227.M1 `.pds` header pragma parser fixture corpus.
//!
//! 15 tests, tagged `r227m1-header-NN`. Each fixture exercises one
//! surface behaviour of `paideia_as_shell_pds::parse_header`.

use paideia_as_shell_pds::{parse_header, Import, PdsHeaderError, SchemaRef, Version};

#[test]
fn r227m1_header_01_single_capability() {
    let src = "#capability \"fs.read.home\"\n\nbody\n";
    let h = parse_header(src).expect("r227m1-header-01: parse must succeed");
    assert_eq!(
        h.capabilities,
        vec!["fs.read.home".to_owned()],
        "r227m1-header-01"
    );
    assert_eq!(h.requires_paideia, None, "r227m1-header-01");
    assert!(h.imports.is_empty(), "r227m1-header-01");
    assert!(h.schemas.is_empty(), "r227m1-header-01");
    assert!(!h.ascii, "r227m1-header-01");
    assert_eq!(&src[h.body_offset..], "body\n", "r227m1-header-01");
}

#[test]
fn r227m1_header_02_multiple_capabilities() {
    let src = "#capability \"fs.read.home\"\n\
               #capability \"fs.write.home/backup\"\n\
               #capability \"net.dial\"\n\n\
               body\n";
    let h = parse_header(src).expect("r227m1-header-02: parse must succeed");
    assert_eq!(
        h.capabilities,
        vec![
            "fs.read.home".to_owned(),
            "fs.write.home/backup".to_owned(),
            "net.dial".to_owned(),
        ],
        "r227m1-header-02: capabilities accumulate in source order"
    );
}

#[test]
fn r227m1_header_03_requires_paideia_parses() {
    let src = "#requires-paideia >= 1.2.3\n\nbody\n";
    let h = parse_header(src).expect("r227m1-header-03: parse must succeed");
    assert_eq!(
        h.requires_paideia,
        Some(Version {
            major: 1,
            minor: 2,
            patch: 3
        }),
        "r227m1-header-03"
    );
}

#[test]
fn r227m1_header_04_malformed_version_rejected() {
    // "1.2" — only two components, not X.Y.Z.
    let src = "#requires-paideia >= 1.2\n\nbody\n";
    let err = parse_header(src).unwrap_err();
    assert!(
        matches!(err, PdsHeaderError::MalformedVersion { .. }),
        "r227m1-header-04: expected MalformedVersion, got {err:?}"
    );
}

#[test]
fn r227m1_header_05_single_import() {
    let src = "#import \"std\" as std\n\nbody\n";
    let h = parse_header(src).expect("r227m1-header-05: parse must succeed");
    assert_eq!(
        h.imports,
        vec![Import {
            path: "std".to_owned(),
            alias: "std".to_owned(),
        }],
        "r227m1-header-05"
    );
}

#[test]
fn r227m1_header_06_multiple_imports() {
    let src = "#import \"lib/util.pds\" as util\n\
               #import \"lib/net.pds\" as net\n\
               #import \"lib/fs.pds\" as fs\n\n\
               body\n";
    let h = parse_header(src).expect("r227m1-header-06: parse must succeed");
    assert_eq!(h.imports.len(), 3, "r227m1-header-06: three imports");
    assert_eq!(h.imports[0].alias, "util", "r227m1-header-06");
    assert_eq!(h.imports[0].path, "lib/util.pds", "r227m1-header-06");
    assert_eq!(h.imports[1].alias, "net", "r227m1-header-06");
    assert_eq!(h.imports[2].alias, "fs", "r227m1-header-06");
}

#[test]
fn r227m1_header_07_schema_with_version() {
    let src = "#schema \"Foo@0.1\"\n\nbody\n";
    let h = parse_header(src).expect("r227m1-header-07: parse must succeed");
    assert_eq!(
        h.schemas,
        vec![SchemaRef {
            name: "Foo".to_owned(),
            version: Some("0.1".to_owned()),
        }],
        "r227m1-header-07: name and version split on `@`"
    );
}

#[test]
fn r227m1_header_08_schema_without_version() {
    let src = "#schema \"Bar\"\n\nbody\n";
    let h = parse_header(src).expect("r227m1-header-08: parse must succeed");
    assert_eq!(
        h.schemas,
        vec![SchemaRef {
            name: "Bar".to_owned(),
            version: None,
        }],
        "r227m1-header-08: unversioned schema"
    );
}

#[test]
fn r227m1_header_09_duplicate_schema_rejected() {
    let src = "#schema \"Foo@0.1\"\n\
               #schema \"Foo@0.2\"\n\n\
               body\n";
    let err = parse_header(src).unwrap_err();
    assert!(
        matches!(&err, PdsHeaderError::DuplicateSchema { schema_name } if schema_name == "Foo"),
        "r227m1-header-09: expected DuplicateSchema for `Foo`, got {err:?}"
    );
}

#[test]
fn r227m1_header_10_ascii_flag_set() {
    let src = "#ascii\n\nbody\n";
    let h = parse_header(src).expect("r227m1-header-10: parse must succeed");
    assert!(h.ascii, "r227m1-header-10: #ascii sets the flag");
}

#[test]
fn r227m1_header_11_unknown_pragma_rejected() {
    let src = "#foobar\n\nbody\n";
    let err = parse_header(src).unwrap_err();
    assert!(
        matches!(&err, PdsHeaderError::UnknownPragma { name, line: 1 } if name == "foobar"),
        "r227m1-header-11: expected UnknownPragma(foobar) on line 1, got {err:?}"
    );
}

#[test]
fn r227m1_header_12_shebang_skipped() {
    let src = "#!/usr/bin/env pds\n\
               #capability \"fs.read\"\n\n\
               body\n";
    let h = parse_header(src).expect("r227m1-header-12: parse must succeed after shebang");
    assert_eq!(
        h.capabilities,
        vec!["fs.read".to_owned()],
        "r227m1-header-12: header pragma after shebang is recognised"
    );
    assert_eq!(&src[h.body_offset..], "body\n", "r227m1-header-12");
}

#[test]
fn r227m1_header_13_full_shape_header() {
    let src = "#capability \"fs.read.home\"\n\
               #requires-paideia >= 0.36.18\n\
               #import \"lib/util.pds\" as util\n\
               #schema \"Backup@1.0\"\n\
               #ascii\n\n\
               body\n";
    let h = parse_header(src).expect("r227m1-header-13: parse must succeed");
    assert_eq!(
        h.capabilities,
        vec!["fs.read.home".to_owned()],
        "r227m1-header-13: capability"
    );
    assert_eq!(
        h.requires_paideia,
        Some(Version {
            major: 0,
            minor: 36,
            patch: 18
        }),
        "r227m1-header-13: version"
    );
    assert_eq!(
        h.imports,
        vec![Import {
            path: "lib/util.pds".to_owned(),
            alias: "util".to_owned(),
        }],
        "r227m1-header-13: import"
    );
    assert_eq!(
        h.schemas,
        vec![SchemaRef {
            name: "Backup".to_owned(),
            version: Some("1.0".to_owned()),
        }],
        "r227m1-header-13: schema"
    );
    assert!(h.ascii, "r227m1-header-13: ascii");
    assert_eq!(&src[h.body_offset..], "body\n", "r227m1-header-13: body");
}

#[test]
fn r227m1_header_14_empty_script_defaults() {
    let src = "";
    let h = parse_header(src).expect("r227m1-header-14: empty must parse");
    assert!(h.capabilities.is_empty(), "r227m1-header-14");
    assert_eq!(h.requires_paideia, None, "r227m1-header-14");
    assert!(h.imports.is_empty(), "r227m1-header-14");
    assert!(h.schemas.is_empty(), "r227m1-header-14");
    assert!(!h.ascii, "r227m1-header-14");
    assert_eq!(
        h.body_offset, 0,
        "r227m1-header-14: body_offset of empty source is 0"
    );
}

#[test]
fn r227m1_header_15_header_only_no_body() {
    let src = "#capability \"fs.read\"\n\
               #ascii\n";
    let h = parse_header(src).expect("r227m1-header-15: header-only must parse");
    assert_eq!(
        h.capabilities,
        vec!["fs.read".to_owned()],
        "r227m1-header-15"
    );
    assert!(h.ascii, "r227m1-header-15");
    assert_eq!(
        h.body_offset,
        src.len(),
        "r227m1-header-15: body_offset equals src.len() when no body follows"
    );
}
