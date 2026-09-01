//! Elliptic-curve primitives over Curve25519 (and, when Ed25519 lands in
//! #1341, over edwards25519 — the twisted-Edwards form of the same
//! underlying prime field).
//!
//! - `field25519` — `GF(2^255 - 19)` field arithmetic (radix-2^51,
//!   constant-time; shared by X25519 and future Ed25519).
//! - `x25519` — RFC 7748 §5 Montgomery-ladder scalar multiplication
//!   plus the standard base-point convenience wrapper.

pub mod field25519;
pub mod x25519;

pub use field25519::FieldElement;
pub use x25519::{x25519_public_from_secret, x25519_scalarmult};
