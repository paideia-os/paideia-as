//! paideia-as-emitter-pe
//!
//! PE/COFF emitter for Microsoft x64 / UEFI binaries.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod header;
pub mod imports;
pub mod reloc;
pub mod section;
pub mod uefi_thunk;

pub use header::{
    COFF_FILE_HEADER_SIZE, CoffFileHeader, DATA_DIRECTORY_SIZE, DOS_HEADER_SIZE, DOS_MAGIC,
    DataDirectory, DosHeader, IMAGE_FILE_EXECUTABLE_IMAGE, IMAGE_FILE_MACHINE_AMD64,
    IMAGE_NT_OPTIONAL_HDR64_MAGIC, IMAGE_SUBSYSTEM_EFI_APPLICATION, NT_SIGNATURE,
    NUMBER_OF_DATA_DIRECTORIES, OPTIONAL_HEADER_PE32PLUS_SIZE, OptionalHeaderPe32Plus,
};

pub use imports::{
    HINT_NAME_ALIGN, IMAGE_IMPORT_DESCRIPTOR_SIZE, IMPORT_ORDINAL_FLAG_64, Import,
    ImportDescriptor, ImportSection,
};

pub use reloc::{
    IMAGE_REL_BASED_ABSOLUTE, IMAGE_REL_BASED_DIR64, PAGE_SIZE, RelocSection, Relocation,
};

pub use section::{
    CHARACTERISTICS_BSS, CHARACTERISTICS_DATA, CHARACTERISTICS_RDATA, CHARACTERISTICS_TEXT,
    IMAGE_SCN_CNT_CODE, IMAGE_SCN_CNT_INITIALIZED_DATA, IMAGE_SCN_CNT_UNINITIALIZED_DATA,
    IMAGE_SCN_MEM_EXECUTE, IMAGE_SCN_MEM_READ, IMAGE_SCN_MEM_WRITE, SECTION_HEADER_SIZE,
    SECTION_NAME_LEN, Section, SectionHeader, SectionTable, align_up,
};

pub use uefi_thunk::{emit_uefi_thunk, emit_uefi_thunk_for_target};
