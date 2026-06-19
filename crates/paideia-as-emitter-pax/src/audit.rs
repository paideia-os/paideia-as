//! PAX audit-trail sections: `.paideia.unsafe`, `.paideia.opt-passes`, `.paideia.lin`.
//!
//! Three related section types for runtime audit and compliance tracking:
//! - `.paideia.unsafe`: unsafe-block audit trail (40-byte entries)
//! - `.paideia.opt-passes`: optimization-pass results (32-byte entries)
//! - `.paideia.lin`: linearity-check witnesses (32-byte entries)

use static_assertions::const_assert_eq;

// ============================================================================
// Unsafe-block audit (.paideia.unsafe)
// ============================================================================

/// Size of a single unsafe-block entry in bytes.
///
/// Layout:
/// - 0..8: function_symbol_id (u64)
/// - 8..16: block_span_start (u64) — source byte offset
/// - 16..24: block_span_len (u64)
/// - 24..32: blake3_block_hash (u64) — first 8 of BLAKE3 of block bytes
/// - 32..40: justification_hash (u64) — BLAKE3-derived hash of the justification text
pub const UNSAFE_ENTRY_SIZE: usize = 40;

// Verify the unsafe entry size is correct at compile time.
const_assert_eq!(UNSAFE_ENTRY_SIZE, 40);

/// A single unsafe-block audit entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnsafeEntry {
    /// The function containing the unsafe block.
    pub function_symbol_id: u64,
    /// Source byte offset of the unsafe block start.
    pub block_span_start: u64,
    /// Length of the unsafe block in bytes.
    pub block_span_len: u64,
    /// First 8 bytes of BLAKE3 hash of the block.
    pub blake3_block_hash: u64,
    /// BLAKE3-derived hash of the justification text.
    pub justification_hash: u64,
}

impl UnsafeEntry {
    /// Create a new unsafe-block entry.
    ///
    /// # Arguments
    ///
    /// * `function_symbol_id` - The function containing the unsafe block
    /// * `block_span_start` - Source byte offset
    /// * `block_span_len` - Length in bytes
    /// * `blake3_block_hash` - BLAKE3 block hash (first 8 bytes)
    /// * `justification_hash` - BLAKE3 justification text hash (first 8 bytes)
    ///
    /// # Returns
    ///
    /// A new UnsafeEntry.
    pub fn new(
        function_symbol_id: u64,
        block_span_start: u64,
        block_span_len: u64,
        blake3_block_hash: u64,
        justification_hash: u64,
    ) -> Self {
        Self {
            function_symbol_id,
            block_span_start,
            block_span_len,
            blake3_block_hash,
            justification_hash,
        }
    }

    /// Serialize this entry to its 40-byte little-endian representation.
    pub fn to_bytes(&self) -> [u8; UNSAFE_ENTRY_SIZE] {
        let mut bytes = [0u8; UNSAFE_ENTRY_SIZE];

        // Offset 0: function_symbol_id (8 bytes, little-endian)
        bytes[0..8].copy_from_slice(&self.function_symbol_id.to_le_bytes());

        // Offset 8: block_span_start (8 bytes, little-endian)
        bytes[8..16].copy_from_slice(&self.block_span_start.to_le_bytes());

        // Offset 16: block_span_len (8 bytes, little-endian)
        bytes[16..24].copy_from_slice(&self.block_span_len.to_le_bytes());

        // Offset 24: blake3_block_hash (8 bytes, little-endian)
        bytes[24..32].copy_from_slice(&self.blake3_block_hash.to_le_bytes());

        // Offset 32: justification_hash (8 bytes, little-endian)
        bytes[32..40].copy_from_slice(&self.justification_hash.to_le_bytes());

        bytes
    }

