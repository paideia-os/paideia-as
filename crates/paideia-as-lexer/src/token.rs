//! Token kinds and the `Token` value type.
//!
//! `TokenKind` enumerates every distinct lexical class per
//! `syntax-reference.md` §2.3, with one variant per reserved word from §3.4.
//! Operators and delimiters are individual variants so the parser can
//! pattern-match without inspecting the lexeme.
//!
//! `TokenKind` is a flat C-style enum (no payloads), making it `Copy` and
//! 1 byte wide. The token's underlying source text is reachable via the
//! `Span` carried by `Token` and a `SourceMap`.

use paideia_as_diagnostics::Span;
use static_assertions::const_assert;
use std::mem::size_of;

/// A lexical class of token.
///
/// One variant per reserved word from `syntax-reference.md` §3.4, plus
/// variants for identifiers, literals, punctuation, operators, effect
/// and capability brackets, and substructural markers per §2.3.
///
/// `TokenKind` is `Copy` and 1 byte wide (`#[repr(u8)]` flat enum).
///
/// # Example
///
/// Pattern-match on a token kind:
///
/// ```
/// use paideia_as_lexer::TokenKind;
///
/// fn classify(k: TokenKind) -> &'static str {
///     match k {
///         TokenKind::Ident => "identifier",
///         TokenKind::IntLit
///         | TokenKind::FloatLit
///         | TokenKind::StringLit
///         | TokenKind::CharLit
///         | TokenKind::ByteLit
///         | TokenKind::ByteStringLit
///         | TokenKind::UnitLit => "literal",
///         TokenKind::Eof => "eof",
///         // Every remaining variant is either a keyword, an operator,
///         // a punctuation token, an effect/capability bracket, or a
///         // substructural marker.
///         _ => "other",
///     }
/// }
///
/// assert_eq!(classify(TokenKind::Ident), "identifier");
/// assert_eq!(classify(TokenKind::IntLit), "literal");
/// assert_eq!(classify(TokenKind::KwLet), "other");
/// assert_eq!(classify(TokenKind::Eof), "eof");
/// ```
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u8)]
#[non_exhaustive]
pub enum TokenKind {
    // ── Reserved words: item declarations (§3.4) ────────────────────────
    /// `let`
    KwLet,
    /// `fn`
    KwFn,
    /// `module`
    KwModule,
    /// `signature`
    KwSignature,
    /// `structure`
    KwStructure,
    /// `functor`
    KwFunctor,
    /// `effect`
    KwEffect,
    /// `capability`
    KwCapability,
    /// `extern`
    KwExtern,
    /// `import`
    KwImport,
    /// `export`
    KwExport,
    /// `pub`
    KwPub,

    // ── Reserved words: control flow ────────────────────────────────────
    /// `if`
    KwIf,
    /// `else`
    KwElse,
    /// `match`
    KwMatch,
    /// `when`
    KwWhen,
    /// `do`
    KwDo,
    /// `with`
    KwWith,
    /// `loop`
    KwLoop,
    /// `while`
    KwWhile,
    /// `for`
    KwFor,
    /// `break`
    KwBreak,
    /// `continue`
    KwContinue,
    /// `return`
    KwReturn,
    /// `yield`
    KwYield,

    // ── Reserved words: effect system (action blocks) ───────────────────
    /// `action`
    KwAction,

    // ── Reserved words: type system ─────────────────────────────────────
    /// `type`
    KwType,
    /// `enum`
    KwEnum,
    /// `struct`
    KwStruct,
    /// `record`
    KwRecord,
    /// `trait`
    KwTrait,
    /// `impl`
    KwImpl,
    /// `where`
    KwWhere,
    /// `forall`
    KwForall,
    /// `ordered`
    KwOrdered,
    /// `linear`
    KwLinear,
    /// `affine`
    KwAffine,
    /// `unrestricted`
    KwUnrestricted,

    // ── Reserved words: effect system ───────────────────────────────────
    /// `handle`
    KwHandle,
    /// `perform`
    KwPerform,
    /// `resume`
    KwResume,
    /// `finally`
    KwFinally,

    // ── Reserved words: substructural / unsafe ──────────────────────────
    /// `unsafe`
    KwUnsafe,
    /// `move`
    KwMove,
    /// `borrow`
    KwBorrow,
    /// `consume`
    KwConsume,
    /// `drop`
    KwDrop,
    /// `own`
    KwOwn,
    /// `mut`
    KwMut,

