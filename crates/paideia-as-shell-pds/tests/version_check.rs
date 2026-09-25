//! R227.M6 `#requires-paideia` version-compatibility fixture corpus.
//!
//! 8 tests, tagged `r227m6-ver-NN`, exercising the tuple-order `>=`
//! semantics of `paideia_as_shell_pds::check_requires_paideia`,
//! `Version::is_at_least`, and `PdsHeader::check_version`; the final
//! fixture is an end-to-end pair that runs `parse_header` followed by
//! `check_version` for both the pass and fail paths, confirming the
//! header/checker seam parallel to R227.M2's cap-check integration.

use paideia_as_shell_pds::{
    check_requires_paideia, parse_header, PdsHeader, Version, VersionCheckError,
    SYSTEM_VERSION,
};

/// Convenience: build a header that carries only a `#requires-paideia`
/// pin, at the given `(major, minor, patch)`.
///
/// The tests below never care about capabilities, imports, schemas,
/// or `body_offset` — only `requires_paideia` — so building the
/// header directly keeps the fixtures noise-free and independent of
/// the M1 parser's surface syntax (which is exercised by its own
/// 15-test corpus).
fn header_pinning(major: u32, minor: u32, patch: u32) -> PdsHeader {
    PdsHeader {
        requires_paideia: Some(Version {
            major,
            minor,
            patch,
        }),
        ..PdsHeader::default()
    }
}

// ────────────────────────────────────────────────────────────────
// r227m6-ver-01 .. r227m6-ver-04 — passing cases
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m6_ver_01_no_requires_pragma_ok() {
    // A header without `#requires-paideia` at all — the pragma is
    // opt-in, so the check must pass trivially.
    let header = PdsHeader::default();
    assert_eq!(
        header.requires_paideia, None,
        "r227m6-ver-01: sanity — default header has no pin"
    );
    let result = check_requires_paideia(&header);
    assert!(
        result.is_ok(),
        "r227m6-ver-01: no pin ⇒ nothing to check, got {result:?}"
    );
    // Method form parallels the free function.
    assert!(
        header.check_version().is_ok(),
        "r227m6-ver-01: PdsHeader::check_version agrees with the free fn"
    );
}

#[test]
fn r227m6_ver_02_exact_match_ok() {
    // A pin at exactly SYSTEM_VERSION — the tuple `>=` says yes.
    let header = header_pinning(
        SYSTEM_VERSION.major,
        SYSTEM_VERSION.minor,
        SYSTEM_VERSION.patch,
    );
    let result = check_requires_paideia(&header);
    assert!(
        result.is_ok(),
        "r227m6-ver-02: exact match satisfies `>=`, got {result:?}"
    );
    // Version::is_at_least is the load-bearing primitive — pin it.
    let pin = header.requires_paideia.expect("r227m6-ver-02: pin present");
    assert!(
        SYSTEM_VERSION.is_at_least(&pin),
        "r227m6-ver-02: SYSTEM_VERSION is_at_least equal version"
    );
}

#[test]
fn r227m6_ver_03_lower_minor_ok() {
    // A pin one minor lower than SYSTEM_VERSION with a very high
    // patch (0.35.99) — even the maxed-out patch cannot lift the
    // pin above SYSTEM_VERSION under tuple ordering. Guards against a
    // future refactor that accidentally compares patch first.
    let header = header_pinning(0, 35, 99);
    let result = check_requires_paideia(&header);
    assert!(
        result.is_ok(),
        "r227m6-ver-03: lower minor wins under tuple order, got {result:?}"
    );
    assert!(
        SYSTEM_VERSION.is_at_least(&Version {
            major: 0,
            minor: 35,
            patch: 99,
        }),
        "r227m6-ver-03: Version::is_at_least agrees"
    );
}

#[test]
fn r227m6_ver_04_lower_patch_ok() {
    // A pin one patch lower than SYSTEM_VERSION — a fresh script
    // pinning the immediately-previous release must still load on the
    // current system.
    let header = header_pinning(
        SYSTEM_VERSION.major,
        SYSTEM_VERSION.minor,
        SYSTEM_VERSION.patch - 1,
    );
    let result = check_requires_paideia(&header);
    assert!(
        result.is_ok(),
        "r227m6-ver-04: lower patch is compatible, got {result:?}"
    );
}

