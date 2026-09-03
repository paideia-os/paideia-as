//! Recursive-descent combinator toolbox for AML byte-stream parsing —
//! Wave 0 Batch 4, v0.26-M1-001 (paideia-as#1360).
//!
//! # What this module is
//!
//! A minimal, hand-rolled, zero-external-crate combinator surface over
//! `&[u8]`. It is the substrate the v0.26 AML front-end will build on
//! (`NameString`, `NameSeg`, `PkgLength`, plus every downstream `TermObj`
//! shape from ACPI 6.5 §20.2). We do NOT pull nom / chumsky / winnow — the
//! rest of the paideia-as workspace hand-rolls its byte-shape decoders, and
//! the AML parser follows the same discipline so the entire lowering chain
//! stays inside the workspace without a third-party grammar library in the
//! trust boundary.
//!
//! # Design decisions
//!
//! - **Combinators return closures.** Each combinator is a free function
//!   returning `impl Fn(Input<'a>) -> ParseResult<'a, O>`. This is the
//!   standard "parser-as-value" shape and composes freely.
//! - **Zero-alloc where possible.** [`Input`] borrows the source slice; it
//!   is `Copy`. Every primitive that returns a byte run returns a *borrowed*
//!   sub-slice (`&'a [u8]`), never a copy. The only combinators that
//!   allocate are [`many`] / [`many1`], and only because a variable-length
//!   sequence naturally needs a `Vec`.
//! - **Non-consuming on failure.** A combinator that returns `Err` yields
//!   the caller its *original* [`Input`]. That gives [`alt`] fully
//!   backtracking semantics for free — no committed-vs-uncommitted state
//!   machine, no `cut` combinator, matching what AML actually needs (the
//!   grammar is LL(1) with distinguishing prefix bytes, and any real error
//!   is a whole-path failure the caller reports up to the interpreter).
//! - **Error carries absolute offset.** [`ParseError`] records
//!   `(byte_offset, expected, actual)` — `byte_offset` is measured from the
//!   *start of the original input*, not from the current remainder, so a
//!   diagnostic points at the byte that failed in the DSDT / SSDT dump
//!   without the caller doing arithmetic.
//! - **Static `expected` strings.** `&'static str` — every expectation is a
//!   compile-time constant naming what the parser wanted. This keeps
//!   [`ParseError`] `Copy`-cheap and lets error messages sit in `.rodata`.
//!
//! # Scoped to AML NameString shapes (ACPI 6.5 §20.2)
//!
//! The AML grammar the v0.26 milestone needs from this module:
//!
//! ```text
//!   NameString      := <RootChar NamePath> | <PrefixPath NamePath>
//!   PrefixPath      := Nothing | <'^' PrefixPath>
//!   NamePath        := NameSeg | DualNamePath | MultiNamePath | NullName
//!   NameSeg         := <LeadNameChar NameChar NameChar NameChar>
//!   LeadNameChar    := 'A'-'Z' | '_'
//!   NameChar        := DigitChar | LeadNameChar
//!   DigitChar       := '0'-'9'
//!   RootChar        := '\\'  (0x5C)
//!   ParentPrefixChar:= '^'   (0x5E)
//!   NullName        := 0x00
//!   DualNamePrefix  := 0x2E
//!   MultiNamePrefix := 0x2F
//!
//!   PkgLength       := PkgLeadByte | <PkgLeadByte ByteData> |
//!                      <PkgLeadByte ByteData ByteData> |
//!                      <PkgLeadByte ByteData ByteData ByteData>
//! ```
//!
//! Every shape above lowers to a straight composition of the eight
//! combinators exported here plus the byte-level primitives in the
//! `bytes` module. The [`is_lead_name_char`] / [`is_name_char`] helpers
//! encode the character classes verbatim so a downstream `parse_name_seg`
//! reads as `take_while(is_name_char)` after asserting the
//! lead byte — no bit-magic at the call site.

// ---------------------------------------------------------------------------
// Input + error types
// ---------------------------------------------------------------------------

