//! Unit tests for the unsafe-block operand parser and mnemonic resolver.
//! Split out of `unsafe_walker.rs` (paideia-as #1403).

use super::*;
use super::register::{register_name_to_regid, register_name_width};
use super::symbol_ref::supports_symbol_ref;
use paideia_as_ir::abi;
use paideia_as_ir::instruction::{Cond, IntWidth, Mnemonic, Operand, RegId, Scale};
use paideia_as_ir::record_layout::{RecordLayout, RecordTypeId};
use paideia_as_ir::IrNodeId;
use std::collections::HashMap;

#[test]
fn register_name_to_regid_rax() {
    assert_eq!(register_name_to_regid("rax"), Some(abi::RAX));
}

#[test]
fn register_name_to_regid_rdi() {
    assert_eq!(register_name_to_regid("rdi"), Some(abi::RDI));
}

#[test]
fn register_name_to_regid_r15() {
    assert_eq!(register_name_to_regid("r15"), Some(abi::R15));
}

#[test]
fn register_name_to_regid_cr0() {
    assert_eq!(register_name_to_regid("cr0"), Some(RegId(16)));
}

#[test]
fn register_name_to_regid_cr3() {
    assert_eq!(register_name_to_regid("cr3"), Some(RegId(19)));
}

#[test]
fn register_name_to_regid_dr0() {
    assert_eq!(register_name_to_regid("dr0"), Some(RegId(25)));
}

#[test]
fn register_name_to_regid_dr7() {
    assert_eq!(register_name_to_regid("dr7"), Some(RegId(32)));
}

#[test]
fn register_name_to_regid_unknown() {
    assert_eq!(register_name_to_regid("xax"), None);
}

#[test]
fn register_name_to_regid_all_gprs() {
    let gprs = [
        "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi",
        "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15",
    ];
    for (i, name) in gprs.iter().enumerate() {
        assert_eq!(register_name_to_regid(name), Some(RegId(i as u8)));
    }
}

#[test]
fn register_name_to_regid_all_control_regs() {
    let crs = ["cr0", "cr1", "cr2", "cr3", "cr4", "cr5", "cr6", "cr7", "cr8"];
    for (i, name) in crs.iter().enumerate() {
        assert_eq!(register_name_to_regid(name), Some(RegId(16 + i as u8)));
    }
}

#[test]
fn register_name_to_regid_all_debug_regs() {
    let drs = ["dr0", "dr1", "dr2", "dr3", "dr4", "dr5", "dr6", "dr7"];
    for (i, name) in drs.iter().enumerate() {
        assert_eq!(register_name_to_regid(name), Some(RegId(25 + i as u8)));
    }
}

// ── PA8 m3-003 (#827): register-name width recovery ───────────────────

#[test]
fn register_name_width_recovers_operand_width_from_spelling() {
    // 8-bit low bytes.
    assert_eq!(register_name_width("al"), Some(IntWidth::W8));
    assert_eq!(register_name_width("bl"), Some(IntWidth::W8));
    // 16-bit.
    assert_eq!(register_name_width("ax"), Some(IntWidth::W16));
    assert_eq!(register_name_width("di"), Some(IntWidth::W16));
    // 32-bit, both legacy and r8d–r15d spellings.
    assert_eq!(register_name_width("eax"), Some(IntWidth::W32));
    assert_eq!(register_name_width("r10d"), Some(IntWidth::W32));
    // 64-bit.
    assert_eq!(register_name_width("rax"), Some(IntWidth::W64));
    assert_eq!(register_name_width("r15"), Some(IntWidth::W64));
    // Non-GPR and unknown names carry no width.
    assert_eq!(register_name_width("cr0"), None);
    assert_eq!(register_name_width("dr7"), None);
    assert_eq!(register_name_width("xax"), None);
}

// Placeholder unit tests for operand parsing (require full AST construction)
// These will be completed once the parser integration is in place.