    /// Parse an unsafe-block entry from a byte slice.
    ///
    /// Returns `Some(entry)` if the input contains at least UNSAFE_ENTRY_SIZE bytes.
    /// Returns `None` on short input.
    ///
    /// All multi-byte integers are decoded as little-endian.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < UNSAFE_ENTRY_SIZE {
            return None;
        }

        // Offset 0: function_symbol_id (8 bytes, little-endian)
        let function_symbol_id = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);

        // Offset 8: block_span_start (8 bytes, little-endian)
        let block_span_start = u64::from_le_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ]);

        // Offset 16: block_span_len (8 bytes, little-endian)
        let block_span_len = u64::from_le_bytes([
            bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23],
        ]);

        // Offset 24: blake3_block_hash (8 bytes, little-endian)
        let blake3_block_hash = u64::from_le_bytes([
            bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31],
        ]);

        // Offset 32: justification_hash (8 bytes, little-endian)
        let justification_hash = u64::from_le_bytes([
            bytes[32], bytes[33], bytes[34], bytes[35], bytes[36], bytes[37], bytes[38], bytes[39],
        ]);

        Some(Self {
            function_symbol_id,
            block_span_start,
            block_span_len,
            blake3_block_hash,
            justification_hash,
        })
    }
}

/// Whole-section content: a sequence of UnsafeEntry.
#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub struct UnsafeSection {
    /// List of unsafe-block audit entries.
    pub entries: Vec<UnsafeEntry>,
}

impl UnsafeSection {
    /// Create a new, empty unsafe audit section.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an unsafe-block entry to the section.
    pub fn push(&mut self, e: UnsafeEntry) {
        self.entries.push(e);
    }

    /// Serialize the entire section to a byte vector.
    ///
    /// Each entry is serialized to UNSAFE_ENTRY_SIZE bytes in order.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.entries.len() * UNSAFE_ENTRY_SIZE);
        for entry in &self.entries {
            bytes.extend_from_slice(&entry.to_bytes());
        }
        bytes
    }

    /// Parse an unsafe audit section from a byte slice.
    ///
    /// Returns `Some(section)` if the input length is a multiple of UNSAFE_ENTRY_SIZE
    /// and all entries parse successfully. Returns `None` on invalid size.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if !bytes.len().is_multiple_of(UNSAFE_ENTRY_SIZE) {
            return None;
        }

        let count = bytes.len() / UNSAFE_ENTRY_SIZE;
        let mut entries = Vec::with_capacity(count);

        for i in 0..count {
            let offset = i * UNSAFE_ENTRY_SIZE;
            let entry_bytes = &bytes[offset..offset + UNSAFE_ENTRY_SIZE];
            let entry = UnsafeEntry::from_bytes(entry_bytes)?;
            entries.push(entry);
        }

        Some(Self { entries })
    }

    /// Return the number of entries in the section.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the section is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ============================================================================
// Opt-passes (.paideia.opt-passes)
// ============================================================================

/// Size of a single opt-pass entry in bytes.
///
/// Layout:
/// - 0..8: function_symbol_id (u64)
/// - 8..12: pass_id (u32) — see PassId enum
/// - 12..16: iterations (u32) — how many fixed-point iterations
/// - 16..24: delta_size_bytes (i64) — signed bytes added/removed
/// - 24..32: blake3_pre_hash (u64) — first 8 of BLAKE3 of pre-pass IR
pub const OPT_ENTRY_SIZE: usize = 32;

// Verify the opt entry size is correct at compile time.
const_assert_eq!(OPT_ENTRY_SIZE, 32);

/// Optimization pass identifier.
#[repr(u32)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum PassId {
    /// Peephole optimization.
    Peephole = 1,
    /// Administrative Normal Form.
    Anf = 2,
    /// Dead Store Elimination.
    Dse = 3,
    /// Constant Folding.
    ConstFold = 4,
    /// Effect rewrite.
    EffectRewrite = 5,
    /// Other / extensible.
    Other = 0xFFFFFFFF,
}