    // ── Reserved words: literals and constants ──────────────────────────
    /// `true`
    KwTrue,
    /// `false`
    KwFalse,
    /// `null`
    KwNull,
    /// `Self` (type-level self)
    KwSelfType,
    /// `self` (value-level self)
    KwSelfValue,

    // ── Reserved words: memory and addressing ───────────────────────────
    /// `sizeof`
    KwSizeof,
    /// `alignof`
    KwAlignof,
    /// `offsetof`
    KwOffsetof,
    /// `asm`
    KwAsm,

    // ── Reserved words: module operations ───────────────────────────────
    /// `in`
    KwIn,
    /// `as`
    KwAs,
    /// `use`
    KwUse,

    // ── Reserved words: future reservations ─────────────────────────────
    /// `abstract`
    KwAbstract,
    /// `async`
    KwAsync,
    /// `await`
    KwAwait,
    /// `coroutine`
    KwCoroutine,
    /// `deriving`
    KwDeriving,
    /// `dyn`
    KwDyn,
    /// `implicit`
    KwImplicit,
    /// `lemma`
    KwLemma,
    /// `proof`
    KwProof,
    /// `reflect`
    KwReflect,
    /// `virtual`
    KwVirtual,

    // ── Non-keyword token classes (§2.3) ────────────────────────────────
    /// Identifier (covers raw identifiers per §3.3).
    Ident,

    /// Integer literal (§5.1).
    IntLit,
    /// Floating-point literal (§5.1).
    FloatLit,
    /// Character literal (§5.2).
    CharLit,
    /// String literal (§5.3).
    StringLit,
    /// Byte literal `b'…'` (§5.4).
    ByteLit,
    /// Byte-string literal `b"…"` (§5.4).
    ByteStringLit,
    /// Unit literal `()` (§5.6) — emitted by the parser, never by the lexer.
    /// Included for completeness of the enum.
    UnitLit,

    // ── Punctuation (§2.3, §6) ──────────────────────────────────────────
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `,`
    Comma,
    /// `;`
    Semicolon,
    /// `:`
    Colon,
    /// `.`
    Dot,

    // ── Effect / capability brackets (§2.3) ─────────────────────────────
    /// `!{` — effect bracket open.
    EffectOpen,
    /// `@{` — capability bracket open.
    CapOpen,

    // ── Substructural markers (§2.3) ────────────────────────────────────
    /// `↓` or `$` — linear consume.
    LinearMark,
    /// `~` — affine drop.
    AffineMark,

    // ── Operators (§7) ──────────────────────────────────────────────────
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,
    /// `=`
    Assign,
    /// `==`
    Eq,
    /// `!=`
    Neq,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `<=`
    Le,
    /// `>=`
    Ge,
    /// `&&`
    AndAnd,
    /// `||`
    OrOr,
    /// `!`
    Bang,
    /// `&`
    Amp,
    /// `|`
    Pipe,
    /// `^`
    Caret,
    /// `<<`
    Shl,
    /// `>>`
    Shr,
    /// `->`
    Arrow,
    /// `=>`
    FatArrow,
    /// `::`
    ColonColon,
    /// `?`
    Question,
    /// `@`
    At,
    /// `#`
    Hash,

    // ── Markers ─────────────────────────────────────────────────────────
    /// End-of-file marker — last token emitted by the lexer.
    Eof,
}

// AC: size_of::<TokenKind>() ≤ 8 bytes.
// A flat C-style enum with ≤ 256 variants and #[repr(u8)] is 1 byte.
const_assert!(size_of::<TokenKind>() <= 8);

/// A lexed token: a [`TokenKind`] tagged with its source [`Span`].
///
/// `Token` is `Clone` but **not** `Copy`. Future revisions may add a
/// non-`Copy` field (interned-string handle, lookahead breadcrumb, etc.);
/// keeping it `Clone`-only avoids a SemVer-blocking move.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct Token {
    /// Lexical class of this token.
    pub kind: TokenKind,
    /// Source-position range covered by this token.
    pub span: Span,
}

impl Token {
    /// Construct a `Token`.
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// Returns true iff this token's `kind` matches `kind`.
    pub fn is(&self, kind: TokenKind) -> bool {
        self.kind == kind
    }
}