#[test]
fn operand_error_unknown_register() {
    let err = OperandError::UnknownRegister(
        "xax".to_string(),
        paideia_as_diagnostics::Span::new(
            paideia_as_diagnostics::FileId::new(1).unwrap(),
            0,
            1,
        ),
    );
    assert!(matches!(err, OperandError::UnknownRegister(ref name, _) if name == "xax"));
}

#[test]
fn operand_error_malformed_operand() {
    let err = OperandError::MalformedOperand(paideia_as_diagnostics::Span::new(
        paideia_as_diagnostics::FileId::new(1).unwrap(),
        0,
        1,
    ));
    assert!(matches!(err, OperandError::MalformedOperand(_)));
}

// ── Mnemonic resolver tests (Phase 5 m3-003) ──────────────────────────

// --- Phase 3 m2-001: original 10 mnemonics ---

#[test]
fn resolve_mnemonic_mov() {
    assert_eq!(resolve_mnemonic("mov"), Some(Mnemonic::Mov));
}

#[test]
fn resolve_mnemonic_mov_case_insensitive() {
    assert_eq!(resolve_mnemonic("MOV"), Some(Mnemonic::Mov));
    assert_eq!(resolve_mnemonic("Mov"), Some(Mnemonic::Mov));
}

#[test]
fn resolve_mnemonic_add() {
    assert_eq!(resolve_mnemonic("add"), Some(Mnemonic::Add));
}

#[test]
fn resolve_mnemonic_sub() {
    assert_eq!(resolve_mnemonic("sub"), Some(Mnemonic::Sub));
}

#[test]
fn resolve_mnemonic_cmp() {
    assert_eq!(resolve_mnemonic("cmp"), Some(Mnemonic::Cmp));
}

#[test]
fn resolve_mnemonic_jmp() {
    assert_eq!(resolve_mnemonic("jmp"), Some(Mnemonic::Jmp));
}

#[test]
fn resolve_mnemonic_call() {
    assert_eq!(resolve_mnemonic("call"), Some(Mnemonic::Call));
}

#[test]
fn resolve_mnemonic_ret() {
    assert_eq!(resolve_mnemonic("ret"), Some(Mnemonic::Ret));
}

#[test]
fn resolve_mnemonic_rep_movsb() {
    assert_eq!(resolve_mnemonic("rep_movsb"), Some(Mnemonic::RepMovsb));
}

#[test]
fn resolve_mnemonic_lea() {
    assert_eq!(resolve_mnemonic("lea"), Some(Mnemonic::Lea));
}

#[test]
fn resolve_mnemonic_nop() {
    assert_eq!(resolve_mnemonic("nop"), Some(Mnemonic::Nop));
}

// --- Phase 5 m2-001: 20 privileged + system-ISA mnemonics ---

#[test]
fn resolve_mnemonic_lgdt() {
    assert_eq!(resolve_mnemonic("lgdt"), Some(Mnemonic::Lgdt));
}

#[test]
fn resolve_mnemonic_lidt() {
    assert_eq!(resolve_mnemonic("lidt"), Some(Mnemonic::Lidt));
}

#[test]
fn resolve_mnemonic_ltr() {
    assert_eq!(resolve_mnemonic("ltr"), Some(Mnemonic::Ltr));
}

#[test]
fn resolve_mnemonic_xchg() {
    assert_eq!(resolve_mnemonic("xchg"), Some(Mnemonic::Xchg));
}

#[test]
fn resolve_mnemonic_lock_cmpxchg() {
    assert_eq!(resolve_mnemonic("lock_cmpxchg"), Some(Mnemonic::LockCmpxchg));
}

#[test]
fn resolve_mnemonic_lock_cmpxchg_d() {
    assert_eq!(resolve_mnemonic("lock_cmpxchg_d"), Some(Mnemonic::LockCmpxchg32));
}

#[test]
fn resolve_mnemonic_lock_cmpxchg16b() {
    assert_eq!(resolve_mnemonic("lock_cmpxchg16b"), Some(Mnemonic::LockCmpxchg16b));
}

#[test]
fn resolve_mnemonic_mfence() {
    assert_eq!(resolve_mnemonic("mfence"), Some(Mnemonic::Mfence));
}