/// Position-tracking borrow into the source byte stream.
///
/// The `bytes` field is what parsers actually consume from; `offset` is the
/// byte position of `bytes[0]` in the *original* input handed to the outer
/// parser. Advancing `n` bytes yields a new [`Input`] with the sub-slice
/// `&bytes[n..]` and `offset + n` — the offset stays absolute so errors can
/// name the failing byte's position in the original DSDT / SSDT dump.
///
/// `Copy` and small (`&[u8]` fat pointer + `usize` = 24 bytes on x86_64) —
/// pass by value everywhere. Combinators that fail return `Err` and the
/// caller keeps its original `Input`, which is what gives [`alt`] its
/// backtracking semantics without any extra state.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Input<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Input<'a> {
    /// Build an `Input` at absolute offset `0`, i.e. treat `bytes` as the
    /// original source stream. Downstream advances update `offset` from
    /// there.
    #[inline]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    /// Build an `Input` at an arbitrary starting offset — for slicing into
    /// the middle of a larger buffer while keeping error offsets meaningful
    /// in the outer coordinate system (e.g. parsing one `TermObj` inside a
    /// `PkgLength`-scoped run).
    #[inline]
    pub const fn with_offset(bytes: &'a [u8], offset: usize) -> Self {
        Self { bytes, offset }
    }

    /// Bytes not yet consumed.
    #[inline]
    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// Absolute byte position of `self.bytes[0]` in the original input.
    #[inline]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// Remaining byte count. `is_empty` is the natural EOF sentinel.
    #[inline]
    pub const fn len(&self) -> usize {
        self.bytes.len()
    }

    /// `true` when no bytes remain — the natural EOF check.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Peek at the first remaining byte without consuming it.
    #[inline]
    pub fn peek(&self) -> Option<u8> {
        self.bytes.first().copied()
    }

    /// Advance `n` bytes. Panics if `n > self.len()` — callers must have
    /// already checked bounds (every combinator here does).
    #[inline]
    fn advance(self, n: usize) -> Input<'a> {
        Input {
            bytes: &self.bytes[n..],
            offset: self.offset + n,
        }
    }
}

/// Parse-failure diagnostic.
///
/// - `byte_offset`: absolute offset (from the original input's start) of
///   the byte that failed the expectation. When `actual` is `None`
///   (unexpected EOF), this is the offset *at which the next byte was
///   expected*.
/// - `expected`: static description of what the parser wanted at that
///   byte. Chosen at each combinator call site (`"NameSeg"`, `"'^'"`,
///   `"PkgLead byte-count 0..=3"`, …).
/// - `actual`: the offending byte, or `None` on unexpected end-of-input.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub byte_offset: usize,
    pub expected: &'static str,
    pub actual: Option<u8>,
}

impl ParseError {
    /// EOF at `offset` while expecting `expected`.
    #[inline]
    pub const fn eof(offset: usize, expected: &'static str) -> Self {
        Self { byte_offset: offset, expected, actual: None }
    }

    /// Byte `b` at `offset` where `expected` was wanted.
    #[inline]
    pub const fn unexpected(offset: usize, expected: &'static str, b: u8) -> Self {
        Self { byte_offset: offset, expected, actual: Some(b) }
    }
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.actual {
            Some(b) => write!(
                f,
                "at byte {}: expected {}, got 0x{:02x}",
                self.byte_offset, self.expected, b
            ),
            None => write!(
                f,
                "at byte {}: expected {}, got end-of-input",
                self.byte_offset, self.expected
            ),
        }
    }
}

impl std::error::Error for ParseError {}

/// Standard parser return shape: `(remaining input, produced value)` on
/// success, [`ParseError`] on failure. On `Err` the caller's `Input` is
/// unchanged — no bytes were consumed.
pub type ParseResult<'a, O> = Result<(Input<'a>, O), ParseError>;

// ---------------------------------------------------------------------------
// Byte-level primitives — the leaves the combinators compose over.
// ---------------------------------------------------------------------------

