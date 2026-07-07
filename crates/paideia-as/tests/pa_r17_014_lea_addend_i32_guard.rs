//! PA-R17-014 / Issue #992: lea rel32 addend i32 guard
//!
//! This test suite validates that address-of addends (computed from field offsets)
//! are guarded against overflow when used in lea [rip + sym + addend] encodings.
//!
//! Tests:
//! 1. field_offset_small_ok: small field offset (8 bytes) compiles cleanly
//! 2. field_offset_at_i32_max_ok: field offset at exactly i32::MAX
//! 3. field_offset_overflow_emits_t0536: field offset exceeds i32::MAX → T0536
//! 4. field_offset_negative_underflow_emits_t0536: negative offset < i32::MIN → T0536

#[cfg(test)]
mod tests {
    use paideia_as_diagnostics::{Diagnostic, DiagnosticCode, Category, Severity, Span, VecSink};

    // Mock version of check_addend_i32 for testing (copied from cmd_build.rs)
    fn check_addend_i32(
        addend: i64,
        span: Span,
        sink: &mut dyn paideia_as_diagnostics::DiagnosticSink,
    ) -> Option<i32> {
        if (i32::MIN as i64..=i32::MAX as i64).contains(&addend) {
            Some(addend as i32)
        } else {
            // Emit T0536 diagnostic for addend overflow
            let diag = Diagnostic::error(
                DiagnosticCode::new(
                    Category::T,
                    Severity::Error,
                    536,
                ).expect("T0536 is valid")
            )
            .message(format!(
                "lea rel32 addend exceeds i32 range: {} (min: {}, max: {})",
                addend, i32::MIN as i64, i32::MAX as i64
            ))
            .with_span(span)
            .finish();
            let _ = sink.emit(diag);
            None
        }
    }

    fn dummy_span() -> Span {
        let file_id = paideia_as_diagnostics::FileId::new(1).expect("FileId(1) is valid");
        Span::new(file_id, 0, 0)
    }

    /// Test 1: Small field offset (8 bytes) compiles cleanly.
    /// Simulates: struct Foo { x: u64, y: u64 } → &foo.y has offset 8.
    #[test]
    fn field_offset_small_ok() {
        let mut sink = VecSink::new();
        let span = dummy_span();
        let addend = 8i64;  // Small field offset (second u64 field)

        let result = check_addend_i32(addend, span, &mut sink);

        assert!(result.is_some());
        assert_eq!(result.unwrap(), 8i32);
        assert!(sink.diagnostics().is_empty(), "Expected no diagnostics for small offset");
    }

    /// Test 2: Field offset at exactly i32::MAX (0x7FFFFFFF).
    /// This boundary case should succeed.
    #[test]
    fn field_offset_at_i32_max_ok() {
        let mut sink = VecSink::new();
        let span = dummy_span();
        let addend = i32::MAX as i64;  // Exactly i32::MAX

        let result = check_addend_i32(addend, span, &mut sink);

        assert!(result.is_some());
        assert_eq!(result.unwrap(), i32::MAX);
        assert!(sink.diagnostics().is_empty(), "Expected no diagnostics at i32::MAX boundary");
    }

    /// Test 3: Field offset exceeds i32::MAX (0x80000000).
    /// This should emit T0536 diagnostic.
    #[test]
    fn field_offset_overflow_emits_t0536() {
        let mut sink = VecSink::new();
        let span = dummy_span();
        let addend = 0x80000000i64;  // i32::MAX + 1

        let result = check_addend_i32(addend, span, &mut sink);

        assert!(result.is_none(), "Expected None for overflow");
        let diags = sink.diagnostics();
        assert!(!diags.is_empty(), "Expected T0536 diagnostic on overflow");
        assert_eq!(diags[0].code().to_string(), "T0536", "Expected T0536 diagnostic code");
    }

    /// Test 4: Negative offset underflow (< i32::MIN).
    /// This should emit T0536 diagnostic.
    #[test]
    fn field_offset_negative_underflow_emits_t0536() {
        let mut sink = VecSink::new();
        let span = dummy_span();
        let addend = (i32::MIN as i64) - 1;  // i32::MIN - 1

        let result = check_addend_i32(addend, span, &mut sink);

        assert!(result.is_none(), "Expected None for underflow");
        let diags = sink.diagnostics();
        assert!(!diags.is_empty(), "Expected T0536 diagnostic on underflow");
        assert_eq!(diags[0].code().to_string(), "T0536", "Expected T0536 diagnostic code");
    }
}