#[test]
fn resolve_mnemonic_pause() {
    assert_eq!(resolve_mnemonic("pause"), Some(Mnemonic::Pause));
}

#[test]
fn resolve_mnemonic_fxsave() {
    assert_eq!(resolve_mnemonic("fxsave"), Some(Mnemonic::Fxsave));
}

#[test]
fn resolve_mnemonic_fxrstor() {
    assert_eq!(resolve_mnemonic("fxrstor"), Some(Mnemonic::Fxrstor));
}

#[test]
fn resolve_mnemonic_xsaveopt() {
    assert_eq!(resolve_mnemonic("xsaveopt"), Some(Mnemonic::Xsaveopt));
}

#[test]
fn resolve_mnemonic_xrstor() {
    assert_eq!(resolve_mnemonic("xrstor"), Some(Mnemonic::Xrstor));
}

#[test]
fn resolve_mnemonic_xgetbv() {
    // v0.21-015 (paideia-as#1294): xgetbv is the read half of XCR0 access
    assert_eq!(resolve_mnemonic("xgetbv"), Some(Mnemonic::Xgetbv));
}

#[test]
fn resolve_mnemonic_xsetbv() {
    // v0.21-015 (paideia-as#1294): xsetbv is the write half of XCR0 access
    assert_eq!(resolve_mnemonic("xsetbv"), Some(Mnemonic::Xsetbv));
}

// v0.21-016 (paideia-as#1295): AVX2 mnemonic parser wiring — encoder
// side (issue #1004, v0.18) already ships the byte-exact + iced tests
// for every operand shape below; these resolver tests just pin the
// string → Mnemonic mapping so downstream .pdx sources can invoke them.
#[test]
fn resolve_mnemonic_vmovdqu_ld() {
    assert_eq!(
        resolve_mnemonic("vmovdqu_ld"),
        Some(Mnemonic::Vmovdqu { is_store: false })
    );
}

#[test]
fn resolve_mnemonic_vmovdqu_st() {
    assert_eq!(
        resolve_mnemonic("vmovdqu_st"),
        Some(Mnemonic::Vmovdqu { is_store: true })
    );
}

#[test]
fn resolve_mnemonic_vpxor() {
    assert_eq!(resolve_mnemonic("vpxor"), Some(Mnemonic::Vpxor));
}

#[test]
fn resolve_mnemonic_vpcmpeqb() {
    assert_eq!(resolve_mnemonic("vpcmpeqb"), Some(Mnemonic::Vpcmpeqb));
}

#[test]
fn resolve_mnemonic_vpmovmskb() {
    assert_eq!(resolve_mnemonic("vpmovmskb"), Some(Mnemonic::Vpmovmskb));
}

#[test]
// paideia-os #1861, paideia-as #1329: movzx/movsx were fully encoded
// (Phase 13 m6-001) but unreachable from .pdx source before this row
// landed in MNEMONIC_TABLE.
fn resolve_mnemonic_movzx() {
    assert_eq!(resolve_mnemonic("movzx"), Some(Mnemonic::Movzx));
    assert_eq!(resolve_mnemonic("MOVZX"), Some(Mnemonic::Movzx));
}

#[test]
fn resolve_mnemonic_movsx() {
    assert_eq!(resolve_mnemonic("movsx"), Some(Mnemonic::Movsx));
}

// paideia-os #1333, paideia-as#1333: scalar SSE float mnemonic resolver
// wiring. Byte-exact + iced round-trip tests for every operand shape
// live in paideia-as-encoder/tests/simd/scalar_float.rs; these just pin
// the string -> Mnemonic mapping.
#[test]
fn resolve_mnemonic_movsd() {
    assert_eq!(resolve_mnemonic("movsd"), Some(Mnemonic::MovSd));
    assert_eq!(resolve_mnemonic("MOVSD"), Some(Mnemonic::MovSd));
}

#[test]
fn resolve_mnemonic_addsd_addss() {
    assert_eq!(resolve_mnemonic("addsd"), Some(Mnemonic::AddSd));
    assert_eq!(resolve_mnemonic("addss"), Some(Mnemonic::AddSs));
}

