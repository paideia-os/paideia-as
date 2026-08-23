//! v0.22.0 (#1326 phase 5): SysV stack arg -- module-level Object constant
//! at position 7 (0-indexed arg 6, first stack-passed slot) of a 9-arg
//! SysV call.
//!
//! Companion to `sysv_x64_9arg_call.rs` (all-literal args): here `SEVENTH`
//! is a module-level `Object` symbol, so the caller must materialise it
//! via a RIP-relative load into a scratch register before storing it to
//! the stack slot, rather than treating it as an unresolvable `Var`
//! (which pre-#1326-phase-2 would have fired T0521 for register args, and
//! would be the "kind not yet supported for SysV stack passing" arm for
//! stack args if phase 2 hadn't already extended the Object-const branch
//! to the stack-target case).

use iced_x86::{Decoder, DecoderOptions, Mnemonic, OpKind, Register};
use object::{Object, ObjectSection, ObjectSymbol, RelocationKind, RelocationTarget};

use crate::common::elf;
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn sysv_x64_9arg_call_object_const_compiles_cleanly() {
    let out = run_build(build_emit("sysv_x64_9arg_call_object_const.pdx"));
    out.assert_ok();
    assert!(
        !out.stderr_contains("T0521"),
        "9-arg SysV call with Object-const at pos 7 must NOT emit T0521; stderr:\n{}",
        out.stderr
    );

    let bytes = out.artifact_bytes();
    elf::assert_elf64_magic(&bytes);
}

