//! Bitwise / bit-manipulation instructions: AND/OR/XOR, BT-family
//! (bt/bts/btr/btc), BSF/BSR/TZCNT, POPCNT, CRC32, BSWAP, ROL/ROR. Grouped as the
//! "bit-level operators" that share `0F`-escape opcode patterns and the
//! sign-extension trap logic for immediates.

mod bitwise_arith_real;
mod bsf_bsr_tzcnt;
mod bswap_r32;
mod bswap_r64;
mod bt_family;
mod crc32;
mod or_r32_imm32_signbit;
mod or_r32_imm_mode32;
mod or_r64_imm;
mod popcnt;
mod rol_ror;