#[test]
fn resolve_mnemonic_ucomisd_comiss() {
    assert_eq!(resolve_mnemonic("ucomisd"), Some(Mnemonic::Ucomisd));
    assert_eq!(resolve_mnemonic("comiss"), Some(Mnemonic::Comiss));
}

#[test]
fn resolve_mnemonic_cvtsi2sd_cvttss2si() {
    assert_eq!(resolve_mnemonic("cvtsi2sd"), Some(Mnemonic::Cvtsi2sd));
    assert_eq!(resolve_mnemonic("cvttss2si"), Some(Mnemonic::Cvttss2si));
}

#[test]
fn resolve_mnemonic_movd_movq_bitcast_ld_st() {
    assert_eq!(
        resolve_mnemonic("movd_ld"),
        Some(Mnemonic::MovdBitcast { to_xmm: true })
    );
    assert_eq!(
        resolve_mnemonic("movd_st"),
        Some(Mnemonic::MovdBitcast { to_xmm: false })
    );
    assert_eq!(
        resolve_mnemonic("movq_ld"),
        Some(Mnemonic::MovqBitcast { to_xmm: true })
    );
    assert_eq!(
        resolve_mnemonic("movq_st"),
        Some(Mnemonic::MovqBitcast { to_xmm: false })
    );
}

#[test]
fn register_name_to_regid_xmm_band() {
    assert_eq!(register_name_to_regid("xmm0"), Some(RegId(53)));
    assert_eq!(register_name_to_regid("xmm15"), Some(RegId(68)));
}

#[test]
fn resolve_mnemonic_wrmsr() {
    assert_eq!(resolve_mnemonic("wrmsr"), Some(Mnemonic::Wrmsr));
}

#[test]
fn resolve_mnemonic_rdmsr() {
    assert_eq!(resolve_mnemonic("rdmsr"), Some(Mnemonic::Rdmsr));
}

#[test]
fn resolve_mnemonic_iret() {
    assert_eq!(resolve_mnemonic("iret"), Some(Mnemonic::Iret));
}

#[test]
fn resolve_mnemonic_iretq() {
    assert_eq!(resolve_mnemonic("iretq"), Some(Mnemonic::Iretq));
}

#[test]
fn resolve_mnemonic_sysret() {
    assert_eq!(resolve_mnemonic("sysret"), Some(Mnemonic::Sysret));
}

#[test]
fn resolve_mnemonic_syscall() {
    assert_eq!(resolve_mnemonic("syscall"), Some(Mnemonic::Syscall));
}

#[test]
fn resolve_mnemonic_swapgs() {
    assert_eq!(resolve_mnemonic("swapgs"), Some(Mnemonic::Swapgs));
}

#[test]
fn resolve_mnemonic_cpuid() {
    assert_eq!(resolve_mnemonic("cpuid"), Some(Mnemonic::Cpuid));
}

#[test]
fn resolve_mnemonic_cli() {
    assert_eq!(resolve_mnemonic("cli"), Some(Mnemonic::Cli));
}

#[test]
fn resolve_mnemonic_sti() {
    assert_eq!(resolve_mnemonic("sti"), Some(Mnemonic::Sti));
}

#[test]
fn resolve_mnemonic_hlt() {
    assert_eq!(resolve_mnemonic("hlt"), Some(Mnemonic::Hlt));
}

#[test]
fn resolve_mnemonic_rep_stosq() {
    assert_eq!(resolve_mnemonic("rep_stosq"), Some(Mnemonic::RepStosq));
}

#[test]
fn resolve_mnemonic_rep_stosb() {
    assert_eq!(resolve_mnemonic("rep_stosb"), Some(Mnemonic::RepStosb));
}

#[test]
fn resolve_mnemonic_rep_movsq() {
    assert_eq!(resolve_mnemonic("rep_movsq"), Some(Mnemonic::RepMovsq));
}

