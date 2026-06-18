//! Parser plumbing: state struct, expect/peek primitives, recovery
//! synchronization.
//!
//! Phase-1 produces parser diagnostics in the `P0100-P0299` range per
//! `diagnostics.md` §2. `expect` emits `P0100` ("unexpected token") on
//! mismatch and returns `Err(())`; the parser body matches on that to
//! call [`Parser::recover_to_one_of`] and continue.

use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{
    Category, Diagnostic, DiagnosticCode, DiagnosticSink, FileId, Severity,
};
use paideia_as_lexer::{Token, TokenKind};

/// Error returned when [`Parser::expect`] sees the wrong token. Carries
/// no payload because the diagnostic has already been emitted; callers
/// check for `Err` and dispatch to [`Parser::recover_to_one_of`].
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ParseError;

use crate::cursor::TokenCursor;

/// Parser state: cursor + arena + diagnostic sink + the source file.
///
/// The sink is borrowed mutably; the arena is borrowed mutably; the
/// cursor is owned by the parser and can be cheaply restored from a
/// snapshot. The lifetime parameters: `'tok` is the token slice, `'ast`
/// is the AST arena, `'snk` is the sink.
pub struct Parser<'tok, 'ast, 'snk> {
    cursor: TokenCursor<'tok>,
    arena: &'ast mut AstArena,
    sink: &'snk mut dyn DiagnosticSink,
    file: FileId,
}

impl<'tok, 'ast, 'snk> Parser<'tok, 'ast, 'snk> {
    /// Construct a parser over `tokens` writing AST into `arena` and
    /// diagnostics into `sink`.
    pub fn new(
        tokens: &'tok [Token],
        file: FileId,
        arena: &'ast mut AstArena,
        sink: &'snk mut dyn DiagnosticSink,
    ) -> Self {
        Self {
            cursor: TokenCursor::new(tokens, file),
            arena,
            sink,
            file,
        }
    }

    /// Borrow the arena (immutable view).
    #[must_use]
    pub fn arena(&self) -> &AstArena {
        self.arena
    }

    /// Borrow the arena mutably.
    pub fn arena_mut(&mut self) -> &mut AstArena {
        self.arena
    }

    /// The file this parser is parsing.
    #[must_use]
    pub fn file(&self) -> FileId {
        self.file
    }

    /// Peek the current token (None past EOF).
    #[must_use]
    pub fn peek(&self) -> Option<&'tok Token> {
        self.cursor.peek()
    }

    /// `true` if the current token is `kind`.
    #[must_use]
    pub fn at(&self, kind: TokenKind) -> bool {
        self.cursor.at(kind)
    }

    /// Consume and return the current token.
    pub fn bump(&mut self) -> Option<Token> {
        self.cursor.bump()
    }

    /// Consume the current token if it matches `kind`, returning whether
    /// the consumption occurred. Useful for optional punctuation.
    pub fn eat(&mut self, kind: TokenKind) -> bool {
        if self.cursor.at(kind) {
            self.cursor.bump();
            true
        } else {
            false
        }
    }

    /// Consume the current token if its kind equals `kind` (returning it
    /// on success), or emit `P0100` and return `Err(())` on mismatch.
    ///
    /// The diagnostic message reads `"expected <kind>, found <actual>"`.
    /// On error, the cursor is NOT advanced — the parser body is
    /// responsible for calling [`Parser::recover_to_one_of`] to skip
    /// ahead.
    pub fn expect(&mut self, kind: TokenKind) -> Result<Token, ParseError> {
        if self.cursor.at(kind) {
            Ok(self.cursor.bump().expect("at(kind) implies peek() is Some"))
        } else {
            let actual = self.cursor.current_kind();
            let span = self.cursor.current_span();
            let diag = Diagnostic::error(p_code(100))
                .message(format!(
                    "expected {}, found {}",
                    debug_kind(kind),
                    debug_kind(actual)
                ))
                .with_span(span)
                .finish();
            let _ = self.sink.emit(diag);
            Err(ParseError)
        }
    }

    #[cfg(test)]
    pub(crate) fn cursor_position_for_test(&self) -> usize {
        self.cursor.position()
    }

    #[cfg(test)]
    pub(crate) fn cursor_current_kind_for_test(&self) -> TokenKind {
        self.cursor.current_kind()
    }

    #[cfg(test)]
    pub(crate) fn cursor_is_at_end_for_test(&self) -> bool {
        self.cursor.is_at_end()
    }

    /// Skip tokens until the current token's kind is one of `kinds` (or
    /// EOF). Does not consume the matching token. Used to synchronize
    /// after a parse error to a known recovery point — typically the
    /// next statement separator or a closing brace.
    pub fn recover_to_one_of(&mut self, kinds: &[TokenKind]) {
        loop {
            if self.cursor.is_at_end() {
                return;
            }
            let cur = self.cursor.current_kind();
            if kinds.contains(&cur) {
                return;
            }
            self.cursor.bump();
        }
    }

    /// Emit a diagnostic through the sink. Used by parsers to report errors.
    pub(crate) fn emit_diagnostic(&mut self, diag: Diagnostic) {
        let _ = self.sink.emit(diag);
    }
}

