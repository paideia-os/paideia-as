//! UNI-3 (paideia-as#1590) — UAX #14 line-break + `wrap_at_width`
//! fingerprinted conformance tests.
//!
//! Each test carries a `uni3-seg-NN` fingerprint tag so the debugger
//! can correlate test-binary output against the UNI-3 acceptance line
//! item per `feedback_workerbee_verify_claims.md`.
//!
//! # Offset convention
//!
//! `line_break_opportunities` and `next_line_break` use
//! `unicode-linebreak`'s native convention: the returned byte offset is
//! the index of the *first character of the next line* were the break
//! taken. In particular, `s.split_at(opp.byte_offset)` is a valid
//! UTF-8-boundary split, and the end-of-string sentinel opportunity has
//! `byte_offset == s.len()`.
//!
//! For `"hello world"` this makes the Allowed opportunity between the
//! space and `'w'` be at byte 6 (where `'w'` begins). For `"ab cd ef"`
//! the first Allowed opportunity is at byte 3 (where `'c'` begins).
//! See the module header of `src/segment.rs` for the full rationale.

use paideia_as_unicode::{
    LineBreak, line_break_opportunities, next_line_break, wrap_at_width,
};

// ---------------------------------------------------------------------
// 1. Line-break opportunity enumeration
// ---------------------------------------------------------------------

#[test]
fn uni3_seg_01_mandatory_lf_in_middle() {
    // "foo\nbar": LF (UAX #14 BK class) forces a mandatory break after
    // the newline. The offset is 4 — the index of 'b' (character
    // following the break). EOS is also Mandatory at byte 7.
    let opps = line_break_opportunities("foo\nbar");
    assert!(!opps.is_empty(), "LF must produce at least one opp");
    // First opportunity after the LF.
    assert_eq!(opps[0].byte_offset, 4);
    assert_eq!(opps[0].kind, LineBreak::Mandatory);
    // Last opportunity is the EOS Mandatory sentinel.
    let last = opps.last().expect("non-empty");
    assert_eq!(last.byte_offset, 7);
    assert_eq!(last.kind, LineBreak::Mandatory);
}

#[test]
fn uni3_seg_02_crlf_single_mandatory() {
    // "foo\r\nbar": CR LF is one break (UAX #14 LB5). The offset after
    // the LF is 5. EOS Mandatory at byte 8.
    let opps = line_break_opportunities("foo\r\nbar");
    // Count Mandatory opportunities *before* the EOS sentinel: should
    // be exactly one (not two — CRLF is a single break).
    let mids: Vec<_> = opps
        .iter()
        .filter(|o| o.byte_offset < "foo\r\nbar".len())
        .collect();
    assert_eq!(mids.len(), 1, "CRLF must collapse to a single break: {:?}", opps);
    assert_eq!(mids[0].byte_offset, 5);
    assert_eq!(mids[0].kind, LineBreak::Mandatory);
    // EOS sentinel.
    let last = opps.last().expect("non-empty");
    assert_eq!(last.byte_offset, 8);
    assert_eq!(last.kind, LineBreak::Mandatory);
}

#[test]
fn uni3_seg_03_url_mostly_prohibited() {
    // "https://example.com/path": URL characters are heavily
    // prohibited-between (AL + IS + SY class interactions). The only
    // guaranteed opportunity is the EOS Mandatory sentinel. We assert
    // that no Mandatory break appears mid-string.
    let s = "https://example.com/path";
    let opps = line_break_opportunities(s);
    let mandatory_mid = opps
        .iter()
        .filter(|o| o.kind == LineBreak::Mandatory && o.byte_offset < s.len())
        .count();
    assert_eq!(mandatory_mid, 0, "no mandatory break inside URL: {:?}", opps);
    // The last opportunity is EOS Mandatory.
    let last = opps.last().expect("non-empty");
    assert_eq!(last.byte_offset, s.len());
    assert_eq!(last.kind, LineBreak::Mandatory);
}

#[test]
fn uni3_seg_04_spaces_allow_break_after() {
    // "hello world": break Allowed between ' ' and 'w' at byte 6.
    // EOS Mandatory at byte 11.
    let opps = line_break_opportunities("hello world");
    // Collect (offset, kind) pairs.
    let pairs: Vec<_> = opps.iter().map(|o| (o.byte_offset, o.kind)).collect();
    assert!(
        pairs.contains(&(6, LineBreak::Allowed)),
        "expected (6, Allowed) in {:?}",
        pairs
    );
    assert!(
        pairs.contains(&(11, LineBreak::Mandatory)),
        "expected (11, Mandatory) in {:?}",
        pairs
    );
}

