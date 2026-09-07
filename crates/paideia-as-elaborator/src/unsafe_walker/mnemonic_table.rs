//! Table-driven mnemonic resolver for x86_64 instructions.
//! Split out of `unsafe_walker.rs` (paideia-as #1403).
//!
//! Maps canonical mnemonic spellings (case-insensitive) to IR Mnemonic variants.
//! Covers the Phase 3 m2-001 + Phase 5 m2-001 combined set (30+ mnemonics).
//!
//! Canonical spellings for payload variants:
//! - Jcc: `je` (Eq), `jne` (Ne), `jl` (Lt), `jle` (Le), `jg` (Gt), `jge` (Ge),
//!   `jb` (Below), `jbe` (BelowOrEqual), `ja` (Above), `jae` (AboveOrEqual),
//!   `jz` (Zero), `jnz` (NonZero), `js` (Sign), `jns` (NotSign),
//!   `jo` (Overflow), `jno` (NotOverflow)
//! - MovCr: `mov_cr` (write=true), `mov_from_cr` (write=false)
//! - MovDr: `mov_dr` (write=true), `mov_from_dr` (write=false)
//! - In: `in_al` (width=1), `in_ax` (width=2), `in_eax` (width=4)
//! - Out: `out_al` (width=1), `out_ax` (width=2), `out_eax` (width=4)

use paideia_as_ir::instruction::{Cond, IntWidth, Mnemonic};

