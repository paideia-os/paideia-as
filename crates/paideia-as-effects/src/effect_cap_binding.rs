//! Effect-name → required-capability-name binding registry.
//!
//! Encodes the R29 driver-substrate rule from
//! `design/roadmap/next-wave-softarch.md` §3 R29 — Structural witness:
//! every driver process' effect row is checked at link time; a driver
//! claiming `!{mmio_read}` in its effect row without holding an
//! `MmioMemCap` in its cap declaration must fail elaboration.
//!
//! This module is the source-of-truth for which cap each named effect
//! demands. Consumers:
//!   - `paideia-as-elaborator::effect_cap_coupling` — the AST-level
//!     coupling walker that emits `C1301` when the coupling fails.
//!   - future paideia-os link-time checkers over PAX cap manifests
//!     (R29.M6+).
//!
//! ## Vocabulary
//!
//! The binding table lists **root effects only** — effects that never
//! get resolved by a user-installed `with handle` block because they
//! bottom out in the runtime or in hardware. A function that declares
//! a root effect in its row must therefore hold the matching cap;
//! there is no handler chain that can substitute.
//!
//! Non-root effects (`Io`, `Net`, …) are intentionally absent: those
//! are typically handled by user code, and the coupling constraint
//! should apply at the handler-installation site rather than at every
//! caller of an `Io`-shaped function. When R29+ introduces a
//! Root-effect vs Handler-effect distinction at the parser level (see
//! `design/roadmap/next-wave-softarch.md` §3 R29), this table becomes
//! the source of truth for the "root" side.
//!
//! - `RawMem` — pointer / index-* intrinsics; runtime-provided.
//! - `Mmio` / `MmioRead` / `MmioWrite` — driver MMIO poke; hardware.
//! - `mmio_read` / `mmio_write` — softarch §5.5 op-style spellings of
//!   the same MMIO cap.
//! - `PortIo` — port I/O (`in_al`/`in_ax`/`in_eax`/`out_al`/`out_ax`/
//!   `out_eax`); hardware. paideia-as#1312: a single root effect covers
//!   both directions rather than splitting `PortIn`/`PortOut` — port
//!   reads routinely mutate device state (status registers clearing on
//!   read, FIFOs popping), so "read" is not the safer half.
//!
//! Additions go at the bottom of `BINDINGS` — the array is scanned in
//! order but each key must appear at most once.

/// A single effect → cap binding.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct EffectCapBinding {
    /// The effect ident as it appears inside a `!{...}` row.
    pub effect: &'static str,
    /// The capability ident as it appears inside a `@{...}` set.
    pub required_cap: &'static str,
}

/// Static table of every effect → required-cap coupling recognised at
/// R29. Order matches append discipline (oldest at top); duplicates are
/// forbidden and enforced by the `bindings_are_unique_by_effect` test.
pub const BINDINGS: &[EffectCapBinding] = &[
    // Pointer/raw-memory family — 07_pointers.pdx, index_* intrinsics.
    EffectCapBinding {
        effect: "RawMem",
        required_cap: "paideia.raw_mem",
    },
    // MMIO family — driver framework (R29+). Compound and op-style
    // spellings all map to the same cap.
    EffectCapBinding {
        effect: "Mmio",
        required_cap: "paideia.mmio",
    },
    EffectCapBinding {
        effect: "MmioRead",
        required_cap: "paideia.mmio",
    },
    EffectCapBinding {
        effect: "MmioWrite",
        required_cap: "paideia.mmio",
    },
    EffectCapBinding {
        effect: "mmio_read",
        required_cap: "paideia.mmio",
    },
    EffectCapBinding {
        effect: "mmio_write",
        required_cap: "paideia.mmio",
    },
    // Port I/O family (paideia-as#1312) — in/out mnemonics. A single
    // root effect covers both directions; see the module-level doc
    // comment above for the rationale.
    EffectCapBinding {
        effect: "PortIo",
        required_cap: "paideia.port_io",
    },
];