#[test]
fn uni3_seg_05_cjk_allows_break_every_ideograph() {
    // "日本語テキスト": CJK ideographs (ID class) allow break between
    // every pair. Each ideograph is 3 bytes in UTF-8.
    let s = "日本語テキスト";
    let opps = line_break_opportunities(s);
    // We expect Allowed opportunities at byte 3, 6, 9, 12, 15, 18
    // (between each pair) and Mandatory at byte 21 (EOS; 7 * 3 = 21).
    let allowed_offsets: Vec<_> = opps
        .iter()
        .filter(|o| o.kind == LineBreak::Allowed)
        .map(|o| o.byte_offset)
        .collect();
    for expected in [3, 6, 9, 12, 15, 18] {
        assert!(
            allowed_offsets.contains(&expected),
            "expected Allowed @ {} in {:?}",
            expected,
            allowed_offsets
        );
    }
    // EOS Mandatory at 21.
    let last = opps.last().expect("non-empty");
    assert_eq!(last.byte_offset, 21);
    assert_eq!(last.kind, LineBreak::Mandatory);
}

#[test]
fn uni3_seg_06_empty_input_yields_no_opportunities() {
    // Empty &str: UAX #14 has nothing to break on; our contract is an
    // empty Vec (no EOS sentinel either, since there is no end).
    let opps = line_break_opportunities("");
    assert!(opps.is_empty(), "empty input must yield empty Vec: {:?}", opps);
}

// ---------------------------------------------------------------------
// 2. `wrap_at_width`
// ---------------------------------------------------------------------

#[test]
fn uni3_seg_07_wrap_hello_world_at_5() {
    // Trailing-space trim: "hello " → "hello" (width 5 fits exactly).
    assert_eq!(wrap_at_width("hello world", 5), vec!["hello", "world"]);
}

#[test]
fn uni3_seg_08_wrap_greedy_fit() {
    // Three-word greedy fit: "hello" + "world foo" (width 9) + "bar".
    assert_eq!(
        wrap_at_width("hello world foo bar", 10),
        vec!["hello", "world foo", "bar"]
    );
}

#[test]
fn uni3_seg_09_wrap_honors_mandatory() {
    // Even though max_width = 10 fits "ab\nc" (width 3), the mandatory
    // break at the LF must be honored.
    assert_eq!(wrap_at_width("ab\nc", 10), vec!["ab", "c"]);
}

#[test]
fn uni3_seg_10_wrap_cjk_each_ideograph_width_two() {
    // Each ideograph is width 2 (UAX #11 W class); max_width = 2 means
    // one ideograph per line. Allowed breaks between every pair make
    // this a clean split (no forced-break fallback needed).
    assert_eq!(wrap_at_width("日本語", 2), vec!["日", "本", "語"]);
}

#[test]
fn uni3_seg_11_wrap_ascii_fast_path_character_chunk() {
    // "aaaa" has no mid-string break opportunity, so UAX #14 wrap would
    // overflow; our ASCII fast path falls back to byte-level chunking
    // at max_width. The fast path is width-aware (bytes == columns for
    // ASCII).
    assert_eq!(wrap_at_width("aaaa", 2), vec!["aa", "aa"]);
}

#[test]
fn uni3_seg_12_wrap_empty_input_empty_result() {
    assert_eq!(wrap_at_width("", 10), Vec::<&str>::new());
}

#[test]
fn uni3_seg_13_wrap_single_word_fits_whole() {
    // max_width wide enough for the entire string: single-line result.
    assert_eq!(wrap_at_width("hello", 10), vec!["hello"]);
    assert_eq!(wrap_at_width("hello world", 100), vec!["hello world"]);
}

#[test]
fn uni3_seg_14_wrap_zero_width_treated_as_one() {
    // max_width == 0 is coerced to 1 to avoid infinite loops. "ab"
    // chunks into one-byte pieces.
    assert_eq!(wrap_at_width("ab", 0), vec!["a", "b"]);
}

// ---------------------------------------------------------------------
// 3. Invariants: idempotence + join-stability
// ---------------------------------------------------------------------