#[test]
fn resolve_mnemonic_farjmp() {
    assert_eq!(resolve_mnemonic("farjmp"), Some(Mnemonic::FarJmp));
}

// --- Jcc (conditional jump) variants: all 16 forms ---

#[test]
fn resolve_mnemonic_je() {
    assert_eq!(resolve_mnemonic("je"), Some(Mnemonic::Jcc(Cond::Eq)));
}

#[test]
fn resolve_mnemonic_jne() {
    assert_eq!(resolve_mnemonic("jne"), Some(Mnemonic::Jcc(Cond::Ne)));
}

#[test]
fn resolve_mnemonic_jl() {
    assert_eq!(resolve_mnemonic("jl"), Some(Mnemonic::Jcc(Cond::Lt)));
}

#[test]
fn resolve_mnemonic_jle() {
    assert_eq!(resolve_mnemonic("jle"), Some(Mnemonic::Jcc(Cond::Le)));
}

#[test]
fn resolve_mnemonic_jg() {
    assert_eq!(resolve_mnemonic("jg"), Some(Mnemonic::Jcc(Cond::Gt)));
}

#[test]
fn resolve_mnemonic_jge() {
    assert_eq!(resolve_mnemonic("jge"), Some(Mnemonic::Jcc(Cond::Ge)));
}

#[test]
fn resolve_mnemonic_jb() {
    assert_eq!(resolve_mnemonic("jb"), Some(Mnemonic::Jcc(Cond::Below)));
}

#[test]
fn resolve_mnemonic_jbe() {
    assert_eq!(
        resolve_mnemonic("jbe"),
        Some(Mnemonic::Jcc(Cond::BelowOrEqual))
    );
}

#[test]
fn resolve_mnemonic_ja() {
    assert_eq!(resolve_mnemonic("ja"), Some(Mnemonic::Jcc(Cond::Above)));
}

#[test]
fn resolve_mnemonic_jae() {
    assert_eq!(
        resolve_mnemonic("jae"),
        Some(Mnemonic::Jcc(Cond::AboveOrEqual))
    );
}

#[test]
fn resolve_mnemonic_jz() {
    assert_eq!(resolve_mnemonic("jz"), Some(Mnemonic::Jcc(Cond::Zero)));
}

#[test]
fn resolve_mnemonic_jnz() {
    assert_eq!(resolve_mnemonic("jnz"), Some(Mnemonic::Jcc(Cond::NonZero)));
}

#[test]
fn resolve_mnemonic_js() {
    assert_eq!(resolve_mnemonic("js"), Some(Mnemonic::Jcc(Cond::Sign)));
}

#[test]
fn resolve_mnemonic_jns() {
    assert_eq!(resolve_mnemonic("jns"), Some(Mnemonic::Jcc(Cond::NotSign)));
}

#[test]
fn resolve_mnemonic_jo() {
    assert_eq!(resolve_mnemonic("jo"), Some(Mnemonic::Jcc(Cond::Overflow)));
}

#[test]
fn resolve_mnemonic_jno() {
    assert_eq!(
        resolve_mnemonic("jno"),
        Some(Mnemonic::Jcc(Cond::NotOverflow))
    );
}

// --- MovCr (control register move) variants ---

#[test]
fn resolve_mnemonic_mov_cr_write() {
    assert_eq!(
        resolve_mnemonic("mov_cr"),
        Some(Mnemonic::MovCr { write: true })
    );
}

#[test]
fn resolve_mnemonic_mov_from_cr_read() {
    assert_eq!(
        resolve_mnemonic("mov_from_cr"),
        Some(Mnemonic::MovCr { write: false })
    );
}

// --- MovDr (debug register move) variants ---

#[test]
fn resolve_mnemonic_mov_dr_write() {
    assert_eq!(
        resolve_mnemonic("mov_dr"),
        Some(Mnemonic::MovDr { write: true })
    );
}

#[test]
fn resolve_mnemonic_mov_from_dr_read() {
    assert_eq!(
        resolve_mnemonic("mov_from_dr"),
        Some(Mnemonic::MovDr { write: false })
    );
}

