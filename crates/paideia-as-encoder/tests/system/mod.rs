//! Supervisor / system-control encoders and cross-cutting survey
//! witnesses: LGDT abs32, mode-agnostic supervisor mnemonics, and the
//! v15 32-bit-mode instruction-survey witness. Grouped as "instructions
//! and audits scoped to the supervisor / boot-mode surface".

mod invpcid;
mod lgdt_abs32;
mod supervisor_mode_agnostic;
mod v15_survey_witness;
