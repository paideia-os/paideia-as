//! paideia-as#1327 (v0.22-post): mount.pdx regression coverage.
//!
//! The reporter observed that `paideia-as check src/user/mount.pdx`
//! terminated with a bewildering
//! `error[P0100]: expected item (module, signature, let, effect,
//! capability, struct, enum, trait, impl, macro, or unsafe)`
//! whose caret they attributed to the module-close `}` after the
//! `@no_frame` on the last lambda. Isolating the fixture showed the
//! diagnosis was misdirected: the failing byte is a `record` binding
//! name, and `record` has been a reserved keyword since #637
//! (m7-001 record-type grammar). The token-per-`@no_frame` claim
//! never reproduces — the four-lambda, four-`@no_frame` shape
//! parses cleanly.
//!
//! What was actually wrong was diagnostic quality: `record` is a
//! `Kw*` variant that never made it into #1263's
//! `keyword_source_text` table, so `expect(Ident)` fell to the
//! "expected identifier, found token" fallback with no rename hint;
//! the identical omission in `parse_primary::error_expected_expression`
//! turned `[rip + record]` into a bare "expected expression". The
//! cascade of skipped items after those two errors is what surfaced
//! as the misattributed P0100 at the module-close `}`.
//!
//! These tests pin the fix in three shapes:
//! 1. `record` as a `let mut` binding name gets the actionable
//!    "reserved keyword" hint (identifier context).
//! 2. `record` used in an operand position gets the same hint
//!    (expression context).
//! 3. A four-lambda module with `@no_frame` on every lambda —
//!    including the last — parses with zero diagnostics, refuting
//!    the reporter's theory that the parser refuses trailing
//!    `@no_frame` on the terminal lambda.

use paideia_as_diagnostics::{Diagnostic, DiagnosticSink, Severity, VecSink};
use paideia_as_lexer::{Lexer, SourceText};
use paideia_as_parser::Parser;
use std::path::PathBuf;

/// Parse `source` and return the resulting arena, parse result, and diagnostics.
fn parse_and_check(
    source: &str,
) -> (
    paideia_as_ast::AstArena,
    Result<paideia_as_ast::NodeId, paideia_as_parser::ParseError>,
    Vec<Diagnostic>,
) {
    let mut source_map = paideia_as_diagnostics::SourceMap::new();
    let file = source_map.add_file(PathBuf::from("test.pdx"), source.to_string());
    let source_text = SourceText::from_bytes(file, source.as_bytes()).expect("valid utf-8");
    let mut arena = paideia_as_ast::AstArena::new();
    let mut sink = VecSink::new();
    let mut lex = Lexer::new(file, &source_text);
    let mut collector = VecSink::new();
    let tokens = lex.collect_tokens(&mut collector);
    for d in collector.into_diagnostics() {
        let _ = sink.emit(d);
    }
    let result = {
        let mut p = Parser::new(&tokens, source_text.content(), file, &mut arena, &mut sink);
        p.parse_source_file()
    };
    (arena, result, sink.into_diagnostics())
}

/// Assert exactly one P0100 error diagnostic whose message contains every
/// `must_contain` fragment. Fails with the full diagnostic list on
/// mismatch so regressions read like intent.
fn assert_p0100_with_fragments(diags: &[Diagnostic], must_contain: &[&str]) {
    let p0100: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| {
            d.code().category().letter() == 'P'
                && d.code().number() == 100
                && d.code().severity() == Severity::Error
        })
        .collect();
    assert!(
        !p0100.is_empty(),
        "expected at least one P0100 error, got: {:?}",
        diags
            .iter()
            .map(|d| format!(
                "{}{:04}: {}",
                d.code().category().letter(),
                d.code().number(),
                d.message()
            ))
            .collect::<Vec<_>>()
    );

    let hit = p0100.iter().find(|d| {
        must_contain
            .iter()
            .all(|frag| d.message().contains(frag))
    });
    assert!(
        hit.is_some(),
        "no P0100 diagnostic contained every fragment {:?}; got messages: {:?}",
        must_contain,
        p0100.iter().map(|d| d.message().to_string()).collect::<Vec<_>>()
    );
}

// -------------------------------------------------------------------------
// Case 1: `record` as a binding name — identifier context.
// -------------------------------------------------------------------------

#[test]
fn record_as_let_mut_binding_name_gets_reserved_keyword_hint() {
    // Minimal mount.pdx-shape: the only trigger is the `record` name.
    // Before #1327: "expected identifier, found token" — `record` fell
    // through both `keyword_source_text` and `debug_kind`.
    // After: the actionable "rename it" hint from #1263.
    let source = "\
module Mount = structure {
  pub let mut record : u64 = 0
}
";
    let (_arena, _result, diags) = parse_and_check(source);
    assert_p0100_with_fragments(
        &diags,
        &[
            "`record`",
            "reserved keyword",
            "rename it",
            // The suggested rename is derived from the offending
            // spelling — no more "expected identifier, found token".
            "record_",
        ],
    );
}

