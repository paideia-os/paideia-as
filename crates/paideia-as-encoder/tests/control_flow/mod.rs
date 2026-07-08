//! Control-flow encoders: indirect CALL, long JMP (segment:offset),
//! SETcc (conditional byte-set — the SETcc family is bundled here since
//! it consumes flag state produced by comparisons that drive branches),
//! UD2 (undefined-instruction trap), and PUSH immediate encoding.

mod call_indirect;
mod endbr;
mod ljmp_two_operand;
mod push_imm;
mod setcc_reg8;
mod ud2;
