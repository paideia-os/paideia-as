//! `@wgsl_module(path)` — WGSL compute-shader source-text intrinsic.
//!
//! **Wave 0, Batch 4, row v0.30-M1-002 (paideia-as#1380).**
//! First-class embed directive for WGSL (WebGPU Shading Language) source
//! text loaded from an on-disk file at compile time. The text is
//! emitted verbatim into `.rodata.wgsl` so the WebGPU / Vulkan / Vello
//! runtime can hand it to `wgpu::Device::create_shader_module` without
//! re-reading the file at boot. Sister intrinsic to `@spirv_module`
//! (batch mate `v0.30-M1-001` / b4-05), which handles the pre-compiled
//! binary counterpart.
//!
//! # What "WGSL is source text" changes vs. SPIR-V
//!
//! SPIR-V is a binary word stream; the sibling intrinsic validates a
//! magic word (`0x07230203`) and a version header. WGSL is UTF-8
//! source text (WebGPU spec §3.4 "Source Text") with no framing at
//! all. The compile-time contract we enforce here is therefore
//! text-shaped:
//!
//! 1. **UTF-8 well-formedness** — the file's bytes must decode as
//!    valid UTF-8. `wgpu`'s downstream parser requires this and would
//!    otherwise fail at device-init time on the target, far from the
//!    caller's terminal.
//! 2. **No embedded NUL bytes** — WGSL forbids `U+0000` in source
//!    (WebGPU §3.4 "The source text must not contain a null character
//!    U+0000"). We reject them here so the operator's build fails at
//!    `pdxc build` on the machine that owns the file, not at boot on
//!    a headless target.
//! 3. **No byte-order mark** — WGSL prohibits a leading `U+FEFF` BOM.
//!    Common on files edited in Windows-native editors; catching it
//!    at the assembler prevents opaque "shader failed to compile"
//!    diagnostics on the runtime side.
//! 4. **Soft size cap: 1 MiB** — the largest realistic Vello shader
//!    we anticipate is ~150 KiB; a 1 MiB ceiling is 6x that headroom
//!    while still catching the `path.png` foot-gun (developer types
//!    the wrong extension and embeds a texture as if it were a
//!    shader). The cap lives on the `spec` side, not on `std::fs`
//!    itself, so the diagnostic can name the offending path and
//!    actual size.
//!
//! # Handoff shape
//!
//! On success, [`parse_wgsl_module`] returns a [`WgslModuleSpec`]
//! carrying the *validated* WGSL text (already decoded to `String`)
//! plus the source `PathBuf` for provenance tracking. The encoder
//! writes the text as a `.rodata.wgsl` section via a synthesised
//! [`KindMemorySymbol`] descriptor — identical handoff shape to the
//! SPIR-V sibling so downstream `.rodata` emission dedups the two
//! into one code path.
//!
//! # Diagnostics (`I0210`–`I0212`)
//!
//! Codes live in a self-contained namespace on [`WgslModuleError`]
//! (via [`WgslModuleError::code`]), matching the pattern established
//! in the Batch 3 intrinsics (`include_signed`, `atomic128`,
//! `dma_buffer`). When the shared `paideia-as-diagnostics` catalog
//! reserves an `I` (intrinsic) category, these strings promote to a
//! typed `DiagnosticCode` without any call-site churn.
//!
//! Assignments — leave gaps closed and additive:
//!
//! | Code    | Condition                                                 |
//! |---------|-----------------------------------------------------------|
//! | I0210   | WGSL source exceeds the 1 MiB soft cap                    |
//! | I0211   | WGSL source is not well-formed UTF-8                      |
//! | I0212   | WGSL source contains a forbidden byte (NUL or BOM prefix) |
//!
//! I0213–I0219 are reserved for the encoder-half rejections (unknown
//! path, unreadable path, section-name collision) that land alongside
//! `.rodata.wgsl` emission in the v0.30-M2 encoder row. Contiguity
//! makes the intrinsic's error surface greppable.

use std::path::{Path, PathBuf};

// -----------------------------------------------------------------------------
// Handoff descriptor (shared shape with `@spirv_module`)
// -----------------------------------------------------------------------------

