//! Deterministic layout engine for the [`crate::doc::Doc`] IR.
//!
//! This is a single-pass, non-backtracking implementation of the
//! Wadler / Lindig "best" algorithm. It matches the style spec from
//! paideia-as#1561 (`max_width = 100`, `indent = 2`) and the deterministic
//! group-fit strategy rustfmt uses: for each [`Doc::Group`] the
//! engine measures the group's content in flat mode (treating all
//! inner groups optimistically as flat as well) extended by the
//! pending work from the enclosing context, and commits to flat
//! if-and-only-if that measurement stays within the remaining
//! budget on the current line.
//!
//! # Why "deterministic"
//!
//! The classic Wadler algorithm is specified as a choice between
//! two layouts filtered by a `fits` predicate. In the Haskell paper
//! this is encoded with a layout algebra and relies on laziness to
//! prune. In Rust we fold the choice into the greedy single-pass
//! walk: at every `Group` the mode is decided once, by [`fits`],
//! and never revisited. This matches rustfmt's approach and keeps
//! the renderer `O(n)` in the Doc size for the common case; the
//! `fits` probe is bounded by the remaining budget plus the
//! lookahead to the next hard break, so a pathological doc is
//! `O(n * max_width)` in the worst case — acceptable for source
//! files on the order of 10^4 nodes.
//!
//! # Deviation from Wadler
//!
//! * We treat all enclosed groups in the `fits` probe as flat
//!   (standard rustfmt / Prettier behaviour; cheaper than
//!   recursive sub-measurement, and matches what the committed
//!   layout will produce if the outer group fits).
//! * [`Doc::HardLine`] is split by position: a HardLine *inside*
//!   the group being probed forces the group to break (Prettier
//!   convention — a group that must contain a newline cannot be
//!   "one line flat"); a HardLine in the *pending* tail after
//!   the group short-circuits `fits` to `true` (Lindig
//!   convention — the remaining work moves off the current line
//!   so stops competing for budget).
//! * [`Doc::IfBreak`] is resolved against the *committed* mode of
//!   the enclosing group when it is reached in rendering; during
//!   the `fits` probe's group-content phase it is resolved as
//!   `when_flat` (that is the mode we're probing); during the
//!   pending-tail phase it is resolved against the pending
//!   item's own already-committed mode.
//!
//! # Indentation
//!
//! Indentation is applied *after* every newline the engine emits
//! (from `Line`, `SoftLine`, or `HardLine` in break mode). The
//! current indent is pushed by [`Doc::Nest`] in units of spaces,
//! not multiples of the style spec's `indent = 2`; callers compose
//! `nest(2, …)` themselves. This keeps the engine agnostic of the
//! style spec's exact indent width.
//!
//! # Width accounting
//!
//! Width is counted in Unicode scalar values (`char::count()`),
//! which is a reasonable approximation for monospace terminals and
//! what rustfmt / Prettier both do. Grapheme-cluster width is out
//! of scope for FMT-2; FMT-10 (final width policy) may revisit.

use crate::doc::Doc;

/// Layout options for the Doc renderer.
///
/// Named `FormatOptions` to match the issue spec. Note that
/// `crate::FormatOptions` (the older whitespace/glyph formatter's
/// options) is a *different* struct in the crate root; disambiguate
/// via `crate::layout::FormatOptions`.
#[derive(Clone, Copy, Debug)]
pub struct FormatOptions {
    /// Maximum column width before a group must break. The style
    /// spec from paideia-as#1561 fixes this at 100 for paideia-as
    /// sources.
    pub max_width: usize,
}

impl Default for FormatOptions {
    fn default() -> Self {
        FormatOptions { max_width: 100 }
    }
}

/// Rendering mode for a [`Doc::Group`].
///
/// `Flat` means `Line` becomes a space, `SoftLine` becomes nothing,
/// and `IfBreak` picks its `when_flat` branch. `Break` means `Line`
/// and `SoftLine` become `"\n" + indent`, and `IfBreak` picks
/// `when_break`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Flat,
    Break,
}

