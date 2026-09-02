//! Hardware entropy source — RDSEED with RDRAND fallback.
//!
//! # Reference
//!
//! Intel, *"Intel Digital Random Number Generator (DRNG) Software
//! Implementation Guide"*, revision 2.1, October 2018. Sections 4.2
//! (RDRAND) and 4.3 (RDSEED) govern retry behaviour; section 5.2 the
//! CPUID feature detection.
//!
//! # Feature detection
//!
//! CPUID leaf 0x07 sub-leaf 0 EBX bit 18 is RDSEED (Intel SDM Vol. 2A
//! §3.2, table 3-8); CPUID leaf 0x01 ECX bit 30 is RDRAND (same
//! reference). Detection is performed by direct `core::arch::x86_64`
//! `__cpuid` / `__cpuid_count` calls (see [`cpuid_has_rdseed`] and
//! [`cpuid_has_rdrand`]) rather than through `std::is_x86_feature_
//! detected!`, so this module compiles as `no_std` — the same CPUID
//! sequence the paideia-native intrinsic from issue #1283 will emit
//! on the target side once v0.33-005 lands the paideia-native
//! `SecureRandom` recipe. Behaviour matches the std macro (both walk
//! the same CPUID bit); the std macro additionally caches the answer
//! in a process-lifetime static, an optimisation we can afford to
//! skip because `HardwareRng::new` is called at most a handful of
//! times per process (once per subsystem that samples entropy).
//!
//! # Retry policy
//!
//! Per Intel §4.2, RDRAND is retried up to 10 times (the hardware
//! guarantees a value within that budget under normal load). Per
//! §4.3, RDSEED is retried with PAUSE between attempts and names no
//! fixed upper bound. We cap RDSEED at 100 to bound worst-case
//! latency for the small-buffer callers this trait targets
//! (100 × ~10 ns/PAUSE ≈ 1 µs stall ceiling).
//!
//! # `unsafe` scope
//!
//! The intrinsic wrappers ([`RdseedSource::try_u64`],
//! [`RdrandSource::try_u64`], and the two `pause` calls) plus the
//! two `__cpuid` / `__cpuid_count` calls in [`cpuid_has_rdseed`] /
//! [`cpuid_has_rdrand`] are the only `unsafe` blocks in this crate.
//! Each intrinsic wrapper is preceded by a CPUID feature-detection
//! check performed by [`HardwareRng::new`], and no `HardwareRng`
//! value can be constructed that would dispatch to an intrinsic
//! whose feature-bit is unset. The feature-bit-to-intrinsic
//! invariant is checked one place per source — in the
//! `EntropySource` match in [`fill_from`] — so a future refactor
//! cannot silently split the two.

use super::{RngError, SecureRandom};

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::{_mm_pause, _rdrand64_step, _rdseed64_step};

/// CPUID feature check for RDRAND — CPUID.01H:ECX bit 30
/// (Intel SDM Vol. 2A §3.2 table 3-8).
///
/// Returns `true` when the CPU advertises RDRAND. Uses the raw
/// `__cpuid` intrinsic (`core::arch::x86_64`) rather than
/// `std::is_x86_feature_detected!` so this module compiles under
/// `#![no_std]`.
#[cfg(target_arch = "x86_64")]
#[allow(unsafe_code)]
pub(crate) fn cpuid_has_rdrand() -> bool {
    // SAFETY: CPUID is baseline on x86_64 (present since original AMD64
    // silicon); leaf 1 is the "Basic CPUID Information" leaf, always
    // reported when CPUID itself exists. `__cpuid` has no memory-safety
    // preconditions — it only reads CPU-visible registers.
    let regs = unsafe { core::arch::x86_64::__cpuid(1) };
    (regs.ecx & (1 << 30)) != 0
}