// ────────────────────────────────────────────────────────────────
// r227m6-ver-05 .. r227m6-ver-07 — failure cases
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m6_ver_05_higher_major_err() {
    // A pin at the next major — 1.0.0 outranks 0.x.y regardless of
    // how high the minor or patch climb.
    let header = header_pinning(1, 0, 0);
    let err = check_requires_paideia(&header)
        .expect_err("r227m6-ver-05: must fail");
    match err {
        VersionCheckError::VersionTooLow { required, actual } => {
            assert_eq!(
                required,
                Version {
                    major: 1,
                    minor: 0,
                    patch: 0,
                },
                "r227m6-ver-05: required echoes the pinned demand"
            );
            assert_eq!(
                actual, SYSTEM_VERSION,
                "r227m6-ver-05: actual echoes SYSTEM_VERSION"
            );
        }
    }
    // Symmetric cross-check on the primitive.
    assert!(
        !SYSTEM_VERSION.is_at_least(&Version {
            major: 1,
            minor: 0,
            patch: 0,
        }),
        "r227m6-ver-05: Version::is_at_least refuses higher major"
    );
}

#[test]
fn r227m6_ver_06_higher_minor_err() {
    // A pin one minor above SYSTEM_VERSION with a zeroed patch —
    // still strictly greater, still refused.
    let header = header_pinning(
        SYSTEM_VERSION.major,
        SYSTEM_VERSION.minor + 1,
        0,
    );
    let err = check_requires_paideia(&header)
        .expect_err("r227m6-ver-06: must fail");
    match err {
        VersionCheckError::VersionTooLow { required, actual } => {
            assert_eq!(
                required,
                Version {
                    major: SYSTEM_VERSION.major,
                    minor: SYSTEM_VERSION.minor + 1,
                    patch: 0,
                },
                "r227m6-ver-06: required echoes the pinned demand"
            );
            assert_eq!(
                actual, SYSTEM_VERSION,
                "r227m6-ver-06: actual echoes SYSTEM_VERSION"
            );
        }
    }
}

#[test]
fn r227m6_ver_07_higher_patch_err() {
    // A pin one patch above SYSTEM_VERSION — the tightest possible
    // failure, guarding against off-by-one in the tuple compare.
    let header = header_pinning(
        SYSTEM_VERSION.major,
        SYSTEM_VERSION.minor,
        SYSTEM_VERSION.patch + 1,
    );
    let err = check_requires_paideia(&header)
        .expect_err("r227m6-ver-07: must fail");
    match err {
        VersionCheckError::VersionTooLow { required, actual } => {
            assert_eq!(
                required,
                Version {
                    major: SYSTEM_VERSION.major,
                    minor: SYSTEM_VERSION.minor,
                    patch: SYSTEM_VERSION.patch + 1,
                },
                "r227m6-ver-07: required echoes the pinned demand"
            );
            assert_eq!(
                actual, SYSTEM_VERSION,
                "r227m6-ver-07: actual echoes SYSTEM_VERSION"
            );
        }
    }
}

// ────────────────────────────────────────────────────────────────
// Integration — parse_header → PdsHeader::check_version
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m6_ver_08_parse_then_check() {
    // Pass path: a script pinning one patch below SYSTEM_VERSION goes
    // through `parse_header` and then `check_version` without a hitch.
    let pass_src = format!(
        "#requires-paideia >= {}.{}.{}\n\nbody\n",
        SYSTEM_VERSION.major,
        SYSTEM_VERSION.minor,
        SYSTEM_VERSION.patch - 1,
    );
    let pass_header = parse_header(&pass_src)
        .expect("r227m6-ver-08: parse of pass fixture must succeed");
    assert_eq!(
        pass_header.requires_paideia,
        Some(Version {
            major: SYSTEM_VERSION.major,
            minor: SYSTEM_VERSION.minor,
            patch: SYSTEM_VERSION.patch - 1,
        }),
        "r227m6-ver-08: parser captured the pin",
    );
    pass_header
        .check_version()
        .expect("r227m6-ver-08: pass fixture must satisfy check_version");

    // Fail path: a script pinning the next major goes through
    // `parse_header` and is refused by `check_version` with the
    // pinned demand echoed back in `required`.
    let fail_src = format!(
        "#requires-paideia >= {}.0.0\n\nbody\n",
        SYSTEM_VERSION.major + 1,
    );
    let fail_header = parse_header(&fail_src)
        .expect("r227m6-ver-08: parse of fail fixture must succeed");
    let err = fail_header
        .check_version()
        .expect_err("r227m6-ver-08: fail fixture must be refused");
    match err {
        VersionCheckError::VersionTooLow { required, actual } => {
            assert_eq!(
                required,
                Version {
                    major: SYSTEM_VERSION.major + 1,
                    minor: 0,
                    patch: 0,
                },
                "r227m6-ver-08: required echoes the pinned demand end-to-end"
            );
            assert_eq!(
                actual, SYSTEM_VERSION,
                "r227m6-ver-08: actual echoes SYSTEM_VERSION end-to-end"
            );
        }
    }
}