/// Consume one specific byte. Fails with `expected` on mismatch or EOF.
///
/// Used everywhere AML has a fixed sentinel: `\` (RootChar, 0x5C),
/// `^` (ParentPrefixChar, 0x5E), `0x2E` (DualNamePrefix), `0x2F`
/// (MultiNamePrefix), `0x00` (NullName).
pub fn byte<'a>(b: u8, expected: &'static str) -> impl Fn(Input<'a>) -> ParseResult<'a, u8> {
    move |input: Input<'a>| match input.peek() {
        Some(x) if x == b => Ok((input.advance(1), x)),
        Some(x) => Err(ParseError::unexpected(input.offset(), expected, x)),
        None => Err(ParseError::eof(input.offset(), expected)),
    }
}

/// Consume one byte satisfying `pred`. Fails with `expected` on mismatch
/// or EOF. This is the character-class primitive: every AML `NameChar` /
/// `LeadNameChar` / `DigitChar` test funnels through here (usually via
/// [`take_while`], but the singleton form is useful when the caller needs
/// exactly one byte, e.g. the lead byte of a `NameSeg`).
pub fn satisfy<'a, F>(pred: F, expected: &'static str) -> impl Fn(Input<'a>) -> ParseResult<'a, u8>
where
    F: Fn(u8) -> bool,
{
    move |input: Input<'a>| match input.peek() {
        Some(x) if pred(x) => Ok((input.advance(1), x)),
        Some(x) => Err(ParseError::unexpected(input.offset(), expected, x)),
        None => Err(ParseError::eof(input.offset(), expected)),
    }
}

/// Consume exactly `n` bytes, returning the borrowed sub-slice. Fails if
/// fewer than `n` bytes remain. Used for the four-byte `NameSeg` body and
/// for the `PkgLength`-scoped span of a `TermObj`.
pub fn take<'a>(
    n: usize,
    expected: &'static str,
) -> impl Fn(Input<'a>) -> ParseResult<'a, &'a [u8]> {
    move |input: Input<'a>| {
        if input.len() < n {
            Err(ParseError::eof(input.offset() + input.len(), expected))
        } else {
            let out = &input.bytes()[..n];
            Ok((input.advance(n), out))
        }
    }
}

// ---------------------------------------------------------------------------
// Combinators
// ---------------------------------------------------------------------------

/// Consume the longest prefix of bytes satisfying `pred`. Always succeeds
/// — a zero-length match is a valid result, so this combinator never
/// returns `Err`. For "one or more" semantics wrap the returned slice with
/// an explicit non-empty check and produce a [`ParseError`] against the
/// original `Input`'s offset.
///
/// Returns a *borrowed* sub-slice — zero copies.
///
/// The AML `NameChar*` tail of a `NameSeg` after its `LeadNameChar` head
/// is exactly `take_while(is_name_char)`. `PkgLength`'s trailing byte-data
/// run is `take_while(|_| true)` bounded by a separate length count from
/// the lead byte.
pub fn take_while<'a, F>(pred: F) -> impl Fn(Input<'a>) -> ParseResult<'a, &'a [u8]>
where
    F: Fn(u8) -> bool,
{
    move |input: Input<'a>| {
        let n = input.bytes().iter().take_while(|&&b| pred(b)).count();
        let out = &input.bytes()[..n];
        Ok((input.advance(n), out))
    }
}

