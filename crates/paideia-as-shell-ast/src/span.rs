//! Source-span provenance for every [`crate::SyntaxNode`].
//!
//! Every node carries a [`NodeSpan`] with three fields per Q-A5:
//!
//! * `original` — byte range into the source *the user typed*, before
//!   any NFC transform. This is what LSP diagnostics resolve so
//!   squigglies land under the user's own bytes.
//! * `nfc` — byte range into the post-NFC source the parser walked.
//!   Used for round-trip pretty-printing and inter-node ordering.
//! * `context` — the sub-language grammar the node was parsed under
//!   (from [`paideia_as_shell_lex::Context`]). Kept on the span rather
//!   than the node variant because a REPL syntax colorer wants to key
//!   off it without dispatching over 20+ enum arms.

use paideia_as_shell_lex::Context;

/// A closed byte range `[start, end)` into some source. Kept as a bare
/// tuple pair on [`NodeSpan`] rather than a nested struct so `Debug`
/// output for a node stays legible at a glance (`(3, 10)` reads faster
/// than `Range { start: 3, end: 10 }`).
pub type ByteRange = (usize, usize);

/// The span metadata every [`crate::SyntaxNode`] carries.
///
/// # Layout
///
/// `#[repr(C)]` would fix the field order for FFI, but this type never
/// crosses an FFI boundary — the shell AST lives entirely in-process
/// (R226 evaluator, R229 REPL) — so we let the compiler reorder for
/// packing. `Copy` because the three ranges + context enum fit in
/// 5 × usize (40 B on 64-bit), and pass-by-value keeps the parser's
/// hot loop free of clone traffic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeSpan {
    /// Byte range into the pre-NFC source. Diagnostics resolve this.
    pub original: ByteRange,
    /// Byte range into the post-NFC source. The parser walked this.
    pub nfc: ByteRange,
    /// Sub-language grammar under which the node was parsed.
    pub context: Context,
}

impl NodeSpan {
    /// Construct a span from its components.
    #[inline]
    pub fn new(original: ByteRange, nfc: ByteRange, context: Context) -> Self {
        debug_assert!(original.0 <= original.1, "reversed original span");
        debug_assert!(nfc.0 <= nfc.1, "reversed nfc span");
        Self { original, nfc, context }
    }

    /// A synthetic span used by tests and constructors that predate a
    /// real source position. Both ranges collapse to `(0, 0)`.
    #[inline]
    pub fn synthetic(context: Context) -> Self {
        Self {
            original: (0, 0),
            nfc: (0, 0),
            context,
        }
    }

    /// Union of two spans; the result covers the leftmost start and
    /// rightmost end of both spans in both coordinate systems and
    /// keeps `self.context`. Cross-context unions are legitimate: a
    /// Pipeline `Cmd` node whose args include a Lambda block or a
    /// Datalog block sits at Pipeline context but spans bytes stamped
    /// with the sub-block's own context. The parent node's context is
    /// always the LHS's — the caller controls the operand order.
    pub fn union(self, other: NodeSpan) -> NodeSpan {
        NodeSpan {
            original: (
                self.original.0.min(other.original.0),
                self.original.1.max(other.original.1),
            ),
            nfc: (self.nfc.0.min(other.nfc.0), self.nfc.1.max(other.nfc.1)),
            context: self.context,
        }
    }

    /// Union with an explicit context override — for the DatalogBlock /
    /// Lambda constructors where the block-as-a-whole sits at a
    /// different context than either of the spans being unioned.
    pub fn union_with_context(
        self,
        other: NodeSpan,
        context: Context,
    ) -> NodeSpan {
        let u = self.union(other);
        NodeSpan { context, ..u }
    }

    /// Length in bytes of the pre-NFC (user-typed) source range.
    #[inline]
    pub fn original_len(&self) -> usize {
        self.original.1 - self.original.0
    }

    /// Length in bytes of the post-NFC (parser-observed) source range.
    #[inline]
    pub fn nfc_len(&self) -> usize {
        self.nfc.1 - self.nfc.0
    }
}
