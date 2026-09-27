//! `cargo test --test runner -p paideia-reflection-corpus` runs the reflection corpus.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use paideia_reflection_corpus::{m_codes_for, parse_expect_file};

fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn collect_pdx_files(dir: &Path) -> Vec<PathBuf> {
    if !dir.exists() {
        return Vec::new();
    }
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("read corpus dir")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("pdx"))
        .collect();
    out.sort();
    out
}

/// Accept corpus test: each `.pdx` file in `corpus/accept/` must emit
/// zero M-codes (M0308, M0309, M0311, M0312).
#[test]
fn accept_corpus_emits_no_macro_codes() {
    let dir = corpus_root().join("corpus/accept");
    let files = collect_pdx_files(&dir);
    let mut failures = Vec::new();
    for path in &files {
        match m_codes_for(path) {
            Ok(codes) if codes.is_empty() => {}
            Ok(codes) => failures.push(format!(
                "{}: expected no M-codes, got {:?}",
                path.file_name().unwrap().to_string_lossy(),
                codes
            )),
            Err(e) => failures.push(format!(
                "{}: harness error: {e}",
                path.file_name().unwrap().to_string_lossy()
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "{} accept files failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Reject corpus test: each `.pdx` file in `corpus/reject/` has a companion
/// `.expect` file that lists the expected M-codes. This test validates that
/// the emitted M-codes match expectations. Fixtures marked `#[ignore]` with
/// explicit reasons await further driver implementation.
///
/// PAS-DEBT-B7-002 (Wave-38 retest, v0.36.66 Slices B+C landed):
/// staying `#[ignore]`'d. R220.M2 Slice B (#1541, macro template expansion)
/// and Slice C (#1542, macro repetition + soft hygiene) shipped the
/// **library-side** expander: `paideia_as_elaborator::expand_macro` now
/// emits M0308 (macro_match.rs, no matching rule), M0309 (macro_expand.rs,
/// unbound `$name` in template), M0310 (macro_match.rs, ragged repetition
/// counts), M0311 (macro_expand.rs, depth guard), and M0314 (macro_expand.rs,
/// template repetition misuse). The `paideia-as build` pipeline, however,
/// does NOT yet invoke `expand_macro` on user source — two glue layers are
/// still missing:
///
/// 1. **Invocation-site parsing.** The parser has `macro foo(...) => {...}`
///    decl support but no macro *call* syntax (no `foo!(...)` and no DSL
///    context-lexed `foo { ... }` recognition). Per
///    `crates/paideia-as-elaborator/src/dsl_parser_registry.rs` module doc,
///    the context lexer that recognises `dsl_name { <body> }` lands in
///    **R221.M4** (see `design/terminal/semantic-shell-language-plan.md`
///    §4). Until then, no `.pdx` source can hold a macro invocation, so
///    no fixture can trigger `expand_macro` end-to-end.
/// 2. **Pipeline wiring.** `crates/paideia-as/src/cmd_build/` contains no
///    reference to `expand_macro`, `expand_reflective_hygienic`, or any
///    `MacroCall`/`MacroInvoke` node — the reflection comparator's
///    `paideia-as build --emit placeholder <file>` invocation therefore
///    would ignore any macro call even if the parser produced one. Slice D
///    (or a distinct pipeline-integration issue) must call the expander
///    on invocation nodes for stderr M-code emission to reach the
///    corpus runner.
///
/// Consequences for the 8 reject fixtures:
///   - 4 fixtures (`r_macro_no_matching_rule`, `r_pattern_match_failure`,
///     `r_recursion_depth`, `r_unbound_metavariable`) remain placeholder
///     `.pdx` files (`let m = 1`) with `.expect` sidecars carrying only
///     a comment. Their real bodies cannot be authored until (1) and (2)
///     both land. Placeholder .expect notes now name the precise blocker
///     per fixture.
///   - 4 fixtures (`r_antiquote_outside_quote`, `r_finally_not_last`,
///     `r_malformed_quote`, `r_unknown_fragment_kind`) target P-category
///     codes (P0170/P0162/P0171/P0110) that the parser already emits
///     (see `crates/paideia-as-parser/src/{quote.rs,parse_handler.rs,
///     parse_item.rs,parse_macro.rs}`). The comparator only extracts
///     M-codes, so these fixtures belong in a parser-reject corpus.
///     No such corpus exists yet; recommend a follow-up issue to add a
///     `tests/parser-reject-corpus` sibling harness and relocate them.
///
/// Recommended disposition for #1530: keep OPEN, retitle to track two
/// concrete blockers (R221.M4 invocation lexer + cmd_build expander
/// wiring), or split into two smaller issues if either lands
/// independently. Close only after both glue layers exist AND the four
/// placeholder .pdx files are rewritten around real invocations.
#[test]
#[ignore = "reject corpus is not runnable end-to-end: the paideia-as build \
    pipeline does not yet invoke expand_macro on user source (no macro-call \
    parsing — blocked on R221.M4 context lexer — and cmd_build carries no \
    expander wiring), so the 4 M-code fixtures remain placeholder modules \
    and the 4 P-code fixtures belong in a parser-reject corpus. \
    See PAS-DEBT-B7-002; Slices B/C (#1541 #1542) landed the library-side \
    expander (M0308/M0309/M0310/M0311/M0314) but not the pipeline glue."]
fn reject_corpus_emits_expected_codes() {
    let dir = corpus_root().join("corpus/reject");
    let files = collect_pdx_files(&dir);
    let mut failures = Vec::new();
    for path in &files {
        let expect_path = path.with_extension("expect");
        let expected: BTreeSet<String> = match std::fs::read_to_string(&expect_path) {
            Ok(s) => parse_expect_file(&s),
            Err(_) => {
                failures.push(format!(
                    "{}: missing .expect sidecar at {}",
                    path.file_name().unwrap().to_string_lossy(),
                    expect_path.display()
                ));
                continue;
            }
        };
        match m_codes_for(path) {
            Ok(codes) if codes == expected => {}
            Ok(codes) => failures.push(format!(
                "{}: expected {:?}, got {:?}",
                path.file_name().unwrap().to_string_lossy(),
                expected,
                codes
            )),
            Err(e) => failures.push(format!(
                "{}: harness error: {e}",
                path.file_name().unwrap().to_string_lossy()
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "{} reject files failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
