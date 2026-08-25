//! MOV family: reg-imm narrow forms, memory loads/stores, abs32 forms,
//! MOV-to/from debug registers, MOVNTI non-temporal stores. Grouped as
//! everything named `mov*` — every variant of moving data between registers,
//! memory, and immediates.

mod mov_abs32;
mod mov_dr_dispatch;
mod mov_mem_abs_disp32;
mod mov_mem_narrow;
mod mov_mem_narrow_store;
mod mov_ms_x64_args;
mod mov_narrow;
mod mov_q_mmio_atomic_1315;
mod mov_r32_load;
mod movnti_store;
mod movzx_movsx_reg_1329;