/// Try `p`; if it fails, restore the original input and try `q`. Fully
/// backtracking — since a failing parser returns `Err` without consuming,
/// no `cut` combinator is needed. The error returned on total failure is
/// `q`'s, on the theory that `q` is the "more specific" alternative in
/// AML's LL(1) grammar (the caller lists the fallback last).
///
/// Both branches must produce the same output type `O`; for heterogeneous
/// branches, wrap each in a shared sum before the alt.
pub fn alt<'a, O, P, Q>(p: P, q: Q) -> impl Fn(Input<'a>) -> ParseResult<'a, O>
where
    P: Fn(Input<'a>) -> ParseResult<'a, O>,
    Q: Fn(Input<'a>) -> ParseResult<'a, O>,
{
    move |input: Input<'a>| match p(input) {
        Ok(v) => Ok(v),
        Err(_) => q(input),
    }
}

/// Run `p`, then `q` on the remainder, returning both outputs as a tuple.
/// If either fails, the whole sequence fails. On `p`'s failure the input
/// is untouched; on `q`'s failure `p`'s side effects are lost too (they
/// couldn't have mattered — parsers are pure over `Input`).
pub fn seq<'a, O1, O2, P, Q>(p: P, q: Q) -> impl Fn(Input<'a>) -> ParseResult<'a, (O1, O2)>
where
    P: Fn(Input<'a>) -> ParseResult<'a, O1>,
    Q: Fn(Input<'a>) -> ParseResult<'a, O2>,
{
    move |input: Input<'a>| {
        let (rest, a) = p(input)?;
        let (rest, b) = q(rest)?;
        Ok((rest, (a, b)))
    }
}

/// Run `p` zero or more times. Stops on the first failure of `p` (that
/// failure is discarded — `many` always succeeds). Guarantees forward
/// progress: if `p` succeeds *without* consuming, `many` breaks the loop
/// to avoid infinite iteration.
///
/// Allocates a `Vec<O>` because the count is unknown up front. For the
/// bounded case (e.g. `NameSeg * SegCount` in `MultiNamePath`) prefer an
/// explicit `for _ in 0..count { ... }` loop over [`seq`].
pub fn many<'a, O, P>(p: P) -> impl Fn(Input<'a>) -> ParseResult<'a, Vec<O>>
where
    P: Fn(Input<'a>) -> ParseResult<'a, O>,
{
    move |mut input: Input<'a>| {
        let mut out = Vec::new();
        loop {
            match p(input) {
                Ok((rest, v)) => {
                    // Forward-progress guard — a non-consuming success on
                    // every iteration would loop forever. Break instead;
                    // the caller has whatever it collected so far.
                    if rest.offset() == input.offset() {
                        out.push(v);
                        break;
                    }
                    out.push(v);
                    input = rest;
                }
                Err(_) => break,
            }
        }
        Ok((input, out))
    }
}

/// Run `p` one or more times. Fails with `p`'s first-attempt error if the
/// initial application does not match — that error is the meaningful one
/// (it tells the caller what the *first* item should have been).
pub fn many1<'a, O, P>(p: P) -> impl Fn(Input<'a>) -> ParseResult<'a, Vec<O>>
where
    P: Fn(Input<'a>) -> ParseResult<'a, O>,
{
    move |input: Input<'a>| {
        let (mut rest, first) = p(input)?;
        let mut out = vec![first];
        loop {
            match p(rest) {
                Ok((next, v)) => {
                    if next.offset() == rest.offset() {
                        out.push(v);
                        break;
                    }
                    out.push(v);
                    rest = next;
                }
                Err(_) => break,
            }
        }
        Ok((rest, out))
    }
}

/// Run `p`; on success wrap the result in `Some`, on failure yield `None`
/// without consuming. The `NullName` alternative of AML's `NamePath` is a
/// classic `opt(byte(0x00, "NullName"))` at the front of a longer alt.
pub fn opt<'a, O, P>(p: P) -> impl Fn(Input<'a>) -> ParseResult<'a, Option<O>>
where
    P: Fn(Input<'a>) -> ParseResult<'a, O>,
{
    move |input: Input<'a>| match p(input) {
        Ok((rest, v)) => Ok((rest, Some(v))),
        Err(_) => Ok((input, None)),
    }
}

/// Transform a parser's output with a pure function. `f` cannot fail — for
/// a fallible transform use [`bind`] and produce an explicit
/// [`ParseError`].
pub fn map<'a, O1, O2, P, F>(p: P, f: F) -> impl Fn(Input<'a>) -> ParseResult<'a, O2>
where
    P: Fn(Input<'a>) -> ParseResult<'a, O1>,
    F: Fn(O1) -> O2,
{
    move |input: Input<'a>| {
        let (rest, v) = p(input)?;
        Ok((rest, f(v)))
    }
}