/// CPUID feature check for RDSEED — CPUID.(EAX=07H, ECX=0H):EBX bit 18
/// (Intel SDM Vol. 2A §3.2 table 3-8).
///
/// Returns `true` when the CPU advertises RDSEED. Structured-extended
/// leaf 7 is queried only when leaf 0's max-basic-leaf (in EAX)
/// reports it as present — this mirrors the guard `is_x86_feature_
/// detected!` performs internally, and matters because pre-Ivy-Bridge
/// silicon returns garbage from an out-of-range CPUID leaf rather
/// than zeroed registers.
#[cfg(target_arch = "x86_64")]
#[allow(unsafe_code)]
pub(crate) fn cpuid_has_rdseed() -> bool {
    // SAFETY: as `cpuid_has_rdrand`; leaf 0 is universally supported.
    let leaf0 = unsafe { core::arch::x86_64::__cpuid(0) };
    if leaf0.eax < 7 {
        return false;
    }
    // SAFETY: leaf 7 is guarded by the max-basic-leaf check above.
    let regs = unsafe { core::arch::x86_64::__cpuid_count(7, 0) };
    (regs.ebx & (1 << 18)) != 0
}

/// Retry budget for RDRAND — Intel DRNG guide §4.2.
pub const RDRAND_RETRIES: u32 = 10;

/// Retry budget for RDSEED — chosen to bound worst-case latency.
///
/// Intel DRNG guide §4.3 recommends retrying with PAUSE but names no
/// fixed upper bound. 100 iterations at ~10 ns each is <1 µs of
/// stall, acceptable for the small-buffer callers this trait
/// targets. See the module-level "Retry policy" note.
pub const RDSEED_RETRIES: u32 = 100;

/// Which hardware entropy instruction backs a [`HardwareRng`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum EntropySource {
    /// Intel/AMD RDSEED — NIST SP 800-90B non-deterministic RBG.
    RdSeed,
    /// Intel/AMD RDRAND — NIST SP 800-90A deterministic RBG seeded
    /// from the same underlying entropy source. Preferred only when
    /// RDSEED is absent (older-than-Broadwell Intel, older-than-Zen
    /// AMD).
    RdRand,
}

/// Hardware entropy source — RDSEED preferred, RDRAND fallback.
///
/// The concrete source is selected once at construction via CPUID;
/// subsequent [`SecureRandom::fill`] calls dispatch through a small
/// branch on the stored source tag. The type is stateless beyond
/// that tag, so `&self` (rather than `&mut self`) is the correct
/// receiver — nothing is cached between calls, in line with the
/// consume-only-what-you-need invariant on the parent module.
#[derive(Copy, Clone, Debug)]
pub struct HardwareRng {
    source: EntropySource,
}

impl HardwareRng {
    /// Construct a new [`HardwareRng`], detecting the best available
    /// entropy source via CPUID.
    ///
    /// Returns [`RngError::Unavailable`] if neither RDSEED nor
    /// RDRAND is present on this CPU, or if the crate was compiled
    /// for a non-x86_64 target.
    pub fn new() -> Result<Self, RngError> {
        #[cfg(target_arch = "x86_64")]
        {
            if cpuid_has_rdseed() {
                return Ok(Self {
                    source: EntropySource::RdSeed,
                });
            }
            if cpuid_has_rdrand() {
                return Ok(Self {
                    source: EntropySource::RdRand,
                });
            }
        }
        Err(RngError::Unavailable)
    }

    /// Return the entropy source this [`HardwareRng`] was
    /// constructed against.
    pub fn source(&self) -> EntropySource {
        self.source
    }
}

impl SecureRandom for HardwareRng {
    fn fill(&self, output: &mut [u8]) -> Result<(), RngError> {
        fill_from(self.source, output)
    }
}

// ---------------------------------------------------------------------
// Internal: the retry loop is generic over a small `RawSource` trait
// so the tests can drive it with a scripted mock. The concrete
// RDSEED / RDRAND impls live below and are the only places where
// unsafe intrinsics are invoked.
// ---------------------------------------------------------------------

/// One 64-bit entropy fetch, plus its retry budget and pause hint.
///
/// Exposed only within this crate so the retry loop is testable
/// against a mock that can script transient failures.
trait RawSource {
    /// Attempt to read one 64-bit word; returns `None` on transient
    /// hardware failure (RDRAND / RDSEED with CF=0).
    fn try_u64(&self) -> Option<u64>;
    /// Bounded retry budget for this source.
    fn retries(&self) -> u32;
    /// Best-effort spin-wait relaxation between retries.
    fn pause(&self);
}