/// Construct a P-category diagnostic code at the given number, returning
/// the `DiagnosticCode`.
fn p_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::P, Severity::Error, n).expect("valid P code")
}

/// Human-readable label for a TokenKind. Used in P0100 error messages.
/// Intentionally not exhaustive: every variant maps to its source-text
/// form where one exists, falling back to the variant name.
fn debug_kind(kind: TokenKind) -> &'static str {
    use TokenKind::*;
    match kind {
        // Keywords print as their source spelling.
        KwLet => "`let`",
        KwFn => "`fn`",
        KwModule => "`module`",
        KwSignature => "`signature`",
        KwStructure => "`structure`",
        KwFunctor => "`functor`",
        KwEffect => "`effect`",
        KwCapability => "`capability`",
        KwExtern => "`extern`",
        KwImport => "`import`",
        KwExport => "`export`",
        KwPub => "`pub`",
        KwIf => "`if`",
        KwElse => "`else`",
        KwMatch => "`match`",
        KwWhen => "`when`",
        KwDo => "`do`",
        KwWith => "`with`",
        KwLoop => "`loop`",
        KwWhile => "`while`",
        KwFor => "`for`",
        KwBreak => "`break`",
        KwContinue => "`continue`",
        KwReturn => "`return`",
        KwYield => "`yield`",
        KwAction => "`action`",
        KwType => "`type`",
        KwEnum => "`enum`",
        KwStruct => "`struct`",
        KwTrait => "`trait`",
        KwWhere => "`where`",
        KwForall => "`forall`",
        KwOrdered => "`ordered`",
        KwLinear => "`linear`",
        KwAffine => "`affine`",
        KwUnrestricted => "`unrestricted`",
        KwHandle => "`handle`",
        KwPerform => "`perform`",
        KwResume => "`resume`",
        KwFinally => "`finally`",
        KwUnsafe => "`unsafe`",
        KwMove => "`move`",
        KwBorrow => "`borrow`",
        KwConsume => "`consume`",
        KwDrop => "`drop`",
        KwOwn => "`own`",
        KwTrue => "`true`",
        KwFalse => "`false`",
        KwNull => "`null`",
        KwSelfType => "`Self`",
        KwSelfValue => "`self`",
        KwSizeof => "`sizeof`",
        KwAlignof => "`alignof`",
        KwOffsetof => "`offsetof`",
        KwAsm => "`asm`",
        KwIn => "`in`",
        KwAs => "`as`",
        KwUse => "`use`",
        KwAbstract | KwAsync | KwAwait | KwCoroutine | KwDeriving | KwDyn | KwImplicit
        | KwLemma | KwProof | KwReflect | KwVirtual => "reserved word",
        Ident => "identifier",
        IntLit | FloatLit | CharLit | StringLit | ByteLit | ByteStringLit | UnitLit => "literal",
        LParen => "`(`",
        RParen => "`)`",
        LBrace => "`{`",
        RBrace => "`}`",
        LBracket => "`[`",
        RBracket => "`]`",
        Comma => "`,`",
        Semicolon => "`;`",
        Colon => "`:`",
        Dot => "`.`",
        EffectOpen => "`!{`",
        CapOpen => "`@{`",
        LinearMark => "linear-consume marker",
        AffineMark => "`~`",
        Plus => "`+`",
        Minus => "`-`",
        Star => "`*`",
        Slash => "`/`",
        Percent => "`%`",
        Assign => "`=`",
        Eq => "`==`",
        Neq => "`!=`",
        Lt => "`<`",
        Gt => "`>`",
        Le => "`<=`",
        Ge => "`>=`",
        AndAnd => "`&&`",
        OrOr => "`||`",
        Bang => "`!`",
        Amp => "`&`",
        Pipe => "`|`",
        Caret => "`^`",
        Shl => "`<<`",
        Shr => "`>>`",
        Arrow => "`->`",
        FatArrow => "`=>`",
        ColonColon => "`::`",
        Question => "`?`",
        At => "`@`",
        Hash => "`#`",
        Eof => "end of input",
        _ => "token",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{Span, VecSink};

    fn span(byte_start: u32, byte_len: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), byte_start, byte_len)
    }

    fn tok(kind: TokenKind, byte_start: u32, byte_len: u32) -> Token {
        Token::new(kind, span(byte_start, byte_len))
    }

    #[test]
    fn expect_passes_on_match() {
        let toks = vec![tok(TokenKind::KwLet, 0, 3)];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(&toks, FileId::new(1).unwrap(), &mut arena, &mut sink);
        let tok_consumed = p.expect(TokenKind::KwLet).unwrap();
        assert_eq!(tok_consumed.kind, TokenKind::KwLet);
        assert_eq!(sink.diagnostics().len(), 0);
    }

    #[test]
    fn expect_emits_p0100_on_mismatch() {
        let toks = vec![tok(TokenKind::KwFn, 0, 2)];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        {
            let mut p = Parser::new(&toks, FileId::new(1).unwrap(), &mut arena, &mut sink);
            let result = p.expect(TokenKind::KwLet);
            assert!(result.is_err());
            // Cursor not advanced: re-peek should still see `fn`.
            assert!(p.at(TokenKind::KwFn));
        }
        assert_eq!(sink.diagnostics().len(), 1);
        let diag = &sink.diagnostics()[0];
        assert_eq!(diag.code().category(), Category::P);
        assert_eq!(diag.code().number(), 100);
        assert!(diag.message().contains("`let`"));
        assert!(diag.message().contains("`fn`"));
    }

    #[test]
    fn at_is_non_consuming() {
        let toks = vec![tok(TokenKind::KwLet, 0, 3)];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let p = Parser::new(&toks, FileId::new(1).unwrap(), &mut arena, &mut sink);
        assert!(p.at(TokenKind::KwLet));
        assert!(!p.at(TokenKind::KwFn));
        assert_eq!(p.cursor_position_for_test(), 0);
    }

    #[test]
    fn eat_consumes_on_match() {
        let toks = vec![tok(TokenKind::Semicolon, 0, 1), tok(TokenKind::KwLet, 1, 3)];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(&toks, FileId::new(1).unwrap(), &mut arena, &mut sink);
        assert!(p.eat(TokenKind::Semicolon));
        assert_eq!(p.cursor_position_for_test(), 1);
        assert!(!p.eat(TokenKind::Semicolon));
        assert_eq!(p.cursor_position_for_test(), 1);
    }

    #[test]
    fn recover_to_one_of_skips_through_semicolon() {
        let toks = vec![
            tok(TokenKind::KwLet, 0, 3),
            tok(TokenKind::Ident, 4, 1),
            tok(TokenKind::Plus, 5, 1),
            tok(TokenKind::Semicolon, 6, 1),
            tok(TokenKind::KwFn, 7, 2),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(&toks, FileId::new(1).unwrap(), &mut arena, &mut sink);
        p.recover_to_one_of(&[TokenKind::Semicolon, TokenKind::RBrace]);
        // Stopped at the semicolon (not consumed).
        assert_eq!(p.cursor_current_kind_for_test(), TokenKind::Semicolon);
    }

    #[test]
    fn recover_to_one_of_handles_eof_without_panic() {
        let toks = vec![tok(TokenKind::KwLet, 0, 3), tok(TokenKind::Ident, 4, 1)];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(&toks, FileId::new(1).unwrap(), &mut arena, &mut sink);
        // No matching kind in the stream — must terminate at end-of-input.
        p.recover_to_one_of(&[TokenKind::Semicolon, TokenKind::RBrace]);
        assert!(p.cursor_is_at_end_for_test());
    }

    #[test]
    fn recover_to_one_of_stops_at_brace_close() {
        let toks = vec![
            tok(TokenKind::KwLet, 0, 3),
            tok(TokenKind::Ident, 4, 1),
            tok(TokenKind::RBrace, 5, 1),
            tok(TokenKind::KwFn, 6, 2),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(&toks, FileId::new(1).unwrap(), &mut arena, &mut sink);
        p.recover_to_one_of(&[TokenKind::Semicolon, TokenKind::RBrace]);
        assert_eq!(p.cursor_current_kind_for_test(), TokenKind::RBrace);
    }

    #[test]
    fn expect_then_recover_round_trip() {
        // Simulate: expected `let`, got `fn`; recover to `;` and continue.
        let toks = vec![
            tok(TokenKind::KwFn, 0, 2),
            tok(TokenKind::Ident, 3, 1),
            tok(TokenKind::Semicolon, 4, 1),
            tok(TokenKind::KwLet, 5, 3),
        ];
        let mut arena = AstArena::new();
        let mut sink = VecSink::new();
        let mut p = Parser::new(&toks, FileId::new(1).unwrap(), &mut arena, &mut sink);
        let result = p.expect(TokenKind::KwLet);
        assert!(result.is_err());
        p.recover_to_one_of(&[TokenKind::Semicolon]);
        assert_eq!(p.cursor_current_kind_for_test(), TokenKind::Semicolon);
        p.bump();
        let next = p.expect(TokenKind::KwLet);
        assert!(next.is_ok());
    }
}
