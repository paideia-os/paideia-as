//! Consolidated integration-test entry for `paideia-as-parser`.
//!
//! Each `mod` below pulls in a topical test module from a sibling
//! `tests/<topic>.rs` file. Cargo builds a single integration binary
//! (`integration`) instead of one binary per file, which cuts link overhead
//! on the workspace-wide test cycle. See
//! `.plans/test-restructure-2026-07-08.md` for the rationale.
//!
//! Behavior-preserving: each leaf test keeps its original name and source
//! location, so `insta` snapshots continue to resolve to their existing
//! paths under `tests/snapshots/`.

mod align_attr_errors;
mod assoc_projection;
mod empty_fn_args;
mod endian_attr_snapshots;
mod fn_type_param_names;
mod for_pattern_extensions;
mod example_files;
mod functor_attr_binding;
mod inner_attr_bits;
mod issue_1327_record_reserved_diag;
mod ljmp_instruction;
mod packed_struct_snapshots;
mod pattern_extensions;
mod range_expr;
mod ring_attr_errors;
mod snapshots_gpu_context;
mod snapshots_modules;
mod stmt_multiline;
mod struct_type_def;
mod tuple_expr;
mod type_forall;