/// `.rodata`-family memory-symbol descriptor the encoder consumes.
///
/// Mirrors the shape produced by the SPIR-V sibling so the encoder's
/// `.rodata` writer has a single lowering path for compile-time
/// GPU-payload embeds. The section string is populated by the
/// intrinsic (`.rodata.wgsl` here, `.rodata.spirv` in the sibling)
/// so the linker script can group related payloads without the
/// encoder having to introspect the payload variant.
///
/// This is defined locally rather than pulled from `paideia-as-ir`
/// because Batch 4 lands parser-half only; the encoder-half wiring
/// (v0.30-M2) will re-home this struct to the IR crate and this
/// module will re-export from there. Keeping the shape stable across
/// that move is the whole reason it exists as a named type today.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KindMemorySymbol {
    /// ELF section name, e.g. `".rodata.wgsl"` or `".rodata.spirv"`.
    /// A leading `.` is required; the encoder does not add one.
    pub section: String,
    /// Raw bytes to emit into the section. For WGSL this is the
    /// validated UTF-8 source text; for SPIR-V it is the binary
    /// word stream.
    pub bytes: Vec<u8>,
}

// -----------------------------------------------------------------------------
// Public spec
// -----------------------------------------------------------------------------

/// Soft cap on WGSL source size, in bytes (1 MiB).
///
/// Chosen at ~6x the largest realistic Vello compute shader (~150
/// KiB) so legitimate use stays comfortable, while a two-decimal
/// jump (embedding a texture or a full SPIR-V binary by mistake)
/// trips I0210. Kept `pub` so tests, downstream tooling, and the
/// eventual `pdx.toml` override key can share a single source of
/// truth.
pub const WGSL_MAX_SIZE_BYTES: usize = 1024 * 1024;

/// UTF-8 byte-order-mark, forbidden as a WGSL source prefix.
///
/// WebGPU §3.4 "Source Text": the source text must not begin with a
/// byte-order mark. Rendering this as a byte-string constant (rather
/// than the Unicode scalar `U+FEFF`) keeps the check on the raw byte
/// stream, where the caller supplies it, and avoids paying the
/// `char_indices` cost just to spot three bytes at the start.
pub const UTF8_BOM: &[u8; 3] = b"\xEF\xBB\xBF";

/// Validated `@wgsl_module(path)` invocation.
///
/// Pure data record — the source `path` is retained for provenance
/// (diagnostics, incremental-build cache keys) and `source` holds
/// the already-validated UTF-8 text ready for `.rodata.wgsl`
/// emission. The encoder calls [`WgslModuleSpec::to_memory_symbol`]
/// to lift this into the shared [`KindMemorySymbol`] shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgslModuleSpec {
    /// Path the parser resolved and read. Absolute or relative — the
    /// parser does not restrict shape; sandboxing is applied one
    /// layer up by the surface parser against `pdx.toml`
    /// `include_roots`.
    pub path: PathBuf,
    /// Validated WGSL source text. UTF-8 by construction, NUL-free,
    /// no BOM prefix, ≤ [`WGSL_MAX_SIZE_BYTES`] bytes.
    pub source: String,
}

impl WgslModuleSpec {
    /// Lift the spec into the shared handoff shape the encoder
    /// consumes for both WGSL and SPIR-V embeds. Emits into
    /// `.rodata.wgsl`; the linker script groups the family.
    #[must_use]
    pub fn to_memory_symbol(&self) -> KindMemorySymbol {
        KindMemorySymbol {
            section: ".rodata.wgsl".to_string(),
            bytes: self.source.as_bytes().to_vec(),
        }
    }
}

// -----------------------------------------------------------------------------
// Error surface
// -----------------------------------------------------------------------------

