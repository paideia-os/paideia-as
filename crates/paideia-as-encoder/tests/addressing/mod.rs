//! Addressing-mode encoders: segment prefixes (GS-relative), LEA in
//! Mode32, SIB + REX audits, symbol-with-addend, and the 4-site
//! reloc-offset regression suite. Grouped as "everything about how
//! operands compute or reference an effective address".

mod gs_relative;
mod lea_mode32;
mod reloc_offset_4_sites;
mod sib_rex_audit;
mod symbol_addend_encode;