// -------------------------------------------------------------------------
// Case 2: `record` in an operand position — expression context.
// -------------------------------------------------------------------------

#[test]
fn record_in_expression_position_gets_reserved_keyword_hint() {
    // `record` appears inside `[rip + record]`, a memory operand. The
    // parser's memref path reaches parse_primary with `record` as the
    // current token, which reserves the token → `error_expected_expression`
    // used to emit a bare "expected expression". Post-#1327 it routes
    // through the shared reserved_keyword_hint.
    let source = "\
module Mount = structure {
  pub let f : (u64) -> () !{mem} @{fs} =
    fn (x: u64) -> unsafe {
      effects: {mem}, capabilities: {fs},
      justification: \"record-op\",
      block: {
        lea rdi, [rip + record];
        ret
      }
    } @no_frame
}
";
    let (_arena, _result, diags) = parse_and_check(source);
    assert_p0100_with_fragments(
        &diags,
        &[
            "`record`",
            "reserved keyword",
            "start of an expression",
        ],
    );
}

// -------------------------------------------------------------------------
// Case 3: four-lambda module with `@no_frame` on every lambda — the
// reporter's claimed failure shape. Must parse with zero diagnostics.
// -------------------------------------------------------------------------

#[test]
fn four_lambda_module_all_no_frame_parses_cleanly() {
    // Same skeleton as the drafted mount.pdx sans the `record` binding
    // (which is the real trigger; see case 1). Every `@no_frame` sits
    // on a `pub let` lambda, and the final one is followed only by the
    // module-close `}` — exactly the shape the reporter claimed was
    // rejected. With no reserved-word misuse in scope, parsing yields
    // zero diagnostics; the `@no_frame`-at-terminal-lambda hypothesis
    // does not reproduce.
    let source = "\
module Mount = structure {

  pub let print_u64_dec : (u64) -> () !{mem, sysreg} @{fs} =
    fn (value: u64) -> unsafe {
      effects: {mem, sysreg}, capabilities: {fs},
      justification: \"a\",
      block: { ret }
    } @no_frame

  pub let write_bytes : (u64, u64) -> () !{mem, sysreg} @{fs} =
    fn (buf: u64, len: u64) -> unsafe {
      effects: {mem, sysreg}, capabilities: {fs},
      justification: \"b\",
      block: { ret }
    } @no_frame

  pub let print_backend_name : (u64) -> () !{mem, sysreg} @{fs} =
    fn (b: u64) -> unsafe {
      effects: {mem, sysreg}, capabilities: {fs},
      justification: \"c\",
      block: { ret }
    } @no_frame

  pub let _start : () -> () !{mem, sysreg} @{fs, sched} =
    fn () -> unsafe {
      effects: {mem, sysreg}, capabilities: {fs, sched},
      justification: \"start\",
      block: { hlt }
    } @no_frame
}
";
    let (_arena, result, diags) = parse_and_check(source);
    assert!(
        result.is_ok(),
        "parse_source_file must succeed; diagnostics: {:?}",
        diags
            .iter()
            .map(|d| format!(
                "{}{:04}: {}",
                d.code().category().letter(),
                d.code().number(),
                d.message()
            ))
            .collect::<Vec<_>>()
    );
    assert!(
        diags.is_empty(),
        "expected zero diagnostics; got: {:?}",
        diags
            .iter()
            .map(|d| format!(
                "{}{:04}: {}",
                d.code().category().letter(),
                d.code().number(),
                d.message()
            ))
            .collect::<Vec<_>>()
    );
}

// -------------------------------------------------------------------------
// Case 4: sanity — the #1263 `loop` case still works after the shared
// helper refactor (guards against a regression in the identifier path
// while we route through `reserved_keyword_hint`).
// -------------------------------------------------------------------------

#[test]
fn loop_as_binding_name_still_gets_reserved_keyword_hint() {
    let source = "let loop = 42";
    let (_arena, _result, diags) = parse_and_check(source);
    assert_p0100_with_fragments(
        &diags,
        &["`loop`", "reserved keyword", "rename it", "loop_"],
    );
}

// -------------------------------------------------------------------------
// Case 5: sanity — `mut` mispositioned in `let mut mut x = 0`. Before
// #1327 the second `mut` fell through debug_kind to "token"; after, it
// gets its own actionable hint (identifier context).
// -------------------------------------------------------------------------

#[test]
fn stray_mut_in_binding_position_gets_reserved_keyword_hint() {
    let source = "let mut mut x = 0";
    let (_arena, _result, diags) = parse_and_check(source);
    assert_p0100_with_fragments(&diags, &["`mut`", "reserved keyword", "rename it"]);
}
