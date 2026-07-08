//! Control-flow encoders: indirect CALL, long JMP (segment:offset),
//! SETcc (conditional byte-set — the SETcc family is bundled here since
//! it consumes flag state produced by comparisons that drive branches),
//! and UD2 (undefined-instruction trap).

mod call_indirect;
mod ljmp_two_operand;
mod setcc_reg8;
mod ud2;