/// Look up the required capability name for a given effect name.
///
/// Returns `Some(cap_name)` if the effect appears in the binding table,
/// `None` otherwise (either an effect with no cap requirement, or an
/// unknown effect — the coupling walker skips both, since unknown
/// effects are diagnosed elsewhere).
#[must_use]
pub fn required_cap_for_effect(effect: &str) -> Option<&'static str> {
    BINDINGS
        .iter()
        .find(|b| b.effect == effect)
        .map(|b| b.required_cap)
}

/// Iterate every effect that maps to `cap`. Used by cross-repo audit
/// tools (paideia-os R29.M6 audit surface) to enumerate the effect
/// vocabulary a driver must claim when it holds a given cap.
#[must_use]
pub fn effects_requiring_cap(cap: &str) -> impl Iterator<Item = &'static str> + '_ {
    BINDINGS
        .iter()
        .filter(move |b| b.required_cap == cap)
        .map(|b| b.effect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn required_cap_for_rawmem_returns_paideia_raw_mem() {
        assert_eq!(
            required_cap_for_effect("RawMem"),
            Some("paideia.raw_mem")
        );
    }

    #[test]
    fn required_cap_for_mmio_returns_paideia_mmio() {
        assert_eq!(required_cap_for_effect("Mmio"), Some("paideia.mmio"));
    }

    #[test]
    fn required_cap_for_mmio_read_returns_paideia_mmio() {
        // R29.M2-002: `!{mmio_read}` is the op-style spelling used by the
        // softarch §5.5 fine-grained effect vocabulary.
        assert_eq!(
            required_cap_for_effect("mmio_read"),
            Some("paideia.mmio")
        );
    }

    #[test]
    fn required_cap_for_port_io_returns_paideia_port_io() {
        // paideia-as#1312: PortIo (in_al/in_ax/in_eax/out_al/out_ax/out_eax)
        // mirrors the Mmio family exactly.
        assert_eq!(
            required_cap_for_effect("PortIo"),
            Some("paideia.port_io")
        );
    }

    #[test]
    fn io_is_not_a_root_effect_returns_none() {
        // `Io` and `IO` are non-root — they can be handled by user
        // `with handle Io` blocks (see effects-corpus accept fixtures
        // `polymorphic_through_handler.pdx` etc.). The coupling walker
        // must not flag every Io-shaped function.
        assert_eq!(required_cap_for_effect("Io"), None);
        assert_eq!(required_cap_for_effect("IO"), None);
    }

    #[test]
    fn required_cap_for_unknown_returns_none() {
        assert_eq!(required_cap_for_effect("Unknown"), None);
        assert_eq!(required_cap_for_effect(""), None);
    }

    #[test]
    fn bindings_are_unique_by_effect() {
        // Duplicate effect keys would let ordering silently pick a winner;
        // forbid them at compile-time-adjacent test time.
        let mut seen: HashSet<&'static str> = HashSet::new();
        for b in BINDINGS {
            assert!(
                seen.insert(b.effect),
                "duplicate binding for effect `{}`",
                b.effect
            );
        }
    }

    #[test]
    fn effects_requiring_paideia_mmio_covers_all_mmio_spellings() {
        let set: HashSet<&'static str> =
            effects_requiring_cap("paideia.mmio").collect();
        assert!(set.contains("Mmio"));
        assert!(set.contains("MmioRead"));
        assert!(set.contains("MmioWrite"));
        assert!(set.contains("mmio_read"));
        assert!(set.contains("mmio_write"));
    }

    #[test]
    fn effects_requiring_paideia_port_io_covers_port_io() {
        let set: HashSet<&'static str> =
            effects_requiring_cap("paideia.port_io").collect();
        assert!(set.contains("PortIo"));
    }

    #[test]
    fn effects_requiring_nonexistent_cap_is_empty() {
        assert_eq!(
            effects_requiring_cap("paideia.nowhere").count(),
            0
        );
    }
}
