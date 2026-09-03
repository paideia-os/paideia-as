//! paideia-satellite-runtime — link-line shim for satellite host tools.
//!
//! # Why this crate exists
//!
//! `.pdx` code compiled by `paideia-as` emits `call` relocations
//! against four FFI intrinsics (declared by the
//! `stdlib_lowering::cryptoops` and `stdlib_lowering::mldsaops`
//! recipes):
//!
//! - `paideia_crypto_argon2id_derive`
//! - `paideia_crypto_chacha20_poly1305_seal`
//! - `paideia_crypto_chacha20_poly1305_open`
//! - `paideia_crypto_ml_kem_768_keygen`   (paideia-as#1352)
//! - `paideia_crypto_ml_kem_768_encaps`   (paideia-as#1352)
//! - `paideia_crypto_ml_kem_768_decaps`   (paideia-as#1352)
//! - `mldsa65_sign_runtime_entry`
//!
//! On the paideia-os KERNEL build these resolve at link time against
//! the `paideia-as-crypto` and `paideia-pq-sign` rlibs. On satellite
//! HOST-TOOLCHAIN builds (`mkfs.pdxfs`, `mount.pdxfs`, `umount.pdxfs`,
//! and future kin) neither rlib is on the link line — the final
//! `ld` step consequently fails with four unresolved symbols. See
//! `design/infrastructure/satellite-runtime-shim.md` for the full
//! problem statement, gap analysis, and rationale for the fix chosen
//! here.
//!
//! This crate closes that gap by shipping a single satellite-linkable
//! `.a` archive — `target/release/libpaideia_satellite_runtime.a` —
//! that carries all four symbols AND the small no_std runtime
//! infrastructure (bump allocator + panic handler + eh_personality
//! stub) that lets a satellite `ld -nostdlib` line resolve without
//! dragging std / libc / `_Unwind_*` scaffolding.
//!
//! # Symbol sourcing
//!
//! - The three crypto thunks are re-exported (via Rust-level
//!   [`pub use`]) from `paideia-as-crypto`. This is deliberate: the
//!   design §3.1 R1 mitigation requires one source of truth for the
//!   AEAD (RFC 8439) and Argon2id (RFC 9106) bodies so a volume
//!   `mkfs`'d on the host is guaranteed byte-readable by the
//!   kernel-side `mount`. Rust's re-export mechanism (as opposed to
//!   a hand-written trampoline that re-declares the extern-C symbol
//!   in this crate) ensures `ld` finds exactly one definition of
//!   each symbol; the crypto crate's `#[unsafe(no_mangle)]` remains
//!   the sole authority on the symbol's name and body.
//!
//! - `mldsa65_sign_runtime_entry` is DEFINED HERE as a fail-closed
//!   stub returning [`PDX_MLDSA_ERR_NO_SIGNER`] (-6). This is
//!   band-consistent with `paideia-pq-sign`'s existing sentinel band
//!   (-1..-3 in use, -4/-5 reserved). See design §3.2 for the
//!   rationale: none of the currently-shipping satellite tools
//!   INVOKE this intrinsic at runtime (they only need it to link),
//!   and any future signing satellite (fsck.pdxfs / pkg.paideia-os)
//!   will link against a purpose-built `paideia-pq-sign` staticlib
//!   instead of pulling `yubihsm` / `cryptoki` / `reqwest` into
//!   every `/bin` binary's dependency closure.
//!
//! # Consumer contract
//!
//! Satellite `build.sh` invocations pass
//! `--extra-archive $PAS_TARGET/libpaideia_satellite_runtime.a`
//! as the FINAL link input (after all `.pdx`-compiled `.o`'s and
//! after any other satellite archives). The four FFI symbols above
//! resolve; the archive also provides the small runtime pieces every
//! Rust `no_std` staticlib must define — a bump allocator, an
//! abort-shape panic handler, and an `eh_personality` stub. Together
//! these let a `ld -nostdlib --warn-common --fatal-warnings
//! -T link.ld` link line close without a single unresolved symbol
//! from paideia-as's side of the wire.
//!
//! # Runtime infrastructure — architecture rationale
//!
//! The 0.29.0/0.29.1 landings tried to make `paideia-as-crypto`
//! itself a staticlib. That failed at compile time: emitting a
//! `no_std` staticlib forces the Rust compiler to require a
//! `#[global_allocator]` + `#[panic_handler]` + `panic = "abort"`
//! inside the crate producing the staticlib. Those three decisions
//! do not belong to a leaf crypto library — they belong to the
//! final ELF. The corrected shape (from 0.29.1 onward) is:
//!
//!   * `paideia-as-crypto` is `no_std + alloc`, `rlib`-only. It
//!     never sees `#[global_allocator]` / `#[panic_handler]`; the
//!     Rust compiler defers those requirements to whichever crate
//!     eventually links its object code into a binary or staticlib.
//!   * `paideia-satellite-runtime` is the ONE staticlib in the
//!     workspace. It consumes `paideia-as-crypto` as an rlib,
//!     wraps it with the runtime pieces the Rust compiler demands
//!     (allocator + panic handler + eh_personality), and emits
//!     the archive satellite `build.sh` scripts consume.
//!
//! Design doc rationale: `design/infrastructure/satellite-runtime-
//! shim.md` §4.1 is the authoritative statement of the shape.
//!
//! ## Bump allocator
//!
//! A 4 MiB static byte pool + a monotonically-increasing offset. No
//! free operation — allocations "leak" for the process lifetime.
//! That is intentional and correct for the satellite tool profile:
//!
//!   * Every currently-shipping satellite (`mkfs.pdxfs`,
//!     `mount.pdxfs`, `umount.pdxfs`) is a short-lived one-shot
//!     process. The peak Rust-side allocation is bounded by argon2's
//!     memory-hard buffer (parameter-limited to a few MiB at the
//!     settings paideia-os uses) plus the ChaCha20-Poly1305 seal /
//!     open output vectors (bounded by caller-supplied plaintext
//!     length). 4 MiB is comfortably above the observed high-water
//!     mark and still tiny relative to any modern /bin binary.
//!   * A free-supporting allocator (e.g. `linked_list_allocator` at
//!     ~2 KiB of code) would add a Rust dep to this crate that no
//!     current satellite requires. If a future satellite runs long
//!     enough to matter, swap this bump for `linked_list_allocator`
//!     behind an off-by-default feature — the trait shape at the
//!     `#[global_allocator]` boundary does not change.
//!
//! Alignment is honoured (allocations round up to the requested
//! `Layout::align()`); requests that would overflow the pool return
//! null, which the Rust `alloc` layer surfaces as an allocation
//! failure the caller can observe.
//!
//! ## Panic handler
//!
//! Abort-shape: on panic, execute the x86_64 `hlt` instruction in
//! an infinite loop. Emitting `hlt` (rather than calling
//! `core::intrinsics::abort()`) avoids pulling any additional
//! compiler-builtins path and keeps the handler's symbol closure
//! empty — no `libc::abort`, no `_Unwind_*`, no `SIGABRT` wiring.
//! For a satellite host tool this is a hard-fault of the current
//! process, which is exactly the semantics we want: a satellite
//! that has panicked into unreachable state must not proceed to
//! commit partial state to a volume.
//!
//! ## eh_personality stub
//!
//! `panic = "abort"` at the workspace `[profile.release]` level
//! eliminates the compiler's own unwind emission. A single empty
//! `rust_eh_personality` symbol is still exported as a defensive
//! belt-and-suspenders: some downstream `.o` produced by a
//! non-workspace toolchain revision could hold a residual
//! `.eh_frame` FDE referencing this personality routine by name.
//! Exporting an empty function costs zero runtime and eliminates
//! that class of link-time surprise for satellite ELFs.
//!
//! # Safety
//!
//! The re-exported crypto thunks retain the safety obligations
//! documented on their original definitions in
//! `paideia-as-crypto::ffi::{paideia_crypto_argon2id_derive,
//! paideia_crypto_chacha20_poly1305_seal,
//! paideia_crypto_chacha20_poly1305_open}`.
//!
//! The `mldsa65_sign_runtime_entry` stub does no pointer
//! dereferences — it returns the error sentinel unconditionally
//! without inspecting inputs, precisely because the fail-closed
//! contract guarantees no caller reaches the sign path in a
//! satellite build.
//!
//! The bump allocator's `GlobalAlloc::alloc` uses an unchecked
//! `unsafe` block to fabricate a `*mut u8` from an offset into the
//! static pool; the safety obligation on the returned pointer is
//! that it be valid for `layout.size()` bytes and aligned to
//! `layout.align()`, which the offset arithmetic satisfies (see
//! comments on the impl). The `dealloc` is a no-op (see rationale
//! above) and thus trivially safe.
//!
//! The `#[panic_handler]` and the `eh_personality` stub are
//! language items; the Rust compiler is responsible for calling
//! them with well-formed arguments.