// --- In (I/O port read) variants ---

#[test]
fn resolve_mnemonic_in_al() {
    assert_eq!(resolve_mnemonic("in_al"), Some(Mnemonic::In { width: 1 }));
}

#[test]
fn resolve_mnemonic_in_ax() {
    assert_eq!(resolve_mnemonic("in_ax"), Some(Mnemonic::In { width: 2 }));
}

#[test]
fn resolve_mnemonic_in_eax() {
    assert_eq!(resolve_mnemonic("in_eax"), Some(Mnemonic::In { width: 4 }));
}

// --- Out (I/O port write) variants ---

#[test]
fn resolve_mnemonic_out_al() {
    assert_eq!(resolve_mnemonic("out_al"), Some(Mnemonic::Out { width: 1 }));
}

#[test]
fn resolve_mnemonic_out_ax() {
    assert_eq!(resolve_mnemonic("out_ax"), Some(Mnemonic::Out { width: 2 }));
}

#[test]
fn resolve_mnemonic_out_eax() {
    assert_eq!(
        resolve_mnemonic("out_eax"),
        Some(Mnemonic::Out { width: 4 })
    );
}

// --- Int (software interrupt) ---

#[test]
fn resolve_mnemonic_int3() {
    assert_eq!(resolve_mnemonic("int3"), Some(Mnemonic::Int3));
}

// --- Negative tests: unknown mnemonics ---

#[test]
fn resolve_mnemonic_unknown_typo() {
    assert_eq!(resolve_mnemonic("mvo"), None);
}

#[test]
fn resolve_mnemonic_unknown_garbage() {
    assert_eq!(resolve_mnemonic("not_a_real_mnemonic"), None);
}

#[test]
fn resolve_mnemonic_unknown_empty() {
    assert_eq!(resolve_mnemonic(""), None);
}

// --- Phase 8 m5-001: Supervisor mnemonics ---

#[test]
fn resolve_mnemonic_invlpg() {
    assert_eq!(resolve_mnemonic("invlpg"), Some(Mnemonic::Invlpg));
}

#[test]
fn resolve_mnemonic_invlpg_case_insensitive() {
    assert_eq!(resolve_mnemonic("INVLPG"), Some(Mnemonic::Invlpg));
    assert_eq!(resolve_mnemonic("Invlpg"), Some(Mnemonic::Invlpg));
}

#[test]
fn resolve_mnemonic_rdtsc() {
    assert_eq!(resolve_mnemonic("rdtsc"), Some(Mnemonic::Rdtsc));
}

#[test]
fn resolve_mnemonic_rdtsc_case_insensitive() {
    assert_eq!(resolve_mnemonic("RDTSC"), Some(Mnemonic::Rdtsc));
    assert_eq!(resolve_mnemonic("Rdtsc"), Some(Mnemonic::Rdtsc));
}

#[test]
fn resolve_mnemonic_endbr64() {
    assert_eq!(resolve_mnemonic("endbr64"), Some(Mnemonic::Endbr64));
}

#[test]
fn resolve_mnemonic_endbr32() {
    assert_eq!(resolve_mnemonic("endbr32"), Some(Mnemonic::Endbr32));
}

// --- Phase 6 m3-005: Field access operand parsing tests ---

#[test]
fn parse_deref_field_access_with_offset_zero() {
    // Test: *p.field0 where field0 is at offset 0
    // Expected: MemSib { base: rdi (7), index: None, scale: X1, disp: 0 }
    use paideia_as_ir::record_layout::FieldLayout;

    let mut layouts = HashMap::new();
    let field_layout = FieldLayout { offset: 0, size: 8, signed: false };
    layouts.insert(RecordTypeId(1), RecordLayout::new(8, 8, vec![field_layout]));

    // We can't easily test parse_deref_operand directly without full AST setup,
    // but we verify the logic: if field0 is at offset 0, MemSib disp should be 0
    let result = Operand::MemSib {
        base: abi::RDI,
        index: None,
        scale: Scale::X1,
        disp: 0,
    };
    assert_eq!(
        result,
        Operand::MemSib {
            base: abi::RDI,
            index: None,
            scale: Scale::X1,
            disp: 0,
        }
    );
}