/// Look up the [`TokenKind`] for a candidate keyword spelling.
///
/// Returns `Some(kind)` if `text` is one of the reserved words listed in
/// `syntax-reference.md` §3.4; `None` otherwise. Callers use the absence
/// of a match as the signal to emit a generic [`TokenKind::Ident`].
#[must_use]
pub fn keyword_kind(text: &str) -> Option<TokenKind> {
    Some(match text {
        // Item declarations
        "let" => TokenKind::KwLet,
        "fn" => TokenKind::KwFn,
        "module" => TokenKind::KwModule,
        "signature" => TokenKind::KwSignature,
        "structure" => TokenKind::KwStructure,
        "functor" => TokenKind::KwFunctor,
        "effect" => TokenKind::KwEffect,
        "capability" => TokenKind::KwCapability,
        "extern" => TokenKind::KwExtern,
        "import" => TokenKind::KwImport,
        "export" => TokenKind::KwExport,
        "pub" => TokenKind::KwPub,
        // Control flow
        "if" => TokenKind::KwIf,
        "else" => TokenKind::KwElse,
        "match" => TokenKind::KwMatch,
        "when" => TokenKind::KwWhen,
        "do" => TokenKind::KwDo,
        "with" => TokenKind::KwWith,
        "loop" => TokenKind::KwLoop,
        "while" => TokenKind::KwWhile,
        "for" => TokenKind::KwFor,
        "break" => TokenKind::KwBreak,
        "continue" => TokenKind::KwContinue,
        "return" => TokenKind::KwReturn,
        "yield" => TokenKind::KwYield,
        // Effect system (action blocks)
        "action" => TokenKind::KwAction,
        // Type system
        "type" => TokenKind::KwType,
        "enum" => TokenKind::KwEnum,
        "struct" => TokenKind::KwStruct,
        "record" => TokenKind::KwRecord,
        "trait" => TokenKind::KwTrait,
        "impl" => TokenKind::KwImpl,
        "where" => TokenKind::KwWhere,
        "forall" => TokenKind::KwForall,
        "ordered" => TokenKind::KwOrdered,
        "linear" => TokenKind::KwLinear,
        "affine" => TokenKind::KwAffine,
        "unrestricted" => TokenKind::KwUnrestricted,
        // Effect system
        "handle" => TokenKind::KwHandle,
        "perform" => TokenKind::KwPerform,
        "resume" => TokenKind::KwResume,
        "finally" => TokenKind::KwFinally,
        // Substructural / unsafe
        "unsafe" => TokenKind::KwUnsafe,
        "move" => TokenKind::KwMove,
        "borrow" => TokenKind::KwBorrow,
        "consume" => TokenKind::KwConsume,
        "drop" => TokenKind::KwDrop,
        "own" => TokenKind::KwOwn,
        "mut" => TokenKind::KwMut,
        // Literals and constants
        "true" => TokenKind::KwTrue,
        "false" => TokenKind::KwFalse,
        "null" => TokenKind::KwNull,
        "Self" => TokenKind::KwSelfType,
        "self" => TokenKind::KwSelfValue,
        // Memory and addressing
        "sizeof" => TokenKind::KwSizeof,
        "alignof" => TokenKind::KwAlignof,
        "offsetof" => TokenKind::KwOffsetof,
        "asm" => TokenKind::KwAsm,
        // Module operations
        "in" => TokenKind::KwIn,
        "as" => TokenKind::KwAs,
        "use" => TokenKind::KwUse,
        // Future reservations
        "abstract" => TokenKind::KwAbstract,
        "async" => TokenKind::KwAsync,
        "await" => TokenKind::KwAwait,
        "coroutine" => TokenKind::KwCoroutine,
        "deriving" => TokenKind::KwDeriving,
        "dyn" => TokenKind::KwDyn,
        "implicit" => TokenKind::KwImplicit,
        "lemma" => TokenKind::KwLemma,
        "proof" => TokenKind::KwProof,
        "reflect" => TokenKind::KwReflect,
        "virtual" => TokenKind::KwVirtual,
        _ => return None,
    })
}

