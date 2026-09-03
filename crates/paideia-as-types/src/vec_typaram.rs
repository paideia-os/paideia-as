//! `vec<T, N>` — parameterized SIMD / shader-vector type constructor.
//!
//! Introduced by v0.28-M1-002 (paideia-as#1371) as the natural argument
//! shape inside `@gpu_context` blocks (b2-03) and the assembler's SIMD
//! lowering path. A `vec<T, N>` is a packed, homogeneous value with a
//! fixed lane count `N` — typically 2/3/4/8/16 for x86_64 SIMD, and
//! larger `N` for shader/kernel vectors.
//!
//! Semantics
//! ---------
//! - `T` is a scalar with kind `type` (i8/i16/i32/i64/u*/f16/f32/f64).
//! - `N` is a const-generic `nat` (encoded as `u32` here — matching the
//!   phase-1 sentinel width for integer types).
//! - Encoding: [`crate::types::Type::Vec`] with fields `{ elem, n }`.
//! - Layout: `size = N * size_of(T)`; `align = align_of(T)` clamped to
//!   16 for `N >= 2` (SIMD alignment floor for AVX/AVX-2 `movaps`).
//! - Kind: `vec<T, N> : type` iff `T : type` and `N : nat` (N ≥ 1).
//!
//! Rationale for factoring out of [`crate::layout`]
//! ------------------------------------------------
//! v0.28-M1-002 explicitly forbids editing `layout.rs`. SIMD alignment
//! policy is expected to evolve (AVX-512 32-byte and 64-byte clamps,
//! sub-16-byte lanes for `vec<u8,3>`-style shader shapes, GPU-side
//! natural alignments, …). Keeping the vec-specific rules local here
//! makes those future tuning knobs self-contained and keeps the general
//! `layout_of` walker unbiased.

use crate::intern::TypeInterner;
use crate::kinds::{Kind, type_kind};
use crate::layout::{Layout, layout_of};
use crate::types::{Type, TypeId};

/// SIMD alignment floor for multi-lane vectors.
///
/// x86_64 AVX / AVX-2 require 16-byte alignment for aligned SIMD loads
/// (`movaps` / `movdqa`). AVX-512 uses 32/64-byte alignments that later
/// codegen layers can raise on top of this baseline; this module never
/// lowers below 16 for `N >= 2`.
const VEC_ALIGN_CLAMP: u64 = 16;

/// Builder / handle for a `vec<T, N>` type.
///
/// Wraps the `(elem, n)` pair with a small, ergonomic surface so callers
/// (elaborator, GPU-context lowering, MIR printer) don't repeat the raw
/// [`Type::Vec`] construction site. Interning is preserved: two
/// [`VecTy::intern`] calls with the same `elem` and `n` map to the same
/// [`TypeId`] via hash-consing.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct VecTy {
    /// Lane / element scalar type.
    pub elem: TypeId,
    /// Lane count (const-generic `nat`).
    pub n: u32,
}

impl VecTy {
    /// Construct a `VecTy` handle from an element type and lane count.
    ///
    /// Does not validate the element's kind — that is the province of
    /// [`kind_of_vec`]; construction is intentionally cheap and infallible
    /// so front-end pipelines can assemble the handle before the kind
    /// checker has run.
    #[inline]
    pub const fn new(elem: TypeId, n: u32) -> Self {
        Self { elem, n }
    }

    /// Intern this `vec<T, N>` into the type interner and return its id.
    ///
    /// Idempotent under hash-consing: repeated interning of the same
    /// `(elem, n)` returns the same [`TypeId`].
    pub fn intern(self, interner: &mut TypeInterner) -> TypeId {
        interner.intern(Type::Vec {
            elem: self.elem,
            n: self.n,
        })
    }

    /// Compute the layout of this vector type via [`vec_layout`].
    #[inline]
    pub fn layout(self, interner: &TypeInterner) -> Layout {
        vec_layout(interner, self.elem, self.n)
    }

    /// Kind check via [`kind_of_vec`].
    #[inline]
    pub fn kind(self, interner: &TypeInterner) -> Option<Kind> {
        kind_of_vec(interner, self.elem, self.n)
    }
}

