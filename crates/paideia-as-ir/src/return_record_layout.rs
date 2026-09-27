//! Side-table for **function-symbol return-record layouts**.
//!
//! PAS-DEBT-B4-002 Slice A (paideia-as#1554). Records the finalised
//! `RecordLayout` of the value a function returns when its declared
//! return type is a record (either the anonymous inline shape
//! `record { … }` or a named struct in return position such as
//! `fn(...) -> CpuidRegs`).
//!
//! Keyed by the **item-level Let node's IrNodeId** (i.e. the outer
//! `pub let cpuid_leaf : (u32, u32) -> CpuidRegs = fn …` binding, not
//! the inner Lambda's node id). The elaborator's `emit_walker::walk`
//! constructs its `Symbol` from that same Let id and reads this table
//! to populate `Symbol::return_record_layout`; Slice B / C code then
//! drives the SysV / MS aggregate-return classifier and the record
//! pair-unpack lowering off `Symbol`.
//!
//! An entry here is a positive statement — "this Let's return type is
//! a record and here is its byte-exact layout". The absence of an
//! entry is not a negative statement: it may mean the function has a
//! scalar return, or that the layout-computation pass could not fold
//! the return type (unresolved name, unsupported field width, etc.).
//! Slice B / C consumers must key on `Symbol::return_record_layout ==
//! Some(_)` rather than probing this table directly.

use crate::impl_named_side_table;
use crate::node::IrNodeId;
use crate::record_layout::RecordLayout;

impl_named_side_table!(
    /// Side-table mapping item-level Let node ids → finalised
    /// `RecordLayout` for that binding's return-record type.
    ///
    /// See the module docblock for the "keyed by Let, not by Lambda"
    /// convention and the entry/absence-vs-scalar semantics.
    pub struct ReturnRecordLayoutTable, IrNodeId => RecordLayout
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record_layout::{FieldLayout, RecordLayout};

    fn cpuid_regs_layout() -> RecordLayout {
        // 4 × u32 unsigned, offsets 0/4/8/12, align 4, size 16.
        RecordLayout::with_field_names(
            16,
            4,
            vec![
                FieldLayout { offset: 0,  size: 4, signed: false, is_float: false },
                FieldLayout { offset: 4,  size: 4, signed: false, is_float: false },
                FieldLayout { offset: 8,  size: 4, signed: false, is_float: false },
                FieldLayout { offset: 12, size: 4, signed: false, is_float: false },
            ],
            vec!["eax".to_string(), "ebx".to_string(), "ecx".to_string(), "edx".to_string()],
        )
    }

    #[test]
    fn table_empty_by_default() {
        let table = ReturnRecordLayoutTable::new();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn insert_and_get_cpuid_regs_layout() {
        let mut table = ReturnRecordLayoutTable::new();
        let let_id = IrNodeId::new(7).unwrap();
        let layout = cpuid_regs_layout();

        table.insert(let_id, layout.clone());

        let got = table.get(let_id).expect("layout present");
        assert_eq!(got.size, 16);
        assert_eq!(got.align, 4);
        assert_eq!(got.fields.len(), 4);
        assert_eq!(got.field_names, vec!["eax", "ebx", "ecx", "edx"]);
        assert_eq!(*got, layout);
    }

    #[test]
    fn missing_key_returns_none() {
        let table = ReturnRecordLayoutTable::new();
        let missing = IrNodeId::new(42).unwrap();
        assert!(table.get(missing).is_none());
    }

    #[test]
    fn insert_overwrites_previous_layout() {
        let mut table = ReturnRecordLayoutTable::new();
        let let_id = IrNodeId::new(3).unwrap();

        let small = RecordLayout::new(
            4,
            4,
            vec![FieldLayout { offset: 0, size: 4, signed: false, is_float: false }],
        );
        let big = cpuid_regs_layout();

        table.insert(let_id, small.clone());
        table.insert(let_id, big.clone());

        assert_eq!(*table.get(let_id).unwrap(), big);
    }
}