/// Monadic bind: run `p`, then feed its output to `f` which produces the
/// *next* parser. The natural shape for AML `MultiNamePath`, where the
/// second byte (`SegCount`) determines how many `NameSeg`s follow — the
/// count is not a fixed compile-time constant, so it cannot be encoded by
/// [`seq`] alone.
///
/// The returned parser is `impl Fn`; if `f` needs to close over runtime
/// state, capture it before calling `bind`.
pub fn bind<'a, O1, O2, P, F, Q>(p: P, f: F) -> impl Fn(Input<'a>) -> ParseResult<'a, O2>
where
    P: Fn(Input<'a>) -> ParseResult<'a, O1>,
    F: Fn(O1) -> Q,
    Q: Fn(Input<'a>) -> ParseResult<'a, O2>,
{
    move |input: Input<'a>| {
        let (rest, v) = p(input)?;
        let next = f(v);
        next(rest)
    }
}

// ---------------------------------------------------------------------------
// AML character-class helpers (§20.2 encoding)
// ---------------------------------------------------------------------------

/// ACPI 6.5 §20.2.2 `LeadNameChar := 'A'-'Z' | '_'`.
#[inline]
pub const fn is_lead_name_char(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'_')
}

/// ACPI 6.5 §20.2.2 `NameChar := DigitChar | LeadNameChar`.
#[inline]
pub const fn is_name_char(b: u8) -> bool {
    matches!(b, b'0'..=b'9' | b'A'..=b'Z' | b'_')
}

