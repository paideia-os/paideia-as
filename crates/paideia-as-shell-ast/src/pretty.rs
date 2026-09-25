//! Canonical pretty-printer for [`SyntaxNode`], used by the R221.M7
//! round-trip corpus (`parse(pp(parse(src))) == parse(src)` for the
//! 100-sample fixture set) and by R229 REPL's history-recall feature
//! (a de-normalized-back-to-source rendering of a canonicalized AST).
//!
//! # Canonical form
//!
//! * Single space between pipeline stages and their arguments.
//! * ` | ` between pipeline stages.
//! * `; ` between top-level sequences.
//! * Lambda parameters space-separated, no trailing comma.
//! * Datalog rules `head => body1, body2.` with a trailing dot.
//! * Datalog facts `pred(a, b).` with a trailing dot.
//! * Record fields `name: value` with `, ` between them.
//! * Strings quoted with `"`; interior escapes preserved verbatim
//!   (the parser did not resolve them, so the printer round-trips
//!   whatever the user typed).
//! * Integers printed decimal, minus-sign for negatives.
//!
//! The round-trip corpus feeds canonical-form inputs so the fixed-point
//! `pretty_print(parse(pretty_print(parse(src)))) == pretty_print(parse(src))`
//! holds by construction on the first iteration.

use crate::ast::{MatchArm, RecordField, RedirectKind, SyntaxNode};

/// Render a `SyntaxNode` to canonical source text.
pub fn pretty_print(node: &SyntaxNode) -> String {
    let mut out = String::new();
    emit(node, &mut out);
    out
}

fn emit(node: &SyntaxNode, out: &mut String) {
    match node {
        SyntaxNode::Cmd { name, args, .. } => {
            emit(name, out);
            for a in args {
                out.push(' ');
                emit(a, out);
            }
        }
        SyntaxNode::Pipe { lhs, rhs, .. } => {
            emit(lhs, out);
            out.push_str(" | ");
            emit(rhs, out);
        }
        SyntaxNode::Seq { items, .. } => {
            let mut first = true;
            for it in items {
                if !first {
                    out.push_str("; ");
                }
                emit(it, out);
                first = false;
            }
        }
        SyntaxNode::Redirect {
            source,
            kind,
            target,
            ..
        } => {
            emit(source, out);
            out.push(' ');
            out.push_str(redir_glyph(*kind));
            out.push(' ');
            emit(target, out);
        }
        SyntaxNode::Background { inner, .. } => {
            emit(inner, out);
            out.push_str(" &");
        }
        SyntaxNode::Group { inner, .. } => {
            out.push('(');
            emit(inner, out);
            out.push(')');
        }
        SyntaxNode::DatalogBlock { items, .. } => {
            out.push_str("datalog {");
            for (i, it) in items.iter().enumerate() {
                if i == 0 {
                    out.push(' ');
                } else {
                    out.push(' ');
                }
                emit_datalog_item(it, out);
            }
            if !items.is_empty() {
                out.push(' ');
            }
            out.push('}');
        }
        // Bare atoms / rules / negations show up outside a block for
        // testing convenience; use the same shape.
        SyntaxNode::Atom { .. }
        | SyntaxNode::Rule { .. }
        | SyntaxNode::NotAtom { .. } => emit_datalog_item(node, out),
        SyntaxNode::QVar { name, .. } => {
            out.push('?');
            out.push_str(name);
        }
        SyntaxNode::InterpVar { name, .. } => {
            out.push('$');
            out.push_str(name);
        }
        SyntaxNode::Lambda { params, body, .. } => {
            out.push('{');
            if !params.is_empty() {
                out.push('|');
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    out.push_str(p);
                }
                out.push('|');
                out.push(' ');
            } else {
                out.push(' ');
            }
            emit(body, out);
            out.push_str(" }");
        }
        SyntaxNode::App { func, args, .. } => {
            emit(func, out);
            for a in args {
                out.push(' ');
                emit(a, out);
            }
        }
        SyntaxNode::Var { name, .. } => out.push_str(name),
        SyntaxNode::Let {
            name, value, body, ..
        } => {
            out.push_str("let ");
            out.push_str(name);
            out.push_str(" = ");
            emit(value, out);
            out.push_str(" in ");
            emit(body, out);
        }
        SyntaxNode::Match { scrutinee, arms, .. } => {
            out.push_str("match ");
            emit(scrutinee, out);
            out.push_str(" {");
            for (i, arm) in arms.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push(' ');
                emit_match_arm(arm, out);
            }
            if !arms.is_empty() {
                out.push(' ');
            }
            out.push('}');
        }
        SyntaxNode::BinOp { op, lhs, rhs, .. } => {
            emit(lhs, out);
            out.push(' ');
            out.push_str(op);
            out.push(' ');
            emit(rhs, out);
        }
        SyntaxNode::UnaryOp { op, inner, .. } => {
            out.push_str(op);
            // `not x` needs a space; `-x` does not.
            if op == "not" {
                out.push(' ');
            }
            emit(inner, out);
        }
        SyntaxNode::FieldAccess { base, field, .. } => {
            emit(base, out);
            out.push('.');
            out.push_str(field);
        }
        SyntaxNode::RecordExpr { fields, .. } => {
            out.push('{');
            for (i, f) in fields.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push(' ');
                emit_record_field(f, out);
            }
            if !fields.is_empty() {
                out.push(' ');
            }
            out.push('}');
        }
        SyntaxNode::LitStr { value, .. } => {
            out.push('"');
            out.push_str(value);
            out.push('"');
        }
        SyntaxNode::LitInt { value, .. } => {
            use std::fmt::Write;
            let _ = write!(out, "{value}");
        }
        SyntaxNode::LitBool { value, .. } => {
            out.push_str(if *value { "true" } else { "false" });
        }
        SyntaxNode::Ident { name, .. } => out.push_str(name),
    }
}

