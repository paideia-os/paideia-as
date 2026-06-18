//! paideia-as-emitter-elf
//!
//! ELF64 writer for paideia-as object files per
//! `design/toolchain/custom-assembler.md` §12.1.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod sections;
mod writer;

pub use sections::{PAIDEIA_SECTIONS, STANDARD_SECTIONS, all_sections};
pub use writer::{Arch, ElfWriter, Kind};