/// Render `doc` to a `String` under `opts`.
///
/// Deterministic, single-pass, `O(n)` amortised in the Doc size.
/// See the module-level docs for the full algorithm.
pub fn render(doc: &Doc, opts: &FormatOptions) -> String {
    let mut out = String::new();
    let mut col: usize = 0;
    // Processing stack. Top of stack = next item to render. We push
    // Concat children in reverse so left-to-right order comes out
    // correctly on pop.
    let mut stack: Vec<(usize, Mode, &Doc)> = Vec::with_capacity(16);
    // Outermost context: zero indent, "break" mode so that bare
    // HardLines at the top level produce actual newlines (their
    // committed behaviour) rather than being treated as a group
    // decision. The outermost mode is only consulted by Line /
    // SoftLine / IfBreak that are *not* wrapped in any Group; the
    // natural reading is "the whole document" is in break mode.
    stack.push((0, Mode::Break, doc));

    while let Some((indent, mode, d)) = stack.pop() {
        match d {
            Doc::Nil => {}
            Doc::Text(s) => {
                out.push_str(s);
                col += s.chars().count();
            }
            Doc::Line => match mode {
                Mode::Flat => {
                    out.push(' ');
                    col += 1;
                }
                Mode::Break => {
                    emit_newline(&mut out, &mut col, indent);
                }
            },
            Doc::SoftLine => match mode {
                Mode::Flat => {}
                Mode::Break => {
                    emit_newline(&mut out, &mut col, indent);
                }
            },
            Doc::HardLine => {
                emit_newline(&mut out, &mut col, indent);
            }
            Doc::Nest(n, inner) => {
                stack.push((indent + n, mode, inner));
            }
            Doc::Group(inner) => {
                // Decide this group's mode once. Budget = columns
                // left on the current line. `fits` extends the probe
                // past the group with the pending stack so a group
                // that *just* fits but is followed by text that
                // would overflow still breaks correctly.
                let budget = opts.max_width.saturating_sub(col);
                if fits(budget as isize, indent, inner, &stack) {
                    stack.push((indent, Mode::Flat, inner));
                } else {
                    stack.push((indent, Mode::Break, inner));
                }
            }
            Doc::Concat(items) => {
                for item in items.iter().rev() {
                    stack.push((indent, mode, item));
                }
            }
            Doc::IfBreak {
                when_break,
                when_flat,
            } => match mode {
                Mode::Flat => stack.push((indent, mode, when_flat)),
                Mode::Break => stack.push((indent, mode, when_break)),
            },
        }
    }

    out
}

/// Emit `"\n"` followed by `indent` spaces; reset `col` to `indent`.
fn emit_newline(out: &mut String, col: &mut usize, indent: usize) {
    out.push('\n');
    for _ in 0..indent {
        out.push(' ');
    }
    *col = indent;
}