/// Parse-time failure modes for `@wgsl_module`.
///
/// Every variant carries the data needed to render a human message
/// without reaching back into the surface parser, matching the
/// self-contained style of `IncludeSignedError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WgslModuleError {
    /// I0210 — source exceeds the 1 MiB soft cap.
    TooLarge {
        /// Path we probed, for the operator's diagnostic.
        path: PathBuf,
        /// Actual size seen, in bytes.
        got: usize,
        /// The cap ([`WGSL_MAX_SIZE_BYTES`]), carried explicitly so
        /// tests can pin the error shape without hard-coding the
        /// constant in two places.
        cap: usize,
    },
    /// I0211 — bytes did not decode as valid UTF-8.
    NotUtf8 {
        /// Path we probed.
        path: PathBuf,
        /// Byte offset of the first invalid sequence, as reported by
        /// `std::str::from_utf8`. Handy for surface-parser callers
        /// that want to render a span.
        error_offset: usize,
    },
    /// I0212 — source contains a forbidden byte. WGSL forbids NUL
    /// (`U+0000`) anywhere in the text and BOM (`U+FEFF`) at the
    /// start; both collapse into one code because the operator-side
    /// fix is the same ("strip the offending bytes from the file")
    /// and the [`ForbiddenKind`] discriminator keeps the message
    /// precise.
    Forbidden {
        /// Path we probed.
        path: PathBuf,
        /// Which forbidden marker fired.
        kind: ForbiddenKind,
        /// Byte offset of the offending byte (0 for a BOM).
        offset: usize,
    },
}

/// Which WGSL-forbidden byte sequence tripped [`WgslModuleError::Forbidden`].
///
/// Modeled as an enum rather than folded into two error variants
/// because the two cases share the same operator-side response
/// (edit the file) and the same reporting shape (path + offset),
/// so collapsing them keeps the diagnostic namespace one code
/// wide while retaining precision in the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ForbiddenKind {
    /// A `U+0000` (NUL) byte anywhere in the source.
    NulByte,
    /// A UTF-8 byte-order-mark (`EF BB BF`) at offset 0.
    Bom,
}

impl ForbiddenKind {
    /// Short label used in diagnostic messages.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ForbiddenKind::NulByte => "NUL byte (U+0000)",
            ForbiddenKind::Bom => "UTF-8 byte-order-mark (U+FEFF)",
        }
    }
}

impl WgslModuleError {
    /// Stable string diagnostic code (`"I0210"`–`"I0212"`).
    ///
    /// See the module-level table for the full mapping.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            WgslModuleError::TooLarge { .. } => "I0210",
            WgslModuleError::NotUtf8 { .. } => "I0211",
            WgslModuleError::Forbidden { .. } => "I0212",
        }
    }

    /// One-line human-readable message, safe to render directly in
    /// terminal diagnostics.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            WgslModuleError::TooLarge { path, got, cap } => format!(
                "@wgsl_module: source `{}` is {got} bytes, exceeds {cap}-byte soft cap",
                path.display()
            ),
            WgslModuleError::NotUtf8 { path, error_offset } => format!(
                "@wgsl_module: source `{}` is not valid UTF-8 (first invalid sequence at byte {error_offset})",
                path.display()
            ),
            WgslModuleError::Forbidden { path, kind, offset } => format!(
                "@wgsl_module: source `{}` contains forbidden {} at byte {offset}",
                path.display(),
                kind.as_str()
            ),
        }
    }
}

impl std::fmt::Display for WgslModuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code(), self.message())
    }
}

impl std::error::Error for WgslModuleError {}

// -----------------------------------------------------------------------------
// Validator
// -----------------------------------------------------------------------------

