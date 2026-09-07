/// Centralized SIB encoding helper for memory operands with scale, index, base, and displacement.
///
/// Handles the special case where RBP (id=5) or R13 (id=13) as base requires a disp8=0
/// escape even when disp=0 (since mod=00 with base=5 is reserved for RIP-relative).
///
/// # Arguments
/// - `buf`: code buffer to append bytes to
/// - `reg_field`: register field for ModR/M (3 bits, typically dst or src register)
/// - `base_id`: base register id (0-15; only low 3 bits used in SIB)
/// - `index_id`: index register id (0-15; only low 3 bits used in SIB)
/// - `scale_bits`: scale encoding (0=1x, 1=2x, 2=4x, 3=8x)
/// - `disp`: displacement value (0, disp8, or disp32)
///
/// Emits:
/// - ModR/M byte with scale-index-base indicator (rm=100)
/// - SIB byte
/// - Displacement bytes (0, 1, or 4 bytes) as needed
pub(crate) fn emit_mem_sib_disp(
    buf: &mut CodeBuffer,
    reg_field: u8,
    base_id: u8,
    index_id: u8,
    scale_bits: u8,
    disp: i32,
) {
    let base_low = base_id & 7;
    let bp_escape = base_low == 5;  // RBP / R13
    let (mod_bits, disp_len) = if disp == 0 && !bp_escape {
        (0x00u8, 0)
    } else if (-128..=127).contains(&disp) {
        (0x40u8, 1)
    } else {
        (0x80u8, 4)
    };
    buf.bytes.push(mod_bits | ((reg_field & 7) << 3) | 0b100);
    buf.bytes.push(((scale_bits & 3) << 6) | ((index_id & 7) << 3) | base_low);
    match disp_len {
        0 => {}
        1 => buf.bytes.push(disp as u8),
        _ => buf.bytes.extend(disp.to_le_bytes()),
    }
}
