//! Small helper enums used across `Mnemonic` and `Operand`.
//!
//! Extracted from the monolithic `instruction.rs` (issue #1402, umbrella #1399).
//! Public paths preserved: `paideia_as_runtime::instruction::{InstrMode, IntWidth, Cond}`.

/// Instruction execution mode (bit-width).
///
/// Phase 15 m2-002: instruction mode (64-bit or 32-bit) propagated from
/// module-level #![bits=...] inner_attrs through the emit walk.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum InstrMode {
    /// 64-bit mode (default).
    #[default]
    Mode64,
    /// 32-bit mode.
    Mode32,
}

/// Integer operand width for width-threaded immediate moves.
///
/// Phase 7 m4-003 (PA7C-m4-003): maps a bound integer literal's bit-width
/// (from its declared type) to the encoded move form. `from_bits` converts
/// a layout bit-width (8/16/32/64) into the corresponding variant.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum IntWidth {
    /// 8-bit operand (`B0+rb imm8`).
    W8,
    /// 16-bit operand (`66 B8+rd imm16`).
    W16,
    /// 32-bit operand (`B8+rd imm32`, implicit zero-extend).
    W32,
    /// 64-bit operand (delegates to the generic `Mov` path).
    W64,
}

impl IntWidth {
    /// Map a bit-width (8/16/32/64) to an `IntWidth`.
    ///
    /// Returns `None` for any other bit-width (e.g. 128, or non-power-of-two
    /// widths), in which case callers fall back to the generic 64-bit path.
    #[must_use]
    pub fn from_bits(bits: u16) -> Option<Self> {
        match bits {
            8 => Some(IntWidth::W8),
            16 => Some(IntWidth::W16),
            32 => Some(IntWidth::W32),
            64 => Some(IntWidth::W64),
            _ => None,
        }
    }

    /// Conservative upper bound on the encoded byte length for this width.
    ///
    /// - W8  → 3 bytes (`REX.B B0+rb imm8`, REX present only for r8–r15)
    /// - W16 → 4 bytes (`66 B8+rd imm16`)
    /// - W32 → 5 bytes (`B8+rd imm32`)
    /// - W64 → 10 bytes (generic `Mov` upper bound)
    #[must_use]
    pub fn estimated_size(self) -> u32 {
        match self {
            IntWidth::W8 => 3,
            IntWidth::W16 => 4,
            IntWidth::W32 => 5,
            IntWidth::W64 => 10,
        }
    }
}

/// Condition code for Jcc instructions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Cond {
    /// Equal (je).
    Eq,
    /// Not equal (jne).
    Ne,
    /// Signed less than (jl).
    Lt,
    /// Signed less than or equal (jle).
    Le,
    /// Signed greater than (jg).
    Gt,
    /// Signed greater than or equal (jge).
    Ge,
    /// Unsigned less than (jb).
    Below,
    /// Unsigned less than or equal (jbe).
    BelowOrEqual,
    /// Unsigned greater than (ja).
    Above,
    /// Unsigned greater than or equal (jae).
    AboveOrEqual,
    /// Zero (jz).
    Zero,
    /// Not zero (jnz).
    NonZero,
    /// Sign flag set (js).
    Sign,
    /// Sign flag not set (jns).
    NotSign,
    /// Overflow flag set (jo).
    Overflow,
    /// Overflow flag not set (jno).
    NotOverflow,
    /// Parity flag set (jp/setp).
    Parity,
    /// Parity flag not set (jnp/setnp).
    NotParity,
}
