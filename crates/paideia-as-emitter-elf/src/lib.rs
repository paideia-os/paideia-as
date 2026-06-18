//! paideia-as-emitter-elf
//!
//! ELF64 writer for paideia-as object files per
//! `design/toolchain/custom-assembler.md` §12.1.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod encode;
mod sections;
mod writer;

pub use encode::{
    CodeBuffer, Cond, Reg32, Reg64, add_reg64_reg64, call_rel32, cmp_reg64_reg64, jcc_rel32,
    jmp_rel8, jmp_rel32, mov_mem_rbp_disp_reg64, mov_reg64_imm32, mov_reg64_imm64,
    mov_reg64_mem_rbp_disp, mov_reg64_reg64, pop_reg64, push_reg64, ret, sub_reg64_reg64,
    test_reg64_reg64, xor_reg64_reg64,
};
pub use sections::{PAIDEIA_SECTIONS, STANDARD_SECTIONS, all_sections};
pub use writer::{Arch, ElfWriter, Kind};