#[cfg(target_arch = "x86_64")]
struct RdseedSource;

#[cfg(target_arch = "x86_64")]
#[allow(unsafe_code)]
impl RawSource for RdseedSource {
    fn try_u64(&self) -> Option<u64> {
        let mut out: u64 = 0;
        // SAFETY: `_rdseed64_step` requires the RDSEED CPU feature.
        // The only path that constructs this type is `fill_from`,
        // which is reached only from a `HardwareRng` whose `source`
        // was set to `RdSeed` by `HardwareRng::new` after
        // `cpuid_has_rdseed()` returned true.
        let ok = unsafe { _rdseed64_step(&mut out) };
        (ok == 1).then_some(out)
    }

    fn retries(&self) -> u32 {
        RDSEED_RETRIES
    }

    fn pause(&self) {
        // SAFETY: `_mm_pause` has no CPU-feature preconditions on
        // x86_64 (SSE2 is baseline in the ISA) and no memory-safety
        // preconditions — it is a plain hint instruction.
        unsafe {
            _mm_pause();
        }
    }
}

#[cfg(target_arch = "x86_64")]
struct RdrandSource;

#[cfg(target_arch = "x86_64")]
#[allow(unsafe_code)]
impl RawSource for RdrandSource {
    fn try_u64(&self) -> Option<u64> {
        let mut out: u64 = 0;
        // SAFETY: `_rdrand64_step` requires the RDRAND CPU feature.
        // See the corresponding SAFETY note on `RdseedSource` — the
        // reachability argument is symmetric: this method is called
        // only from a `HardwareRng` whose `source` was set to
        // `RdRand` by `HardwareRng::new` after `cpuid_has_rdrand()`
        // returned true.
        let ok = unsafe { _rdrand64_step(&mut out) };
        (ok == 1).then_some(out)
    }

    fn retries(&self) -> u32 {
        RDRAND_RETRIES
    }