const MNEMONIC_TABLE: &[(&str, Mnemonic)] = &[
    // Phase 3 m2-001: original 10 mnemonics
    ("mov", Mnemonic::Mov),
    ("add", Mnemonic::Add),
    ("sub", Mnemonic::Sub),
    ("cmp", Mnemonic::Cmp),
    ("jmp", Mnemonic::Jmp),
    ("call", Mnemonic::Call),
    ("ret", Mnemonic::Ret),
    ("rep_movsb", Mnemonic::RepMovsb),
    ("lea", Mnemonic::Lea),
    ("nop", Mnemonic::Nop),
    // Phase 5 m2-001: 20 privileged + system-ISA mnemonics
    ("lgdt", Mnemonic::Lgdt),
    ("lidt", Mnemonic::Lidt),
    ("wrmsr", Mnemonic::Wrmsr),
    ("rdmsr", Mnemonic::Rdmsr),
    ("iret", Mnemonic::Iret),
    ("iretq", Mnemonic::Iretq),
    ("sysret", Mnemonic::Sysret),
    ("syscall", Mnemonic::Syscall),
    ("swapgs", Mnemonic::Swapgs),
    ("cpuid", Mnemonic::Cpuid),
    ("cli", Mnemonic::Cli),
    ("cld", Mnemonic::Cld),
    ("sti", Mnemonic::Sti),
    ("std", Mnemonic::Std),
    ("hlt", Mnemonic::Hlt),
    ("ud2", Mnemonic::Ud2),
    ("endbr64", Mnemonic::Endbr64),
    ("endbr32", Mnemonic::Endbr32),
    ("rep_stosq", Mnemonic::RepStosq),
    ("rep_stosb", Mnemonic::RepStosb),
    ("rep_movsq", Mnemonic::RepMovsq),
    ("farjmp", Mnemonic::FarJmp),
    ("ljmp", Mnemonic::FarJmp), // PA10-006h: ljmp alias for farjmp (two-operand form)
    // Jcc (conditional jump) variants (16 forms)
    ("je", Mnemonic::Jcc(Cond::Eq)),
    ("jne", Mnemonic::Jcc(Cond::Ne)),
    ("jl", Mnemonic::Jcc(Cond::Lt)),
    ("jle", Mnemonic::Jcc(Cond::Le)),
    ("jg", Mnemonic::Jcc(Cond::Gt)),
    ("jge", Mnemonic::Jcc(Cond::Ge)),
    ("jb", Mnemonic::Jcc(Cond::Below)),
    ("jbe", Mnemonic::Jcc(Cond::BelowOrEqual)),
    ("ja", Mnemonic::Jcc(Cond::Above)),
    ("jae", Mnemonic::Jcc(Cond::AboveOrEqual)),
    ("jz", Mnemonic::Jcc(Cond::Zero)),
    ("jnz", Mnemonic::Jcc(Cond::NonZero)),
    ("js", Mnemonic::Jcc(Cond::Sign)),
    ("jns", Mnemonic::Jcc(Cond::NotSign)),
    ("jo", Mnemonic::Jcc(Cond::Overflow)),
    ("jno", Mnemonic::Jcc(Cond::NotOverflow)),
    // Setcc (conditional set byte) variants (16 primary + ~12 aliases)
    ("seto", Mnemonic::Setcc(Cond::Overflow)),
    ("setno", Mnemonic::Setcc(Cond::NotOverflow)),
    ("setb", Mnemonic::Setcc(Cond::Below)),
    ("setnb", Mnemonic::Setcc(Cond::AboveOrEqual)),
    ("setc", Mnemonic::Setcc(Cond::Below)),       // alias for setb
    ("setnc", Mnemonic::Setcc(Cond::AboveOrEqual)), // alias for setnb
    ("setnae", Mnemonic::Setcc(Cond::Below)),     // alias for setb
    ("setae", Mnemonic::Setcc(Cond::AboveOrEqual)), // alias for setnb
    ("sete", Mnemonic::Setcc(Cond::Eq)),
    ("setz", Mnemonic::Setcc(Cond::Zero)),        // alias for sete
    ("setne", Mnemonic::Setcc(Cond::Ne)),
    ("setnz", Mnemonic::Setcc(Cond::NonZero)),    // alias for setne
    ("setbe", Mnemonic::Setcc(Cond::BelowOrEqual)),
    ("setna", Mnemonic::Setcc(Cond::BelowOrEqual)), // alias for setbe
    ("seta", Mnemonic::Setcc(Cond::Above)),
    ("setnbe", Mnemonic::Setcc(Cond::Above)),     // alias for seta
    ("sets", Mnemonic::Setcc(Cond::Sign)),
    ("setns", Mnemonic::Setcc(Cond::NotSign)),
    ("setp", Mnemonic::Setcc(Cond::Parity)),
    ("setpe", Mnemonic::Setcc(Cond::Parity)),     // alias for setp
    ("setnp", Mnemonic::Setcc(Cond::NotParity)),
    ("setpo", Mnemonic::Setcc(Cond::NotParity)),  // alias for setnp
    ("setl", Mnemonic::Setcc(Cond::Lt)),
    ("setnge", Mnemonic::Setcc(Cond::Lt)),        // alias for setl
    ("setge", Mnemonic::Setcc(Cond::Ge)),
    ("setnl", Mnemonic::Setcc(Cond::Ge)),         // alias for setge
    ("setle", Mnemonic::Setcc(Cond::Le)),
    ("setng", Mnemonic::Setcc(Cond::Le)),         // alias for setle
    ("setg", Mnemonic::Setcc(Cond::Gt)),
    ("setnle", Mnemonic::Setcc(Cond::Gt)),        // alias for setg
    // MovCr (control register move) variants (2 forms)
    ("mov_cr", Mnemonic::MovCr { write: true }),
    ("mov_from_cr", Mnemonic::MovCr { write: false }),
    // MovDr (debug register move) variants (2 forms)
    ("mov_dr", Mnemonic::MovDr { write: true }),
    ("mov_from_dr", Mnemonic::MovDr { write: false }),
    // In (I/O port read) variants (3 forms)
    ("in_al", Mnemonic::In { width: 1 }),
    ("in_ax", Mnemonic::In { width: 2 }),
    ("in_eax", Mnemonic::In { width: 4 }),
    // Out (I/O port write) variants (3 forms)
    ("out_al", Mnemonic::Out { width: 1 }),
    ("out_ax", Mnemonic::Out { width: 2 }),
    ("out_eax", Mnemonic::Out { width: 4 }),
    // Note: Int (software interrupt) uses int3 as canonical (see resolve_mnemonic)
    // Phase 8 m5-001: Additional supervisor mnemonics
    ("invlpg", Mnemonic::Invlpg),
    // v0.21-009-followup (#1297): INVPCID mnemonic — unblocks TlbOps::invpcid_*.
    ("invpcid", Mnemonic::Invpcid),
    ("rdtsc", Mnemonic::Rdtsc),
    // Phase 10 m2-001 (PA10-006b): Bitwise operation mnemonics
    ("and", Mnemonic::And),
    ("or", Mnemonic::Or),
    ("xor", Mnemonic::Xor),
    ("shl", Mnemonic::Shl),
    ("shr", Mnemonic::Shr),
    ("sar", Mnemonic::Sar),
    ("rol_d", Mnemonic::Rol { width: IntWidth::W32 }),
    ("rol_q", Mnemonic::Rol { width: IntWidth::W64 }),
    ("ror_d", Mnemonic::Ror { width: IntWidth::W32 }),
    ("ror_q", Mnemonic::Ror { width: IntWidth::W64 }),
    ("imul", Mnemonic::Imul),
    // paideia-as#1398: unsigned wide multiply (`mul r64` — REX.W F7 /4).
    // Complements Imul (signed low-64) and Div/Idiv (128÷64) for wide-integer
    // software emulation (postui#43 Fixed64 32×32-split multiply).
    ("mul", Mnemonic::Mul),
    // Phase R9 m2-001 (PA-R9-001): Push/pop instructions
    ("push", Mnemonic::Push),
    ("pop", Mnemonic::Pop),
    // Phase R9 m2-002 (PA-R9-002): Pushfq/Popfq instructions
    ("pushfq", Mnemonic::Pushfq),
    ("popfq", Mnemonic::Popfq),
    // Phase R9 m2-003 (PA-R9-003): Int3 instruction
    ("int3", Mnemonic::Int3),
    // Phase R11 PA-R11-006 (issue #909): Div/Idiv r64 instructions
    ("div", Mnemonic::Div),
    ("idiv", Mnemonic::Idiv),
    // Phase R13 PA-R13-001 (issue #914): Ltr (load task register) r16
    ("ltr", Mnemonic::Ltr),
    // Phase R13 PA-R13-003 (issue #916): xchg r/m64, r64
    ("xchg", Mnemonic::Xchg),
    // Phase R13 PA-R13-004 (issue #917): lock cmpxchg r/m64, r64
    ("lock_cmpxchg", Mnemonic::LockCmpxchg),
    // Phase R16 PA-R16-003 (issue #969): lock cmpxchg r/m32, r32
    ("lock_cmpxchg_d", Mnemonic::LockCmpxchg32),
    // Phase R16 PA-R16-004 (issue #970): lock cmpxchg16b m128
    ("lock_cmpxchg16b", Mnemonic::LockCmpxchg16b),
    // Phase R13 PA-R13-005 (issue #918): mfence
    ("mfence", Mnemonic::Mfence),
    // Phase R14 PA-R14-004 (issue #947): sfence/lfence
    ("sfence", Mnemonic::Sfence),
    ("lfence", Mnemonic::Lfence),
    // Phase R16 PA-R16-007 (issue #973): pause spinloop hint
    ("pause", Mnemonic::Pause),
    // Phase R14 PA-R14-005 (issue #948): wbinvd/invd/clflush/clflushopt
    ("wbinvd", Mnemonic::Wbinvd),
    ("invd", Mnemonic::Invd),
    ("clflush", Mnemonic::Clflush),
    ("clflushopt", Mnemonic::Clflushopt),
    // Phase R13 PA-R13-007 (issue #920): fxsave/fxrstor
    ("fxsave", Mnemonic::Fxsave),
    ("fxrstor", Mnemonic::Fxrstor),
    // Phase R15 PA-R15-m4-005 (issue #1022): xsaveopt/xrstor
    ("xsaveopt", Mnemonic::Xsaveopt),
    ("xrstor", Mnemonic::Xrstor),
    // v0.21-015 (paideia-as#1294): XCR0 read/write (paideia-os R21.M1 #826)
    ("xgetbv", Mnemonic::Xgetbv),
    ("xsetbv", Mnemonic::Xsetbv),
    // Phase R13 PA-R13-005 (issue #934): inc/dec r64
    ("inc", Mnemonic::Inc),
    ("dec", Mnemonic::Dec),
    // Phase R30 (issue #1311): not r64 — one's complement. Mnemonic::Not,
    // encode_not and its dispatch arm all predate this row; only the
    // source-level name was missing, which made a fully-encoded instruction
    // unreachable from .pdx. ACPI 6.5 §19.6 Not/Nand/Nor need it.
    ("not", Mnemonic::Not),
    // Phase R13 PA-R13-014 (issue #943): bswap r64
    ("bswap", Mnemonic::Bswap),
    // Phase R15 PA-R15-001 (issue #956): bswap r32
    ("bswap_d", Mnemonic::Bswap32),
    // Phase R14 PA-R14-001 (issue #944): narrow-width mov [mem], imm
    ("mov_b", Mnemonic::MovSized { width: IntWidth::W8 }),
    ("mov_w", Mnemonic::MovSized { width: IntWidth::W16 }),
    ("mov_d", Mnemonic::MovSized { width: IntWidth::W32 }),
    ("mov_q", Mnemonic::MovSized { width: IntWidth::W64 }),
    // Phase R14 PA-R14-003 (issue #946): non-temporal store movnti [mem], r32/r64
    ("movnti_d", Mnemonic::Movnti { width: IntWidth::W32 }),
    ("movnti_q", Mnemonic::Movnti { width: IntWidth::W64 }),
    // Phase R15 PA-R15-002 (issue #957): lock xadd [mem], r32/r64
    ("lock_xadd_d", Mnemonic::LockXadd { width: IntWidth::W32 }),
    ("lock_xadd_q", Mnemonic::LockXadd { width: IntWidth::W64 }),
    // Phase R15 PA-R15-003 (issue #958): lock add [mem], imm/r32/r64
    ("lock_add_d", Mnemonic::LockAdd { width: IntWidth::W32 }),
    ("lock_add_q", Mnemonic::LockAdd { width: IntWidth::W64 }),
    // Phase R15 PA-R15-003 (issue #958): lock sub [mem], imm/r32/r64
    ("lock_sub_d", Mnemonic::LockSub { width: IntWidth::W32 }),
    ("lock_sub_q", Mnemonic::LockSub { width: IntWidth::W64 }),
    // Phase R15 PA-R15-005 (issue #960): adc/sbb with carry
    ("adc_d", Mnemonic::Adc { width: IntWidth::W32 }),
    ("adc_q", Mnemonic::Adc { width: IntWidth::W64 }),
    ("sbb_d", Mnemonic::Sbb { width: IntWidth::W32 }),
    ("sbb_q", Mnemonic::Sbb { width: IntWidth::W64 }),
    // Phase R15 PA-R15-006 (issue #961): popcnt population count
    ("popcnt_d", Mnemonic::Popcnt { width: IntWidth::W32 }),
    ("popcnt_q", Mnemonic::Popcnt { width: IntWidth::W64 }),
    // Phase R15 PA-R15-006 (issue #1005): crc32 checksum (W64 only)
    ("crc32_q", Mnemonic::Crc32 { width: IntWidth::W64 }),
    // Phase R16 PA-R16-008 (issue #974): bit scan and trailing-zero count (W64 only)
    ("bsf_q", Mnemonic::Bsf { width: IntWidth::W64 }),
    ("bsr_q", Mnemonic::Bsr { width: IntWidth::W64 }),
    ("tzcnt_q", Mnemonic::Tzcnt { width: IntWidth::W64 }),
    // Phase R16 PA-R16-001 (issue #967): bit test operations
    ("bt_d", Mnemonic::Bt { width: IntWidth::W32 }),
    ("bt_q", Mnemonic::Bt { width: IntWidth::W64 }),
    ("bts_d", Mnemonic::Bts { width: IntWidth::W32 }),
    ("bts_q", Mnemonic::Bts { width: IntWidth::W64 }),
    ("btr_d", Mnemonic::Btr { width: IntWidth::W32 }),
    ("btr_q", Mnemonic::Btr { width: IntWidth::W64 }),
    ("btc_d", Mnemonic::Btc { width: IntWidth::W32 }),
    ("btc_q", Mnemonic::Btc { width: IntWidth::W64 }),
    // Phase R16 PA-R16-002 (issue #968): lock bit test operations (W64 only)
    ("lock_bts_q", Mnemonic::LockBts { width: IntWidth::W64 }),
    ("lock_btr_q", Mnemonic::LockBtr { width: IntWidth::W64 }),
    ("lock_btc_q", Mnemonic::LockBtc { width: IntWidth::W64 }),
    // Phase R16 PA-R16-006 (issue #972): lock bitwise operations (W64 only)
    ("lock_and_q", Mnemonic::LockAnd { width: IntWidth::W64 }),
    ("lock_or_q",  Mnemonic::LockOr  { width: IntWidth::W64 }),
    ("lock_xor_q", Mnemonic::LockXor { width: IntWidth::W64 }),
    // Phase R14 PA-R14-006 (issue #949): prefetch instructions
    ("prefetchnta", Mnemonic::Prefetchnta),
    ("prefetcht0", Mnemonic::Prefetcht0),
    ("prefetcht1", Mnemonic::Prefetcht1),
    ("prefetcht2", Mnemonic::Prefetcht2),
    // v0.21-016 (paideia-as#1295, paideia-os R21.M2 #832): AVX2 mnemonic
    // parser wiring. #1004 landed the encoder + IR primitives in v0.18 but
    // never wired the string spellings into the elaborator, so no .pdx
    // source could actually emit them. The two Vmovdqu forms differ only
    // in their `is_store: bool` variant field, which can't be inferred
    // from the mnemonic string alone — hence two distinct spellings.
    //   vmovdqu_ld : (ymm dst, [mem] src) OR (ymm dst, ymm src)  — VEX.256 F3 0F 6F /r
    //   vmovdqu_st : ([mem] dst, ymm src)                        — VEX.256 F3 0F 7F /r
    // The three-operand VEX-encoded AVX2 arithmetic mnemonics remain in
    // their single canonical spelling (no is_store distinction).
    ("vmovdqu_ld", Mnemonic::Vmovdqu { is_store: false }),
    ("vmovdqu_st", Mnemonic::Vmovdqu { is_store: true }),
    ("vpxor", Mnemonic::Vpxor),
    ("vpcmpeqb", Mnemonic::Vpcmpeqb),
    ("vpmovmskb", Mnemonic::Vpmovmskb),
    // Phase R68 (paideia-os #1861, paideia-as #1329): movzx/movsx
    // reg-to-reg mnemonics. Mnemonic::Movzx/Movsx and their encoders
    // (encode_movzx/encode_movsx in paideia-as-encoder) have existed since
    // Phase 13 m6-001 for field-access lowering, and stdlib_lowering
    // constructs them directly — but no MNEMONIC_TABLE row ever wired the
    // canonical `movzx`/`movsx` spellings into the unsafe-block parser, so
    // no .pdx source could spell them directly. Same "fully-encoded but
    // unreachable from .pdx" gap as `not` (#1311) above; mkfs-pdxb's
    // decimal-parse loop (main.pdx) is the first source to hit it.
    ("movzx", Mnemonic::Movzx),
    ("movsx", Mnemonic::Movsx),
    // paideia-os #1333, paideia-as#1333: scalar SSE float mnemonics
    // (register-register only; memory-operand forms deferred). Encoder side
    // lives in encode_sse.rs. movd/movq are direction-ambiguous (bitcast
    // load into xmm vs. store from xmm) so — mirroring the vmovdqu_ld/
    // vmovdqu_st precedent above — each gets two spellings.
    ("movsd", Mnemonic::MovSd),
    ("movss", Mnemonic::MovSs),
    ("addsd", Mnemonic::AddSd),
    ("addss", Mnemonic::AddSs),
    ("subsd", Mnemonic::SubSd),
    ("subss", Mnemonic::SubSs),
    ("mulsd", Mnemonic::MulSd),
    ("mulss", Mnemonic::MulSs),
    ("divsd", Mnemonic::DivSd),
    ("divss", Mnemonic::DivSs),
    ("sqrtsd", Mnemonic::Sqrtsd),
    ("sqrtss", Mnemonic::Sqrtss),
    ("ucomisd", Mnemonic::Ucomisd),
    ("ucomiss", Mnemonic::Ucomiss),
    ("comisd", Mnemonic::Comisd),
    ("comiss", Mnemonic::Comiss),
    ("cvtsi2sd", Mnemonic::Cvtsi2sd),
    ("cvtsi2ss", Mnemonic::Cvtsi2ss),
    ("cvttsd2si", Mnemonic::Cvttsd2si),
    ("cvttss2si", Mnemonic::Cvttss2si),
    ("movd_ld", Mnemonic::MovdBitcast { to_xmm: true }),
    ("movd_st", Mnemonic::MovdBitcast { to_xmm: false }),
    ("movq_ld", Mnemonic::MovqBitcast { to_xmm: true }),
    ("movq_st", Mnemonic::MovqBitcast { to_xmm: false }),
];

