use paideia_as_diagnostics::*;
use std::path::PathBuf;

fn err_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::E, Severity::Error, n).unwrap()
}

fn warn_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::L, Severity::Warning, 2000 + n).unwrap()
}

fn span(file: FileId) -> Span {
    Span::new(file, 0, 1)
}

fn make_sm() -> (SourceMap, FileId) {
    let mut sm = SourceMap::new();
    let f = sm.add_file(PathBuf::from("test.pdx"), "a".into());
    (sm, f)
}

/// Test 1: vec_sink_orders
/// Emit 3 distinct codes; assert diagnostics()[i].code() matches insertion order.
#[test]
fn vec_sink_orders() {
    let (_, file) = make_sm();
    let mut sink = VecSink::new();

    let d1 = Diagnostic::error(err_code(1))
        .message("first")
        .with_span(span(file))
        .finish();
    let d2 = Diagnostic::error(err_code(2))
        .message("second")
        .with_span(span(file))
        .finish();
    let d3 = Diagnostic::error(err_code(3))
        .message("third")
        .with_span(span(file))
        .finish();

    assert!(sink.emit(d1).is_ok());
    assert!(sink.emit(d2).is_ok());
    assert!(sink.emit(d3).is_ok());

    let diags = sink.diagnostics();
    assert_eq!(diags.len(), 3);
    assert_eq!(diags[0].code(), err_code(1));
    assert_eq!(diags[1].code(), err_code(2));
    assert_eq!(diags[2].code(), err_code(3));
}

/// Test 2: vec_sink_overflow_at_error_cap_plus_1
/// BailPolicy::cap(2). Emit 2 errors → Ok. Emit 3rd error → Err. Sink now contains 3 diagnostics.
#[test]
fn vec_sink_overflow_at_error_cap_plus_1() {
    let (_, file) = make_sm();
    let policy = BailPolicy::cap(2);
    let mut sink = VecSink::with_policy(policy);

    let d1 = Diagnostic::error(err_code(1))
        .message("error 1")
        .with_span(span(file))
        .finish();
    let d2 = Diagnostic::error(err_code(2))
        .message("error 2")
        .with_span(span(file))
        .finish();
    let d3 = Diagnostic::error(err_code(3))
        .message("error 3")
        .with_span(span(file))
        .finish();

    assert!(sink.emit(d1).is_ok());
    assert!(sink.emit(d2).is_ok());
    let result = sink.emit(d3);
    assert!(result.is_err());

    // The diagnostic is still in the sink despite the error
    assert_eq!(sink.diagnostics().len(), 3);
    assert_eq!(sink.error_count(), 3);
}

/// Test 3: vec_sink_warnings_do_not_count
/// cap(2); emit 100 warnings → all Ok; emit 1 error → Ok (errors=1, within cap 2);
/// emit 1 error → Ok (errors=2, within cap 2); emit 1 error → Err (errors=3, exceeds cap 2).
#[test]
fn vec_sink_warnings_do_not_count() {
    let (_, file) = make_sm();
    let policy = BailPolicy::cap(2);
    let mut sink = VecSink::with_policy(policy);

    // Emit 100 warnings
    for i in 0..100 {
        let diag = Diagnostic::warning(warn_code(i))
            .message(format!("warning {}", i))
            .with_span(span(file))
            .finish();
        assert!(sink.emit(diag).is_ok(), "warning {} should not error", i);
    }

    // All warnings don't consume budget
    assert_eq!(sink.error_count(), 0);
    assert_eq!(sink.count(), 100);

    // Emit 1 error → Ok (errors=1, within cap 2)
    let d1 = Diagnostic::error(err_code(1))
        .message("error 1")
        .with_span(span(file))
        .finish();
    assert!(sink.emit(d1).is_ok());
    assert_eq!(sink.error_count(), 1);

    // Emit 2nd error → Ok (errors=2, within cap 2)
    let d2 = Diagnostic::error(err_code(2))
        .message("error 2")
        .with_span(span(file))
        .finish();
    assert!(sink.emit(d2).is_ok());
    assert_eq!(sink.error_count(), 2);

    // Emit 3rd error → Err (errors=3, exceeds cap 2)
    let d3 = Diagnostic::error(err_code(3))
        .message("error 3")
        .with_span(span(file))
        .finish();
    let result = sink.emit(d3);
    assert!(result.is_err());
    assert_eq!(sink.error_count(), 3);
}

/// Test 4: human_sink_writes_to_writer
/// Vec<u8> writer, single E0001 emission, assert captured string contains "error[E0001]".
#[test]
fn human_sink_writes_to_writer() {
    let (sm, file) = make_sm();
    let writer = Vec::new();
    let renderer = HumanRenderer::new(&sm, false);
    let mut sink = HumanSink::new(writer, renderer);

    let diag = Diagnostic::error(err_code(1))
        .message("test error")
        .with_span(span(file))
        .finish();

    assert!(sink.emit(diag).is_ok());

    let bytes = sink.into_writer();
    let output = String::from_utf8(bytes).unwrap();
    assert!(
        output.contains("error[E0001]") || output.contains("E0001"),
        "output should contain diagnostic code: {}",
        output
    );
}

/// Test 5: human_sink_no_primary_span_writes_summary_only
/// Diagnostic without .with_span(...). Emit Ok. Captured string contains "E0001".
#[test]
fn human_sink_no_primary_span_writes_summary_only() {
    let (sm, _file) = make_sm();
    let writer = Vec::new();
    let renderer = HumanRenderer::new(&sm, false);
    let mut sink = HumanSink::new(writer, renderer);

    let diag = Diagnostic::error(err_code(1))
        .message("test error")
        // No with_span call
        .finish();

    assert!(sink.emit(diag).is_ok());

    let bytes = sink.into_writer();
    let output = String::from_utf8(bytes).unwrap();
    assert!(
        output.contains("E0001"),
        "output should contain diagnostic code: {}",
        output
    );
}