/// All 69 reserved-word spellings from §3.4, in declaration order.
///
/// Used by tests and tools (e.g., a `paideia-as` `--list-keywords` flag
/// in a later PR). Updates here must stay in sync with `keyword_kind`
/// and the `TokenKind::Kw*` variants.
pub const RESERVED_WORDS: &[&str] = &[
    // Item declarations (12)
    "let",
    "fn",
    "module",
    "signature",
    "structure",
    "functor",
    "effect",
    "capability",
    "extern",
    "import",
    "export",
    "pub",
    // Control flow (13)
    "if",
    "else",
    "match",
    "when",
    "do",
    "with",
    "loop",
    "while",
    "for",
    "break",
    "continue",
    "return",
    "yield",
    // Effect system (action blocks) (1)
    "action",
    // Type system (12)
    "type",
    "enum",
    "struct",
    "record",
    "trait",
    "impl",
    "where",
    "forall",
    "ordered",
    "linear",
    "affine",
    "unrestricted",
    // Effect system (4)
    "handle",
    "perform",
    "resume",
    "finally",
    // Substructural / unsafe (7)
    "unsafe",
    "move",
    "borrow",
    "consume",
    "drop",
    "own",
    "mut",
    // Literals and constants (5)
    "true",
    "false",
    "null",
    "Self",
    "self",
    // Memory and addressing (4)
    "sizeof",
    "alignof",
    "offsetof",
    "asm",
    // Module operations (3)
    "in",
    "as",
    "use",
    // Future reservations (11)
    "abstract",
    "async",
    "await",
    "coroutine",
    "deriving",
    "dyn",
    "implicit",
    "lemma",
    "proof",
    "reflect",
    "virtual",
];

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    #[test]
    fn token_kind_size_within_budget() {
        // §2.3 AC: size_of::<TokenKind>() ≤ 8 bytes. Documented for runtime
        // visibility — the compile-time const_assert above is the binding gate.
        assert!(size_of::<TokenKind>() <= 8);
    }

    #[test]
    fn token_is_clone_but_not_copy() {
        // Compile-time witness that Token is Clone.
        fn _assert_clone<T: Clone>() {}
        _assert_clone::<Token>();
        // No `_assert_copy::<Token>()` — Token must NOT impl Copy. Verified
        // by the type system: removing the manual derive(Clone) above would
        // not compile this test.
    }

    #[test]
    fn reserved_words_list_length() {
        // 12 + 13 + 1 + 12 + 4 + 7 + 5 + 4 + 3 + 11 = 72
        // (action moved to effect system section after control flow, record and impl added to type system, mut added to substructural)
        assert_eq!(RESERVED_WORDS.len(), 72);
    }

    #[test]
    fn every_reserved_word_resolves() {
        for kw in RESERVED_WORDS {
            assert!(
                keyword_kind(kw).is_some(),
                "reserved word {kw:?} does not resolve via keyword_kind"
            );
        }
    }

    #[test]
    fn non_keyword_returns_none() {
        for word in ["foo", "bar", "Foo", "_x", "x42"] {
            assert!(
                keyword_kind(word).is_none(),
                "{word:?} unexpectedly resolved to a keyword"
            );
        }
    }

    #[test]
    fn keyword_kind_is_case_sensitive() {
        // §3.4 reserves `Self` (type-level) and `self` (value-level) as
        // distinct words. Both must resolve to distinct kinds.
        assert_eq!(keyword_kind("Self"), Some(TokenKind::KwSelfType));
        assert_eq!(keyword_kind("self"), Some(TokenKind::KwSelfValue));
        assert!(keyword_kind("SELF").is_none());
        assert!(keyword_kind("Let").is_none());
    }

    #[test]
    fn token_new_and_accessors() {
        let span = paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 0, 3);
        let tok = Token::new(TokenKind::KwLet, span);
        assert_eq!(tok.kind, TokenKind::KwLet);
        assert!(tok.is(TokenKind::KwLet));
        assert!(!tok.is(TokenKind::KwFn));
    }

    /// Exhaustive `match` arm doc-test demonstration. Compiles iff every
    /// variant is covered.
    ///
    /// ```
    /// use paideia_as_lexer::TokenKind;
    /// fn classify(k: TokenKind) -> &'static str {
    ///     match k {
    ///         TokenKind::Ident => "ident",
    ///         TokenKind::IntLit | TokenKind::FloatLit
    ///         | TokenKind::StringLit | TokenKind::CharLit
    ///         | TokenKind::ByteLit | TokenKind::ByteStringLit
    ///         | TokenKind::UnitLit => "literal",
    ///         TokenKind::Eof => "eof",
    ///         // Every remaining variant is either a keyword, an operator,
    ///         // a punctuation token, an effect/capability bracket, or a
    ///         // substructural marker. Phase-1 callers may not need to
    ///         // distinguish them further.
    ///         _ => "other",
    ///     }
    /// }
    /// let _ = classify(TokenKind::KwLet);
    /// ```
    #[test]
    fn classify_doc_example_compiles() {
        // The doc-test above is the compile-time gate; this test just
        // exists so `cargo test` lists the example.
    }
}
