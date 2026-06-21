//! paideia-as doc generator.
//!
//! Phase-4-m12-003 minimum:
//! - Walk a parsed .pdx file's AST.
//! - For each top-level item (let / fn / struct / enum / trait / impl /
//!   effect / capability / macro), extract: name + signature + leading
//!   doc-comment (`///` style if present; else first `//` block).
//! - Emit HTML with one section per item.
//! - Cross-references: link `[Name]` patterns in doc text to the
//!   item's anchor.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

/// A documentation item extracted from source code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocItem {
    /// The name of the documented item.
    pub name: String,
    /// The kind of item: "let", "fn", "struct", etc.
    pub kind: String,
    /// Pretty-printed type signature or declaration.
    pub signature: String,
    /// Documentation text from `///` or `//` comments.
    pub doc: String,
}

/// A collection of extracted documentation items.
#[derive(Clone, Debug)]
pub struct DocCorpus {
    /// All extracted documentation items.
    pub items: Vec<DocItem>,
}

/// Extract documentation from source text.
///
/// Walks the source line-by-line looking for `///` doc-comments and
/// item declarations (let, fn, struct, etc.). Captures the name,
/// kind, signature, and documentation for each top-level item.
///
/// # Arguments
///
/// * `source` - The source code as a string.
///
/// # Returns
///
/// A `DocCorpus` containing all extracted items.
pub fn extract(source: &str) -> DocCorpus {
    let mut items = Vec::new();
    let mut pending_doc = String::new();

    for line in source.lines() {
        let trimmed = line.trim_start();

        // Accumulate doc comments (/// style).
        if let Some(rest) = trimmed.strip_prefix("///") {
            if !pending_doc.is_empty() {
                pending_doc.push('\n');
            }
            pending_doc.push_str(rest.trim_start());
        }
        // Check for item declarations.
        else if let Some((kind, name)) = item_kind_and_name(trimmed) {
            items.push(DocItem {
                name,
                kind: kind.to_string(),
                signature: trimmed.to_string(),
                doc: std::mem::take(&mut pending_doc),
            });
        }
        // Reset doc accumulator on non-item, non-comment, non-blank.
        else if !trimmed.starts_with("//") && !trimmed.is_empty() {
            pending_doc.clear();
        }
    }

    DocCorpus { items }
}

/// Detect and extract item kind and name from a source line.
///
/// Recognizes declarations like `let foo`, `fn bar`, `struct Baz`, etc.
///
/// # Returns
///
/// `Some((kind, name))` if the line is recognized as an item declaration,
/// otherwise `None`.
fn item_kind_and_name(line: &str) -> Option<(&'static str, String)> {
    for kind in &[
        "let",
        "fn",
        "struct",
        "enum",
        "trait",
        "impl",
        "effect",
        "capability",
        "module",
    ] {
        if let Some(rest) = line.strip_prefix(kind) {
            if let Some(rest) = rest.strip_prefix(' ') {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    return Some((kind, name));
                }
            }
        }
    }
    None
}

/// Render a `DocCorpus` as HTML.
///
/// Produces a standalone HTML document with one section per item,
/// including cross-references for `[Name]` patterns in documentation.
///
/// # Arguments
///
/// * `corpus` - The documentation corpus to render.
///
/// # Returns
///
/// An HTML string.
pub fn render_html(corpus: &DocCorpus) -> String {
    let mut html = String::from("<!DOCTYPE html>\n<html><head><title>paideia-as docs</title>\n");
    html.push_str("<meta charset=\"UTF-8\" />\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\" />\n");
    html.push_str("<style>\n");
    html.push_str("body { font-family: sans-serif; max-width: 60em; margin: 2em auto; }\n");
    html.push_str("h1 { border-bottom: 2px solid #333; padding-bottom: 0.5em; }\n");
    html.push_str("h2 { font-family: monospace; background: #eee; padding: 0.3em; }\n");
    html.push_str("pre { background: #f5f5f5; padding: 0.5em; overflow-x: auto; }\n");
    html.push_str("a { color: #0066cc; text-decoration: none; }\n");
    html.push_str("a:hover { text-decoration: underline; }\n");
    html.push_str("section { margin: 2em 0; padding: 1em; border: 1px solid #ddd; }\n");
    html.push_str("</style>\n");
    html.push_str("</head><body>\n");
    html.push_str("<h1>paideia-as Documentation</h1>\n");

    for item in &corpus.items {
        html.push_str(&format!("<section id=\"{}\">\n", html_escape(&item.name)));
        html.push_str(&format!(
            "  <h2>{} <code>{}</code></h2>\n",
            item.kind,
            html_escape(&item.name)
        ));
        html.push_str(&format!(
            "  <pre><code>{}</code></pre>\n",
            html_escape(&item.signature)
        ));
        if !item.doc.is_empty() {
            html.push_str(&format!(
                "  <div class=\"doc\">{}</div>\n",
                render_doc_markdown(&item.doc, corpus)
            ));
        }
        html.push_str("</section>\n");
    }

    html.push_str("</body></html>\n");
    html
}