fn emit_datalog_item(node: &SyntaxNode, out: &mut String) {
    match node {
        SyntaxNode::Atom { pred, args, .. } => {
            out.push_str(pred);
            if !args.is_empty() {
                out.push('(');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    emit(a, out);
                }
                out.push(')');
            }
            out.push('.');
        }
        SyntaxNode::Rule { head, body, .. } => {
            // Head is an Atom; emit without trailing `.`.
            if let SyntaxNode::Atom { pred, args, .. } = head.as_ref() {
                out.push_str(pred);
                if !args.is_empty() {
                    out.push('(');
                    for (i, a) in args.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        emit(a, out);
                    }
                    out.push(')');
                }
            } else {
                // Non-atom head (shouldn't happen in well-formed input);
                // fall back to generic emit.
                emit(head, out);
            }
            out.push_str(" => ");
            for (i, b) in body.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                // Body atoms are emitted without their own trailing `.`.
                if let SyntaxNode::Atom { pred, args, .. } = b {
                    out.push_str(pred);
                    if !args.is_empty() {
                        out.push('(');
                        for (i, a) in args.iter().enumerate() {
                            if i > 0 {
                                out.push_str(", ");
                            }
                            emit(a, out);
                        }
                        out.push(')');
                    }
                } else {
                    emit(b, out);
                }
            }
            out.push('.');
        }
        SyntaxNode::NotAtom { inner, .. } => {
            out.push_str("not ");
            emit_datalog_item(inner, out);
        }
        _ => emit(node, out),
    }
}

fn emit_match_arm(arm: &MatchArm, out: &mut String) {
    emit(&arm.pattern, out);
    if let Some(g) = &arm.guard {
        out.push_str(" if ");
        emit(g, out);
    }
    out.push_str(" => ");
    emit(&arm.body, out);
}

fn emit_record_field(f: &RecordField, out: &mut String) {
    out.push_str(&f.name);
    out.push_str(": ");
    emit(&f.value, out);
}

fn redir_glyph(k: RedirectKind) -> &'static str {
    match k {
        RedirectKind::StdoutOverwrite => ">",
        RedirectKind::StdoutAppend => ">>",
        RedirectKind::StdinFrom => "<",
        RedirectKind::StderrOverwrite => "2>",
        RedirectKind::StderrAppend => "2>>",
        RedirectKind::BothOverwrite => "&>",
    }
}
