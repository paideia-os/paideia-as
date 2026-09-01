//! Elliptic-curve primitives over Curve25519 and edwards25519 — the
//! twisted-Edwards form of the same underlying prime field.
//!
//! - `field25519` — `GF(2^255 - 19)` field arithmetic (radix-2^51,
//!   constant-time; shared by X25519 and Ed25519).
//! - `x25519` — RFC 7748 §5 Montgomery-ladder scalar multiplication
//!   plus the standard base-point convenience wrapper.
//! - `ed25519` — RFC 8032 §5.1 sign + verify (PureEdDSA over
//!   edwards25519 with SHA-512).

pub mod ed25519;
pub mod field25519;
pub mod x25519;

pub use ed25519::{ed25519_public_from_secret, ed25519_sign, ed25519_verify};
pub use field25519::FieldElement;
pub use x25519::{x25519_public_from_secret, x25519_scalarmult};
