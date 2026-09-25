//! R228.M2 Candidate enrichment (type_hint) + 1-level Field
//! completion — fixture corpus.
//!
//! Each test is fingerprinted `r228m2-cmp-NN` so a git-log grep
//! against the CHANGELOG-r228m2 fragment lands on the exact fixture
//! that motivated a change. The fingerprints match the 8-fixture
//! plan in paideia-as issue #1470.

use std::collections::HashMap;

use paideia_as_shell_completion::{
    CandidateKind, CompletionEngine, CompletionRequest, complete,
};

// r228m2-cmp-01 -- Command candidate with a matching type-hint map
// entry surfaces `type_hint = Some("Fn(Path -> [Path])")`.
#[test]
fn r228m2_cmp_01_command_type_hint_populated() {
    let mut types: HashMap<String, String> = HashMap::new();
    types.insert("ls".to_string(), "Fn(Path -> [Path])".to_string());
    let engine = CompletionEngine::with_lists(vec!["ls".to_string()], Vec::new())
        .with_command_types(types);
    let req = CompletionRequest {
        source: "l".to_string(),
        cursor_byte: 1,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 1, "expected the sole `ls` command");
    let cand = &resp.candidates[0];
    assert_eq!(cand.text, "ls");
    assert_eq!(cand.kind, CandidateKind::Command);
    assert_eq!(cand.type_hint.as_deref(), Some("Fn(Path -> [Path])"));
}

// r228m2-cmp-02 -- Command candidate without a matching type-hint
// map entry leaves `type_hint = None`.
#[test]
fn r228m2_cmp_02_command_type_hint_missing() {
    let engine = CompletionEngine::with_lists(vec!["ls".to_string()], Vec::new());
    let req = CompletionRequest {
        source: "l".to_string(),
        cursor_byte: 1,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 1);
    let cand = &resp.candidates[0];
    assert_eq!(cand.text, "ls");
    assert_eq!(cand.kind, CandidateKind::Command);
    assert_eq!(cand.type_hint, None);
}

// r228m2-cmp-03 -- `foo.` with records={foo:[x,y,z]} produces three
// Field candidates as an insertion at the cursor.
#[test]
fn r228m2_cmp_03_field_all_after_dot() {
    let mut records: HashMap<String, Vec<String>> = HashMap::new();
    records.insert(
        "foo".to_string(),
        vec!["x".to_string(), "y".to_string(), "z".to_string()],
    );
    let engine = CompletionEngine::empty().with_records(records);
    let req = CompletionRequest {
        source: "foo.".to_string(),
        cursor_byte: 4,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 3, "expected all three fields");
    let texts: Vec<&str> = resp.candidates.iter().map(|c| c.text.as_str()).collect();
    assert!(texts.contains(&"x"));
    assert!(texts.contains(&"y"));
    assert!(texts.contains(&"z"));
    // Insertion at the cursor -- nothing to overwrite.
    assert_eq!(resp.prefix_start, 4);
    assert_eq!(resp.prefix_end, 4);
}

// r228m2-cmp-04 -- `foo.xy` with records={foo:[xyz, xww]} filters
// to the single `xyz` match and names the `xy` span as the overwrite
// range.
#[test]
fn r228m2_cmp_04_field_prefix_filter() {
    let mut records: HashMap<String, Vec<String>> = HashMap::new();
    records.insert(
        "foo".to_string(),
        vec!["xyz".to_string(), "xww".to_string()],
    );
    let engine = CompletionEngine::empty().with_records(records);
    let req = CompletionRequest {
        source: "foo.xy".to_string(),
        cursor_byte: 6,
    };
    let resp = complete(&engine, &req);
    assert_eq!(resp.candidates.len(), 1, "only xyz matches `xy`");
    let cand = &resp.candidates[0];
    assert_eq!(cand.text, "xyz");
    assert_eq!(cand.kind, CandidateKind::Field);
    // Overwrite the `xy` token span.
    assert_eq!(resp.prefix_start, 4);
    assert_eq!(resp.prefix_end, 6);
}

// r228m2-cmp-05 -- `bar.` with records={foo:[x]} but no `bar` entry
// produces zero candidates (record name miss).
#[test]
fn r228m2_cmp_05_unknown_record_empty() {
    let mut records: HashMap<String, Vec<String>> = HashMap::new();
    records.insert("foo".to_string(), vec!["x".to_string()]);
    let engine = CompletionEngine::empty().with_records(records);
    let req = CompletionRequest {
        source: "bar.".to_string(),
        cursor_byte: 4,
    };
    let resp = complete(&engine, &req);
    assert!(
        resp.candidates.is_empty(),
        "unknown record must yield no candidates; got {:?}",
        resp.candidates
    );
}

// r228m2-cmp-06 -- Field candidates carry `kind: CandidateKind::Field`
// (verified across both dispatch shapes: insertion after Dot AND
// overwrite of a prefix Ident).
#[test]
fn r228m2_cmp_06_field_kind_tag() {
    let mut records: HashMap<String, Vec<String>> = HashMap::new();
    records.insert(
        "rec".to_string(),
        vec!["alpha".to_string(), "beta".to_string()],
    );
    let engine = CompletionEngine::empty().with_records(records);

    // Insertion shape.
    let resp_a = complete(
        &engine,
        &CompletionRequest {
            source: "rec.".to_string(),
            cursor_byte: 4,
        },
    );
    assert_eq!(resp_a.candidates.len(), 2);
    for cand in &resp_a.candidates {
        assert_eq!(cand.kind, CandidateKind::Field);
    }

    // Overwrite shape.
    let resp_b = complete(
        &engine,
        &CompletionRequest {
            source: "rec.al".to_string(),
            cursor_byte: 6,
        },
    );
    assert_eq!(resp_b.candidates.len(), 1);
    assert_eq!(resp_b.candidates[0].kind, CandidateKind::Field);
    assert_eq!(resp_b.candidates[0].text, "alpha");
}

// r228m2-cmp-07 -- Chained `foo.bar.` returns empty even when `foo`
// is in the records map. Nested lookup is deferred to R228.M4.
#[test]
fn r228m2_cmp_07_chained_lookup_deferred() {
    let mut records: HashMap<String, Vec<String>> = HashMap::new();
    records.insert("foo".to_string(), vec!["bar".to_string()]);
    let engine = CompletionEngine::empty().with_records(records);
    let req = CompletionRequest {
        source: "foo.bar.".to_string(),
        cursor_byte: 8,
    };
    let resp = complete(&engine, &req);
    assert!(
        resp.candidates.is_empty(),
        "chained lookup deferred; got {:?}",
        resp.candidates
    );
}

// r228m2-cmp-08 -- Empty records catalogue never emits Field
// candidates, regardless of source shape.
#[test]
fn r228m2_cmp_08_empty_records_never_emits_fields() {
    let engine = CompletionEngine::empty();
    for source in ["foo.", "foo.x", "foo.bar."] {
        let req = CompletionRequest {
            source: source.to_string(),
            cursor_byte: source.len(),
        };
        let resp = complete(&engine, &req);
        for cand in &resp.candidates {
            assert_ne!(
                cand.kind,
                CandidateKind::Field,
                "empty records must never yield a Field candidate; source={source:?}"
            );
        }
    }
}
