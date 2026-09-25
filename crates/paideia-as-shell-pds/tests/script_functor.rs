//! R227.M5 script-as-functor fixture corpus.
//!
//! 8 unit tests tagged `r227m5-func-NN`, exercising
//! `paideia_as_shell_pds::make_functor` and `paideia_as_shell_pds::apply`.
//! Test 08 is the end-to-end fixture that walks `parse_header` →
//! `make_functor` → `apply`.

use std::path::PathBuf;

use paideia_as_shell_pds::{
    apply, make_functor, parse_header, ApplyError, PdsHeader,
};

/// Convenience: build a `PdsHeader` with only the `capabilities` field
/// populated. R227.M5 does not consult any other header field, so the
/// remaining fields stay at their `Default` values.
fn header_with_caps(caps: &[&str]) -> PdsHeader {
    PdsHeader {
        capabilities: caps.iter().map(|s| (*s).to_owned()).collect(),
        ..PdsHeader::default()
    }
}

/// Convenience: `&[&str]` → `Vec<String>` for the resolved-cap arg to
/// [`apply`].
fn resolved(caps: &[&str]) -> Vec<String> {
    caps.iter().map(|s| (*s).to_owned()).collect()
}

// ────────────────────────────────────────────────────────────────
// r227m5-func-01 .. r227m5-func-03 — make_functor: arity
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m5_func_01_empty_capability_yields_zero_arity() {
    let h = header_with_caps(&[]);
    let path = PathBuf::from("/tmp/r227m5-func-01.pds");
    let f = make_functor(&h, b"body-01\n", path.clone());
    assert_eq!(
        f.params.len(),
        0,
        "r227m5-func-01: empty header ⇒ zero-arity functor"
    );
    assert!(
        f.params.is_empty(),
        "r227m5-func-01: params vector is empty, not merely zero-length via arithmetic"
    );
    assert_eq!(
        f.body,
        b"body-01\n".to_vec(),
        "r227m5-func-01: body bytes are cloned verbatim"
    );
    assert_eq!(
        f.source_path, path,
        "r227m5-func-01: source_path preserved on the zero-arity path"
    );
}

#[test]
fn r227m5_func_02_single_cap_yields_one_param() {
    let h = header_with_caps(&["fs.read.home"]);
    let path = PathBuf::from("/tmp/r227m5-func-02.pds");
    let f = make_functor(&h, b"body-02\n", path);
    assert_eq!(
        f.params.len(),
        1,
        "r227m5-func-02: one cap ⇒ one-arity functor"
    );
    assert_eq!(
        f.params[0].name, "p0",
        "r227m5-func-02: positional name is `p0`"
    );
    assert_eq!(
        f.params[0].cap_ident, "fs.read.home",
        "r227m5-func-02: cap_ident is the header string verbatim"
    );
}

#[test]
fn r227m5_func_03_three_caps_yields_three_params_in_order() {
    let h = header_with_caps(&["fs.read.home", "net.dial", "sys.time"]);
    let path = PathBuf::from("/tmp/r227m5-func-03.pds");
    let f = make_functor(&h, b"body-03\n", path);
    assert_eq!(
        f.params.len(),
        3,
        "r227m5-func-03: three caps ⇒ three-arity functor"
    );
    let names: Vec<&str> = f.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["p0", "p1", "p2"],
        "r227m5-func-03: positional names are `p0`, `p1`, `p2` in header order"
    );
    let idents: Vec<&str> =
        f.params.iter().map(|p| p.cap_ident.as_str()).collect();
    assert_eq!(
        idents,
        vec!["fs.read.home", "net.dial", "sys.time"],
        "r227m5-func-03: cap_idents preserve header-declaration order"
    );
}

// ────────────────────────────────────────────────────────────────
// r227m5-func-04 .. r227m5-func-06 — apply: bindings + arity
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m5_func_04_apply_exact_match_ok_bindings() {
    let h = header_with_caps(&["fs.read.home", "net.dial"]);
    let f = make_functor(&h, b"body-04\n", PathBuf::from("/tmp/r227m5-func-04.pds"));
    let caps = resolved(&["fs.read.home", "net.dial"]);
    let app =
        apply(f, &caps).expect("r227m5-func-04: exact-arity apply must succeed");
    assert_eq!(
        app.bindings.len(),
        2,
        "r227m5-func-04: bindings length equals param arity"
    );
    assert_eq!(
        app.bindings,
        vec![
            ("p0".to_owned(), "fs.read.home".to_owned()),
            ("p1".to_owned(), "net.dial".to_owned()),
        ],
        "r227m5-func-04: bindings pair positional name with resolved cap in order"
    );
    assert_eq!(
        app.functor.params.len(),
        2,
        "r227m5-func-04: functor is preserved on the application"
    );
}

