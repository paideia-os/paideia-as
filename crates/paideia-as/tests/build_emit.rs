//! `build --emit` corpus of integration tests.
//!
//! Consolidates every ex-`build_emit_*.rs` binary into one link target. Each
//! submodule maps 1:1 to an ex-file; the leaf `#[test]` names are preserved
//! so `cargo test -- <name>` filters continue to work.

mod common;

mod build_emit {
    pub mod abi_probe;
    pub mod back_to_back_labels;
    pub mod bridge_thunk;
    pub mod bss_reloc;
    pub mod call_sym;
    pub mod cap_smoke;
    pub mod control_flow_corpus;
    pub mod encoder_strict;
    pub mod enum_cons;
    pub mod enum_record_payload;
    pub mod field_access_call;
    pub mod field_read;
    pub mod field_read_multi_field;
    pub mod field_read_u32;
    pub mod field_write_u32;
    pub mod guid_inline_bytes;
    pub mod imm64_top_bit;
    pub mod include_bytes_probe;
    pub mod include_str_probe;
    pub mod label_patches;
    pub mod link_section_probe;
    pub mod match_enum_pattern;
    pub mod module_let_no_text_emission;
    pub mod module_let_mut_assign_negatives;
    pub mod module_let_mut_assign_via_lambda;
    pub mod multi_fn_store_dispatch_ordering;
    pub mod pa10_006i_imm;
    pub mod pa10_006k_ljmp;
    pub mod pa10_006l_inout;
    pub mod pa10_007_data_symbol_names;
    pub mod pa10_008_multi_fn_offsets;
    pub mod pa7c_expr_surface;
    pub mod pa7c_plt32_witness;
    pub mod pa7c_symbol_export;
    pub mod pa7c_unsafe_body;
    pub mod pa8_call_st_value;
    pub mod pa8_cross_module_call;
    pub mod pa8_m1_001b_lambda_params;
    pub mod pa8_m3_004;
    pub mod pa8_mixed_shapes;
    pub mod pa8_single_fn_regression;
    pub mod pa8_st_value;
    pub mod pa8_unsafe_block_st_value;
    pub mod pa_r12_004_pure_body;
    pub mod pa_r13_008_mut_literal_data;
    pub mod pa_r17_004_identity_multi_param;
    pub mod pa_r17_005_flat_multi_param;
    pub mod pa_r19_1100_byte_order;
    pub mod phase6_cr_moves;
    pub mod record_reorder;
    pub mod rep_movsb;
    pub mod rep_stosq;
    pub mod smoke;
    pub mod stmt_assign_call_rhs;
    pub mod stmt_assign_local_let_mut;
    pub mod stmt_assign_module_let_mut_plain;
    pub mod target_triplet;
    pub mod typed_encoder_diagnostics;
    pub mod unsafe_call_stmt_diagnostic;
    pub mod unsafe_stmt_kinds_diagnostic;
    pub mod uefi_stub;
}
