//! R225.M8 LSP-hover fixture corpus.
//!
//! Eight tests, tagged `r225m8-hov-01`..`r225m8-hov-08`. The corpus
//! partitions into two halves:
//!
//! * `01..05` — the [`hover_at`] lookup semantics: empty cache, single
//!   hit, out-of-range miss, nested-span narrowest-wins, and disjoint
//!   sibling spans.
//! * `06..08` — the [`render_hover`] presentation: a plain constant, a
//!   record row, and a `TypedValue` (record row paired with an effect
//!   row).
//!
//! Fixtures 06–08 construct [`MonoType`] values directly rather than
//! routing through inference — the M8 layer's contract is a display
//! surface over whatever monotype the caller hands it, and its
//! rendering delegates to `MonoType`'s existing `Display` impl. That
//! `Display` grammar is exhaustively exercised by the R225.M2–M5
//! corpora already; the assertions here check only the framing (label
//! plus separator, brace-delimited row and typed-value shapes).

use std::collections::{BTreeMap, HashMap};

use paideia_as_shell_hm::{
    hover_at, render_hover, EffectRow, HoverEntry, MonoType, RowType, TypeCache, TypeSpan,
    TypeVar, TypedValue,
};

// Small helper: build a HoverEntry without re-typing the field names in
// every fixture. Kept local to the test file so the crate surface stays
// exactly what the module docs promise.
fn entry(start: usize, end: usize, label: &str, mono: MonoType) -> HoverEntry {
    HoverEntry {
        span: TypeSpan::new(start, end),
        mono,
        label: label.to_owned(),
    }
}

// ---------------------------------------------------------------------
// 01 — empty cache: any lookup returns None.
// ---------------------------------------------------------------------

#[test]
fn r225m8_hov_01_empty_cache_returns_none() {
    let cache = TypeCache::new();
    assert!(
        cache.is_empty(),
        "r225m8-hov-01: fresh cache is empty"
    );
    assert_eq!(cache.len(), 0, "r225m8-hov-01: len is zero");
    assert!(
        hover_at(&cache, 10).is_none(),
        "r225m8-hov-01: lookup on empty cache is None"
    );
}

// ---------------------------------------------------------------------
// 02 — single entry, cursor inside the span: Some(entry) with the
// stored mono and label.
// ---------------------------------------------------------------------

#[test]
fn r225m8_hov_02_single_entry_inside_span() {
    let mut cache = TypeCache::new();
    cache.push(entry(4, 12, "greeting", MonoType::Con("Str".to_owned())));
    assert_eq!(cache.len(), 1, "r225m8-hov-02: one entry pushed");
    let hit = hover_at(&cache, 7).expect("r225m8-hov-02: pos 7 lies inside [4,12)");
    assert_eq!(hit.label, "greeting", "r225m8-hov-02: label preserved");
    assert_eq!(
        hit.mono,
        MonoType::Con("Str".to_owned()),
        "r225m8-hov-02: mono preserved"
    );
    assert_eq!(hit.span.start, 4, "r225m8-hov-02: span start");
    assert_eq!(hit.span.end, 12, "r225m8-hov-02: span end");
}

// ---------------------------------------------------------------------
// 03 — cursor outside every span: None. Also exercises the half-open
// upper edge — pos == end must not match a span that ends there.
// ---------------------------------------------------------------------

#[test]
fn r225m8_hov_03_pos_outside_all_spans() {
    let mut cache = TypeCache::new();
    cache.push(entry(0, 5, "lhs", MonoType::Con("Int".to_owned())));
    cache.push(entry(10, 15, "rhs", MonoType::Con("Int".to_owned())));
    // pos 7 is in the gap between the two spans.
    assert!(
        hover_at(&cache, 7).is_none(),
        "r225m8-hov-03: pos in gap returns None"
    );
    // pos 15 is exactly at the exclusive end of the second span — still
    // outside per half-open semantics.
    assert!(
        hover_at(&cache, 15).is_none(),
        "r225m8-hov-03: pos at exclusive end returns None"
    );
    // pos 20 is past every span.
    assert!(
        hover_at(&cache, 20).is_none(),
        "r225m8-hov-03: pos past every span returns None"
    );
}

// ---------------------------------------------------------------------
// 04 — nested spans: the tighter enclosing span wins. Outer [0,20)
// covers the inner [5,10); a cursor at 7 must resolve to the inner
// entry, regardless of insertion order.
// ---------------------------------------------------------------------

#[test]
fn r225m8_hov_04_nested_spans_narrower_wins() {
    let mut cache = TypeCache::new();
    // Push the outer entry FIRST so a naive "first match" lookup would
    // return it — the smallest-enclosing policy must override that.
    cache.push(entry(0, 20, "outer", MonoType::Con("Int".to_owned())));
    cache.push(entry(5, 10, "inner", MonoType::Con("Str".to_owned())));
    let hit = hover_at(&cache, 7).expect("r225m8-hov-04: pos 7 lies inside both spans");
    assert_eq!(
        hit.label, "inner",
        "r225m8-hov-04: narrower span wins the tie"
    );
    assert_eq!(
        hit.mono,
        MonoType::Con("Str".to_owned()),
        "r225m8-hov-04: inner mono returned"
    );
    // Sanity: a cursor at 2 (inside outer, outside inner) still hits
    // the outer entry.
    let hit_outer = hover_at(&cache, 2)
        .expect("r225m8-hov-04: pos 2 lies inside outer but not inner");
    assert_eq!(hit_outer.label, "outer", "r225m8-hov-04: outer as fallback");
}