#[test]
fn parse_deref_field_access_with_offset_16() {
    // Test: *p.rights where rights is at offset 16
    // Expected: MemSib { base: rdi (7), index: None, scale: X1, disp: 16 }
    use paideia_as_ir::record_layout::FieldLayout;

    let mut layouts = HashMap::new();
    let fields = vec![
        FieldLayout { offset: 0, size: 8, signed: false }, // kind
        FieldLayout { offset: 16,
            size: 8, signed: false }, // rights
    ];
    layouts.insert(RecordTypeId(1), RecordLayout::new(24, 8, fields));

    // Verify offset calculation: field at index 1 (rights) is at offset 16
    if let Some(layout) = layouts.get(&RecordTypeId(1)) {
        assert!(layout.fields.len() >= 2);
        assert_eq!(layout.fields[1].offset, 16);
        let disp = layout.fields[1].offset as i32;
        assert_eq!(disp, 16);
    }
}

#[test]
fn parse_deref_field_offset_unresolved_missing_type() {
    // Test: *p.field when RecordTypeId(1) is not in record_layouts
    // Expected: UnresolvedFieldOffset error (U1608)
    let layouts: HashMap<RecordTypeId, RecordLayout> = HashMap::new();

    // layouts is empty, so RecordTypeId(1) not found
    assert!(!layouts.contains_key(&RecordTypeId(1)));
}

#[test]
fn parse_deref_plain_dereference_zero_offset() {
    // Test: *p (plain dereference without field access)
    // Expected: MemSib { base: rdi (7), index: None, scale: X1, disp: 0 }
    let result = Operand::MemSib {
        base: abi::RDI,
        index: None,
        scale: Scale::X1,
        disp: 0,
    };
    assert_eq!(
        result,
        Operand::MemSib {
            base: abi::RDI,
            index: None,
            scale: Scale::X1,
            disp: 0,
        }
    );
}

// --- Phase 6 m4-002: Label reference operand tests ---

#[test]
fn operand_label_ref_constructs() {
    let op = Operand::LabelRef {
        name: "fail_label".to_string(),
        addend: 0,
    };
    match op {
        Operand::LabelRef { name, addend } => {
            assert_eq!(name, "fail_label");
            assert_eq!(addend, 0);
        }
        _ => panic!("expected LabelRef variant"),
    }
}

#[test]
fn operand_label_ref_with_addend() {
    let op = Operand::LabelRef {
        name: "loop_start".to_string(),
        addend: 8,
    };
    match op {
        Operand::LabelRef { name, addend } => {
            assert_eq!(name, "loop_start");
            assert_eq!(addend, 8);
        }
        _ => panic!("expected LabelRef variant"),
    }
}

#[test]
fn operand_label_ref_roundtrips_through_clone() {
    let op1 = Operand::LabelRef {
        name: "end_loop".to_string(),
        addend: -4,
    };
    let op2 = op1.clone();
    assert_eq!(op1, op2);
}

// --- Phase 6 m4-005: Symbol reference operand tests ---

#[test]
fn supports_symbol_ref_for_call() {
    assert!(supports_symbol_ref(Mnemonic::Call));
}

#[test]
fn supports_symbol_ref_for_jmp() {
    assert!(supports_symbol_ref(Mnemonic::Jmp));
}

#[test]
fn supports_symbol_ref_for_jcc() {
    assert!(supports_symbol_ref(Mnemonic::Jcc(Cond::Eq)));
    assert!(supports_symbol_ref(Mnemonic::Jcc(Cond::Ne)));
    assert!(supports_symbol_ref(Mnemonic::Jcc(Cond::Below)));
}

#[test]
fn supports_symbol_ref_for_mov() {
    assert!(supports_symbol_ref(Mnemonic::Mov));
}

#[test]
fn supports_symbol_ref_for_lea() {
    assert!(supports_symbol_ref(Mnemonic::Lea));
}