#![no_std]
// NOTE: this crate deliberately uses `unsafe` for the language-item
// pieces the Rust compiler requires of every `no_std` staticlib —
// `GlobalAlloc`, `#[panic_handler]`, and `rust_eh_personality`.
// The crate-scope `#![deny(unsafe_code)]` from earlier revisions has
// been dropped for exactly that reason; local safety notes on each
// `unsafe` block document the obligation being upheld.

extern crate alloc;

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

// ---------------------------------------------------------------------
// Crypto — re-export from the single-source `paideia-as-crypto` crate.
// ---------------------------------------------------------------------
//
// These three `pub use` statements are how design §3.1's "single
// source of truth" invariant is enforced in Rust:
//   * The symbol NAMES are `#[unsafe(no_mangle)]` in the source crate,
//     so `ld` sees exactly one archive-scope definition each — no
//     duplicate-symbol linker error, no silent shadowing.
//   * Making them `pub use` (rather than `#[allow(dead_code)] use`)
//     documents that they are part of the satellite runtime's public
//     ABI surface. A future audit that walks
//     `libpaideia_satellite_runtime.a`'s exported symbols will find
//     all four in one place.
//   * The `pub use` also forces cargo to depend on
//     `paideia-as-crypto`'s object code, so the FFI thunks are
//     definitely packed into the staticlib archive (rather than
//     dead-code-eliminated).
pub use paideia_as_crypto::ffi::paideia_crypto_argon2id_derive;
pub use paideia_as_crypto::ffi::paideia_crypto_chacha20_poly1305_open;
pub use paideia_as_crypto::ffi::paideia_crypto_chacha20_poly1305_seal;
// paideia-as#1352 — ML-KEM-768 KEM (FIPS 203). Same re-export
// discipline as the crypto trio above: the `#[unsafe(no_mangle)]`
// bodies live in `paideia-as-crypto`, `pub use` here forces cargo
// to pack them into `libpaideia_satellite_runtime.a` so satellite
// `ld -nostdlib` lines resolve. R108 device-to-device key
// agreement and R91+ networking are the intended consumers; a
// satellite whose object graph never reaches the KEM call chain
// still needs these symbols to resolve because `.pdx` compilation
// emits the `call` relocations regardless of runtime reachability.
pub use paideia_as_crypto::ffi::paideia_crypto_ml_kem_768_decaps;
pub use paideia_as_crypto::ffi::paideia_crypto_ml_kem_768_encaps;
pub use paideia_as_crypto::ffi::paideia_crypto_ml_kem_768_keygen;