/// Would `doc` (assumed flat, with `indent` as its current indent)
/// extended by the `pending` work still on the main stack fit
/// within `budget` columns before the next newline?
///
/// Two-phase:
///
/// * Phase 1 walks the group content with mode `Flat`. A `HardLine`
///   here returns `false` (group must break). All inner groups are
///   assumed flat (rustfmt / Prettier convention).
/// * Phase 2 walks the pending tail *in its already-committed
///   mode*. A `HardLine` here, or a `Line` / `SoftLine` whose
///   enclosing pending mode is `Break`, returns `true` (remaining
///   work moves off the current line).
fn fits(
    budget: isize,
    indent: usize,
    doc: &Doc,
    pending: &[(usize, Mode, &Doc)],
) -> bool {
    if budget < 0 {
        return false;
    }
    let mut budget = budget;
    let mut work: Vec<(usize, Mode, &Doc)> = Vec::with_capacity(8);

    // Phase 1: group content. Mode is always Flat here (we are
    // probing whether the group fits flat); even if the main
    // engine's current mode is Break, this probe asks a
    // mode-independent question about this group.
    work.push((indent, Mode::Flat, doc));
    while let Some((ind, _mode, d)) = work.pop() {
        match d {
            Doc::Nil => {}
            Doc::Text(s) => {
                budget -= s.chars().count() as isize;
                if budget < 0 {
                    return false;
                }
            }
            Doc::Line => {
                budget -= 1;
                if budget < 0 {
                    return false;
                }
            }
            Doc::SoftLine => {}
            // HardLine inside the group being probed → group cannot
            // be one line → does not fit flat.
            Doc::HardLine => return false,
            Doc::Nest(n, inner) => {
                work.push((ind + n, Mode::Flat, inner));
            }
            Doc::Group(inner) => {
                work.push((ind, Mode::Flat, inner));
            }
            Doc::Concat(items) => {
                for item in items.iter().rev() {
                    work.push((ind, Mode::Flat, item));
                }
            }
            // Probing Flat → pick when_flat.
            Doc::IfBreak { when_flat, .. } => {
                work.push((ind, Mode::Flat, when_flat));
            }
        }
    }

    // Phase 2: pending tail in its own committed modes. Pending is
    // the main stack; highest index = next to process, which is
    // what pop produces when we re-push in natural order.
    for item in pending.iter() {
        work.push((item.0, item.1, item.2));
    }
    while let Some((ind, mode, d)) = work.pop() {
        match d {
            Doc::Nil => {}
            Doc::Text(s) => {
                budget -= s.chars().count() as isize;
                if budget < 0 {
                    return false;
                }
            }
            Doc::Line => match mode {
                Mode::Flat => {
                    budget -= 1;
                    if budget < 0 {
                        return false;
                    }
                }
                Mode::Break => return true,
            },
            Doc::SoftLine => match mode {
                Mode::Flat => {}
                Mode::Break => return true,
            },
            // HardLine beyond the group → rest moves to a fresh
            // line → current measurement is complete.
            Doc::HardLine => return true,
            Doc::Nest(n, inner) => {
                work.push((ind + n, mode, inner));
            }
            Doc::Group(inner) => {
                work.push((ind, Mode::Flat, inner));
            }
            Doc::Concat(items) => {
                for item in items.iter().rev() {
                    work.push((ind, mode, item));
                }
            }
            Doc::IfBreak {
                when_break,
                when_flat,
            } => match mode {
                Mode::Flat => work.push((ind, mode, when_flat)),
                Mode::Break => work.push((ind, mode, when_break)),
            },
        }
    }

    budget >= 0
}

#[cfg(test)]
mod tests {
    //! # Idempotence note
    //!
    //! `render(d, opts)` yields a plain `String`; there is no
    //! `String -> Doc` parser in this crate (parsing is FMT-1a's
    //! job over the paideia-as CST, not over the renderer's output).
    //! The idempotence invariant we actually care about is: if a
    //! `Doc` consisting of `Text("foo\nbar")` + `HardLine` is
    //! re-ingested as `Text("foo") + HardLine + Text("bar") +
    //! HardLine`, both render to the same string. The engine never
    //! scans `Text` for newlines, so this holds by construction;
    //! we do not add an explicit test because the "parse back"
    //! half of the invariant has no counterpart in the crate.

    use super::*;
    use crate::doc::*;

    fn opts(max_width: usize) -> FormatOptions {
        FormatOptions { max_width }
    }

    #[test]
    fn empty_doc_renders_empty() {
        assert_eq!(render(&nil(), &opts(80)), "");
    }

    #[test]
    fn single_text_renders_verbatim() {
        assert_eq!(render(&text("hello"), &opts(80)), "hello");
    }

    #[test]
    fn concat_two_texts() {
        let d = concat(vec![text("hello"), text(" "), text("world")]);
        assert_eq!(render(&d, &opts(80)), "hello world");
    }

    #[test]
    fn group_that_fits_stays_flat() {
        // "fn f(a, b)" is 10 chars; max_width 80 → one line.
        let d = group(concat(vec![
            text("fn f("),
            softline(),
            text("a, b"),
            softline(),
            text(")"),
        ]));
        assert_eq!(render(&d, &opts(80)), "fn f(a, b)");
    }

    #[test]
    fn group_that_overflows_breaks_with_nest() {
        // Width 10 with "fn f(" alone = 5; adding "aaaaa, bbbbb" would
        // overflow, so the group breaks.
        let d = group(concat(vec![
            text("fn f("),
            nest(
                2,
                concat(vec![
                    softline(),
                    text("aaaaa, bbbbb"),
                ]),
            ),
            softline(),
            text(")"),
        ]));
        let out = render(&d, &opts(10));
        assert_eq!(out, "fn f(\n  aaaaa, bbbbb\n)");
    }

