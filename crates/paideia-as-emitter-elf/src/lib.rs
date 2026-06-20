//! paideia-as-emitter-elf
//!
//! ELF64 writer for paideia-as object files per
//! `design/toolchain/custom-assembler.md` §12.1.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod encode;
pub mod handler;
pub mod lower;
pub mod opt;
pub mod prologue;
pub mod relocs;
mod sections;
pub mod symtab;
pub mod sysv_bridge;
mod writer;

pub use encode::{
    CodeBuffer, Cond, Reg32, Reg64, add_reg64_reg64, call_rel32, cmp_reg64_reg64,
    emit_indexed_load, emit_indexed_store, jcc_rel32, jmp_rel8, jmp_rel32, mov_mem_rbp_disp_reg64,
    mov_reg64_imm32, mov_reg64_imm64, mov_reg64_mem_rbp_disp, mov_reg64_reg64, pop_reg64,
    push_reg64, ret, sub_reg64_reg64, test_reg64_reg64, xor_reg64_reg64,
};
pub use handler::{emit_handler_chain, emit_handler_close, emit_handler_open};
pub use lower::{
    BodyShape, lea_rax_rdi_disp, lower_add_one, lower_app_mnemonic, lower_function_body,
    lower_handler_call, lower_ir_to_bytes, lower_let_literal, lower_local_load,
};
pub use prologue::{FrameLayout, STACK_ALIGN, STACK_PROBE_THRESHOLD, emit_epilogue, emit_prologue};
pub use relocs::{RelocEntry, RelocKind};
pub use sections::{PAIDEIA_SECTIONS, STANDARD_SECTIONS, all_sections};
pub use symtab::{SymKind, SymbolEntry};
pub use sysv_bridge::{emit_sysv_bridge_epilogue, emit_sysv_bridge_prologue};
pub use writer::{Arch, ElfWriter, Kind};