// ---------------------------------------------------------------------
// Signing — fail-closed stub for mldsa65_sign_runtime_entry.
// ---------------------------------------------------------------------

/// Fail-closed sentinel for the satellite `mldsa65_sign_runtime_entry`.
///
/// Value: `-6`. Chosen to be band-consistent with the existing
/// `paideia-pq-sign::ffi::PDX_MLDSA_*` codes (`-1` = InvalidParam,
/// `-2` = Length, `-3` = Authentication; `-4`/`-5` reserved by the
/// paideia-as-crypto crypto-error band for cross-primitive alias
/// reuse). `-6` is the first previously-unused code and unambiguously
/// signals "no signer is compiled into this binary" as distinct from
/// any of the runtime-error variants a real signer might return.
///
/// Consumers on the `.pdx` side (per design §3.2 consequence, tracked
/// as a follow-up in `libpdx-volume`'s `pdxb_sign.pdx`) MUST surface
/// this code upward as a user-visible error rather than silently
/// treating it as success — otherwise a satellite would produce a
/// "signed" volume that is not actually signed and cannot be
/// verified. This is design risk R4 in the shim document.
pub const PDX_MLDSA_ERR_NO_SIGNER: i64 = -6;

/// Satellite build of `mldsa65_sign_runtime_entry` — fail-closed.
///
/// # Contract
///
/// This symbol MUST resolve at satellite link time (the
/// `stdlib_lowering::mldsaops` recipe emits a `call` relocation to
/// it whenever a `.pdx` module imports the ML-DSA-65 sign intrinsic,
/// even when the runtime code path never calls it). This body
/// returns [`PDX_MLDSA_ERR_NO_SIGNER`] unconditionally; no input is
/// inspected, no output is written.
///
/// # Signature — must match `paideia-pq-sign::ffi::mldsa65_sign_runtime_entry`
///
/// Argument order and types are the SysV AMD64 register mapping
/// documented on the kernel-side symbol (see
/// `crates/paideia-pq-sign/src/ffi.rs`):
///
/// | Register | Meaning                                          |
/// |----------|--------------------------------------------------|
/// | RDI      | `seed_ptr`     — `*const u8`, 32-byte seed       |
/// | RSI      | `msg_ptr`      — `*const u8`                     |
/// | RDX      | `msg_len`      — `usize`                         |
/// | RCX      | `sig_out_ptr`  — `*mut u8`, >= 3309 bytes        |
/// | **RAX**  | return code                                      |
///
/// Any drift between this signature and the kernel-side signature
/// would silently break the fallback safety contract — a caller
/// expecting `sig_out_ptr` in RCX would find garbage there and,
/// worse, would not know it. Keep them lockstep.
///
/// # Safety
///
/// The function does not dereference any argument pointer, so the
/// usual `slice::from_raw_parts` safety obligations do not apply on
/// this build. The `#[unsafe(no_mangle)]` attribute is still needed
/// so the symbol is exported under its exact name for the linker.
#[unsafe(no_mangle)]
pub extern "C" fn mldsa65_sign_runtime_entry(
    _seed_ptr: *const u8,
    _msg_ptr: *const u8,
    _msg_len: usize,
    _sig_out_ptr: *mut u8,
) -> i64 {
    PDX_MLDSA_ERR_NO_SIGNER
}

