//! Codegen and CLI corpus.
//!
//! Consolidates the "everything else" set: `abi_pdx.rs`, `array_element_widths.rs`,
//! `bss_emission.rs`, `cli.rs`, `cross_file_relocation.rs`, `pub_cross_module_link.rs`,
//! `data_rodata.rs`, `e2e_elf.rs`, `examples_corpus.rs`, `note_paideia_layouts.rs`,
//! `opt_peephole_smoke.rs`, `relocation_linking.rs`, `symtab_underscore_prefix.rs`,
//! and `build_unsafe.rs`.

mod common;

mod codegen {
    pub mod abi_pdx;
    pub mod array_element_widths;
    pub mod bss_emission;
    pub mod build_unsafe;
    pub mod cli;
    pub mod cross_file_relocation;
    pub mod data_rodata;
    pub mod e2e_elf;
    pub mod examples_corpus;
    pub mod note_paideia_layouts;
    pub mod opt_peephole_smoke;
    pub mod pub_cross_module_link;
    pub mod relocation_linking;
    pub mod symtab_underscore_prefix;
}