#[test]
fn seventh_symbol_has_pc32_relocation() {
    // Mirrors module_const_arg_call.rs's relocation check: the RIP-relative
    // load of the module-level constant must carry an R_X86_64_PC32
    // relocation targeting SEVENTH.
    let out = run_build(build_emit("sysv_x64_9arg_call_object_const.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    let mut seventh_found = false;
    for sym in file.symbols() {
        if sym.name().ok() == Some("SEVENTH") {
            seventh_found = true;
        }
    }
    assert!(seventh_found, "SEVENTH symbol must exist in symbol table");

    let mut reloc_found = false;
    for section in file.sections() {
        for (_offset, relocation) in section.relocations() {
            if let RelocationTarget::Symbol(sym_idx) = relocation.target() {
                if let Ok(sym) = file.symbol_by_index(sym_idx) {
                    if sym.name().ok() == Some("SEVENTH") {
                        assert_eq!(
                            relocation.kind(),
                            RelocationKind::Relative,
                            "SEVENTH relocation must be R_X86_64_PC32 (RelocationKind::Relative)"
                        );
                        assert_eq!(
                            relocation.size(),
                            32,
                            "SEVENTH relocation must be a 32-bit displacement"
                        );
                        reloc_found = true;
                    }
                }
            }
        }
    }
    assert!(
        reloc_found,
        "at least one R_X86_64_PC32 relocation must target SEVENTH"
    );
}

#[test]
fn caller_loads_object_const_then_stores_to_stack_slot_zero_before_call() {
    // Decode-based (register-agnostic) proof: a `mov reg, [rip+disp]`
    // (the Object-const materialisation) is immediately followed -- among
    // the caller's instruction stream, before CALL -- by a
    // `mov qword ptr [rsp+0], reg` using the SAME register, establishing
    // the load->store->call ordering the design doc's #4.3 rationale
    // requires (arg 7 is the first stack slot, stack_off = 0).
    let out = run_build(build_emit("sysv_x64_9arg_call_object_const.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller = elf::symbol_bytes(&bytes, "caller_9arg_object_const")
        .expect("caller_9arg_object_const symbol missing from ELF");

    let mut decoder = Decoder::new(64, &caller, DecoderOptions::NONE);
    let insts: Vec<_> = decoder.iter().collect();

    let mut load_reg: Option<Register> = None;
    let mut load_pos: Option<usize> = None;
    let mut store_pos: Option<usize> = None;
    let mut call_pos: Option<usize> = None;

    for (i, instr) in insts.iter().enumerate() {
        if instr.mnemonic() == Mnemonic::Call && call_pos.is_none() {
            call_pos = Some(i);
        }
        if load_reg.is_none()
            && instr.mnemonic() == Mnemonic::Mov
            && instr.op0_kind() == OpKind::Register
            && instr.op1_kind() == OpKind::Memory
            && instr.is_ip_rel_memory_operand()
        {
            load_reg = Some(instr.op0_register());
            load_pos = Some(i);
        }
        if let Some(reg) = load_reg {
            if store_pos.is_none()
                && instr.mnemonic() == Mnemonic::Mov
                && instr.op0_kind() == OpKind::Memory
                && instr.memory_base() == Register::RSP
                && instr.memory_displacement64() == 0
                && instr.op1_kind() == OpKind::Register
                && instr.op1_register() == reg
            {
                store_pos = Some(i);
            }
        }
    }

    assert!(
        load_reg.is_some(),
        "expected a RIP-relative `mov reg, [rip+SEVENTH]` load in caller_9arg_object_const"
    );
    assert!(
        store_pos.is_some(),
        "expected `mov qword ptr [rsp+0], <loaded-reg>` store after the RIP-relative load"
    );

    let load_pos = load_pos.unwrap();
    let store_pos = store_pos.unwrap();
    let call_pos = call_pos.expect("caller must contain a CALL instruction");

    assert!(
        load_pos < store_pos,
        "load must precede its store: load at {}, store at {}",
        load_pos,
        store_pos
    );
    assert!(
        store_pos < call_pos,
        "store must precede CALL: store at {}, call at {}",
        store_pos,
        call_pos
    );
}

#[test]
fn caller_9arg_object_const_still_writes_flanking_literal_stack_args() {
    // arg[7]=8 at [rsp+8], arg[8]=9 at [rsp+16] must still land correctly;
    // the Object-const branch at arg[6] must not perturb the offsets of
    // its neighbours.
    let out = run_build(build_emit("sysv_x64_9arg_call_object_const.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller = elf::symbol_bytes(&bytes, "caller_9arg_object_const")
        .expect("caller_9arg_object_const symbol missing from ELF");

    let store_off8 = [0x48u8, 0xC7, 0x44, 0x24, 0x08, 0x08, 0x00, 0x00, 0x00];
    let store_off16 = [0x48u8, 0xC7, 0x44, 0x24, 0x10, 0x09, 0x00, 0x00, 0x00];

    assert!(
        caller.windows(store_off8.len()).any(|w| w == store_off8),
        "Expected `mov qword ptr [rsp+8], 8` in caller_9arg_object_const; got: {}",
        caller.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
    assert!(
        caller.windows(store_off16.len()).any(|w| w == store_off16),
        "Expected `mov qword ptr [rsp+16], 9` in caller_9arg_object_const; got: {}",
        caller.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn caller_9arg_object_const_prelude_postlude_bump_32() {
    // Same bump arithmetic as sysv_x64_9arg_call.pdx (9 args -> 3 stack
    // args -> bytes=24, pad=8 -> bump=32); the Object-const arg doesn't
    // change the stack-arg COUNT, only how one slot is populated.
    let out = run_build(build_emit("sysv_x64_9arg_call_object_const.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    let caller = elf::symbol_bytes(&bytes, "caller_9arg_object_const")
        .expect("caller_9arg_object_const symbol missing from ELF");

    let sub_rsp_32 = [0x48u8, 0x83, 0xEC, 0x20];
    let add_rsp_32 = [0x48u8, 0x83, 0xC4, 0x20];
    assert!(
        caller.windows(4).any(|w| w == sub_rsp_32),
        "Expected `sub rsp, 32` prelude in caller_9arg_object_const"
    );
    assert!(
        caller.windows(4).any(|w| w == add_rsp_32),
        "Expected `add rsp, 32` postlude in caller_9arg_object_const"
    );
}
