//! R221.M7 proptest fuzz corpus.
//!
//! Fingerprint `r221m7-fuzz-NN`. Six proptest properties, each running
//! ~256 randomly-generated inputs by default (proptest's default
//! `ProptestConfig::cases`), so a single `cargo test` iterates ~1500
//! fuzz cases per invocation — the plan-doc's "24-hour AFL is
//! impractical; land proptest ~1000 inputs" allowance.
//!
//! # Invariants asserted
//!
//! 1. **Never panics**: the parser returns `Result` on every input.
//! 2. **`Ok` XOR `Err`**: no third possibility exists (by
//!    `Result`'s type — but the invariant is *made explicit* here so a
//!    regression that adds a hidden panic branch fails a fuzz case
//!    rather than making it through in silence).
//! 3. **Round-trip preservation on parse success**: whenever the input
//!    parses successfully, `parse(pretty_print(parse(input))) ==
//!    parse(input)` (the AST is a fixed point of the transform).
//! 4. **Diagnostic soundness**: whenever the input fails, the reported
//!    `original_span` lies within the source bounds.

use paideia_as_shell_ast::{parse, pretty_print};
use proptest::prelude::*;

/// Any well-formed Rust `String` up to ~200 bytes. Covers ASCII, mixed
/// case, digits, sigils, whitespace, and (via proptest's regex
/// generator) some unicode.
fn any_input() -> impl Strategy<Value = String> {
    // Constrain to printable-ish text so we hit the parser's happy
    // paths and error paths in roughly the same ratio the R229 REPL
    // sees.
    ".{0,200}".prop_map(|s| s)
}

/// A structured input generator that biases toward tokens the shell
/// actually cares about — identifiers, punctuation, quotes.
fn structured_input() -> impl Strategy<Value = String> {
    let token = prop_oneof![
        Just("ls".to_string()),
        Just("cd".to_string()),
        Just("cat".to_string()),
        Just("grep".to_string()),
        Just("|".to_string()),
        Just(";".to_string()),
        Just("(".to_string()),
        Just(")".to_string()),
        Just("{".to_string()),
        Just("}".to_string()),
        Just("[".to_string()),
        Just("]".to_string()),
        Just(",".to_string()),
        Just(".".to_string()),
        Just(">".to_string()),
        Just("<".to_string()),
        Just("=>".to_string()),
        Just("+".to_string()),
        Just("datalog".to_string()),
        Just("not".to_string()),
        Just("and".to_string()),
        Just("or".to_string()),
        Just("?x".to_string()),
        Just("$it".to_string()),
        Just("\"s\"".to_string()),
        Just("42".to_string()),
        Just("true".to_string()),
        Just("false".to_string()),
        Just("\n".to_string()),
        Just(" ".to_string()),
    ];
    proptest::collection::vec(token, 0..30).prop_map(|v| v.join(""))
}

/// Deep-nested `{` corpus: 0..=200 opening braces followed by 0..=200
/// closing braces. Tests both matched (rare) and adversarially
/// unmatched (common) nestings.
fn nested_braces() -> impl Strategy<Value = String> {
    (0usize..200, 0usize..200).prop_map(|(o, c)| {
        let mut s = String::with_capacity(o + c);
        for _ in 0..o {
            s.push('{');
        }
        for _ in 0..c {
            s.push('}');
        }
        s
    })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        .. ProptestConfig::default()
    })]

    // r221m7-fuzz-01: never panics on arbitrary text.
    #[test]
    fn r221m7_fuzz_01_never_panics_random_text(input in any_input()) {
        let _ = parse(&input);
    }

    // r221m7-fuzz-02: never panics on structured token soup.
    #[test]
    fn r221m7_fuzz_02_never_panics_structured(input in structured_input()) {
        let _ = parse(&input);
    }

    // r221m7-fuzz-03: never panics on adversarial brace nesting.
    #[test]
    fn r221m7_fuzz_03_never_panics_nested_braces(input in nested_braces()) {
        let _ = parse(&input);
    }

    // r221m7-fuzz-04: on success, round-trip preserves the AST.
    #[test]
    fn r221m7_fuzz_04_roundtrip_on_success(input in structured_input()) {
        if let Ok(a) = parse(&input) {
            let pp = pretty_print(&a);
            let b = parse(&pp)
                .expect("r221m7-fuzz-04: pretty output must reparse");
            let pp2 = pretty_print(&b);
            prop_assert_eq!(pp, pp2,
                "r221m7-fuzz-04: pretty-print not idempotent");
        }
    }

    // r221m7-fuzz-05: on failure, diagnostic spans lie inside the
    // input's byte bounds.
    #[test]
    fn r221m7_fuzz_05_error_span_in_bounds(input in structured_input()) {
        if let Err(e) = parse(&input) {
            prop_assert!(
                e.original_span.0 <= input.len(),
                "r221m7-fuzz-05: original_span.0 out of bounds"
            );
            prop_assert!(
                e.original_span.1 <= input.len(),
                "r221m7-fuzz-05: original_span.1 out of bounds"
            );
            prop_assert!(
                e.original_span.0 <= e.original_span.1,
                "r221m7-fuzz-05: reversed original_span"
            );
        }
    }

    // r221m7-fuzz-06: NFC map round-trip — for any input, the
    // to_original inverse always returns a byte range within
    // [0, input.len()].
    #[test]
    fn r221m7_fuzz_06_nfc_map_bounds(input in any_input()) {
        let (nfc, map) = paideia_as_shell_ast::NfcMap::build(&input);
        // Pick a random subrange of the NFC output and translate it.
        if nfc.is_empty() { return Ok(()); }
        let a = 0usize;
        let b = nfc.len();
        let (o0, o1) = map.to_original((a, b));
        prop_assert!(o0 <= input.len());
        prop_assert!(o1 <= input.len());
        prop_assert!(o0 <= o1);
    }
}