// ---------------------------------------------------------------------
// Global allocator — hand-rolled bump allocator over a static pool.
// ---------------------------------------------------------------------
//
// See the module-level "Bump allocator" section for the rationale.
// Summary: satellite tools are one-shot processes with a bounded
// Rust-side heap footprint; a free-supporting allocator would add a
// Rust dep this crate does not currently need. If a future satellite
// runs long enough for fragmentation or reclamation to matter, swap
// this for `linked_list_allocator` behind an off-by-default feature —
// the `#[global_allocator]` boundary does not change.

/// 4 MiB — comfortably above the observed high-water mark of any
/// currently-shipping satellite's Rust-side allocations (argon2
/// memory-hard buffer at paideia-os settings + ChaCha20-Poly1305
/// seal/open Vec returns bounded by caller plaintext length). Raise
/// if a satellite is observed to hit the pool ceiling.
const BUMP_POOL_BYTES: usize = 4 * 1024 * 1024;

/// The pool itself. `UnsafeCell` because `GlobalAlloc::alloc` needs
/// a `*mut u8` view on interior bytes even though the outer wrapper
/// is `Sync` (via `AtomicUsize` on the offset). Aligned to 4096 so
/// large-align allocation requests can be satisfied at the low end
/// without wasting page-worth of prefix padding.
#[repr(align(4096))]
struct BumpPool(UnsafeCell<[u8; BUMP_POOL_BYTES]>);

