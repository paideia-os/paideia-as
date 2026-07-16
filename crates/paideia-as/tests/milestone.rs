//! Milestone regression corpus.
//!
//! Consolidates the ticket-numbered `pa10_*`, `pa14_*`, `pa15_*`, `pa16_*`,
//! `pa17_*`, `pa_r12_*`, and `pa_r17_*` binaries (non-`build_emit` variety;
//! those live in `build_emit.rs`). Each ex-file is one submodule.

mod common;

mod milestone {
    pub mod pa10_006y_align_default;
    pub mod pa10_006y_align_pml4_bss;
    pub mod pa10_013_local_stub_no_collision;
    pub mod pa14_008_ring_synthesis;
    pub mod pa14_012_driver_corpus;
    pub mod pa15_009_jump_table;
    pub mod pa15_011_udp_echo_canary;
    pub mod pa16_012_cow_fs_canary;
    pub mod pa17_003_fnptr_addr_of;
    pub mod pa17_004_indirect_call;
    pub mod pa_r12_001_string_literal;
    pub mod pa_r17_003b_t0535;
    pub mod pa_r17_004c_call_rip_fnptr;
    pub mod pa_r17_009a_nested_patterns;
    pub mod pa_r17_010_record_fnptr_reloc;
    pub mod pa_r17_010c_record_layout_emit;
    pub mod pa_r17_014_lea_addend_i32_guard;
    pub mod pa_r17_015_call_rip_rel;
    pub mod pa_r17_1074_t0535_record_field;
    pub mod pa_r17_1075_t0535_return_type;
    pub mod pa_r17_1076_t0535_effect_cap_row;
}
