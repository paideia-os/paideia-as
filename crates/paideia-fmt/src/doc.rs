//! Wadler-style pretty-printer Doc IR.
//!
//! This module provides the intermediate representation that
//! [`crate::layout::render`] consumes. The design follows Wadler's
//! "A prettier printer" (ICFP 1998) and Lindig's "Strictly pretty"
//! (2000), adapted to the deterministic single-pass variant that
//! rustfmt uses (no backtracking, greedy group-fit, hard-break
//! short-circuit).
//!
//! # IR overview
//!
//! A [`Doc`] is a tree of combinators. The leaves are literal text
//! and line-break markers; the internal nodes compose them. The
//! layout engine walks the tree with a column counter and a mode
//! (`Flat` / `Break`) chosen per [`Doc::Group`] by measuring whether
//! the group's content fits the remaining budget on the current line.
//!
//! * [`Doc::Nil`] — empty document (identity under concat).
//! * [`Doc::Text`] — a literal string. The engine assumes no embedded
//!   newlines; callers that want a hard newline must use
//!   [`Doc::HardLine`]. Width is counted in Unicode scalar values
//!   (`char` count), not bytes.
//! * [`Doc::Line`] — a *breakable* line: a single space in flat
//!   mode, a newline plus the current indent in break mode.
//! * [`Doc::SoftLine`] — a *breakable* zero-width separator: nothing
//!   in flat mode, a newline plus the current indent in break mode.
//! * [`Doc::HardLine`] — unconditional newline plus the current
//!   indent; the enclosing group is forced into break mode.
//! * [`Doc::Nest`] — pushes `n` extra spaces of indent for the
//!   duration of the inner document (indent is applied after every
//!   newline produced by `Line` / `SoftLine` / `HardLine`).
//! * [`Doc::Group`] — a layout choice point: the engine renders the
//!   inner document flat if it fits the budget, else it renders it
//!   broken.
//! * [`Doc::Concat`] — left-to-right sequence of documents.
//! * [`Doc::IfBreak`] — mode-conditional alternation: `when_break`
//!   when the enclosing group is in break mode, `when_flat` when
//!   flat. The archetypal use is a trailing-comma style:
//!   `if_break(text(","), nil())` keeps one list-element terminator
//!   that only appears in multi-line form.
//!
//! # Builder convention
//!
//! The free functions in the [`doc`](self) namespace are the public
//! smart constructors. They take `impl Into<Cow<'static, str>>` or
//! `Doc`, box as needed, and avoid the `Doc::X(Box::new(...))`
//! boilerplate at call sites.

use std::borrow::Cow;

/// Wadler-style document IR.
///
/// See the module-level docs for the full combinator contract.
///
/// Note on `IfBreak`'s field names: the issue's informal spec writes
/// `IfBreak { break: Box<Doc>, flat: Box<Doc> }`, but `break` is a
/// Rust keyword. We spell them `when_break` and `when_flat`; the
/// builder helper [`if_break`] matches the spec's `(brk, flt)`
/// positional convention.
#[derive(Clone, Debug)]
pub enum Doc {
    /// Empty document; identity under concat.
    Nil,
    /// A literal string. Callers must not embed `\n`; use
    /// [`Doc::HardLine`] for unconditional breaks.
    Text(Cow<'static, str>),
    /// Breakable line: " " flat, "\n" + indent in break mode.
    Line,
    /// Breakable zero-width separator: "" flat, "\n" + indent in
    /// break mode.
    SoftLine,
    /// Unconditional newline + indent; forces the enclosing group
    /// into break mode.
    HardLine,
    /// Add `usize` extra spaces of indent for the inner doc.
    Nest(usize, Box<Doc>),
    /// Layout choice point: flat if it fits the remaining budget,
    /// else broken.
    Group(Box<Doc>),
    /// Left-to-right sequence.
    Concat(Vec<Doc>),
    /// Mode-conditional branch.
    IfBreak {
        /// Rendered when the enclosing group is in break mode.
        when_break: Box<Doc>,
        /// Rendered when the enclosing group is in flat mode.
        when_flat: Box<Doc>,
    },
}

impl Default for Doc {
    fn default() -> Self {
        Doc::Nil
    }
}

/// A literal-text document.
///
/// Accepts `&'static str`, `String`, or any `Into<Cow<'static,
/// str>>`. The caller must guarantee the string has no embedded
/// newlines; the layout engine does not scan.
pub fn text<S: Into<Cow<'static, str>>>(s: S) -> Doc {
    Doc::Text(s.into())
}

