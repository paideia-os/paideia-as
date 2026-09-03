//! paideia-as-stdlib: stdlib source files for paideia-as.
//!
//! See pdx/ for the .pdx source. tests/parse_pdx.rs verifies parse-cleanliness.
//!
//! Key types (v0.18):
//! - Generic `Option<T>` and `Result<T, E>` declarations in pdx/option.pdx and pdx/result.pdx.
//!   Currently used via hand-monomorphized aliases (OptionU64, ResultU64U64) pending #997c.
//!   See design/paideia-as/v0.18-issue-997-option-result-stdlib.md for the v0.18 #997 roadmap.
//! - `Str { ptr: *u8, len: u64 }`: borrowed byte-slice type in pdx/str.pdx with field accessors.
//!   Supports pointer-parameter field reads. Module-level constants and byte indexing deferred (blocked by #998a).
//!   See design/paideia-as/v0.18-issue-998-string-str-stdlib.md for the v0.18 #998 roadmap (downgraded scope).
//! - Free-function API: unwrap_or as a free function in option_u64.pdx and result_u64_u64.pdx.

pub mod cicp;
pub mod gen_index_tree;
pub mod matrix;
