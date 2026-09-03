//! `Matrix<T, R, C>` stdlib type + intrinsic hook (paideia-as#1384,
//! v0.31-M1-002).
//!
//! The surface for the v0.31 color-hdr milestone: a row-major, statically-
//! sized numeric matrix (`Matrix<T, R, C>`) plus the `MatrixOps` trait that
//! declares the accessors and the `mul_matrix_f32` intrinsic hook the
//! elaborator's Wave 1 `stdlib_lowering::matrixops` recipe will bind.
//!
//! # This module today (Wave 0)
//!
//! - Ships `pdx/matrix.pdx` as the .pdx surface. Because generics-in-records
//!   do not parse yet (blocked on paideia-as#997c), the .pdx uses the same
//!   `record` + u64-placeholder shape that `speculative/vec.pdx` and
//!   `speculative/hashmap.pdx` established for `Vec<T>` / `HashMap<K, V>`.
//! - Runs a `#[cfg(test)]` parse-cleanliness check that lexes + parses
//!   `pdx/matrix.pdx` directly through `paideia-as-parser` and asserts the
//!   diagnostic sink is empty of error-severity items. That is the same
//!   contract the shell-out tests in `tests/parse_pdx.rs` verify for the
//!   pre-existing .pdx files, but this one runs entirely inside
//!   `cargo test -p paideia-as-stdlib` — no built CLI binary required.
//!
//! # M2 responsibilities (Wave 1, v0.31-M2)
//!
//! Everything below is DEFERRED — this file only declares the module hook
//! today. The .pdx header carries the same list with more depth; the
//! summary here exists so a reader of the Rust tree does not need to open
//! the .pdx to see what is coming:
//!
//! 1. SIMD dispatch for `mul_matrix_f32` — runtime CPUID probe picks
//!    between SSE4.1, AVX2, and AVX-512F kernels; small-matrix
//!    short-circuit inlines the SSE4.1 kernel directly.
//! 2. LDA (Leading Dimension of A) specialization — an overload that
//!    accepts explicit row-stride parameters so matrix multiplication
//!    operates on sub-matrix views without a bounce buffer.
//! 3. Generic monomorphization end-to-end (blocked on #997c) — replace the
//!    u64 placeholders with the real generic `Matrix<T, R, C>` surface;
//!    retire the `record Matrix` shim in the same PR.
//! 4. Zero-copy `matrix_row` / `matrix_col` returning borrowed `&[T]`
//!    slices — waits on the slice-type infrastructure landing under
//!    paideia-as#998e.
//! 5. Elaborator recipe (`crates/paideia-as-elaborator/src/stdlib_lowering/
//!    matrixops.rs`) mirroring the `mldsaops.rs` / `cryptoops` pattern, plus
//!    a dedicated `Fp` effect so SIMD state exposure becomes visible in
//!    every matrix-op callsite's effect row.

/// Filename of the .pdx surface, relative to this crate's manifest dir.
///
/// Kept as a `#[cfg(test)] const` so the parse check below and any future
/// Wave 1 recipe-side test both name the file through a single symbol; a
/// rename lands in one place. Gated to `cfg(test)` since Wave 0 has no
/// non-test consumer — Wave 1 promotes it to `pub(crate)` when the
/// elaborator recipe lands.
#[cfg(test)]
pub(crate) const MATRIX_PDX: &str = "pdx/matrix.pdx";

#[cfg(test)]
mod tests {
    use super::MATRIX_PDX;

    use std::path::PathBuf;

    use paideia_as_ast::AstArena;
    use paideia_as_diagnostics::{FileId, Severity, VecSink};
    use paideia_as_lexer::{Lexer, SourceText};
    use paideia_as_parser::Parser;

    /// Absolute path to `pdx/matrix.pdx` for this crate.
    fn matrix_pdx_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(MATRIX_PDX)
    }

    /// End-to-end parse-cleanliness check on `pdx/matrix.pdx`.
    ///
    /// Runs the full lex + parse pipeline `cmd_check` runs in production —
    /// `SourceText::from_bytes` for UTF-8 / BOM validation, `Lexer` for
    /// tokenization, `Parser::parse_source_file` for AST construction —
    /// then asserts the collected `VecSink` carries no `Severity::Error`
    /// diagnostics from any stage.
    ///
    /// This is the Rust-side analogue of the shell-out `paideia-as check`
    /// tests already in `tests/parse_pdx.rs`, but it needs no built CLI
    /// binary: `cargo test -p paideia-as-stdlib -- matrix` runs it green
    /// on any developer machine. The shell-out tests remain for the whole-
    /// stdlib parse coverage sweep; this focused one gates matrix.pdx on
    /// every stdlib crate build.
    #[test]
    fn matrix_pdx_parses_cleanly() {
        let path = matrix_pdx_path();
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));

        // FileId(1) is the standard "single-file test" id used elsewhere
        // in the parser's own snapshot tests (see snapshots_modules.rs).
        let file = FileId::new(1).expect("valid FileId");

        // Stage 1: UTF-8 / BOM validation. A stdlib .pdx failing this is
        // a checked-in-source regression, not a runtime user error.
        let source = SourceText::from_bytes(file, &bytes)
            .expect("pdx/matrix.pdx must be valid UTF-8 without a lone CR");

        // Stage 2 + 3: lex and parse into one sink so we count both.
        let mut sink = VecSink::new();

        let mut lexer = Lexer::new(file, &source);
        let tokens = lexer.collect_tokens(&mut sink);

        let mut arena = AstArena::new();
        {
            let mut parser =
                Parser::new(&tokens, source.content(), file, &mut arena, &mut sink);
            // Return value is intentionally discarded — we only care about
            // whether the parser emitted an error-severity diagnostic.
            let _ = parser.parse_source_file();
        }

        let errors: Vec<_> = sink
            .diagnostics()
            .iter()
            .filter(|d| d.severity() == Severity::Error)
            .collect();

        assert!(
            errors.is_empty(),
            "pdx/matrix.pdx must parse cleanly; got {} error diagnostic(s): {:#?}",
            errors.len(),
            errors,
        );
    }
}