/// HTML-escape a string for safe inclusion in HTML content.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Render documentation markdown with cross-reference support.
///
/// Handles:
/// - Paragraph breaks on double newlines.
/// - `[Name]` patterns: if `Name` exists in the corpus, renders as a link.
///
/// # Arguments
///
/// * `doc` - The documentation text.
/// * `corpus` - The documentation corpus (for resolving cross-references).
///
/// # Returns
///
/// HTML-formatted documentation text.
fn render_doc_markdown(doc: &str, corpus: &DocCorpus) -> String {
    let mut out = String::new();

    // Split by double newlines for paragraphs.
    for para in doc.split("\n\n") {
        if para.trim().is_empty() {
            continue;
        }

        out.push_str("<p>");

        // Process tokens within the paragraph.
        let mut chars = para.chars().peekable();
        while let Some(ch) = chars.next() {
            // Handle [Name] cross-references.
            if ch == '[' {
                let mut bracket_content = String::new();
                let mut found_close = false;
                while let Some(inner_ch) = chars.next() {
                    if inner_ch == ']' {
                        found_close = true;
                        break;
                    }
                    bracket_content.push(inner_ch);
                }

                if found_close && !bracket_content.is_empty() {
                    // Check if this name exists in the corpus.
                    if corpus.items.iter().any(|i| i.name == bracket_content) {
                        out.push_str(&format!(
                            "<a href=\"#{}\">[{}]</a>",
                            html_escape(&bracket_content),
                            html_escape(&bracket_content)
                        ));
                    } else {
                        out.push('[');
                        out.push_str(&bracket_content);
                        out.push(']');
                    }
                } else if found_close {
                    out.push_str("[]");
                } else {
                    out.push('[');
                    out.push_str(&bracket_content);
                }
            } else {
                out.push(ch);
            }
        }

        out.push_str("</p>\n");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_no_items_returns_empty() {
        let source = "// This is just a comment\n// No items here";
        let corpus = extract(source);
        assert_eq!(corpus.items.len(), 0);
    }

    #[test]
    fn extract_let_with_doc_comment() {
        let source = "/// This is a let binding\nlet x = 42";
        let corpus = extract(source);
        assert_eq!(corpus.items.len(), 1);
        assert_eq!(corpus.items[0].name, "x");
        assert_eq!(corpus.items[0].kind, "let");
        assert_eq!(corpus.items[0].doc, "This is a let binding");
        assert_eq!(corpus.items[0].signature, "let x = 42");
    }

    #[test]
    fn extract_multiple_items() {
        let source = "/// First function\nfn foo() {}\n\n/// Second function\nfn bar() {}";
        let corpus = extract(source);
        assert_eq!(corpus.items.len(), 2);
        assert_eq!(corpus.items[0].name, "foo");
        assert_eq!(corpus.items[0].kind, "fn");
        assert_eq!(corpus.items[1].name, "bar");
        assert_eq!(corpus.items[1].kind, "fn");
    }

    #[test]
    fn extract_struct_with_doc() {
        let source = "/// A data structure\nstruct Point { x: f64, y: f64 }";
        let corpus = extract(source);
        assert_eq!(corpus.items.len(), 1);
        assert_eq!(corpus.items[0].name, "Point");
        assert_eq!(corpus.items[0].kind, "struct");
    }

    #[test]
    fn render_html_emits_section_per_item() {
        let items = vec![
            DocItem {
                name: "foo".to_string(),
                kind: "fn".to_string(),
                signature: "fn foo()".to_string(),
                doc: "A function".to_string(),
            },
            DocItem {
                name: "bar".to_string(),
                kind: "struct".to_string(),
                signature: "struct bar {}".to_string(),
                doc: "A struct".to_string(),
            },
        ];
        let corpus = DocCorpus { items };
        let html = render_html(&corpus);

        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<section id=\"foo\">"));
        assert!(html.contains("<section id=\"bar\">"));
        assert!(html.contains("<h2>fn <code>foo</code></h2>"));
        assert!(html.contains("<h2>struct <code>bar</code></h2>"));
        assert!(html.contains("A function"));
        assert!(html.contains("A struct"));
    }

    #[test]
    fn render_html_cross_reference_links() {
        let items = vec![
            DocItem {
                name: "Point".to_string(),
                kind: "struct".to_string(),
                signature: "struct Point {}".to_string(),
                doc: String::new(),
            },
            DocItem {
                name: "Vector".to_string(),
                kind: "struct".to_string(),
                signature: "struct Vector {}".to_string(),
                doc: "Contains a [Point]".to_string(),
            },
        ];
        let corpus = DocCorpus { items };
        let html = render_html(&corpus);

        // Should contain a link to Point.
        assert!(html.contains("<a href=\"#Point\">[Point]</a>"));
    }

    #[test]
    fn extract_multiple_doc_lines() {
        let source = "/// First line\n/// Second line\nfn func() {}";
        let corpus = extract(source);
        assert_eq!(corpus.items.len(), 1);
        assert_eq!(corpus.items[0].doc, "First line\nSecond line");
    }

    #[test]
    fn html_escape_handles_special_chars() {
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape("<tag>"), "&lt;tag&gt;");
        assert_eq!(html_escape("\"quoted\""), "&quot;quoted&quot;");
    }

    #[test]
    fn extract_various_item_kinds() {
        let source = "let x = 1\nfn f() {}\nstruct S {}\nenum E {}\ntrait T {}\neffect E {}\ncapability C {}";
        let corpus = extract(source);
        assert_eq!(corpus.items.len(), 7);
        let kinds: Vec<_> = corpus.items.iter().map(|i| i.kind.as_str()).collect();
        assert_eq!(
            kinds,
            vec![
                "let",
                "fn",
                "struct",
                "enum",
                "trait",
                "effect",
                "capability"
            ]
        );
    }

    #[test]
    fn render_doc_markdown_handles_paragraphs() {
        let items = vec![DocItem {
            name: "test".to_string(),
            kind: "fn".to_string(),
            signature: "fn test()".to_string(),
            doc: "First paragraph\n\nSecond paragraph".to_string(),
        }];
        let corpus = DocCorpus { items };
        let html = render_html(&corpus);
        assert!(html.contains("<p>First paragraph</p>"));
        assert!(html.contains("<p>Second paragraph</p>"));
    }
}