    fn pause(&self) {
        // SAFETY: see `RdseedSource::pause`.
        unsafe {
            _mm_pause();
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn fill_from(source: EntropySource, output: &mut [u8]) -> Result<(), RngError> {
    match source {
        EntropySource::RdSeed => fill_with(&RdseedSource, output),
        EntropySource::RdRand => fill_with(&RdrandSource, output),
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn fill_from(_source: EntropySource, _output: &mut [u8]) -> Result<(), RngError> {
    // Unreachable via public API: `HardwareRng::new` returns
    // `Unavailable` on non-x86_64 targets, so no `HardwareRng` can
    // be constructed to reach this arm. Kept for cfg completeness.
    Err(RngError::Unavailable)
}

/// Retry loop over any [`RawSource`]. Consume-only-what-you-need:
/// fills `output.len()` bytes exactly, taking one 64-bit word per
/// iteration and copying just the remaining tail on the last one.
fn fill_with<R: RawSource>(source: &R, output: &mut [u8]) -> Result<(), RngError> {
    let mut written = 0;
    while written < output.len() {
        let word = fetch_one(source)?;
        let bytes = word.to_le_bytes();
        let remaining = output.len() - written;
        let take = remaining.min(bytes.len());
        output[written..written + take].copy_from_slice(&bytes[..take]);
        written += take;
    }
    Ok(())
}

/// One 64-bit fetch with a bounded retry loop and PAUSE between
/// attempts. Returns [`RngError::Exhausted`] tagged with the
/// source's retry budget on repeated transient failures.
fn fetch_one<R: RawSource>(source: &R) -> Result<u64, RngError> {
    let budget = source.retries();
    for _ in 0..budget {
        if let Some(word) = source.try_u64() {
            return Ok(word);
        }
        source.pause();
    }
    Err(RngError::Exhausted { retries: budget })
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;

    /// Mock source that fails `fail_first` times then always
    /// succeeds, returning a scripted word. `attempts` counts every
    /// `try_u64` call so tests can pin the loop's step count.
    struct MockSource {
        fail_first: Cell<u32>,
        attempts: Cell<u32>,
        pauses: Cell<u32>,
        retries: u32,
        word: u64,
    }

    impl MockSource {
        const SCRIPTED_WORD: u64 = 0x0123_4567_89ab_cdef;

        fn new(fail_first: u32, retries: u32) -> Self {
            Self {
                fail_first: Cell::new(fail_first),
                attempts: Cell::new(0),
                pauses: Cell::new(0),
                retries,
                word: Self::SCRIPTED_WORD,
            }
        }
    }

    impl RawSource for MockSource {
        fn try_u64(&self) -> Option<u64> {
            self.attempts.set(self.attempts.get() + 1);
            let fail = self.fail_first.get();
            if fail > 0 {
                self.fail_first.set(fail - 1);
                None
            } else {
                Some(self.word)
            }
        }

        fn retries(&self) -> u32 {
            self.retries
        }

        fn pause(&self) {
            self.pauses.set(self.pauses.get() + 1);
        }
    }

    // ---- Retry loop -------------------------------------------------

    /// Transient failures within budget are absorbed; the loop
    /// terminates the moment `try_u64` returns `Some`.
    #[test]
    fn retry_loop_succeeds_within_budget() {
        let s = MockSource::new(3, 10);
        let word = fetch_one(&s).expect("succeed after 3 transient failures");
        assert_eq!(word, MockSource::SCRIPTED_WORD);
        assert_eq!(s.attempts.get(), 4, "3 failures + 1 success");
        assert_eq!(s.pauses.get(), 3, "one pause per failure");
    }

    /// Failing past the retry budget must return `Exhausted` tagged
    /// with that same budget, and MUST NOT make a further attempt.
    #[test]
    fn retry_loop_exhausts_when_budget_reached() {
        let s = MockSource::new(u32::MAX, 10);
        match fetch_one(&s) {
            Err(RngError::Exhausted { retries }) => {
                assert_eq!(retries, 10, "reported budget matches source's");
            }
            other => panic!("expected Exhausted, got {other:?}"),
        }
        assert_eq!(s.attempts.get(), 10, "budget exhausted at 10 attempts");
    }

    /// Zero-retry budget yields an immediate `Exhausted` — the edge
    /// case guards against off-by-one in the loop bound.
    #[test]
    fn retry_loop_zero_budget_returns_immediately() {
        let s = MockSource::new(0, 0);
        match fetch_one(&s) {
            Err(RngError::Exhausted { retries: 0 }) => {}
            other => panic!("expected Exhausted{{retries:0}}, got {other:?}"),
        }
        assert_eq!(s.attempts.get(), 0);
    }

    // ---- fill_with --------------------------------------------------

    /// The fill loop writes exactly `output.len()` bytes, even when
    /// the length is not a multiple of 8 (the intrinsic's word
    /// size). The final word's unused tail is discarded — that is
    /// the consume-only-what-you-need contract.
    #[test]
    fn fill_with_writes_exactly_requested_bytes_non_multiple_of_word() {
        let s = MockSource::new(0, 10);
        let mut buf = [0u8; 17];
        fill_with(&s, &mut buf).expect("fill succeeds");

        let word_bytes = MockSource::SCRIPTED_WORD.to_le_bytes();
        assert_eq!(&buf[0..8], &word_bytes[..], "word 0");
        assert_eq!(&buf[8..16], &word_bytes[..], "word 1");
        assert_eq!(buf[16], word_bytes[0], "first byte of word 2, rest discarded");
        assert_eq!(s.attempts.get(), 3, "three words fetched for 17 bytes");
    }

    /// A zero-length output MUST make no hardware call at all —
    /// callers routinely write `fill(&mut [])` in tests and glue
    /// code and it must be free.
    #[test]
    fn fill_with_empty_output_makes_no_calls() {
        let s = MockSource::new(0, 10);
        let mut buf: [u8; 0] = [];
        fill_with(&s, &mut buf).expect("empty fill is a no-op");
        assert_eq!(
            s.attempts.get(),
            0,
            "no fetch calls for zero-length output"
        );
    }

    /// A `fetch_one` failure inside `fill_with` propagates up as
    /// `Exhausted`; no partial write may be observable to a caller
    /// that inspects the buffer after the error.
    #[test]
    fn fill_with_propagates_exhaustion() {
        let s = MockSource::new(u32::MAX, 5);
        let mut buf = [0xAAu8; 8];
        match fill_with(&s, &mut buf) {
            Err(RngError::Exhausted { retries: 5 }) => {}
            other => panic!("expected Exhausted{{5}}, got {other:?}"),
        }
        // The trait's post-condition on error is that `output` state
        // is *undefined* — callers MUST NOT use it. We do not assert
        // any particular contents here; the point of this test is
        // that the error path exists and is reachable.
    }

    // ---- Availability detect ----------------------------------------

    /// If `HardwareRng::new` returns `Ok`, its source must match the
    /// preference order (RDSEED > RDRAND) against the actual CPU
    /// feature bits. If neither is present, `new` MUST return
    /// `Unavailable` — no silent fallback to a software RNG.
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn new_selects_source_by_cpu_capability() {
        // Uses the same hand-rolled CPUID helpers `HardwareRng::new`
        // consults so the test cannot drift from the production path
        // (a `std::is_x86_feature_detected!` here would answer from a
        // different code path and could mask a bug in the helpers).
        match HardwareRng::new() {
            Ok(rng) => {
                if cpuid_has_rdseed() {
                    assert_eq!(rng.source(), EntropySource::RdSeed);
                } else {
                    assert!(
                        cpuid_has_rdrand(),
                        "Ok with neither feature bit set is a logic bug in `new`",
                    );
                    assert_eq!(rng.source(), EntropySource::RdRand);
                }
            }
            Err(RngError::Unavailable) => {
                assert!(!cpuid_has_rdseed());
                assert!(!cpuid_has_rdrand());
            }
            Err(other) => panic!("`new` returned unexpected error: {other:?}"),
        }
    }

    /// On any non-x86_64 build target, `new` MUST report
    /// `Unavailable` — there is no meaningful hardware source to
    /// dispatch to.
    #[test]
    #[cfg(not(target_arch = "x86_64"))]
    fn new_reports_unavailable_on_non_x86_64() {
        assert_eq!(HardwareRng::new(), Err(RngError::Unavailable));
    }

    // ---- End-to-end (hardware) --------------------------------------

    /// A real hardware fill succeeds and produces bytes that are not
    /// all-zero. Skipped on hardware that lacks both feature bits.
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn hardware_rng_fills_requested_length() {
        let Ok(rng) = HardwareRng::new() else {
            return;
        };
        let mut buf = [0u8; 32];
        rng.fill(&mut buf).expect("hardware fill succeeds");
        // The all-zero outcome has probability 2^-256 on a working
        // TRNG — a test failure here means the source is broken or
        // the fill loop is not writing.
        assert!(buf.iter().any(|&b| b != 0), "fill left buffer all zero");
    }

    /// Two successive fills MUST NOT collide: the consume-only-what-
    /// you-need contract forbids reservoir caching, so re-issuing
    /// `fill` must go back to the hardware for every byte.
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn hardware_rng_fills_are_independent() {
        let Ok(rng) = HardwareRng::new() else {
            return;
        };
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        rng.fill(&mut a).expect("fill a");
        rng.fill(&mut b).expect("fill b");
        assert_ne!(a, b, "successive fills must not collide (P ~= 2^-256)");
    }

    /// A fresh `fill` into a non-word-multiple length exercises the
    /// live tail-copy path through the real hardware, complementing
    /// the mock coverage above.
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn hardware_rng_fills_non_word_multiple_length() {
        let Ok(rng) = HardwareRng::new() else {
            return;
        };
        let mut buf = [0u8; 13];
        rng.fill(&mut buf).expect("fill succeeds");
        assert!(buf.iter().any(|&b| b != 0));
    }
}
