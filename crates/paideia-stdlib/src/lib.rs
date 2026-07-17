//! paideia-stdlib: stdlib source files for paideia-as.
//!
//! See pdx/ for the .pdx source. tests/parse_pdx.rs verifies parse-cleanliness.
//!
//! Key types (v0.18):
//! - Generic `Option<T>` and `Result<T, E>` declarations in pdx/option.pdx and pdx/result.pdx.
//!   Currently used via hand-monomorphized aliases (OptionU64, ResultU64U64) pending #997c.
//! - Free-function API: unwrap_or as a free function in option_u64.pdx and result_u64_u64.pdx.
//! - See design/paideia-as/v0.18-issue-997-option-result-stdlib.md for the v0.18 #997 roadmap.
