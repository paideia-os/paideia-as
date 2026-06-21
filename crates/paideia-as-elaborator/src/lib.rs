//! paideia-as-elaborator
//!
//! AST → IR lowering and (in later PRs) type/effect/capability checking
//! passes. See `design/toolchain/custom-assembler.md` §6.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod borrow_walker;
pub mod branch_merge;
pub mod cap_infer;
pub mod cap_walker;
pub mod capture;
pub mod check_bounds;
pub mod check_coherence;
pub mod check_expr;
pub mod check_handler;
pub mod check_lambda;
pub mod check_linearity;
pub mod check_match;
pub mod check_ordered;
pub mod check_pattern;
pub mod check_pure;
pub mod derive;
pub mod effect_infer;
pub mod effect_unify;
pub mod effect_walker;
pub mod elab_builtin;
pub mod emit_walker;
pub mod env;
pub mod file_module;
pub mod functor_apply;
pub mod hygiene;
pub mod incremental;
pub mod intrinsics;
pub mod kind;
pub mod last_use;
pub mod lifetime_walker;
pub mod local_binding_table;
pub mod linearity_ctx;
mod lower;
pub mod macro_expand;
pub mod macro_match;
pub mod modules;
pub mod mutation_walker;
pub mod name_resolution;
pub mod name_resolution_walker;
pub mod pack;
pub mod pattern_lower;
mod placeholder_emit;
pub mod populate;
pub mod position_index;
pub mod reflect_api;
pub mod region_elision;
pub mod region_inference;
pub mod resolve;
pub mod sharing;
pub mod sig_match;
pub mod splice;
pub mod term_eval;
pub mod two_phase;
pub mod unsafe_walker;
pub mod walker_pass_state;

pub use borrow_walker::{BorrowKind, BorrowWalker};
pub use branch_merge::{S_BRANCH_MISMATCH, merge_branches};
pub use cap_infer::{C_MISSING_CAP, check_capabilities, compose_caps};
pub use cap_walker::CapWalker;
pub use capture::{CaptureKind, CapturedBinding, analyze_captures};
pub use check_bounds::{
    BoundCache, BoundResolution, T_UNSATISFIED_BOUND, resolve_bound, unsatisfied_bound_diagnostic,
};
pub use check_expr::{InferOutcome, check_annotation, infer_node};
pub use check_handler::{F_HANDLER_MISMATCH, HandlerImpl, check_handler, check_resume};
pub use check_lambda::{S_ILLEGAL_CAPTURE, check_lambda};
pub use check_linearity::{
    LinearityWalker, S_NEVER_USED, S_OVERUSED, validate_scope, walk_expr_for_scope,
};
pub use check_match::{
    ArmPattern, ExhaustivenessResult, M_MATCH_EXHAUSTIVENESS, check_exhaustiveness,
    exhaustiveness_diagnostic,
};
pub use check_ordered::{OrderedEntry, OrderedLog, S_OUT_OF_ORDER};
pub use check_pattern::is_irrefutable;
pub use check_pure::{F_PURE_VIOLATION, check_pure};
pub use derive::{DeriveKind, SyntheticImpl, synthesise_derive};
pub use effect_infer::{
    F_UNHANDLED_EFFECT, RowOutcome, call_row, check_no_unhandled, compose_rows, handle_row,
    perform_row,
};
pub use effect_unify::{
    CallUnifyOutcome, F_HANDLER_ORDER, F_ROW_MISMATCH, check_handler_order, instantiate_fresh_tail,
    unify_call_row,
};
pub use effect_walker::EffectRowWalker;
pub use emit_walker::{EmitPassState, EmitWalker};
pub use env::{Symbol, TypeEnv};
pub use file_module::{
    M_FILE_NAME_MISMATCH, M_MULTIPLE_TOP_MODULES, M_NO_TOP_MODULE, expected_module_name,
    validate_file_module_mapping,
};
pub use functor_apply::{ApplyKey, apply_functor, elaborate_functor_body};
pub use hygiene::{HygieneCache, HygienicName, MacroId};
pub use intrinsics::{IntrinsicSignature, TypeKind, all_intrinsics, lookup_intrinsic};
pub use kind::type_kind;
pub use last_use::LastUseAnalyzer;
pub use lifetime_walker::LifetimeWalker;
pub use linearity_ctx::{Binding, LinearityCtx};
pub use local_binding_table::LocalBindingTable;
pub use lower::{LoweringResult, lower_ast_to_ir};
pub use macro_expand::{
    ExpansionOutcome, M_MACRO_EFFECT_VIOLATION, M_RECURSION_LIMIT, M_UNBOUND_META,
    MAX_EXPANSION_DEPTH, check_depth, expand_template,
};
pub use macro_match::{
    InvocationMatch, M_NO_MATCH, MatchBinding, RuleMatch, match_invocation, match_rule,
};
pub use modules::{
    FieldBinding, S_LINEAR_FIELD_OVERUSED, TypedValue, ValueRef, elaborate_structure,
};
pub use mutation_walker::MutationWalker;
pub use name_resolution::{NameResolutionTable, Span};
pub use name_resolution_walker::{
    NameResolutionPassState, NameResolutionTableWriter, NameResolutionWalker,
};
pub use pack::{
    M_UNPACK_NOT_PACKED, PackedValue, elaborate_let_module, elaborate_pack, elaborate_unpack,
};
pub use pattern_lower::{lower_ident_pattern, lower_record_pattern, lower_tuple_pattern};
pub use placeholder_emit::placeholder_for;
pub use populate::{PopulateContext, populate_instruction_table};
pub use position_index::{ByteOffset, FileId, PositionEntry, PositionIndex};
pub use reflect_api::{TypeCache, children, kind, span, type_of};
pub use region_elision::{
    ElisionResult, L_AMBIGUOUS_LIFETIME_ELISION, ambiguous_elision_diagnostic, elide_lifetimes,
};
pub use region_inference::RegionInferenceWalker;
pub use resolve::{HygienicEnv, ResolveValue};
pub use sharing::{M_SHARING_VIOLATED, check_sharing_constraints};
pub use sig_match::{M_SIG_KIND_MISMATCH, M_SIG_MISSING_DECL, match_signature};
pub use splice::{splice, splice_with_hygiene, splice_with_type_check};
pub use two_phase::{TwoPhaseReservation, activate_reservation, reserve_two_phase_borrow};
pub use unsafe_walker::{OperandError, UnsafeWalker, parse_operand_from_ast, resolve_mnemonic};
pub use walker_pass_state::{PositionIndexWriter, WalkerPassState};