/// The empty document (identity under concat).
pub fn nil() -> Doc {
    Doc::Nil
}

/// Wrap `d` with `n` extra spaces of indent.
pub fn nest(n: usize, d: Doc) -> Doc {
    Doc::Nest(n, Box::new(d))
}

/// Make `d` a layout choice point (fit-flat-else-break).
pub fn group(d: Doc) -> Doc {
    Doc::Group(Box::new(d))
}

/// Concatenate a sequence of documents left-to-right.
///
/// Single-element vectors are returned as-is; empty vectors yield
/// [`Doc::Nil`]. The engine handles `Concat` without flattening, so
/// callers may freely nest.
pub fn concat<I: IntoIterator<Item = Doc>>(docs: I) -> Doc {
    let v: Vec<Doc> = docs.into_iter().collect();
    match v.len() {
        0 => Doc::Nil,
        1 => v.into_iter().next().unwrap(),
        _ => Doc::Concat(v),
    }
}

/// A breakable line (space in flat, newline in break mode).
pub fn line() -> Doc {
    Doc::Line
}

/// A breakable zero-width separator (nothing in flat, newline in
/// break mode).
pub fn softline() -> Doc {
    Doc::SoftLine
}

/// An unconditional newline + indent; forces break mode on the
/// enclosing group.
pub fn hardline() -> Doc {
    Doc::HardLine
}

/// Mode-conditional branch; `brk` for break mode, `flt` for flat.
///
/// The canonical use is a trailing comma that only appears when the
/// enclosing group broke across lines:
///
/// ```ignore
/// use paideia_fmt::doc::*;
/// let trailing = if_break(text(","), nil());
/// ```
pub fn if_break(brk: Doc, flt: Doc) -> Doc {
    Doc::IfBreak {
        when_break: Box::new(brk),
        when_flat: Box::new(flt),
    }
}

/// Interleave `sep` between `items`, left-to-right.
///
/// `separate(sep, [a, b, c])` builds `concat([a, sep.clone(), b,
/// sep.clone(), c])`. For zero or one items, no separator is emitted.
///
/// This is the fundamental list combinator for argument lists, enum
/// variants, struct fields, etc. Pair it with [`group`] / [`nest`] /
/// [`line`] / [`if_break`] to build the full fits-flat-else-break
/// idiom:
///
/// ```ignore
/// use paideia_fmt::doc::*;
/// // group([ "fn("    nest(2, [ softline, separate(concat([",", line]), args) ])
/// //        , if_break(",", nil()), softline, ")" ])
/// ```
pub fn separate<I>(sep: Doc, items: I) -> Doc
where
    I: IntoIterator<Item = Doc>,
{
    let items: Vec<Doc> = items.into_iter().collect();
    if items.is_empty() {
        return Doc::Nil;
    }
    let mut out: Vec<Doc> = Vec::with_capacity(items.len() * 2 - 1);
    let last = items.len() - 1;
    for (i, item) in items.into_iter().enumerate() {
        out.push(item);
        if i != last {
            out.push(sep.clone());
        }
    }
    concat(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concat_empty_is_nil() {
        assert!(matches!(concat(std::iter::empty()), Doc::Nil));
    }

    #[test]
    fn concat_single_unwraps() {
        assert!(matches!(concat(vec![text("x")]), Doc::Text(_)));
    }

    #[test]
    fn concat_many_builds_concat() {
        match concat(vec![text("a"), text("b")]) {
            Doc::Concat(v) => assert_eq!(v.len(), 2),
            _ => panic!("expected Concat"),
        }
    }

    #[test]
    fn separate_zero_items_is_nil() {
        assert!(matches!(separate(text(", "), std::iter::empty()), Doc::Nil));
    }

    #[test]
    fn separate_one_item_has_no_sep() {
        match separate(text(", "), vec![text("a")]) {
            Doc::Text(s) => assert_eq!(s, "a"),
            _ => panic!("expected single Text"),
        }
    }

    #[test]
    fn separate_three_items_interleaves() {
        match separate(text(", "), vec![text("a"), text("b"), text("c")]) {
            Doc::Concat(v) => assert_eq!(v.len(), 5),
            _ => panic!("expected Concat of length 5"),
        }
    }
}