impl PassId {
    /// Convert from u32 representation to PassId enum.
    pub fn from_u32(val: u32) -> Option<Self> {
        match val {
            1 => Some(PassId::Peephole),
            2 => Some(PassId::Anf),
            3 => Some(PassId::Dse),
            4 => Some(PassId::ConstFold),
            5 => Some(PassId::EffectRewrite),
            0xFFFFFFFF => Some(PassId::Other),
            _ => None,
        }
    }
}

/// A single optimization-pass entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptEntry {
    /// The function that was optimized.
    pub function_symbol_id: u64,
    /// The optimization pass applied.
    pub pass_id: PassId,
    /// Number of fixed-point iterations.
    pub iterations: u32,
    /// Signed bytes added (positive) or removed (negative).
    pub delta_size_bytes: i64,
    /// First 8 bytes of BLAKE3 hash of pre-pass IR.
    pub blake3_pre_hash: u64,
}

impl OptEntry {
    /// Create a new opt-pass entry.
    ///
    /// # Arguments
    ///
    /// * `function_symbol_id` - The function that was optimized
    /// * `pass_id` - The pass that was applied
    /// * `iterations` - Fixed-point iteration count
    /// * `delta_size_bytes` - Signed size change
    /// * `blake3_pre_hash` - BLAKE3 pre-pass hash (first 8 bytes)
    ///
    /// # Returns
    ///
    /// A new OptEntry.
    pub fn new(
        function_symbol_id: u64,
        pass_id: PassId,
        iterations: u32,
        delta_size_bytes: i64,
        blake3_pre_hash: u64,
    ) -> Self {
        Self {
            function_symbol_id,
            pass_id,
            iterations,
            delta_size_bytes,
            blake3_pre_hash,
        }
    }

    /// Serialize this entry to its 32-byte little-endian representation.
    pub fn to_bytes(&self) -> [u8; OPT_ENTRY_SIZE] {
        let mut bytes = [0u8; OPT_ENTRY_SIZE];

        // Offset 0: function_symbol_id (8 bytes, little-endian)
        bytes[0..8].copy_from_slice(&self.function_symbol_id.to_le_bytes());

        // Offset 8: pass_id (4 bytes, little-endian)
        bytes[8..12].copy_from_slice(&(self.pass_id as u32).to_le_bytes());

        // Offset 12: iterations (4 bytes, little-endian)
        bytes[12..16].copy_from_slice(&self.iterations.to_le_bytes());

        // Offset 16: delta_size_bytes (8 bytes, little-endian)
        bytes[16..24].copy_from_slice(&self.delta_size_bytes.to_le_bytes());

        // Offset 24: blake3_pre_hash (8 bytes, little-endian)
        bytes[24..32].copy_from_slice(&self.blake3_pre_hash.to_le_bytes());

        bytes
    }

    /// Parse an opt-pass entry from a byte slice.
    ///
    /// Returns `Some(entry)` if the input contains at least OPT_ENTRY_SIZE bytes
    /// and the pass_id is valid. Returns `None` on short input or invalid enum value.
    ///
    /// All multi-byte integers are decoded as little-endian.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < OPT_ENTRY_SIZE {
            return None;
        }

        // Offset 0: function_symbol_id (8 bytes, little-endian)
        let function_symbol_id = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);

        // Offset 8: pass_id (4 bytes, little-endian)
        let pass_id_u32 = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        let pass_id = PassId::from_u32(pass_id_u32)?;

        // Offset 12: iterations (4 bytes, little-endian)
        let iterations = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);

        // Offset 16: delta_size_bytes (8 bytes, little-endian)
        let delta_size_bytes = i64::from_le_bytes([
            bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23],
        ]);

        // Offset 24: blake3_pre_hash (8 bytes, little-endian)
        let blake3_pre_hash = u64::from_le_bytes([
            bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31],
        ]);

        Some(Self {
            function_symbol_id,
            pass_id,
            iterations,
            delta_size_bytes,
            blake3_pre_hash,
        })
    }
}

