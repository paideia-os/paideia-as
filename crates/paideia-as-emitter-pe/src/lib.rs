//! paideia-as-emitter-pe
//!
//! PE/COFF emitter for Microsoft x64 / UEFI binaries.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod header;

pub use header::{
    COFF_FILE_HEADER_SIZE, CoffFileHeader, DATA_DIRECTORY_SIZE, DOS_HEADER_SIZE, DOS_MAGIC,
    DataDirectory, DosHeader, IMAGE_FILE_EXECUTABLE_IMAGE, IMAGE_FILE_MACHINE_AMD64,
    IMAGE_NT_OPTIONAL_HDR64_MAGIC, IMAGE_SUBSYSTEM_EFI_APPLICATION, NT_SIGNATURE,
    NUMBER_OF_DATA_DIRECTORIES, OPTIONAL_HEADER_PE32PLUS_SIZE, OptionalHeaderPe32Plus,
};