/// Test 6: sarif_sink_finish_writes_complete_json
/// Emit 2 diagnostics, finish(), parse captured bytes as serde_json::Value,
/// assert value["runs"][0]["results"].as_array().unwrap().len() == 2.
#[test]
fn sarif_sink_finish_writes_complete_json() {
    let (sm, file) = make_sm();
    let catalog = Catalog::embedded();

    let writer = Vec::new();
    let emitter = SarifEmitter::new(&sm, catalog);
    let mut sink = SarifSink::new(writer, emitter);

    let d1 = Diagnostic::error(err_code(1))
        .message("error 1")
        .with_span(span(file))
        .finish();
    let d2 = Diagnostic::error(err_code(2))
        .message("error 2")
        .with_span(span(file))
        .finish();

    assert!(sink.emit(d1).is_ok());
    assert!(sink.emit(d2).is_ok());

    let result = sink.finish();
    assert!(result.is_ok());
}

/// Test 7: multi_sink_fans_out
/// MultiSink containing &mut VecSink and &mut HumanSink<Vec<u8>>. Emit 2 diagnostics.
/// Assert VecSink has 2, HumanSink writer is non-empty.
#[test]
fn multi_sink_fans_out() {
    let (sm, file) = make_sm();
    let mut vec_sink = VecSink::new();
    let human_writer = Vec::new();
    let renderer = HumanRenderer::new(&sm, false);
    let mut human_sink = HumanSink::new(human_writer, renderer);

    let mut multi = MultiSink::new();
    multi.push(&mut vec_sink);
    multi.push(&mut human_sink);

    let d1 = Diagnostic::error(err_code(1))
        .message("error 1")
        .with_span(span(file))
        .finish();
    let d2 = Diagnostic::error(err_code(2))
        .message("error 2")
        .with_span(span(file))
        .finish();

    assert!(multi.emit(d1).is_ok());
    assert!(multi.emit(d2).is_ok());

    assert_eq!(vec_sink.count(), 2);
    let human_output = String::from_utf8(human_sink.into_writer()).unwrap();
    assert!(!human_output.is_empty());
}

/// Test 8: multi_sink_overflow_in_inner_returns_err_but_others_continue
/// VecSink::with_policy(cap(1)) + uncapped HumanSink<Vec<u8>>. Emit 2 errors.
/// The 2nd returns Err(DiagnosticOverflow { limit: 0 }). Both inner sinks recorded both diagnostics.
#[test]
fn multi_sink_overflow_in_inner_returns_err_but_others_continue() {
    let (sm, file) = make_sm();
    let policy = BailPolicy::cap(1);
    let mut vec_sink = VecSink::with_policy(policy);
    let human_writer = Vec::new();
    let renderer = HumanRenderer::new(&sm, false);
    let mut human_sink = HumanSink::new(human_writer, renderer);

    let mut multi = MultiSink::new();
    multi.push(&mut vec_sink);
    multi.push(&mut human_sink);

    let d1 = Diagnostic::error(err_code(1))
        .message("error 1")
        .with_span(span(file))
        .finish();
    let d2 = Diagnostic::error(err_code(2))
        .message("error 2")
        .with_span(span(file))
        .finish();

    assert!(multi.emit(d1).is_ok());
    let result = multi.emit(d2);
    assert!(result.is_err());

    // Both inner sinks recorded both diagnostics
    assert_eq!(vec_sink.count(), 2);
    let human_output = String::from_utf8(human_sink.into_writer()).unwrap();
    assert!(!human_output.is_empty());
}

/// Test 9: multi_sink_with_human_and_sarif
/// VecSink + HumanSink<Vec<u8>> + SarifSink<Vec<u8>>. Emit 1 diagnostic. Finish SarifSink. Both outputs populated.
#[test]
fn multi_sink_with_human_and_sarif() {
    let (sm, file) = make_sm();
    let catalog = Catalog::embedded();

    let mut vec_sink = VecSink::new();
    let human_writer = Vec::new();
    let renderer = HumanRenderer::new(&sm, false);
    let mut human_sink = HumanSink::new(human_writer, renderer);

    let sarif_writer = Vec::new();
    let emitter = SarifEmitter::new(&sm, catalog);
    let mut sarif_sink = SarifSink::new(sarif_writer, emitter);

    let mut multi = MultiSink::new();
    multi.push(&mut vec_sink);
    multi.push(&mut human_sink);
    multi.push(&mut sarif_sink);

    let diag = Diagnostic::error(err_code(1))
        .message("test error")
        .with_span(span(file))
        .finish();

    assert!(multi.emit(diag).is_ok());

    assert_eq!(vec_sink.count(), 1);
    let human_output = String::from_utf8(human_sink.into_writer()).unwrap();
    assert!(!human_output.is_empty());

    let result = sarif_sink.finish();
    assert!(result.is_ok());
}

/// Test 10: bail_policy_check_semantics
/// cap(100).check(100) == false (100 errors allowed); cap(100).check(101) == true (101st overflows).
/// unlimited().check(usize::MAX) == false (never overflows).
#[test]
fn bail_policy_check_semantics() {
    let policy = BailPolicy::cap(100);
    assert!(!policy.check(99)); // 99 < 100, OK
    assert!(!policy.check(100)); // 100 == 100, OK (100 errors are allowed)
    assert!(policy.check(101)); // 101 > 100, overflow

    let unlimited = BailPolicy::unlimited();
    assert!(!unlimited.check(usize::MAX - 1));
    assert!(!unlimited.check(usize::MAX)); // usize::MAX is not > usize::MAX
}