#[test]
fn uni3_seg_15_opportunities_sorted_strictly_ascending() {
    // UAX #14 opportunities are inherently sorted; assert that our
    // Vec reflects that invariant on a representative corpus.
    let corpus = [
        "foo bar baz",
        "日本語テキストです",
        "line one\nline two\nline three",
        "https://example.com/path?q=foo+bar",
        "mixed Latin and 日本語 and emoji 🙂",
        "   leading   and   trailing   ",
    ];
    for s in corpus {
        let opps = line_break_opportunities(s);
        for pair in opps.windows(2) {
            assert!(
                pair[0].byte_offset < pair[1].byte_offset,
                "opportunities not strictly ascending in {:?}: {:?}",
                s,
                opps
            );
        }
        // Last entry (if any) must be at s.len() and Mandatory.
        if let Some(last) = opps.last() {
            assert_eq!(last.byte_offset, s.len(), "EOS offset for {:?}", s);
            assert_eq!(last.kind, LineBreak::Mandatory, "EOS kind for {:?}", s);
        }
    }
}

#[test]
fn uni3_seg_16_wrap_idempotence_under_joined_rewrap() {
    // Wrapping a wrapped text at the same width should be a fixed
    // point when joined back with a single space between parts (for
    // Allowed breaks) — we assert the weaker join-stability property
    // that re-wrapping yields the same set of segments.
    let corpus: [(&str, usize); 3] = [
        ("hello world foo bar baz", 10),
        ("the quick brown fox jumps over the lazy dog", 15),
        ("日本語 テキスト です", 4),
    ];
    for (s, w) in corpus {
        let wrapped = wrap_at_width(s, w);
        // Rejoin with a single space; the UAX #14 Allowed-break points
        // are exactly where spaces split, so joining with spaces
        // reconstructs an equivalent string (up to inter-word space
        // normalization).
        let rejoined = wrapped.join(" ");
        let rewrapped = wrap_at_width(&rejoined, w);
        assert_eq!(
            wrapped, rewrapped,
            "wrap not stable under rejoin+rewrap: s={:?} w={} first={:?} rejoined={:?} rewrapped={:?}",
            s, w, wrapped, rejoined, rewrapped
        );
    }
}

// ---------------------------------------------------------------------
// 4. Streaming helper
// ---------------------------------------------------------------------

#[test]
fn uni3_seg_17_next_line_break_from_zero() {
    // "ab cd ef": first opportunity is after the first space, at byte
    // 3 (where 'c' begins). (unicode-linebreak native offset:
    // position AFTER the break = index of first char on next line.)
    assert_eq!(
        next_line_break("ab cd ef", 0),
        Some((3, LineBreak::Allowed))
    );
}

#[test]
fn uni3_seg_18_next_line_break_from_past_first() {
    // Querying from byte 4 (past the first opp at 3) advances to the
    // second Allowed opportunity at byte 6 (where 'e' begins).
    assert_eq!(
        next_line_break("ab cd ef", 4),
        Some((6, LineBreak::Allowed))
    );
}

#[test]
fn uni3_seg_19_next_line_break_at_eos() {
    // From byte 7 (just before EOS at byte 8), only the Mandatory EOS
    // sentinel remains.
    assert_eq!(
        next_line_break("ab cd ef", 7),
        Some((8, LineBreak::Mandatory))
    );
    // Past the end: None.
    assert_eq!(next_line_break("ab cd ef", 100), None);
}

#[test]
fn uni3_seg_20_next_line_break_empty_input_none() {
    assert_eq!(next_line_break("", 0), None);
    assert_eq!(next_line_break("", 5), None);
}

// ---------------------------------------------------------------------
// 5. Type-level sanity (non_exhaustive + Default + API shape)
// ---------------------------------------------------------------------

#[test]
fn uni3_seg_21_line_break_default_is_allowed() {
    // `#[derive(Default)]` + `#[default] Allowed` on the enum.
    assert_eq!(LineBreak::default(), LineBreak::Allowed);
}

#[test]
fn uni3_seg_22_wrap_lines_fit_in_max_width() {
    // Invariant for every emitted line: str_width(line) <= max_width,
    // across a varied corpus of ASCII, CJK, mixed, and ones with
    // mandatory breaks.
    use paideia_as_unicode::str_width;
    let corpus: [(&str, usize); 5] = [
        ("the quick brown fox", 10),
        ("日本語テキスト", 4),
        ("ab\nc", 10),
        ("https://example.com/path", 15),
        ("aaaa", 2),
    ];
    for (s, w) in corpus {
        let wrapped = wrap_at_width(s, w);
        for line in &wrapped {
            assert!(
                str_width(line) <= w,
                "line {:?} width {} exceeds max_width {} (s={:?}, wrapped={:?})",
                line,
                str_width(line),
                w,
                s,
                wrapped
            );
        }
    }
}
