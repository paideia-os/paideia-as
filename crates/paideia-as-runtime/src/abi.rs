//! Minimal ABI register constants for use in instruction definitions.
//!
//! This module provides only the register ID constants needed by
//! the instruction module. It does not include the full ABI mapping logic
//! (which lives in paideia-as-ir and may depend on std).

use crate::instruction::RegId;

/// SysV integer-return register. First return slot for values that fit.
pub const RAX: RegId = RegId(0);
/// Second scratch caller-saved.
pub const RCX: RegId = RegId(1);
/// SysV arg-2. Second scratch after RAX in the SCRATCH pool.
pub const RDX: RegId = RegId(2);
/// Callee-saved; base for locals; not used by the emitter directly.
pub const RBX: RegId = RegId(3);
