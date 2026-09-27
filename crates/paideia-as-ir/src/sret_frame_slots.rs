//! Side-tables for **caller-side sret persistent frame slots** and per-
//! Lambda frame bumps. Companion of `return_record_layout` for
//! PAS-DEBT-B4-002 Slice C (paideia-as#1554).
//!
//! # Purpose
//!
//! Slice B reserved the sret destination buffer immediately around the
//! CALL (`sub rsp, N; lea rdi, [rsp]; …; call callee; add rsp, N`) — a
//! transient slot released as soon as the call returned. That works
//! only when the caller does not need to read fields out of the
//! returned aggregate past the CALL. Slice C promotes selected sret
//! slots to a persistent frame area so the caller can dereference
//! fields at `[RBP + slot_disp + field_offset]` for the rest of its
//! body.
//!
//! Two side-tables cooperate:
//!
//! * [`CallerSretSlotTable`] — keyed by the **App IrNodeId** of the
//!   record-returning call site. Value: the slot's RBP-relative
//!   displacement and its padded byte size. Populated by
//!   `return_record_cons_pass::populate_return_record_cons_slots` for
//!   every App whose callee has a `Symbol::return_record_layout`
//!   whose placement is `Memory` (i.e. `needs_hidden_sret()`).
//!
//! * [`CallerSretFrameBumpTable`] — keyed by the **caller Lambda
//!   IrNodeId**. Value: the total padded byte size of caller-side sret
//!   slots for that Lambda (sum over its App-node slots). Consumed by
//!   `emit_visit_lambda`'s prologue block to emit `sub rsp, N` after
//!   the frame-pointer prologue (`mov rsp, rbp` in `emit_ret` releases
//!   it symmetrically at exit — no matching `add rsp, N` needed for
//!   frame-pointer functions).
//!
//! # Absence-of-entry policy
//!
//! An App with no entry in [`CallerSretSlotTable`] is either (a) a
//! non-record-returning call, or (b) a record-returning call whose
//! placement is register-return (IntPair, SseSingle, etc). Both fall
//! back to the Slice B path:
//!
//!   * Memory placement without an entry → `emit_call.rs` reverts to
//!     the transient `sub/lea/add` triplet (backwards-compatible with
//!     Slice B fixtures that predate this table).
//!   * Register placement without an entry → CALL emission stays
//!     byte-identical to the scalar path (no post-CALL unpack).
//!
//! # Piece 4 wire-up
//!
//! The caller-side pair-unpack (Piece 4 of Slice C) uses the same
//! [`CallerSretSlotTable`] entry as its destination: when a
//! register-return call site has an entry (i.e. the caller wants to
//! materialise the pair in a persistent slot), `emit_call.rs` splices
//! `sysv_caller_read_return_pair` / `ms_caller_read_return_reg` after
//! the CALL to write RAX/RDX/XMM0/XMM1 into `[RBP + slot_disp]`.

use crate::impl_named_side_table;
use crate::node::IrNodeId;

/// One caller-owned sret slot on a caller Lambda's frame.
///
/// * `rbp_disp` — byte offset from `RBP`. Always negative (slots live
///   below the frame pointer). Computed by the population pass so slots
///   don't overlap within the same caller.
/// * `padded_size` — the slot's byte size, rounded up to a 16-byte
///   multiple so consecutive slots preserve SysV's `rsp mod 16 == 0`
///   invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallerSretSlot {
    /// Byte offset from `RBP` (negative — slot lives below `RBP`).
    pub rbp_disp: i32,
    /// Slot size in bytes, rounded up to a 16-byte multiple.
    pub padded_size: u32,
}

impl CallerSretSlot {
    /// Construct a new slot descriptor.
    #[must_use]
    pub fn new(rbp_disp: i32, padded_size: u32) -> Self {
        Self { rbp_disp, padded_size }
    }
}

impl_named_side_table!(
    /// Side-table mapping **App IrNodeId → caller-owned sret slot**.
    ///
    /// See the module docblock for the "keyed by App, not by callee"
    /// convention and the absence-of-entry semantics.
    pub struct CallerSretSlotTable, IrNodeId => CallerSretSlot
);

impl_named_side_table!(
    /// Side-table mapping **caller Lambda IrNodeId → total padded
    /// bytes** of caller-side sret slots in that Lambda's frame.
    ///
    /// Consumed by `emit_visit_lambda`'s prologue block to emit
    /// `sub rsp, N` after the frame-pointer prologue. The matching
    /// `mov rsp, rbp` in `emit_ret` releases the whole block at once;
    /// no per-slot `add rsp` is needed.
    pub struct CallerSretFrameBumpTable, IrNodeId => u32
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caller_sret_slot_table_empty_by_default() {
        let table = CallerSretSlotTable::new();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn caller_sret_slot_insert_and_get() {
        let mut table = CallerSretSlotTable::new();
        let app_id = IrNodeId::new(11).unwrap();
        let slot = CallerSretSlot::new(-32, 32);

        table.insert(app_id, slot);

        let got = table.get(app_id).expect("slot present");
        assert_eq!(got.rbp_disp, -32);
        assert_eq!(got.padded_size, 32);
    }

    #[test]
    fn caller_sret_frame_bump_table_defaults_and_insert() {
        let mut table = CallerSretFrameBumpTable::new();
        assert!(table.is_empty());

        let lambda_id = IrNodeId::new(3).unwrap();
        table.insert(lambda_id, 48);
        assert_eq!(*table.get(lambda_id).unwrap(), 48);

        let missing = IrNodeId::new(999).unwrap();
        assert!(table.get(missing).is_none());
    }

    #[test]
    fn caller_sret_slot_insert_overwrites_previous() {
        let mut table = CallerSretSlotTable::new();
        let app_id = IrNodeId::new(5).unwrap();
        table.insert(app_id, CallerSretSlot::new(-16, 16));
        table.insert(app_id, CallerSretSlot::new(-48, 48));
        let got = table.get(app_id).unwrap();
        assert_eq!(got.padded_size, 48);
        assert_eq!(got.rbp_disp, -48);
    }
}
