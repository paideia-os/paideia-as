//! paideia-as-emitter-pax
//!
//! PAX (PaideiaOS Architectural Executable) emitter. PAX is the
//! canonical PaideiaOS object format carrying capability sigs,
//! effect rows, PQ signatures, BLAKE3 content hashes.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod audit;
pub mod caps;
pub mod effects;
pub mod header;
pub mod imports;
pub mod relocs;
pub mod section;
pub mod symtab;

pub use audit::{
    LIN_ENTRY_SIZE, LinEntry, LinSection, OPT_ENTRY_SIZE, OptEntry, OptSection, PassId,
    UNSAFE_ENTRY_SIZE, UnsafeEntry, UnsafeSection,
};
pub use caps::{CAP_ENTRY_SIZE, CapEntry, CapKind, CapsSection, LinClass, SiteKind};
pub use effects::{EffectRowEntry, EffectsSection};
pub use header::{
    Architecture, HeaderFlag, PAX_FORMAT_VERSION, PAX_HEADER_SIZE, PAX_MAGIC, PaxHeader,
};
pub use imports::{CAP_DESC_SIZE, CapDescriptor, ExportsSection, ImportsSection, cap_flags};
pub use relocs::{RELOC_ENTRY_SIZE, RelocEntry, RelocKind, Relocs};
pub use section::{
    SECTION_DESCRIPTOR_SIZE, SECTION_NAME_MAX, Section, SectionFlag, SectionTable, SectionType,
};
pub use symtab::{SYM_ENTRY_SIZE, SymBinding, SymEntry, SymTab, SymVisibility};
