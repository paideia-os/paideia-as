//! Arithmetic instruction encoders: ADC/SBB, IMUL (two- and three-operand),
//! and DEC/INC. Grouped because they share the "operate on registers or
//! memory, produce flags" family of encodings.

mod adc_reg_imm;
mod adc_sbb;
mod cmp_narrow;
mod dec_r64;
mod imul_real;
mod imul_three_op;