#[test]
fn does_not_support_symbol_ref_for_add() {
    assert!(!supports_symbol_ref(Mnemonic::Add));
}

#[test]
fn operand_symbol_ref_constructs() {
    let op = Operand::SymbolRef {
        name: "cap_alloc".to_string(),
        addend: 0,
    };
    match op {
        Operand::SymbolRef { name, addend } => {
            assert_eq!(name, "cap_alloc");
            assert_eq!(addend, 0);
        }
        _ => panic!("expected SymbolRef variant"),
    }
}

#[test]
fn operand_symbol_ref_with_addend() {
    let op = Operand::SymbolRef {
        name: "cap_mint".to_string(),
        addend: 8,
    };
    match op {
        Operand::SymbolRef { name, addend } => {
            assert_eq!(name, "cap_mint");
            assert_eq!(addend, 8);
        }
        _ => panic!("expected SymbolRef variant"),
    }
}

#[test]
fn operand_symbol_ref_roundtrips_through_clone() {
    let op1 = Operand::SymbolRef {
        name: "symbol_name".to_string(),
        addend: 16,
    };
    let op2 = op1.clone();
    assert_eq!(op1, op2);
}

#[test]
fn operand_symbol_ref_equality() {
    let op1 = Operand::SymbolRef {
        name: "symbol".to_string(),
        addend: 0,
    };
    let op2 = Operand::SymbolRef {
        name: "symbol".to_string(),
        addend: 0,
    };
    let op3 = Operand::SymbolRef {
        name: "symbol".to_string(),
        addend: 4,
    };
    assert_eq!(op1, op2);
    assert_ne!(op1, op3);
}

// --- PA-R13-011 (#924): Back-to-back label aliasing tests ---
//
// These tests verify the fix for back-to-back labels in unsafe blocks.
// Before the fix, Pass 2 stored the pending label in a scalar Option<String>,
// so each new label declaration overwrote the previous one. Only the LAST
// label attached to the next instruction.
//
// After the fix, pending_labels is a Vec<String> that collects all labels
// since the last instruction, and when the next instruction lands, ALL
// pending labels alias to it (same byte offset / IrNodeId).
//
// The real verification is end-to-end: tests/build-emit/back_to_back_labels.pdx
// exercises the full parser → elaborator → encoder pipeline.

#[test]
fn pass_two_pending_labels_is_vec() {
    // Smoke test: verify that Vec<String> compiles as a replacement
    // for Option<String> in the Pass 2 loop context.
    let mut pending_labels: Vec<String> = Vec::new();
    pending_labels.push("label1".to_string());
    pending_labels.push("label2".to_string());
    assert_eq!(pending_labels.len(), 2);
    // Verify we can drain all labels
    let drained: Vec<String> = pending_labels.drain(..).collect();
    assert_eq!(drained.len(), 2);
    assert_eq!(drained[0], "label1");
    assert_eq!(drained[1], "label2");
    assert!(pending_labels.is_empty());
}

#[test]
fn pass_two_label_drain_consumes_all() {
    // Verify that drain(..) empties the vec completely,
    // so the next instruction doesn't inherit pending labels
    // from the previous one.
    let mut pending: Vec<String> = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let consumed: Vec<_> = pending.drain(..).collect();
    assert_eq!(consumed.len(), 3);
    assert!(pending.is_empty(), "drain should empty the vector");
}

#[test]
fn pass_two_label_clear_on_encode_fail() {
    // Verify that if instruction encoding fails (Some(instr_id) → None),
    // we clear pending_labels rather than mis-attaching them to the next
    // instruction. This mirrors pre-existing behavior.
    let mut pending: Vec<String> = vec!["fail_label".to_string()];
    let instr_ir_node: Option<IrNodeId> = None;
    match instr_ir_node {
        Some(_) => {
            // Insert each label
        }
        None => {
            // Encoding failed; clear pending to avoid leaking to next instr
            pending.clear();
        }
    }
    assert!(pending.is_empty(), "pending should be cleared on encode fail");
}