// ---------------------------------------------------------------------
// 05 — non-overlapping sibling spans: the cursor picks the enclosing
// one. A cursor at 12 must resolve to the second span, not the first.
// ---------------------------------------------------------------------

#[test]
fn r225m8_hov_05_disjoint_siblings_route_by_position() {
    let mut cache = TypeCache::new();
    cache.push(entry(0, 5, "left", MonoType::Con("Int".to_owned())));
    cache.push(entry(10, 15, "right", MonoType::Con("Str".to_owned())));
    let hit = hover_at(&cache, 12).expect("r225m8-hov-05: pos 12 lies inside [10,15)");
    assert_eq!(hit.label, "right", "r225m8-hov-05: right sibling selected");
    assert_eq!(
        hit.mono,
        MonoType::Con("Str".to_owned()),
        "r225m8-hov-05: right mono returned"
    );
    // Symmetrically, pos 3 hits the left sibling.
    let hit_left = hover_at(&cache, 3).expect("r225m8-hov-05: pos 3 lies inside [0,5)");
    assert_eq!(hit_left.label, "left", "r225m8-hov-05: left sibling selected");
}

// ---------------------------------------------------------------------
// 06 — render_hover for a plain type constant: `"<label>: <con>"`.
// MonoType::Con(name) displays as `name` (see crate::ty::MonoType's
// Display impl), so an Int-typed `x` renders as exactly `"x: Int"`.
// ---------------------------------------------------------------------

#[test]
fn r225m8_hov_06_render_constant() {
    let e = entry(0, 1, "x", MonoType::Con("Int".to_owned()));
    let rendered = render_hover(&e);
    assert_eq!(
        rendered, "x: Int",
        "r225m8-hov-06: exact rendering of label + Con"
    );
}

// ---------------------------------------------------------------------
// 07 — render_hover for a record row: the type half is brace-delimited
// (the R225.M2 Display shape). We check the framing and the presence
// of the record's field name / type text, not the exact spacing —
// that grammar is fully covered by the row_types corpus.
// ---------------------------------------------------------------------

#[test]
fn r225m8_hov_07_render_record_row() {
    // Build `{ name: Str }` — one field, closed row.
    let mut fields: HashMap<String, MonoType> = HashMap::new();
    fields.insert("name".to_owned(), MonoType::Con("Str".to_owned()));
    let row = RowType::from_map(fields, None);
    let e = entry(0, 12, "rec", MonoType::Record(row));
    let rendered = render_hover(&e);
    assert!(
        rendered.starts_with("rec: "),
        "r225m8-hov-07: label prefix, got {rendered}"
    );
    assert!(
        rendered.contains('{') && rendered.contains('}'),
        "r225m8-hov-07: record braces present, got {rendered}"
    );
    assert!(
        rendered.contains("name"),
        "r225m8-hov-07: field name present, got {rendered}"
    );
    assert!(
        rendered.contains("Str"),
        "r225m8-hov-07: field type present, got {rendered}"
    );
}

// ---------------------------------------------------------------------
// 08 — render_hover for a TypedValue: both the value-row and the
// effect-row halves must be visible. The R225.M5 Display shape is
// `"{value ! effect}"`; we check the framing plus one witness from
// each half.
// ---------------------------------------------------------------------

#[test]
fn r225m8_hov_08_render_typed_value() {
    // Value row: `{ payload: Int }`.
    let mut v_fields: HashMap<String, MonoType> = HashMap::new();
    v_fields.insert("payload".to_owned(), MonoType::Con("Int".to_owned()));
    let value_row = RowType::from_map(v_fields, None);

    // Effect row: `!{ io }` (pure-tag; Unit payload elides at Display).
    let mut effs: BTreeMap<String, MonoType> = BTreeMap::new();
    effs.insert("io".to_owned(), MonoType::Con("Unit".to_owned()));
    let effect_row = EffectRow {
        present: effs,
        tail: None,
    };
    // TypeVar import kept live: a caller wiring a row-var into the
    // effect tail uses this same builder shape.
    let _tail_marker: Option<TypeVar> = None;

    let tv = TypedValue::with_effect(value_row, effect_row);
    let e = entry(0, 20, "tv", MonoType::Typed(Box::new(tv)));
    let rendered = render_hover(&e);
    assert!(
        rendered.starts_with("tv: "),
        "r225m8-hov-08: label prefix, got {rendered}"
    );
    assert!(
        rendered.contains('{') && rendered.contains('}'),
        "r225m8-hov-08: brace framing, got {rendered}"
    );
    assert!(
        rendered.contains("payload"),
        "r225m8-hov-08: value-row field visible, got {rendered}"
    );
    assert!(
        rendered.contains("Int"),
        "r225m8-hov-08: value-row type visible, got {rendered}"
    );
    assert!(
        rendered.contains("io"),
        "r225m8-hov-08: effect label visible, got {rendered}"
    );
    assert!(
        rendered.contains('!'),
        "r225m8-hov-08: TypedValue separator `!` visible, got {rendered}"
    );
}
