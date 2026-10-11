//! UNI-14 (paideia-as#1601) — libFuzzer target for `Utf8Decoder`.
//!
//! Drives the hand-rolled streaming RFC 3629 decoder with arbitrary
//! byte sequences and asserts the only invariants that make sense on
//! malformed input:
//!
//! * The decoder terminates — `Iterator::next` eventually returns
//!   `None` even on the pathological "all-continuation-byte" or
//!   "truncated-tail-forever" inputs. `src/utf8.rs` enforces this by
//!   bumping `pos` past the offending byte (one past the slice on a
//!   truncated tail) on every branch.
//! * Decoding does not panic — no slice OOB, no unreachable-arm hit,
//!   no `char::from_u32` returning `Some` for a surrogate / out-of-
//!   range code point (the §4 subrange gating is supposed to make
//!   those branches unreachable; the fuzzer confirms that empirically
//!   on the full `[u8; N]` input space).
//! * `position()` is monotonic and bounded by `bytes.len()`.
//!
//! We deliberately do NOT assert that every `Ok(char)` matches
//! `core::str::from_utf8` byte-for-byte — the two decoders disagree on
//! error *localization* (our decoder keeps going; `core::str::from_utf8`
//! returns a single `Utf8Error` at the first bad byte). The equivalence
//! on well-formed input is covered by the property test under
//! `crates/paideia-as-unicode/tests/`, not here.

#![no_main]

use libfuzzer_sys::fuzz_target;
use paideia_as_unicode::Utf8Decoder;

fuzz_target!(|data: &[u8]| {
    let mut decoder = Utf8Decoder::new(data);
    let mut last_pos = 0usize;
    // Bound the iteration at `data.len() + 1` so a buggy decoder that
    // returns `Some(_)` indefinitely trips the libFuzzer timeout at
    // the loop's exit rather than wedging the worker. Every well-
    // behaved `next()` call advances `position()` by at least one
    // byte, so `data.len()` iterations is a strict upper bound.
    let budget = data.len().saturating_add(1);
    for _ in 0..budget {
        match decoder.next() {
            Some(Ok(_c)) => {
                let pos = decoder.position();
                assert!(pos >= last_pos, "position() went backwards");
                assert!(pos <= data.len(), "position() past slice end");
                last_pos = pos;
            }
            Some(Err(_err)) => {
                let pos = decoder.position();
                assert!(pos >= last_pos, "position() went backwards after Err");
                assert!(pos <= data.len(), "position() past slice end after Err");
                last_pos = pos;
            }
            None => {
                assert!(
                    decoder.is_exhausted(),
                    "decoder returned None but is_exhausted() is false",
                );
                return;
            }
        }
    }
    // If we fall out of the loop the decoder is diverging — fail
    // loudly so libFuzzer records the input.
    panic!("Utf8Decoder failed to terminate within {} iterations", budget);
});
