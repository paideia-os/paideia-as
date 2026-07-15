//! Runtime execution tests for compiled Paideia fixtures.
//!
//! This harness:
//! 1. Compiles a .pdx fixture to a .o object file
//! 2. Synthesizes a C driver that poisons CPU registers
//! 3. Links driver.c + fixture.o with gcc
//! 4. Runs the binary and verifies the exit code

mod common;

mod runtime {
    pub mod harness;
    pub mod driver_template;

    mod flat_lambda_binop_canaries;
    mod flat_lambda_binop_mul_var_var_value;
    mod match_enum_pattern;
    mod match_nested_pattern;
    mod ms_call_saves_rcx_value;
    mod four_arg_call_saves_rcx_value;
    mod two_let_with_tail_binop_value;
    mod single_let_then_tail_add_value;
    mod let_with_binop_rhs_value;
    mod let_with_bitnot_rhs_value;
    mod let_with_bitnot_shl_rhs_value;
    mod let_chain_2_of_2_value;
    mod let_chain_3_of_3_value;
    mod let_chain_2_typed_value;
    mod let_chain_2_binop_value;
    mod let_chain_2_call_value;
    mod let_chain_2_match_value;
    mod func_scope_enum_let_a_value;
    mod func_scope_enum_let_b_value;
    mod func_scope_enum_let_c_value;
    mod func_scope_multiple_lets_value;
    mod func_scope_enum_let_shadow_value;
    mod block_match_direct_expr_value;
    mod block_match_braced_2arm_value;
    mod block_match_braced_3arm_value;
    mod block_match_braced_nested_value;
    mod sysv_bridge_bump_value;
    mod let_rhs_binop_mul_value;
    mod module_var_assign_mul_value;
    mod pick3_arm_a_value;
    mod pick3_arm_b_value;
    mod pick3_arm_c_value;
    mod pick4_arm_a_value;
    mod pick4_arm_b_value;
    mod pick4_arm_c_value;
    mod pick4_arm_d_value;
    mod pick5_arm_c_value;
    mod pick5_arm_e_value;
    mod enum_qual_arg_pos0_3arg_value;
    mod enum_qual_arg_pos0_3arg_b_value;
    mod enum_qual_arg_pos0_4arg_value;
    mod enum_qual_arg_pos0_2arg_value;
    mod enum_qual_arg_pos1_2arg_value;
    mod enum_qual_arg_mixed_locals_value;
    mod enum_match_arm_binop_mul_value;
    mod unit_var_module_let_matches_value;
    mod unit_var_module_let_matches_b_value;
    mod unit_var_module_let_matches_c_value;
    mod unit_var_multiple_bindings_value;
}