#[test]
fn r227m5_func_05_apply_fewer_caps_arity_mismatch() {
    let h = header_with_caps(&["fs.read.home", "net.dial", "sys.time"]);
    let f = make_functor(&h, b"body-05\n", PathBuf::from("/tmp/r227m5-func-05.pds"));
    let caps = resolved(&["fs.read.home", "net.dial"]);
    let err = apply(f, &caps).expect_err("r227m5-func-05: under-arity must fail");
    match err {
        ApplyError::ArityMismatch { expected, got } => {
            assert_eq!(
                expected, 3,
                "r227m5-func-05: expected == functor.params.len()"
            );
            assert_eq!(got, 2, "r227m5-func-05: got == resolved_caps.len()");
        }
        other => panic!("r227m5-func-05: wrong ApplyError variant: {other:?}"),
    }
}

#[test]
fn r227m5_func_06_apply_more_caps_arity_mismatch() {
    let h = header_with_caps(&["fs.read.home"]);
    let f = make_functor(&h, b"body-06\n", PathBuf::from("/tmp/r227m5-func-06.pds"));
    let caps = resolved(&["fs.read.home", "net.dial", "sys.time"]);
    let err = apply(f, &caps).expect_err("r227m5-func-06: over-arity must fail");
    match err {
        ApplyError::ArityMismatch { expected, got } => {
            assert_eq!(
                expected, 1,
                "r227m5-func-06: expected == functor.params.len()"
            );
            assert_eq!(got, 3, "r227m5-func-06: got == resolved_caps.len()");
        }
        other => panic!("r227m5-func-06: wrong ApplyError variant: {other:?}"),
    }
}

// ────────────────────────────────────────────────────────────────
// r227m5-func-07 — source_path preservation through apply
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m5_func_07_source_path_preserved_through_apply() {
    let h = header_with_caps(&["fs.read.home"]);
    let path = PathBuf::from("/tmp/r227m5-func-07/script.pds");
    let f = make_functor(&h, b"body-07\n", path.clone());
    // Sanity: make_functor itself preserves the path.
    assert_eq!(
        f.source_path, path,
        "r227m5-func-07: make_functor preserves source_path"
    );
    let caps = resolved(&["fs.read.home"]);
    let app =
        apply(f, &caps).expect("r227m5-func-07: exact-arity apply must succeed");
    assert_eq!(
        app.functor.source_path, path,
        "r227m5-func-07: FunctorApplication.functor.source_path == make_functor's path"
    );
    // Body bytes also come through unchanged, so downstream consumers
    // can reach the body via the application alone.
    assert_eq!(
        app.functor.body,
        b"body-07\n".to_vec(),
        "r227m5-func-07: body bytes survive apply"
    );
}

// ────────────────────────────────────────────────────────────────
// r227m5-func-08 — end-to-end: parse_header → make_functor → apply
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m5_func_08_end_to_end_parse_make_apply() {
    let src = "#capability \"cap1\"\n\
               #capability \"cap2\"\n\n\
               body-08\n";
    let header =
        parse_header(src).expect("r227m5-func-08: parse must succeed");
    assert_eq!(
        header.capabilities,
        vec!["cap1".to_owned(), "cap2".to_owned()],
        "r227m5-func-08: header parsed two capabilities in order"
    );
    let body = &src.as_bytes()[header.body_offset..];
    let path = PathBuf::from("/tmp/r227m5-func-08.pds");
    let f = make_functor(&header, body, path.clone());
    assert_eq!(
        f.params.len(),
        2,
        "r227m5-func-08: functor is two-arity"
    );
    assert_eq!(
        f.body,
        b"body-08\n".to_vec(),
        "r227m5-func-08: functor body matches the slice past body_offset"
    );

    let caps = resolved(&["cap1", "cap2"]);
    let app =
        apply(f, &caps).expect("r227m5-func-08: exact-arity apply must succeed");
    assert_eq!(
        app.bindings,
        vec![
            ("p0".to_owned(), "cap1".to_owned()),
            ("p1".to_owned(), "cap2".to_owned()),
        ],
        "r227m5-func-08: bindings pair `p0`→cap1, `p1`→cap2"
    );
    assert_eq!(
        app.functor.source_path, path,
        "r227m5-func-08: source_path survives the full pipeline"
    );
    // cap_ident on each param preserves the header string, per the
    // R227.M5 → R227.M6 handoff contract.
    assert_eq!(
        app.functor.params[0].cap_ident, "cap1",
        "r227m5-func-08: params[0].cap_ident survives"
    );
    assert_eq!(
        app.functor.params[1].cap_ident, "cap2",
        "r227m5-func-08: params[1].cap_ident survives"
    );
}