/// Whole-section content: a sequence of OptEntry.
#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub struct OptSection {
    /// List of opt-pass entries.
    pub entries: Vec<OptEntry>,
}

impl OptSection {
    /// Create a new, empty opt-passes section.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an opt-pass entry to the section.
    pub fn push(&mut self, e: OptEntry) {
        self.entries.push(e);
    }

    /// Serialize the entire section to a byte vector.
    ///
    /// Each entry is serialized to OPT_ENTRY_SIZE bytes in order.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.entries.len() * OPT_ENTRY_SIZE);
        for entry in &self.entries {
            bytes.extend_from_slice(&entry.to_bytes());
        }
        bytes
    }

    /// Parse an opt-passes section from a byte slice.
    ///
    /// Returns `Some(section)` if the input length is a multiple of OPT_ENTRY_SIZE
    /// and all entries parse successfully. Returns `None` on invalid size or
    /// unparseable entries.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if !bytes.len().is_multiple_of(OPT_ENTRY_SIZE) {
            return None;
        }

        let count = bytes.len() / OPT_ENTRY_SIZE;
        let mut entries = Vec::with_capacity(count);

        for i in 0..count {
            let offset = i * OPT_ENTRY_SIZE;
            let entry_bytes = &bytes[offset..offset + OPT_ENTRY_SIZE];
            let entry = OptEntry::from_bytes(entry_bytes)?;
            entries.push(entry);
        }

        Some(Self { entries })
    }

    /// Return the number of entries in the section.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the section is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ============================================================================
// Linearity-check witnesses (.paideia.lin)
// ============================================================================

/// Size of a single linearity entry in bytes.
///
/// Layout:
/// - 0..8: function_symbol_id (u64)
/// - 8..16: binding_id (u64)
/// - 16..20: class (u32; same as caps.rs LinClass)
/// - 20..24: uses_count (u32)
/// - 24..32: blake3_use_chain_hash (u64) — first 8 of BLAKE3 of the use-site sequence
pub const LIN_ENTRY_SIZE: usize = 32;

// Verify the lin entry size is correct at compile time.
const_assert_eq!(LIN_ENTRY_SIZE, 32);

/// A single linearity-check witness entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinEntry {
    /// The function containing the binding.
    pub function_symbol_id: u64,
    /// The binding id.
    pub binding_id: u64,
    /// Linearity class (same as caps.rs LinClass: 1=Linear, 2=Affine, 3=Ordered, 4=Unrestricted).
    pub class: u32,
    /// Count of uses observed.
    pub uses_count: u32,
    /// BLAKE3 hash of the use-site sequence (first 8 bytes).
    pub blake3_use_chain_hash: u64,
}

impl LinEntry {
    /// Create a new linearity-check witness entry.
    ///
    /// # Arguments
    ///
    /// * `function_symbol_id` - The function containing the binding
    /// * `binding_id` - The binding id
    /// * `class` - Linearity class (1=Linear, 2=Affine, 3=Ordered, 4=Unrestricted)
    /// * `uses_count` - Count of uses
    /// * `blake3_use_chain_hash` - BLAKE3 use-chain hash (first 8 bytes)
    ///
    /// # Returns
    ///
    /// A new LinEntry.
    pub fn new(
        function_symbol_id: u64,
        binding_id: u64,
        class: u32,
        uses_count: u32,
        blake3_use_chain_hash: u64,
    ) -> Self {
        Self {
            function_symbol_id,
            binding_id,
            class,
            uses_count,
            blake3_use_chain_hash,
        }
    }