/// Standalone layout entry for `vec<T, N>`.
///
/// Rules:
/// - `size = N * size_of(T)` — SIMD packs lanes contiguously; no
///   inter-lane padding.
/// - `align = align_of(T)` when `N == 1` (degenerate scalar-alike lane).
/// - `align = max(align_of(T), 16)` when `N >= 2` — the SIMD floor.
/// - `N == 0` yields a zero-sized value with `align = 1`; the parser
///   should never emit `vec<T, 0>` (rejected by [`kind_of_vec`]), but
///   the layout equation is defined defensively so proptests can
///   exercise the boundary without a panic path.
///
/// This function is intentionally NOT dispatched from
/// [`crate::layout::layout_of`] — see the module docstring.
pub fn vec_layout(interner: &TypeInterner, elem: TypeId, n: u32) -> Layout {
    if n == 0 {
        return Layout::new(0, 1);
    }
    let elem_layout = layout_of(interner, elem);
    let size = (n as u64).saturating_mul(elem_layout.size);
    let alignment = if n >= 2 {
        elem_layout.alignment.max(VEC_ALIGN_CLAMP)
    } else {
        elem_layout.alignment
    };
    Layout { size, alignment }
}

/// Well-formedness / kind check for `vec<T, N>`.
///
/// Returns `Some(k)` — the kind of the resulting vector type — iff both
/// premises hold:
/// - `T` (`elem`) has kind `type`, i.e. is an accepted scalar lane type
///   (`SInt`, `UInt`, or `Float`). Records, tuples, pointers, references,
///   and other non-scalars are rejected here; SIMD lanes are scalar.
/// - `N >= 1` (a natural). Zero is rejected as ill-formed at the type
///   level; the layout entry still treats `N == 0` defensively so it
///   never panics on unexpected input.
///
/// Returns `None` on either failure. The kind returned is inherited from
/// the element via [`crate::kinds::type_kind`] (phase-1: `Unrestricted`
/// for every scalar).
pub fn kind_of_vec(interner: &TypeInterner, elem: TypeId, n: u32) -> Option<Kind> {
    if n == 0 {
        return None;
    }
    let elem_ty = interner.get(elem);
    match elem_ty {
        Type::SInt(_) | Type::UInt(_) | Type::Float(_) => {}
        _ => return None,
    }
    Some(type_kind(elem, elem_ty))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// Scalar lane widths admitted by SIMD / shader vectors in phase-1.
    ///
    /// f16 lands with the AVX-512 milestone; the proptest strategy
    /// exercises 8/16/32/64-bit lanes today, which covers the SSE and
    /// AVX-2 baseline.
    fn arb_scalar_bits() -> impl Strategy<Value = u16> {
        prop_oneof![Just(8u16), Just(16), Just(32), Just(64)]
    }

    // ── unit-shaped corner cases ───────────────────────────────────────

    #[test]
    fn vec_f32_4_is_16_bytes_aligned_16() {
        let mut interner = TypeInterner::new();
        let f32_id = interner.float(32);
        let layout = vec_layout(&interner, f32_id, 4);
        assert_eq!(layout.size, 16);
        assert_eq!(layout.alignment, 16);
    }

    #[test]
    fn vec_u8_16_is_16_bytes_aligned_16() {
        let mut interner = TypeInterner::new();
        let u8_id = interner.uint(8);
        let layout = vec_layout(&interner, u8_id, 16);
        assert_eq!(layout.size, 16);
        assert_eq!(layout.alignment, 16, "N>=2 clamps align to 16 even for 1-byte lanes");
    }

    #[test]
    fn vec_u64_1_is_scalar_alike() {
        let mut interner = TypeInterner::new();
        let u64_id = interner.uint(64);
        let layout = vec_layout(&interner, u64_id, 1);
        assert_eq!(layout.size, 8);
        assert_eq!(layout.alignment, 8, "N==1 leaves alignment at element's own");
    }

    #[test]
    fn vec_zero_lanes_is_zst() {
        let mut interner = TypeInterner::new();
        let u32_id = interner.uint(32);
        let layout = vec_layout(&interner, u32_id, 0);
        assert_eq!(layout.size, 0);
        assert_eq!(layout.alignment, 1);
    }

    #[test]
    fn vec_ty_interns_are_hash_consed() {
        let mut interner = TypeInterner::new();
        let f32_id = interner.float(32);
        let a = VecTy::new(f32_id, 4).intern(&mut interner);
        let b = VecTy::new(f32_id, 4).intern(&mut interner);
        assert_eq!(a, b, "hash-consing must dedupe equal vec types");
    }

    #[test]
    fn vec_ty_distinguishes_lane_count() {
        let mut interner = TypeInterner::new();
        let f32_id = interner.float(32);
        let v4 = VecTy::new(f32_id, 4).intern(&mut interner);
        let v8 = VecTy::new(f32_id, 8).intern(&mut interner);
        assert_ne!(v4, v8);
    }

    #[test]
    fn vec_ty_distinguishes_lane_type() {
        let mut interner = TypeInterner::new();
        let f32_id = interner.float(32);
        let i32_id = interner.sint(32);
        let vf = VecTy::new(f32_id, 4).intern(&mut interner);
        let vi = VecTy::new(i32_id, 4).intern(&mut interner);
        assert_ne!(vf, vi);
    }

    #[test]
    fn kind_of_vec_rejects_non_scalar_element() {
        let mut interner = TypeInterner::new();
        let bool_id = interner.bool_ty();
        assert!(kind_of_vec(&interner, bool_id, 4).is_none());

        let unit_id = interner.unit();
        assert!(kind_of_vec(&interner, unit_id, 4).is_none());
    }

    #[test]
    fn kind_of_vec_rejects_zero_lanes() {
        let mut interner = TypeInterner::new();
        let f32_id = interner.float(32);
        assert!(kind_of_vec(&interner, f32_id, 0).is_none());
    }

    #[test]
    fn kind_of_vec_accepts_scalar_lanes() {
        let mut interner = TypeInterner::new();
        let f32_id = interner.float(32);
        assert_eq!(kind_of_vec(&interner, f32_id, 4), Some(Kind::Unrestricted));
    }

    // ── proptest: dimension independence & layout monotonicity ─────────

    proptest! {
        /// Dimension-independence: `size(vec<T,N>) = N * size_of(T)` for every
        /// admitted scalar lane and every N in the tested range. The equation
        /// holds identically across lane widths — that is the linearity
        /// property the codegen depends on.
        #[test]
        fn prop_vec_layout_size_is_n_times_elem_size(
            bits in arb_scalar_bits(),
            n in 1u32..=64,
        ) {
            let mut interner = TypeInterner::new();
            let elem = interner.uint(bits);
            let elem_size = layout_of(&interner, elem).size;
            let vec_size = vec_layout(&interner, elem, n).size;
            prop_assert_eq!(vec_size, (n as u64) * elem_size);
        }

        /// Layout monotonicity: for fixed `T`, `N1 <= N2 ⇒ size1 <= size2`.
        /// Growing the lane count never shrinks the vector.
        #[test]
        fn prop_vec_layout_size_monotonic_in_n(
            bits in arb_scalar_bits(),
            n1 in 1u32..=48,
            n2 in 1u32..=48,
        ) {
            let mut interner = TypeInterner::new();
            let elem = interner.uint(bits);
            let s1 = vec_layout(&interner, elem, n1).size;
            let s2 = vec_layout(&interner, elem, n2).size;
            if n1 <= n2 {
                prop_assert!(s1 <= s2);
            } else {
                prop_assert!(s1 >= s2);
            }
        }

        /// SIMD alignment floor: for `N >= 2` alignment is at least 16 bytes.
        #[test]
        fn prop_vec_layout_align_clamped_when_multi_lane(
            bits in arb_scalar_bits(),
            n in 2u32..=64,
        ) {
            let mut interner = TypeInterner::new();
            let elem = interner.uint(bits);
            let align = vec_layout(&interner, elem, n).alignment;
            prop_assert!(align >= VEC_ALIGN_CLAMP);
        }

        /// Single-lane vectors preserve the element's alignment (no clamp).
        #[test]
        fn prop_vec_layout_align_equals_elem_when_single_lane(
            bits in arb_scalar_bits(),
        ) {
            let mut interner = TypeInterner::new();
            let elem = interner.uint(bits);
            let elem_align = layout_of(&interner, elem).alignment;
            let align = vec_layout(&interner, elem, 1).alignment;
            prop_assert_eq!(align, elem_align);
        }

        /// Hash-consing under proptest: `(elem, n)` maps to a stable TypeId.
        #[test]
        fn prop_vec_ty_intern_is_stable(
            bits in arb_scalar_bits(),
            n in 1u32..=32,
        ) {
            let mut interner = TypeInterner::new();
            let elem = interner.uint(bits);
            let id1 = VecTy::new(elem, n).intern(&mut interner);
            let id2 = VecTy::new(elem, n).intern(&mut interner);
            prop_assert_eq!(id1, id2);
        }
    }
}
