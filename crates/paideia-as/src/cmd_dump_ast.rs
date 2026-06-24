//! `paideia-as dump-ast <file>` — lex + (stub) parse + pretty-print.
//!
//! The parser is not yet wired in (T3, PR-19+); for now the command
//! constructs a synthetic AST containing one `Module` whose `Structure`
//! body holds one `Let` per top-level lexer-recognized identifier in
//! the source. This is enough to validate the pretty-printer and to
//! give phase-1 users a visible handle on the toolchain.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use paideia_as_ast::{AstArena, ItemData, NodeKind};
use paideia_as_diagnostics::{
    DiagnosticSink, FileId, HumanRenderer, HumanSink, SourceMap, Span, VecSink,
};
use paideia_as_lexer::{Lexer, SourceText, TokenKind};

/// Run `paideia-as dump-ast <input>`.
///
/// Returns an `ExitCode` so the CLI can propagate non-zero on errors.
pub fn run(input: &Path) -> ExitCode {
    let bytes = match fs::read(input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("paideia-as: cannot read {}: {e}", input.display());
            return ExitCode::from(2);
        }
    };

    let mut source_map = SourceMap::new();
    let content_string = String::from_utf8_lossy(&bytes).into_owned();
    let file = source_map.add_file(input.to_path_buf(), content_string);

    let source = match SourceText::from_bytes(file, &bytes) {
        Ok(s) => s,
        Err(diag) => {
            let renderer = HumanRenderer::new(&source_map, true);
            eprint!("{}", renderer.render(&diag));
            return ExitCode::from(1);
        }
    };

    let mut sink = HumanSink::new(std::io::stderr(), HumanRenderer::new(&source_map, true));
    let mut collector = VecSink::new();
    let mut lexer = Lexer::new(file, &source);
    let tokens = lexer.collect_tokens(&mut collector);
    // Re-emit the diagnostics from the collector to stderr via the renderer.
    for diag in collector.diagnostics() {
        let _ = sink.emit(diag.clone());
    }

    // Stub parser: build a synthetic Module with a Let per Ident token.
    let arena = stub_parse(file, &tokens);

    let dump = paideia_as_ast::pretty::dump_arena(&arena);
    println!("{dump}");

    ExitCode::SUCCESS
}

/// Synthetic-AST builder used until the real parser lands (PR-19+).
///
/// Produces:
///
/// ```text
/// Module { name: nN, sig: None, body: nM, doc: None }
///   Structure { items: [...], doc: None }
///     Let { name: ..., ty: None, value: ..., doc: None }  // one per Ident token
/// ```
fn stub_parse(file: FileId, tokens: &[paideia_as_lexer::Token]) -> AstArena {
    let mut arena = AstArena::new();

    let zero_span = Span::new(file, 0, 0);

    // Module name (Ident, placeholder).
    let mod_name = arena.alloc(NodeKind::Ident, zero_span);

    // Each Ident token becomes a Let in the body.
    let mut let_ids = Vec::new();
    for tok in tokens {
        if tok.kind == TokenKind::Ident {
            let name = arena.alloc(NodeKind::Ident, tok.span);
            let value = arena.alloc(NodeKind::Placeholder, tok.span);
            let let_id = arena.alloc_item(
                NodeKind::Let,
                tok.span,
                ItemData::Let {
                    public: false,
                    mutable: false,
                    name,
                    generic_params: vec![],
                    ty: None,
                    value,
                    doc: None,
                },
            );
            let_ids.push(let_id);
        }
    }

    let structure = arena.alloc_item(
        NodeKind::Structure,
        zero_span,
        ItemData::Structure {
            items: let_ids,
            inner_attrs: vec![],
            doc: None,
        },
    );

    arena.alloc_item(
        NodeKind::Module,
        zero_span,
        ItemData::Module {
            name: mod_name,
            sig: None,
            body: structure,
            inner_attrs: vec![],
            doc: None,
        },
    );

    arena
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;
    use paideia_as_lexer::{Token, TokenKind};

    #[test]
    fn stub_parse_produces_module_with_lets_per_ident() {
        let file = FileId::new(1).unwrap();
        let span = Span::new(file, 0, 1);
        let tokens = vec![
            Token::new(TokenKind::Ident, span),
            Token::new(TokenKind::Plus, span),
            Token::new(TokenKind::Ident, span),
        ];
        let arena = stub_parse(file, &tokens);
        // 1 ident (name) + 2*(ident + placeholder) = 5 raw nodes; plus
        // 2 Let items + 1 Structure + 1 Module = 4 item nodes; total 9.
        assert_eq!(arena.len(), 9);

        let dump = paideia_as_ast::pretty::dump_arena(&arena);
        assert!(dump.contains("Module"));
        assert!(dump.contains("Structure"));
        assert!(dump.contains("Let"));
    }

    #[test]
    fn dump_arena_is_idempotent() {
        let file = FileId::new(1).unwrap();
        let span = Span::new(file, 0, 1);
        let tokens = vec![Token::new(TokenKind::Ident, span)];
        let arena = stub_parse(file, &tokens);

        let a = paideia_as_ast::pretty::dump_arena(&arena);
        let b = paideia_as_ast::pretty::dump_arena(&arena);
        assert_eq!(a, b);
    }
}