    /// Serialize this entry to its 32-byte little-endian representation.
    pub fn to_bytes(&self) -> [u8; LIN_ENTRY_SIZE] {
        let mut bytes = [0u8; LIN_ENTRY_SIZE];

        // Offset 0: function_symbol_id (8 bytes, little-endian)
        bytes[0..8].copy_from_slice(&self.function_symbol_id.to_le_bytes());

        // Offset 8: binding_id (8 bytes, little-endian)
        bytes[8..16].copy_from_slice(&self.binding_id.to_le_bytes());

        // Offset 16: class (4 bytes, little-endian)
        bytes[16..20].copy_from_slice(&self.class.to_le_bytes());

        // Offset 20: uses_count (4 bytes, little-endian)
        bytes[20..24].copy_from_slice(&self.uses_count.to_le_bytes());

        // Offset 24: blake3_use_chain_hash (8 bytes, little-endian)
        bytes[24..32].copy_from_slice(&self.blake3_use_chain_hash.to_le_bytes());

        bytes
    }

    /// Parse a linearity-check entry from a byte slice.
    ///
    /// Returns `Some(entry)` if the input contains at least LIN_ENTRY_SIZE bytes.
    /// Returns `None` on short input.
    ///
    /// All multi-byte integers are decoded as little-endian.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < LIN_ENTRY_SIZE {
            return None;
        }

        // Offset 0: function_symbol_id (8 bytes, little-endian)
        let function_symbol_id = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);

        // Offset 8: binding_id (8 bytes, little-endian)
        let binding_id = u64::from_le_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ]);

        // Offset 16: class (4 bytes, little-endian)
        let class = u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);

        // Offset 20: uses_count (4 bytes, little-endian)
        let uses_count = u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);

        // Offset 24: blake3_use_chain_hash (8 bytes, little-endian)
        let blake3_use_chain_hash = u64::from_le_bytes([
            bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31],
        ]);

        Some(Self {
            function_symbol_id,
            binding_id,
            class,
            uses_count,
            blake3_use_chain_hash,
        })
    }
}

/// Whole-section content: a sequence of LinEntry.
#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub struct LinSection {
    /// List of linearity-check witness entries.
    pub entries: Vec<LinEntry>,
}

impl LinSection {
    /// Create a new, empty linearity section.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a linearity-check entry to the section.
    pub fn push(&mut self, e: LinEntry) {
        self.entries.push(e);
    }