// SAFETY: interior access is mediated by `AtomicUsize`-serialized
// offset arithmetic in `BumpAllocator::alloc`; concurrent callers
// see distinct non-overlapping slices of the pool.
unsafe impl Sync for BumpPool {}

static POOL: BumpPool = BumpPool(UnsafeCell::new([0u8; BUMP_POOL_BYTES]));

/// Zero-sized ZST — the allocator's state is entirely in the two
/// `static`s above (`POOL` for the byte pool, `NEXT_OFFSET` for the
/// bump cursor). Keeps the `#[global_allocator]` binding shape
/// canonical: `static NAME: TypeName = TypeName;`.
struct BumpAllocator;

static NEXT_OFFSET: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();

        // Bump cursor forward, honouring the layout's alignment. Loop
        // on `compare_exchange_weak` so concurrent callers each end
        // up with a distinct, non-overlapping slice.
        let mut current = NEXT_OFFSET.load(Ordering::Relaxed);
        loop {
            // Round `current` up to `align`. `align` is always a
            // power of two per the `Layout` invariant, so the
            // `(align - 1)` mask is well-defined.
            let aligned = current
                .checked_add(align - 1)
                .and_then(|v| Some(v & !(align - 1)));
            let Some(aligned) = aligned else {
                return core::ptr::null_mut();
            };
            let Some(new_offset) = aligned.checked_add(size) else {
                return core::ptr::null_mut();
            };
            if new_offset > BUMP_POOL_BYTES {
                return core::ptr::null_mut();
            }
            match NEXT_OFFSET.compare_exchange_weak(
                current,
                new_offset,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // SAFETY: `aligned` is < `BUMP_POOL_BYTES` per
                    // the bounds check above, so the pointer is
                    // inside `POOL`. `aligned + size` is also within
                    // bounds per `new_offset <= BUMP_POOL_BYTES`.
                    // The returned pointer is aligned to `align` per
                    // the mask above.
                    let base = POOL.0.get() as *mut u8;
                    return unsafe { base.add(aligned) };
                }
                Err(observed) => {
                    current = observed;
                    // retry with the updated cursor
                }
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator: allocations are leaked for the process
        // lifetime. See rationale on `BUMP_POOL_BYTES`.
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: BumpAllocator = BumpAllocator;

// ---------------------------------------------------------------------
// Panic handler — abort-shape via x86_64 `hlt`.
// ---------------------------------------------------------------------
//
// The `#[panic_handler]` attribute registers this function as the
// language-item panic entry point; the compiler emits calls to
// `rust_begin_unwind` (its underlying symbol) whenever a Rust
// `panic!` fires. This body loops emitting `hlt`, which on x86_64
// halts the CPU until the next external interrupt. In a userspace
// satellite process running under a real OS the effect is to wedge
// the current thread indefinitely; the kernel eventually reaps the
// process via signal (SIGKILL on ^C, etc.). Under paideia-os itself
// with the satellite running in a pdx sandbox, the same `hlt` loop
// is captured as a process-fatal condition by the ambient
// supervisor. Either way: no partial state is committed after a
// panicked Rust primitive returns.

#[panic_handler]
fn on_panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        // SAFETY: `hlt` on x86_64 has no memory effects and no
        // register clobbers other than the transition to a halted
        // state; from the Rust compiler's perspective this is an
        // opaque inline-asm sequence with no operands.
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

// ---------------------------------------------------------------------
// eh_personality stub.
// ---------------------------------------------------------------------
//
// `panic = "abort"` at the workspace release-profile level eliminates
// the Rust compiler's own emission of unwind tables, so this
// personality routine should never be entered at runtime. It is
// exported anyway as a defensive stub — the symbol is a well-known
// `rust_eh_personality` name that satellite ELF objects produced by
// a non-lockstep toolchain revision could still reference via a
// residual `.eh_frame` FDE. An empty exported function is the
// smallest thing that resolves such a reference.

/// # Safety
///
/// The Rust compiler is responsible for calling this with a
/// well-formed personality-routine argument set (or, more commonly,
/// for not calling it at all under `panic = "abort"`). The body
/// does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn rust_eh_personality() {}

// ---------------------------------------------------------------------
// libc memory-op primitives (memcpy / memset / memmove).
// ---------------------------------------------------------------------
//
// Rust's compiler_builtins normally provides these under a `mem`
// cargo feature, but that feature requires a nightly `-Zbuild-std`
// bootstrap flow we don't want in the satellite path. Instead we
// hand-roll pure-Rust bodies here so satellite ELFs linked
// `ld -nostdlib` can resolve the memcpy/memset/memmove refs that
// rustc emits for struct copies, slice ops, and heap-allocator
// bookkeeping. Verified needed by mkfs.pdxfs post-Phase-C: the
// ChaCha20-Poly1305 seal/open path in paideia-as-crypto and
// Poly1305's finalize both emit memcpy calls that survive
// `--gc-sections` because mkfs's --encrypt path transitively
// reaches them.
//
// The three signatures match C's libc.h exactly (SysV: rdi=dst,
// rsi=src, rdx=n; return dst). Marked `#[unsafe(no_mangle)]` so
// ld resolves them by the well-known C name.

/// # Safety
/// Caller must ensure `dst` and `src` are valid for reads/writes of
/// `n` bytes and do not overlap. See C11 §7.24.2.1.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    let mut i = 0usize;
    while i < n {
        unsafe { *dst.add(i) = *src.add(i); }
        i += 1;
    }
    dst
}