/// Resolve a mnemonic name to an IR Mnemonic enum variant.
///
/// Performs case-insensitive lookup against the MNEMONIC_TABLE.
/// Returns `Some(Mnemonic)` if found, `None` if the name is unknown.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(resolve_mnemonic("mov"), Some(Mnemonic::Mov));
/// assert_eq!(resolve_mnemonic("MOV"), Some(Mnemonic::Mov));  // case-insensitive
/// assert_eq!(resolve_mnemonic("je"), Some(Mnemonic::Jcc(Cond::Eq)));
/// assert_eq!(resolve_mnemonic("mov_cr"), Some(Mnemonic::MovCr { write: true }));
/// assert_eq!(resolve_mnemonic("in_al"), Some(Mnemonic::In { width: 1 }));
/// assert_eq!(resolve_mnemonic("not_a_mnemonic"), None);
/// ```
#[must_use]
pub fn resolve_mnemonic(name: &str) -> Option<Mnemonic> {
    let lower_name = name.to_lowercase();

    // Table lookup with case-insensitive ASCII lowercase
    // (includes int3 → Mnemonic::Int3 from MNEMONIC_TABLE)
    for (mnem_name, mnem) in MNEMONIC_TABLE {
        if mnem_name.eq_ignore_ascii_case(&lower_name) {
            return Some(*mnem);
        }
    }

    None
}
