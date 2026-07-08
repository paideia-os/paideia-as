//! Flag-manipulation and TEST instructions: CLD/STD (direction flag),
//! TEST r64,r64 and TEST r64,imm32 forms. Grouped as the "read/write
//! flags without altering the value" family.

mod cld_std;
mod test_reg;
mod test_reg_imm;