/// ACPI 6.5 §20.2.2 `DigitChar := '0'-'9'`.
#[inline]
pub const fn is_digit_char(b: u8) -> bool {
    matches!(b, b'0'..=b'9')
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- primitives ------------------------------------------------------

    #[test]
    fn byte_matches_and_advances_offset() {
        let src = b"\\_SB_";
        let p = byte(b'\\', "RootChar");
        let (rest, got) = p(Input::new(src)).unwrap();
        assert_eq!(got, b'\\');
        assert_eq!(rest.bytes(), b"_SB_");
        assert_eq!(rest.offset(), 1);
    }

    #[test]
    fn byte_mismatch_reports_actual_and_offset() {
        let src = b"X";
        let p = byte(b'\\', "RootChar");
        let err = p(Input::new(src)).unwrap_err();
        assert_eq!(err.byte_offset, 0);
        assert_eq!(err.expected, "RootChar");
        assert_eq!(err.actual, Some(b'X'));
    }

    #[test]
    fn byte_eof_reports_offset_and_none_actual() {
        let src = b"";
        let p = byte(b'\\', "RootChar");
        let err = p(Input::new(src)).unwrap_err();
        assert_eq!(err.byte_offset, 0);
        assert_eq!(err.actual, None);
    }

    #[test]
    fn take_reads_exact_slice_and_reports_eof_offset() {
        let src = b"_SB_stuff";
        let p = take(4, "NameSeg body");
        let (rest, got) = p(Input::new(src)).unwrap();
        assert_eq!(got, b"_SB_");
        assert_eq!(rest.bytes(), b"stuff");
        assert_eq!(rest.offset(), 4);

        let short = b"AB";
        let err = p(Input::new(short)).unwrap_err();
        // EOF is reported at the offset one-past-the-last-available byte —
        // the position at which the missing byte would have appeared.
        assert_eq!(err.byte_offset, 2);
        assert_eq!(err.actual, None);
    }

    #[test]
    fn satisfy_uses_predicate() {
        let src = b"_SB_";
        let p = satisfy(is_lead_name_char, "LeadNameChar");
        let (rest, got) = p(Input::new(src)).unwrap();
        assert_eq!(got, b'_');
        assert_eq!(rest.offset(), 1);

        let bad = b"9SB_";
        let err = p(Input::new(bad)).unwrap_err();
        assert_eq!(err.actual, Some(b'9'));
        assert_eq!(err.expected, "LeadNameChar");
    }

    // ---- take_while ------------------------------------------------------

    #[test]
    fn take_while_matches_longest_run() {
        let src = b"_SB_.PCI0";
        let (rest, got) = take_while(is_name_char)(Input::new(src)).unwrap();
        assert_eq!(got, b"_SB_");
        assert_eq!(rest.bytes(), b".PCI0");
        assert_eq!(rest.offset(), 4);
    }

    #[test]
    fn take_while_empty_input_yields_empty_slice_no_error() {
        let src: &[u8] = b"";
        let (rest, got) = take_while(is_name_char)(Input::new(src)).unwrap();
        assert!(got.is_empty());
        assert_eq!(rest.offset(), 0);
    }

    #[test]
    fn take_while_no_match_at_start_yields_empty_slice() {
        let src = b"999";
        let (rest, got) = take_while(is_lead_name_char)(Input::new(src)).unwrap();
        assert!(got.is_empty());
        assert_eq!(rest.bytes(), b"999"); // caller can distinguish via len
    }

    // ---- alt: matched, unmatched, backtracking ---------------------------

    #[test]
    fn alt_first_branch_matches() {
        let src = b"\\thing";
        let p = alt(byte(b'\\', "RootChar"), byte(b'^', "ParentPrefixChar"));
        let (rest, got) = p(Input::new(src)).unwrap();
        assert_eq!(got, b'\\');
        assert_eq!(rest.offset(), 1);
    }

    #[test]
    fn alt_backtracks_to_second_branch_on_first_failure() {
        let src = b"^thing";
        let p = alt(byte(b'\\', "RootChar"), byte(b'^', "ParentPrefixChar"));
        let (rest, got) = p(Input::new(src)).unwrap();
        // Second branch succeeded → offset advanced by exactly 1, proving
        // the first branch consumed nothing before the fallback ran.
        assert_eq!(got, b'^');
        assert_eq!(rest.offset(), 1);
    }

    #[test]
    fn alt_both_fail_returns_second_branch_error() {
        let src = b"X";
        let p = alt(byte(b'\\', "RootChar"), byte(b'^', "ParentPrefixChar"));
        let err = p(Input::new(src)).unwrap_err();
        // Second branch's expected string surfaces — matches the doc
        // contract that the caller lists the "more specific" alt last.
        assert_eq!(err.expected, "ParentPrefixChar");
        assert_eq!(err.byte_offset, 0);
        assert_eq!(err.actual, Some(b'X'));
    }

    #[test]
    fn alt_backtracks_across_longer_first_branch_that_failed_mid_way() {
        // Build a first branch that *consumes then fails*, mimicking a
        // partial NameSeg attempt: seq(satisfy lead, satisfy lead again).
        // On input "_9", the second satisfy sees '9' (not a lead char)
        // and fails — the whole seq must yield to alt, and the fallback
        // must see the ORIGINAL "_9", not the post-'_' remainder.
        let src = b"_9";
        let two_leads = seq(
            satisfy(is_lead_name_char, "LeadNameChar #1"),
            satisfy(is_lead_name_char, "LeadNameChar #2"),
        );
        let one_lead_only = map(satisfy(is_lead_name_char, "LeadNameChar"), |b| (b, 0u8));
        let p = alt(two_leads, one_lead_only);
        let (rest, got) = p(Input::new(src)).unwrap();
        assert_eq!(got, (b'_', 0u8));
        assert_eq!(rest.bytes(), b"9");
        assert_eq!(rest.offset(), 1); // proves the fallback started at 0, not 1
    }

    // ---- seq -------------------------------------------------------------

    #[test]
    fn seq_produces_pair_and_advances() {
        let src = b"\\_SB_";
        let p = seq(byte(b'\\', "RootChar"), satisfy(is_lead_name_char, "LeadNameChar"));
        let (rest, (a, b)) = p(Input::new(src)).unwrap();
        assert_eq!((a, b), (b'\\', b'_'));
        assert_eq!(rest.offset(), 2);
    }

    #[test]
    fn seq_propagates_second_stage_error_at_advanced_offset() {
        let src = b"\\9";
        let p = seq(byte(b'\\', "RootChar"), satisfy(is_lead_name_char, "LeadNameChar"));
        let err = p(Input::new(src)).unwrap_err();
        assert_eq!(err.byte_offset, 1); // failure is at the second byte
        assert_eq!(err.actual, Some(b'9'));
        assert_eq!(err.expected, "LeadNameChar");
    }

    // ---- many / many1 ---------------------------------------------------

    #[test]
    fn many_zero_matches_on_empty_input() {
        let src: &[u8] = b"";
        let (rest, out) = many(byte(b'^', "ParentPrefixChar"))(Input::new(src)).unwrap();
        assert!(out.is_empty());
        assert_eq!(rest.offset(), 0);
    }

    #[test]
    fn many_zero_matches_when_first_attempt_fails() {
        let src = b"XYZ";
        let (rest, out) = many(byte(b'^', "ParentPrefixChar"))(Input::new(src)).unwrap();
        assert!(out.is_empty());
        assert_eq!(rest.bytes(), b"XYZ");
        assert_eq!(rest.offset(), 0);
    }

    #[test]
    fn many_collects_prefix_chars() {
        // PrefixPath := Nothing | '^' PrefixPath   — many('^') is the AML
        // idiom for the whole PrefixPath run.
        let src = b"^^^_SB_";
        let (rest, out) = many(byte(b'^', "ParentPrefixChar"))(Input::new(src)).unwrap();
        assert_eq!(out, vec![b'^', b'^', b'^']);
        assert_eq!(rest.bytes(), b"_SB_");
        assert_eq!(rest.offset(), 3);
    }

    #[test]
    fn many1_requires_at_least_one() {
        let empty: &[u8] = b"";
        let err = many1(byte(b'^', "ParentPrefixChar"))(Input::new(empty)).unwrap_err();
        assert_eq!(err.expected, "ParentPrefixChar");
        assert_eq!(err.actual, None);

        let one = b"^rest";
        let (rest, out) = many1(byte(b'^', "ParentPrefixChar"))(Input::new(one)).unwrap();
        assert_eq!(out, vec![b'^']);
        assert_eq!(rest.bytes(), b"rest");
    }

    #[test]
    fn many_terminates_on_forward_progress_guard() {
        // A parser that always succeeds without consuming would loop
        // forever in a naive `many`. The guard breaks after one push.
        // Wrap as a proper HRTB `fn(Input) -> ParseResult<_, _>` so the
        // closure's lifetime signature matches the combinator's bound.
        fn no_advance<'a>(input: Input<'a>) -> ParseResult<'a, u8> {
            Ok((input, 42u8))
        }
        let (rest, out) = many(no_advance)(Input::new(b"abc")).unwrap();
        assert_eq!(out, vec![42u8]);
        assert_eq!(rest.offset(), 0);
    }

    // ---- opt / map / bind -----------------------------------------------

    #[test]
    fn opt_some_and_none_paths() {
        let hit = b"\\rest";
        let (rest, got) = opt(byte(b'\\', "RootChar"))(Input::new(hit)).unwrap();
        assert_eq!(got, Some(b'\\'));
        assert_eq!(rest.bytes(), b"rest");

        let miss = b"rest";
        let (rest, got) = opt(byte(b'\\', "RootChar"))(Input::new(miss)).unwrap();
        assert_eq!(got, None);
        assert_eq!(rest.bytes(), b"rest");
        assert_eq!(rest.offset(), 0);
    }

    #[test]
    fn map_transforms_output() {
        let p = map(satisfy(is_digit_char, "DigitChar"), |b| (b - b'0') as u32);
        let (rest, got) = p(Input::new(b"7abc")).unwrap();
        assert_eq!(got, 7u32);
        assert_eq!(rest.bytes(), b"abc");
    }

    #[test]
    fn bind_lets_a_count_drive_the_next_parser() {
        // Mimics MultiNamePath's SegCount-driven repetition: read one
        // count byte, then `take` that many following bytes.
        let src = &[3u8, b'X', b'Y', b'Z', b'!'];
        let p = bind(
            satisfy(|_| true, "count"),
            |n| take(n as usize, "payload"),
        );
        let (rest, got) = p(Input::new(src)).unwrap();
        assert_eq!(got, b"XYZ");
        assert_eq!(rest.bytes(), b"!");
    }

    // ---- error-offset propagation across combinator layers --------------

    #[test]
    fn error_offset_survives_deep_composition() {
        // Simulate parsing `RootChar NameSeg`, failing at the second byte
        // of the NameSeg because it isn't a NameChar. Layers: seq over
        // (byte, seq(satisfy, take_while)). The failing offset should be
        // absolute (byte position in the whole slice), not relative to
        // the innermost sub-parser.
        let src = b"\\_"; // '\' then '_' then EOF — the NameSeg body wants 3 more
        let seg = seq(
            satisfy(is_lead_name_char, "LeadNameChar"),
            take(3, "NameChar[3]"),
        );
        let name_string = seq(byte(b'\\', "RootChar"), seg);
        let err = name_string(Input::new(src)).unwrap_err();
        // '\' consumed at offset 0, '_' consumed at offset 1, then take(3)
        // fails: only 0 bytes remain but 3 wanted → EOF reported at the
        // one-past-end absolute offset (2 + 0 == 2).
        assert_eq!(err.byte_offset, 2);
        assert_eq!(err.expected, "NameChar[3]");
        assert_eq!(err.actual, None);
    }

    #[test]
    fn error_offset_survives_with_offset_starting_position() {
        // Input::with_offset lets a caller frame a sub-buffer inside a
        // larger stream. Errors must be reported in the outer coordinate.
        let inner = b"X";
        let p = byte(b'\\', "RootChar");
        let err = p(Input::with_offset(inner, 100)).unwrap_err();
        assert_eq!(err.byte_offset, 100);
        assert_eq!(err.actual, Some(b'X'));
    }

    // ---- character-class helpers ----------------------------------------

    #[test]
    fn character_class_helpers_match_acpi_spec() {
        for b in b'A'..=b'Z' {
            assert!(is_lead_name_char(b));
            assert!(is_name_char(b));
        }
        assert!(is_lead_name_char(b'_'));
        assert!(is_name_char(b'_'));
        for b in b'0'..=b'9' {
            assert!(!is_lead_name_char(b));
            assert!(is_name_char(b));
            assert!(is_digit_char(b));
        }
        for b in [b'a', b' ', b'\\', b'^', 0u8, 0x2E, 0x2F] {
            assert!(!is_lead_name_char(b));
            assert!(!is_name_char(b));
            assert!(!is_digit_char(b));
        }
    }

    // ---- an integration proof that the surface is enough for NameSeg ----

    #[test]
    fn parse_name_seg_end_to_end() {
        // NameSeg := LeadNameChar NameChar NameChar NameChar
        // Zero-alloc surface: return borrowed &[u8; 4] over the input.
        fn name_seg<'a>(input: Input<'a>) -> ParseResult<'a, &'a [u8]> {
            // One lead byte then three body bytes — use satisfy + take
            // composed by seq, then map away the intermediate tuple.
            let (rest, _lead) = satisfy(is_lead_name_char, "LeadNameChar")(input)?;
            let (rest, _body) = take(3, "NameChar[3]")(rest)?;
            // Recover the whole 4-byte window borrow from the original
            // input (offset-anchored, no copy).
            let seg = &input.bytes()[..4];
            debug_assert!(seg.iter().all(|&b| is_name_char(b)));
            Ok((rest, seg))
        }

        let (rest, seg) = name_seg(Input::new(b"_SB_.rest")).unwrap();
        assert_eq!(seg, b"_SB_");
        assert_eq!(rest.bytes(), b".rest");

        let err = name_seg(Input::new(b"9SB_")).unwrap_err();
        assert_eq!(err.expected, "LeadNameChar");
        assert_eq!(err.actual, Some(b'9'));

        let err = name_seg(Input::new(b"A_")).unwrap_err();
        assert_eq!(err.expected, "NameChar[3]");
        assert_eq!(err.actual, None);
        assert_eq!(err.byte_offset, 2); // one-past-end of the short input
    }
}