    /// Serialize the entire section to a byte vector.
    ///
    /// Each entry is serialized to LIN_ENTRY_SIZE bytes in order.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.entries.len() * LIN_ENTRY_SIZE);
        for entry in &self.entries {
            bytes.extend_from_slice(&entry.to_bytes());
        }
        bytes
    }

    /// Parse a linearity section from a byte slice.
    ///
    /// Returns `Some(section)` if the input length is a multiple of LIN_ENTRY_SIZE
    /// and all entries parse successfully. Returns `None` on invalid size.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if !bytes.len().is_multiple_of(LIN_ENTRY_SIZE) {
            return None;
        }

        let count = bytes.len() / LIN_ENTRY_SIZE;
        let mut entries = Vec::with_capacity(count);

        for i in 0..count {
            let offset = i * LIN_ENTRY_SIZE;
            let entry_bytes = &bytes[offset..offset + LIN_ENTRY_SIZE];
            let entry = LinEntry::from_bytes(entry_bytes)?;
            entries.push(entry);
        }

        Some(Self { entries })
    }

    /// Return the number of entries in the section.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the section is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Unsafe-block audit tests
    // ========================================================================

    #[test]
    fn unsafe_entry_size_is_40_bytes() {
        const _: () = {
            const _: [(); 1] = [(); (UNSAFE_ENTRY_SIZE == 40) as usize];
        };
        assert_eq!(UNSAFE_ENTRY_SIZE, 40);
    }

    #[test]
    fn unsafe_entry_roundtrip() {
        let entry = UnsafeEntry::new(42, 1000, 200, 0xDEADBEEFCAFEBEEF, 0x0102030405060708);

        let bytes = entry.to_bytes();
        assert_eq!(bytes.len(), UNSAFE_ENTRY_SIZE);

        let parsed = UnsafeEntry::from_bytes(&bytes);
        assert_eq!(parsed, Some(entry));
    }

    #[test]
    fn unsafe_section_3_entries_roundtrip() {
        let mut section = UnsafeSection::new();

        let e1 = UnsafeEntry::new(1, 100, 50, 0x1111111111111111, 0x2222222222222222);
        let e2 = UnsafeEntry::new(2, 200, 75, 0x3333333333333333, 0x4444444444444444);
        let e3 = UnsafeEntry::new(3, 300, 100, 0x5555555555555555, 0x6666666666666666);

        section.push(e1);
        section.push(e2);
        section.push(e3);

        let bytes = section.to_bytes();
        assert_eq!(bytes.len(), 3 * UNSAFE_ENTRY_SIZE);

        let parsed = UnsafeSection::from_bytes(&bytes);
        assert_eq!(parsed, Some(section));
    }

    #[test]
    fn unsafe_schema_snapshot_byte_layout() {
        let entry = UnsafeEntry {
            function_symbol_id: 0x0102030405060708,
            block_span_start: 0x0A0B0C0D0E0F1011,
            block_span_len: 0x1213141516171819,
            blake3_block_hash: 0x1A1B1C1D1E1F2021,
            justification_hash: 0x2223242526272829,
        };

        let bytes = entry.to_bytes();

        // Offset 0: function_symbol_id
        assert_eq!(
            &bytes[0..8],
            &[0x08u8, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
        );

        // Offset 8: block_span_start
        assert_eq!(
            &bytes[8..16],
            &[0x11u8, 0x10, 0x0F, 0x0E, 0x0D, 0x0C, 0x0B, 0x0A]
        );

        // Offset 16: block_span_len
        assert_eq!(
            &bytes[16..24],
            &[0x19u8, 0x18, 0x17, 0x16, 0x15, 0x14, 0x13, 0x12]
        );

        // Offset 24: blake3_block_hash
        assert_eq!(
            &bytes[24..32],
            &[0x21u8, 0x20, 0x1F, 0x1E, 0x1D, 0x1C, 0x1B, 0x1A]
        );

        // Offset 32: justification_hash
        assert_eq!(
            &bytes[32..40],
            &[0x29u8, 0x28, 0x27, 0x26, 0x25, 0x24, 0x23, 0x22]
        );
    }

    // ========================================================================
    // Opt-passes tests
    // ========================================================================

    #[test]
    fn opt_entry_size_is_32_bytes() {
        const _: () = {
            const _: [(); 1] = [(); (OPT_ENTRY_SIZE == 32) as usize];
        };
        assert_eq!(OPT_ENTRY_SIZE, 32);
    }

    #[test]
    fn opt_entry_roundtrip() {
        let entry = OptEntry::new(99, PassId::Peephole, 5, -128, 0xCAFEBABEDEADBEEF);

        let bytes = entry.to_bytes();
        assert_eq!(bytes.len(), OPT_ENTRY_SIZE);

        let parsed = OptEntry::from_bytes(&bytes);
        assert_eq!(parsed, Some(entry));
    }

    #[test]
    fn opt_section_3_entries_roundtrip() {
        let mut section = OptSection::new();

        let e1 = OptEntry::new(10, PassId::Peephole, 2, 50, 0x1111111111111111);
        let e2 = OptEntry::new(20, PassId::Anf, 3, -75, 0x2222222222222222);
        let e3 = OptEntry::new(30, PassId::ConstFold, 1, 100, 0x3333333333333333);

        section.push(e1);
        section.push(e2);
        section.push(e3);

        let bytes = section.to_bytes();
        assert_eq!(bytes.len(), 3 * OPT_ENTRY_SIZE);

        let parsed = OptSection::from_bytes(&bytes);
        assert_eq!(parsed, Some(section));
    }

    #[test]
    fn opt_schema_snapshot_byte_layout() {
        let entry = OptEntry {
            function_symbol_id: 0x0102030405060708,
            pass_id: PassId::Anf,
            iterations: 0x0A0B0C0D,
            delta_size_bytes: -9223372036854775807i64,
            blake3_pre_hash: 0x1112131415161718,
        };

        let bytes = entry.to_bytes();

        // Offset 0: function_symbol_id
        assert_eq!(
            &bytes[0..8],
            &[0x08u8, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
        );

        // Offset 8: pass_id (2 for Anf)
        assert_eq!(&bytes[8..12], &[0x02u8, 0x00, 0x00, 0x00]);

        // Offset 12: iterations
        assert_eq!(&bytes[12..16], &[0x0Du8, 0x0C, 0x0B, 0x0A]);

        // Offset 16: delta_size_bytes
        assert_eq!(
            &bytes[16..24],
            &[0x01u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80]
        );

        // Offset 24: blake3_pre_hash
        assert_eq!(
            &bytes[24..32],
            &[0x18u8, 0x17, 0x16, 0x15, 0x14, 0x13, 0x12, 0x11]
        );
    }

    // ========================================================================
    // Linearity-check witness tests
    // ========================================================================

    #[test]
    fn lin_entry_size_is_32_bytes() {
        const _: () = {
            const _: [(); 1] = [(); (LIN_ENTRY_SIZE == 32) as usize];
        };
        assert_eq!(LIN_ENTRY_SIZE, 32);
    }

    #[test]
    fn lin_entry_roundtrip() {
        let entry = LinEntry::new(77, 1337, 1, 3, 0xBEEFCAFEDEADBEEF);

        let bytes = entry.to_bytes();
        assert_eq!(bytes.len(), LIN_ENTRY_SIZE);

        let parsed = LinEntry::from_bytes(&bytes);
        assert_eq!(parsed, Some(entry));
    }

    #[test]
    fn lin_section_3_entries_roundtrip() {
        let mut section = LinSection::new();

        let e1 = LinEntry::new(1, 100, 1, 1, 0x1111111111111111);
        let e2 = LinEntry::new(2, 200, 2, 5, 0x2222222222222222);
        let e3 = LinEntry::new(3, 300, 4, 0, 0x3333333333333333);

        section.push(e1);
        section.push(e2);
        section.push(e3);

        let bytes = section.to_bytes();
        assert_eq!(bytes.len(), 3 * LIN_ENTRY_SIZE);

        let parsed = LinSection::from_bytes(&bytes);
        assert_eq!(parsed, Some(section));
    }

    #[test]
    fn lin_schema_snapshot_byte_layout() {
        let entry = LinEntry {
            function_symbol_id: 0x0102030405060708,
            binding_id: 0x0A0B0C0D0E0F1011,
            class: 0x12131415,
            uses_count: 0x16171819,
            blake3_use_chain_hash: 0x1A1B1C1D1E1F2021,
        };

        let bytes = entry.to_bytes();

        // Offset 0: function_symbol_id
        assert_eq!(
            &bytes[0..8],
            &[0x08u8, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
        );

        // Offset 8: binding_id
        assert_eq!(
            &bytes[8..16],
            &[0x11u8, 0x10, 0x0F, 0x0E, 0x0D, 0x0C, 0x0B, 0x0A]
        );

        // Offset 16: class
        assert_eq!(&bytes[16..20], &[0x15u8, 0x14, 0x13, 0x12]);

        // Offset 20: uses_count
        assert_eq!(&bytes[20..24], &[0x19u8, 0x18, 0x17, 0x16]);

        // Offset 24: blake3_use_chain_hash
        assert_eq!(
            &bytes[24..32],
            &[0x21u8, 0x20, 0x1F, 0x1E, 0x1D, 0x1C, 0x1B, 0x1A]
        );
    }

    // ========================================================================
    // Additional tests: empty sections and edge cases
    // ========================================================================

    #[test]
    fn empty_unsafe_section_roundtrips() {
        let section = UnsafeSection::new();
        assert!(section.is_empty());
        assert_eq!(section.len(), 0);

        let bytes = section.to_bytes();
        assert_eq!(bytes.len(), 0);

        let parsed = UnsafeSection::from_bytes(&bytes);
        assert_eq!(parsed, Some(section));
    }

    #[test]
    fn empty_opt_section_roundtrips() {
        let section = OptSection::new();
        assert!(section.is_empty());
        assert_eq!(section.len(), 0);

        let bytes = section.to_bytes();
        assert_eq!(bytes.len(), 0);

        let parsed = OptSection::from_bytes(&bytes);
        assert_eq!(parsed, Some(section));
    }

    #[test]
    fn empty_lin_section_roundtrips() {
        let section = LinSection::new();
        assert!(section.is_empty());
        assert_eq!(section.len(), 0);

        let bytes = section.to_bytes();
        assert_eq!(bytes.len(), 0);

        let parsed = LinSection::from_bytes(&bytes);
        assert_eq!(parsed, Some(section));
    }

    #[test]
    fn pass_id_from_u32_all_variants() {
        assert_eq!(PassId::from_u32(1), Some(PassId::Peephole));
        assert_eq!(PassId::from_u32(2), Some(PassId::Anf));
        assert_eq!(PassId::from_u32(3), Some(PassId::Dse));
        assert_eq!(PassId::from_u32(4), Some(PassId::ConstFold));
        assert_eq!(PassId::from_u32(5), Some(PassId::EffectRewrite));
        assert_eq!(PassId::from_u32(0xFFFFFFFF), Some(PassId::Other));
        assert_eq!(PassId::from_u32(999), None);
    }

    #[test]
    fn opt_entry_all_pass_ids_roundtrip() {
        let pass_ids = [
            PassId::Peephole,
            PassId::Anf,
            PassId::Dse,
            PassId::ConstFold,
            PassId::EffectRewrite,
            PassId::Other,
        ];

        for expected_pass_id in pass_ids {
            let entry = OptEntry::new(42, expected_pass_id, 1, 0, 0);
            let bytes = entry.to_bytes();
            let parsed = OptEntry::from_bytes(&bytes).expect("Failed to parse entry");
            assert_eq!(parsed.pass_id, expected_pass_id);
        }
    }

    #[test]
    fn from_bytes_rejects_truncated_unsafe() {
        let bytes = [0u8; UNSAFE_ENTRY_SIZE - 1];
        let result = UnsafeEntry::from_bytes(&bytes);
        assert_eq!(result, None);
    }

    #[test]
    fn from_bytes_rejects_truncated_opt() {
        let bytes = [0u8; OPT_ENTRY_SIZE - 1];
        let result = OptEntry::from_bytes(&bytes);
        assert_eq!(result, None);
    }

    #[test]
    fn from_bytes_rejects_truncated_lin() {
        let bytes = [0u8; LIN_ENTRY_SIZE - 1];
        let result = LinEntry::from_bytes(&bytes);
        assert_eq!(result, None);
    }

    #[test]
    fn unsafe_section_rejects_misaligned_bytes() {
        let bytes = [0u8; UNSAFE_ENTRY_SIZE + 1];
        let result = UnsafeSection::from_bytes(&bytes);
        assert_eq!(result, None);
    }

    #[test]
    fn opt_section_rejects_misaligned_bytes() {
        let bytes = [0u8; OPT_ENTRY_SIZE + 1];
        let result = OptSection::from_bytes(&bytes);
        assert_eq!(result, None);
    }

    #[test]
    fn lin_section_rejects_misaligned_bytes() {
        let bytes = [0u8; LIN_ENTRY_SIZE + 1];
        let result = LinSection::from_bytes(&bytes);
        assert_eq!(result, None);
    }
}
