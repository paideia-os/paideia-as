//! paideia-as-dwarf: DWARF debug info emission with paideia vendor extensions.
//!
//! DWARF 5 emitter for paideia-as object files per
//! `design/toolchain/debug-info.md`.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod info;
pub mod line;
pub mod sections;
pub mod vendor;

pub use info::{CompilationUnit, FunctionDie, build_cu};
pub use line::{LineEntry, build_line_program_from_instruction_table};
pub use sections::{
    build_caps_section, build_effects_section, build_sig_section, build_version_section,
};
pub use vendor::{
    DW_AT_PAIDEIA_CAP_KIND, DW_AT_PAIDEIA_EFFECT_ID_LIST, DW_AT_PAIDEIA_LIN_CLASS,
    DW_AT_PAIDEIA_ROW_VAR_ID, DW_AT_PAIDEIA_SIG_BLAKE3, DW_FORM_PAIDEIA_EFFECT_LIST,
    DW_TAG_PAIDEIA_CAPABILITY_BINDING, DW_TAG_PAIDEIA_EFFECT_ROW, DW_TAG_PAIDEIA_SIGNATURE,
    SECTION_CAPS, SECTION_EFFECTS, SECTION_SIG, SECTION_VERSION, VENDOR_ID, VENDOR_SECTIONS,
    VENDOR_VERSION_BYTES, empty_vendor_payloads,
};