/// Validate a WGSL byte stream and produce a [`WgslModuleSpec`].
///
/// Checks apply in the order that produces the most actionable
/// diagnostic:
///
/// 1. **Size** (I0210) — cheapest, and if the source is a megabyte
///    of PNG the UTF-8 check will fire on the *first* invalid byte,
///    which is a much less useful message than "you embedded a
///    1.6 MiB file where a shader was expected".
/// 2. **BOM** (I0212 / [`ForbiddenKind::Bom`]) — before UTF-8
///    decoding, because the BOM itself is valid UTF-8 (it decodes
///    to `U+FEFF`) and would slip past the decoder undetected.
/// 3. **UTF-8** (I0211) — required to safely inspect `char`
///    boundaries in step 4.
/// 4. **NUL scan** (I0212 / [`ForbiddenKind::NulByte`]) — over the
///    raw bytes, since NUL bytes are single-byte and can be spotted
///    with `memchr`-shaped scanning without paying for a
///    `char_indices` traversal.
///
/// # Parameters
///
/// - `path` — provenance path (already resolved by the surface
///   parser). Not touched by the validator; carried through to the
///   returned spec and into every diagnostic.
/// - `bytes` — raw file contents. The caller is responsible for the
///   I/O; deferring the read to the caller keeps this function
///   testable without any filesystem fixtures.
///
/// # What this function deliberately does **not** do
///
/// - It does **not** parse the WGSL grammar. That is `wgpu`'s job at
///   device-init time on the target. The intrinsic's contract is
///   "shape gate", not "semantic gate" — a green result is a promise
///   that the runtime *will get a chance* to parse the text, not
///   that the shader compiles.
/// - It does **not** normalize line endings. WGSL treats `\r\n` and
///   `\n` identically; rewriting would break byte-for-byte fingerprint
///   provenance and complicate the `KindMemorySymbol` handoff.
/// - It does **not** enforce the WebGPU §3.4 forbidden-code-point
///   list beyond NUL. That list (paired UTF-16 surrogates, C0/C1
///   control characters other than tab/newline) is checked by the
///   `naga` frontend downstream; adding it here would double the
///   maintenance burden with no compile-time benefit.
pub fn parse_wgsl_module(path: &Path, bytes: &[u8]) -> Result<WgslModuleSpec, WgslModuleError> {
    // I0210 — size cap. Cheapest check; runs first so the operator
    // sees "wrong file" before "bad bytes in wrong file".
    if bytes.len() > WGSL_MAX_SIZE_BYTES {
        return Err(WgslModuleError::TooLarge {
            path: path.to_path_buf(),
            got: bytes.len(),
            cap: WGSL_MAX_SIZE_BYTES,
        });
    }

    // I0212 — BOM at start. Runs *before* UTF-8 decode because the
    // BOM decodes to U+FEFF and would slip past `from_utf8`.
    if bytes.starts_with(UTF8_BOM) {
        return Err(WgslModuleError::Forbidden {
            path: path.to_path_buf(),
            kind: ForbiddenKind::Bom,
            offset: 0,
        });
    }

    // I0211 — UTF-8 well-formedness. Do this before the NUL scan
    // so a mostly-binary file trips the more specific "not UTF-8"
    // error rather than a stray NUL somewhere in the middle. This
    // also lets us hand the caller a `String` owned buffer for the
    // spec without a second copy.
    let source = match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(e) => {
            return Err(WgslModuleError::NotUtf8 {
                path: path.to_path_buf(),
                error_offset: e.valid_up_to(),
            });
        }
    };

    // I0212 — NUL scan. Operate on the raw bytes: NUL is a single
    // byte in UTF-8 (a valid one-byte scalar), and a byte-level
    // scan is cheaper than iterating `char_indices`. `position`
    // gives us the offset for the diagnostic in one pass.
    if let Some(offset) = bytes.iter().position(|&b| b == 0) {
        return Err(WgslModuleError::Forbidden {
            path: path.to_path_buf(),
            kind: ForbiddenKind::NulByte,
            offset,
        });
    }

    Ok(WgslModuleSpec {
        path: path.to_path_buf(),
        source,
    })
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal but syntactically shaped WGSL fragment. We are not
    /// checking grammar; we just want representative source that
    /// exercises multi-line, ASCII, and non-ASCII (comment string)
    /// bytes without triggering any of the four rejects.
    const MINIMAL_WGSL: &str = "\
// Sample compute shader — Vello ε-approximation kernel
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    // no body — shape gate only
}
";

    fn p() -> PathBuf {
        PathBuf::from("test/fixture.wgsl")
    }

    #[test]
    fn valid_wgsl_text_is_accepted_and_emits_rodata_wgsl_section() {
        let spec =
            parse_wgsl_module(&p(), MINIMAL_WGSL.as_bytes()).expect("valid WGSL must be accepted");

        assert_eq!(spec.path, p());
        assert_eq!(spec.source, MINIMAL_WGSL);

        // Handoff shape matches the SPIR-V sibling contract.
        let sym = spec.to_memory_symbol();
        assert_eq!(sym.section, ".rodata.wgsl");
        assert_eq!(sym.bytes, MINIMAL_WGSL.as_bytes());
    }

    #[test]
    fn empty_source_is_accepted_and_yields_zero_length_section() {
        // WGSL grammar rejects an empty module downstream, but the
        // shape gate does not — a zero-byte file is well-formed
        // UTF-8, has no NUL, no BOM, and fits under the cap. This
        // pin locks that boundary against a well-meaning tightening
        // that would break `@wgsl_module("empty.wgsl")` as a
        // documentation-only sentinel.
        let spec = parse_wgsl_module(&p(), b"").expect("empty file must pass the shape gate");
        assert_eq!(spec.source, "");
        let sym = spec.to_memory_symbol();
        assert_eq!(sym.section, ".rodata.wgsl");
        assert!(sym.bytes.is_empty());
    }

    #[test]
    fn nul_byte_anywhere_is_rejected_with_i0212_nul_variant() {
        // Craft a source that is otherwise valid UTF-8 and short,
        // but carries a NUL in the middle. The offset in the
        // diagnostic must point at the NUL, not at the end of the
        // buffer.
        let mut bytes = b"fn main() {\0}".to_vec();
        let expected_offset = bytes.iter().position(|&b| b == 0).unwrap();

        let err = parse_wgsl_module(&p(), &bytes).expect_err("NUL byte must be rejected");

        assert_eq!(err.code(), "I0212");
        match err {
            WgslModuleError::Forbidden { kind, offset, .. } => {
                assert_eq!(kind, ForbiddenKind::NulByte);
                assert_eq!(offset, expected_offset);
            }
            other => panic!("expected Forbidden::NulByte, got {other:?}"),
        }

        // Guard the message shape so tooling that greps the terminal
        // for `NUL` keeps working.
        bytes.clear();
        bytes.push(0);
        let err2 = parse_wgsl_module(&p(), &bytes).unwrap_err();
        assert!(err2.message().contains("NUL"));
        assert!(err2.message().contains("byte 0"));
    }

    #[test]
    fn nul_at_start_is_rejected_at_offset_zero() {
        // A file that begins with NUL trips the same code path but
        // must not be misclassified as a BOM (the BOM check runs
        // first; NUL is not the BOM prefix).
        let bytes = b"\0 fn main() {}";
        let err = parse_wgsl_module(&p(), bytes).unwrap_err();
        match err {
            WgslModuleError::Forbidden { kind, offset, .. } => {
                assert_eq!(kind, ForbiddenKind::NulByte);
                assert_eq!(offset, 0);
            }
            other => panic!("expected Forbidden::NulByte, got {other:?}"),
        }
    }

    #[test]
    fn bom_prefix_is_rejected_with_i0212_bom_variant() {
        // Byte-order mark U+FEFF encoded as UTF-8 (EF BB BF) followed
        // by an otherwise-valid shader. Must trip I0212 with the BOM
        // discriminator, *not* the NUL discriminator, and *not* I0211
        // (BOM is valid UTF-8 by itself).
        let mut bytes = Vec::new();
        bytes.extend_from_slice(UTF8_BOM);
        bytes.extend_from_slice(MINIMAL_WGSL.as_bytes());

        let err = parse_wgsl_module(&p(), &bytes).expect_err("BOM must be rejected");
        assert_eq!(err.code(), "I0212");
        match err {
            WgslModuleError::Forbidden { kind, offset, .. } => {
                assert_eq!(kind, ForbiddenKind::Bom);
                assert_eq!(offset, 0);
            }
            other => panic!("expected Forbidden::Bom, got {other:?}"),
        }
    }

    #[test]
    fn bom_check_runs_before_utf8_check_on_bom_only_input() {
        // A file that is *only* the BOM is trivially valid UTF-8;
        // I0212 must still fire, proving the BOM check runs before
        // the UTF-8 decode short-circuits to Ok.
        let err = parse_wgsl_module(&p(), UTF8_BOM).unwrap_err();
        assert_eq!(err.code(), "I0212");
        assert!(matches!(
            err,
            WgslModuleError::Forbidden {
                kind: ForbiddenKind::Bom,
                offset: 0,
                ..
            }
        ));
    }

    #[test]
    fn invalid_utf8_is_rejected_with_i0211_at_first_bad_offset() {
        // Continuation-byte-in-lead-position (`0xC0` followed by an
        // ASCII byte the decoder cannot pair with) is the canonical
        // "not UTF-8" failure. The reported offset must equal
        // `valid_up_to` for the operator to jump straight to the
        // bad byte.
        let bytes = [b'f', b'n', b' ', 0xC0, 0x28, b';'];
        let err = parse_wgsl_module(&p(), &bytes).expect_err("invalid UTF-8 must be rejected");
        assert_eq!(err.code(), "I0211");
        match err {
            WgslModuleError::NotUtf8 { error_offset, .. } => {
                assert_eq!(error_offset, 3);
            }
            other => panic!("expected NotUtf8, got {other:?}"),
        }
        assert!(err.message().contains("byte 3"));
    }

    #[test]
    fn oversize_is_rejected_with_i0210_and_carries_both_sizes() {
        // Exactly one byte over the cap. The size check runs before
        // any content inspection, so we can feed a repeating ASCII
        // byte without paying for a bespoke shader fixture.
        let bytes = vec![b'a'; WGSL_MAX_SIZE_BYTES + 1];
        let err = parse_wgsl_module(&p(), &bytes).expect_err("oversize must be rejected");

        assert_eq!(err.code(), "I0210");
        match err {
            WgslModuleError::TooLarge { got, cap, .. } => {
                assert_eq!(got, WGSL_MAX_SIZE_BYTES + 1);
                assert_eq!(cap, WGSL_MAX_SIZE_BYTES);
            }
            other => panic!("expected TooLarge, got {other:?}"),
        }
    }

    #[test]
    fn exactly_at_cap_is_accepted() {
        // Boundary: `>` in the size check, not `>=`. Locking this
        // behaviour prevents a silent tightening that would reject
        // a file that fits exactly.
        let bytes = vec![b'a'; WGSL_MAX_SIZE_BYTES];
        let spec = parse_wgsl_module(&p(), &bytes)
            .expect("source at exactly the cap must be accepted");
        assert_eq!(spec.source.len(), WGSL_MAX_SIZE_BYTES);
    }

    #[test]
    fn size_check_precedes_utf8_check() {
        // Oversize AND invalid UTF-8 — the size error must win, so
        // the operator's diagnostic points at the file-shape problem
        // rather than at a byte deep inside an over-large blob.
        let mut bytes = vec![0xFFu8; WGSL_MAX_SIZE_BYTES + 16];
        // Sprinkle a NUL too, to guard against the NUL check winning
        // as well.
        bytes[100] = 0;
        let err = parse_wgsl_module(&p(), &bytes).unwrap_err();
        assert_eq!(
            err.code(),
            "I0210",
            "size must be reported before content problems"
        );
    }

    #[test]
    fn bom_check_precedes_nul_check() {
        // BOM prefix + a NUL later in the source. The BOM must
        // be the reported failure; NUL is deeper in the file and
        // less actionable when the operator opens their editor.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(UTF8_BOM);
        bytes.extend_from_slice(b"fn main() {\0}");
        let err = parse_wgsl_module(&p(), &bytes).unwrap_err();
        match err {
            WgslModuleError::Forbidden { kind, offset, .. } => {
                assert_eq!(kind, ForbiddenKind::Bom);
                assert_eq!(offset, 0);
            }
            other => panic!("expected BOM to win, got {other:?}"),
        }
    }

    #[test]
    fn display_impl_prepends_code() {
        // Display shape is what non-diagnostic-aware callers (bin
        // drivers, `dbg!`) surface; pin it.
        let err = WgslModuleError::TooLarge {
            path: p(),
            got: 2 * WGSL_MAX_SIZE_BYTES,
            cap: WGSL_MAX_SIZE_BYTES,
        };
        let rendered = format!("{err}");
        assert!(rendered.starts_with("[I0210]"), "got: {rendered}");
    }

    #[test]
    fn forbidden_kind_labels_are_stable() {
        // These labels appear verbatim in operator-facing diagnostics
        // and are grepped by higher-layer tooling; pin them.
        assert_eq!(ForbiddenKind::NulByte.as_str(), "NUL byte (U+0000)");
        assert_eq!(
            ForbiddenKind::Bom.as_str(),
            "UTF-8 byte-order-mark (U+FEFF)"
        );
    }

    #[test]
    fn memory_symbol_section_is_dotted_and_named_wgsl() {
        // Guard the section name against a drift that would silently
        // land WGSL text into `.rodata.spirv` or an undotted variant
        // the linker script wouldn't group.
        let spec = parse_wgsl_module(&p(), MINIMAL_WGSL.as_bytes()).unwrap();
        let sym = spec.to_memory_symbol();
        assert!(sym.section.starts_with('.'), "section must be dotted");
        assert!(
            sym.section.ends_with(".wgsl"),
            "section must end with .wgsl"
        );
        assert_eq!(sym.section, ".rodata.wgsl");
    }
}