/// # Safety
/// Caller must ensure `dst` is valid for writes of `n` bytes.
/// See C11 §7.24.6.1.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(dst: *mut u8, val: i32, n: usize) -> *mut u8 {
    let b = val as u8;
    let mut i = 0usize;
    while i < n {
        unsafe { *dst.add(i) = b; }
        i += 1;
    }
    dst
}

/// # Safety
/// Caller must ensure `dst` and `src` are valid for reads/writes of
/// `n` bytes. Overlapping regions are handled correctly (unlike
/// `memcpy`). See C11 §7.24.2.2.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmove(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if (dst as usize) < (src as usize) {
        let mut i = 0usize;
        while i < n {
            unsafe { *dst.add(i) = *src.add(i); }
            i += 1;
        }
    } else {
        let mut i = n;
        while i > 0 {
            i -= 1;
            unsafe { *dst.add(i) = *src.add(i); }
        }
    }
    dst
}

/// # Safety
/// Caller must ensure `a` and `b` are valid for reads of `n` bytes.
/// See C11 §7.24.4.1.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    let mut i = 0usize;
    while i < n {
        let av = unsafe { *a.add(i) };
        let bv = unsafe { *b.add(i) };
        if av != bv {
            return av as i32 - bv as i32;
        }
        i += 1;
    }
    0
}

/// # Safety
/// Caller must ensure `a` and `b` are valid for reads of `n` bytes.
/// glibc-specific alias for `memcmp` used by some Rust internal calls.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    unsafe { memcmp(a, b, n) }
}