    #[test]
    fn nested_groups_partial_fit() {
        // Outer group: cannot fit at width 20.
        // Inner group with just "a, b" fits inline inside the broken
        // outer group → "fn f(\n  a, b\n)".
        let inner_args = group(concat(vec![text("a"), text(", "), text("b")]));
        let d = group(concat(vec![
            text("fn f("),
            nest(2, concat(vec![softline(), inner_args])),
            softline(),
            text(")"),
        ]));
        // The whole thing fits in 20 chars flat ("fn f(a, b)" = 10),
        // so it stays on one line.
        assert_eq!(render(&d, &opts(20)), "fn f(a, b)");
        // Force a break by setting max_width = 8.
        let out = render(&d, &opts(8));
        assert_eq!(out, "fn f(\n  a, b\n)");
    }

    #[test]
    fn if_break_trailing_comma_flat() {
        // Flat form: items separated by ", ", NO trailing comma.
        // Break form: items separated by ",\n", trailing comma.
        let items = separate(
            concat(vec![text(","), line()]),
            vec![text("a"), text("b"), text("c")],
        );
        let d = group(concat(vec![
            text("["),
            nest(2, concat(vec![softline(), items])),
            if_break(text(","), nil()),
            softline(),
            text("]"),
        ]));
        // Flat (fits width 80): "[a, b, c]" (no trailing comma).
        assert_eq!(render(&d, &opts(80)), "[a, b, c]");
    }

    #[test]
    fn if_break_trailing_comma_break() {
        // Same doc, width 4 → multi-line with trailing comma.
        let items = separate(
            concat(vec![text(","), line()]),
            vec![text("a"), text("b"), text("c")],
        );
        let d = group(concat(vec![
            text("["),
            nest(2, concat(vec![softline(), items])),
            if_break(text(","), nil()),
            softline(),
            text("]"),
        ]));
        let out = render(&d, &opts(4));
        assert_eq!(out, "[\n  a,\n  b,\n  c,\n]");
    }

    #[test]
    fn separate_interleaves_sep() {
        let d = separate(text(", "), vec![text("a"), text("b"), text("c")]);
        assert_eq!(render(&d, &opts(80)), "a, b, c");
    }

    #[test]
    fn softline_no_space_in_flat() {
        let d = group(concat(vec![text("x"), softline(), text("y")]));
        assert_eq!(render(&d, &opts(80)), "xy");
    }

    #[test]
    fn softline_newline_in_break() {
        let d = group(concat(vec![
            text("xxxxx"),
            softline(),
            text("yyyyy"),
        ]));
        assert_eq!(render(&d, &opts(4)), "xxxxx\nyyyyy");
    }

    #[test]
    fn line_space_in_flat() {
        let d = group(concat(vec![text("x"), line(), text("y")]));
        assert_eq!(render(&d, &opts(80)), "x y");
    }

    #[test]
    fn hardline_always_breaks_even_when_group_fits() {
        // Even though "xy" would fit at width 80, HardLine forces a
        // newline (and the Group containing it must be Break mode so
        // IfBreak etc. would also pick their break branch).
        let d = group(concat(vec![text("x"), hardline(), text("y")]));
        assert_eq!(render(&d, &opts(80)), "x\ny");
    }

    #[test]
    fn hardline_applies_current_indent() {
        let d = nest(
            4,
            group(concat(vec![text("x"), hardline(), text("y")])),
        );
        assert_eq!(render(&d, &opts(80)), "x\n    y");
    }

    #[test]
    fn unicode_width_counts_chars_not_bytes() {
        // "αβγ" is 3 chars, 6 bytes. At width 3 flat, "αβγ" fits.
        // At width 2 with a group needing +1 char after → breaks.
        let d = group(concat(vec![text("αβγ"), softline(), text("x")]));
        // "αβγ" + "x" flat = 4 chars; at width 3 → break.
        assert_eq!(render(&d, &opts(3)), "αβγ\nx");
        // At width 4 → fits flat (no separator between for softline).
        assert_eq!(render(&d, &opts(4)), "αβγx");
    }
}
