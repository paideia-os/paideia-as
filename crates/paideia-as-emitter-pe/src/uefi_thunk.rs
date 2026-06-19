//! UEFI thunk generation for Microsoft-x64-ABI → PaideiaOS-native interop.
//!
//! When UEFI firmware (Microsoft x64 ABI) calls into PaideiaOS code, we need a
//! thunk that saves/restores the effect-environment register (R15).
//!
//! This module emits minimal thunks for phase-2-m6-005.

use paideia_as_encoder::{CodeBuffer, Reg64, call_rel32, pop_reg64, push_reg64, ret};

/// Emit a Microsoft-x64-ABI → PaideiaOS-native thunk.
///
/// Phase-2-m6-005 minimum: 10-byte sequence.
///   push r15    (2 bytes: 41 57)
///   call rel32  (5 bytes: E8 <disp32 LE>)   ← TODO(m6-007): backpatched at link time
///   pop r15     (2 bytes: 41 5F)
///   ret         (1 byte: C3)
///
/// Total: 2 + 5 + 2 + 1 = 10 bytes.
///
/// # Notes
///
/// The thunk saves R15 (the effect-environment pointer) across the transition to
/// PaideiaOS-native code because the Microsoft x64 ABI does not preserve it.
/// After the native call returns, we restore R15 and return to the UEFI caller.
///
/// Phase-2-m6-005 asserts no register shuffle: the native function expects
/// arguments in the same registers as the UEFI caller provides (RCX, RDX, R8, R9,
/// XMM0-3). A full mapping table will be added in phase-2-m6-008.
pub fn emit_uefi_thunk(buf: &mut CodeBuffer, call_rel32_disp: i32) {
    // 1. push r15 (save the effect-environment pointer on the stack).
    push_reg64(buf, Reg64::R15);

    // 2. call rel32 (near call to native function).
    //    TODO(m6-007): disp will be backpatched at link time.
    call_rel32(buf, call_rel32_disp);

    // 3. pop r15 (restore the saved environment pointer).
    pop_reg64(buf, Reg64::R15);

    // 4. ret (return to UEFI caller).
    ret(buf);
}

/// Convenience wrapper that computes the rel32 displacement from
/// (thunk_origin_rva, native_target_rva).
///
/// The rel32 displacement is calculated as:
/// ```text
/// rel = native_target_rva as i64 - (thunk_origin_rva as i64 + 5)
/// ```
/// where 5 is the size of the call instruction itself (E8 + 4 bytes disp).
///
/// # Example
///
/// If the thunk is at RVA 0x1000 and the native target is at 0x2000:
/// ```text
/// rel = 0x2000 - (0x1000 + 5) = 0x2000 - 0x1005 = 0xFB
/// ```
pub fn emit_uefi_thunk_for_target(
    buf: &mut CodeBuffer,
    thunk_origin_rva: u32,
    native_target_rva: u32,
) {
    let rel = native_target_rva as i64 - (thunk_origin_rva as i64 + 5);
    let rel32 = rel as i32;
    emit_uefi_thunk(buf, rel32);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_uefi_thunk_prefix_push_r15_suffix_pop_r15_ret() {
        let mut buf = CodeBuffer::new();
        emit_uefi_thunk(&mut buf, 0);

        let bytes = buf.as_slice();

        // Prefix: push r15 (41 57)
        assert_eq!(&bytes[0..2], &[0x41, 0x57]);

        // Suffix: pop r15 (41 5F) + ret (C3)
        assert_eq!(&bytes[bytes.len() - 3..], &[0x41, 0x5F, 0xC3]);
    }

    #[test]
    fn emit_uefi_thunk_length_is_10_bytes() {
        let mut buf = CodeBuffer::new();
        emit_uefi_thunk(&mut buf, 0);

        // push r15 (2) + call rel32 (5) + pop r15 (2) + ret (1) = 10 bytes
        assert_eq!(buf.len(), 10);
    }

    #[test]
    fn emit_uefi_thunk_for_target_computes_rel32() {
        let thunk_origin_rva = 0x1000u32;
        let native_target_rva = 0x2000u32;

        let mut buf = CodeBuffer::new();
        emit_uefi_thunk_for_target(&mut buf, thunk_origin_rva, native_target_rva);

        // Expected rel32 = 0x2000 - (0x1000 + 5) = 0xFB
        let rel = native_target_rva as i64 - (thunk_origin_rva as i64 + 5);
        let rel32 = rel as i32;

        let bytes = buf.as_slice();
        // call rel32 instruction: E8 <disp32 LE>
        // Extract disp from bytes [1..5]
        let extracted_rel32 = i32::from_le_bytes([bytes[3], bytes[4], bytes[5], bytes[6]]);
        assert_eq!(extracted_rel32, rel32);
    }

    #[test]
    fn emit_uefi_thunk_for_target_backward_rel() {
        let thunk_origin_rva = 0x2000u32;
        let native_target_rva = 0x1000u32;

        let mut buf = CodeBuffer::new();
        emit_uefi_thunk_for_target(&mut buf, thunk_origin_rva, native_target_rva);

        // Expected rel32 = 0x1000 - (0x2000 + 5) = -0x1005
        let rel = native_target_rva as i64 - (thunk_origin_rva as i64 + 5);
        let rel32 = rel as i32;

        let bytes = buf.as_slice();
        let extracted_rel32 = i32::from_le_bytes([bytes[3], bytes[4], bytes[5], bytes[6]]);
        assert_eq!(extracted_rel32, rel32);
    }
}
