//! Register-name parsing and RegId lookups for the unsafe-block operand parser.
//! Split out of `unsafe_walker.rs` (2026-07-08).

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind};
use paideia_as_ir::abi;
use paideia_as_ir::instruction::{IntWidth, Operand, RegId};

use super::OperandError;

/// Parse a register operand from an Ident node or ExprPath (single-segment).
pub(super) fn parse_register_from_ident(
    ast: &AstArena,
    ident_node: NodeId,
    source_map: &paideia_as_diagnostics::SourceMap,
) -> Result<Operand, OperandError> {
    let span = ast.get(ident_node).map(|n| n.span).unwrap_or_else(|| {
        paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
    });

    // Handle both Ident and ExprPath (single-segment) node kinds
    let node = ast
        .get(ident_node)
        .ok_or(OperandError::MalformedOperand(span))?;
    let actual_ident_node = match node.kind {
        NodeKind::Ident => ident_node,
        NodeKind::ExprPath => {
            // For ExprPath, extract the first segment (should be single-segment)
            match ast.expr_data(ident_node) {
                Some(ExprData::Path { segments }) if segments.len() == 1 => segments[0],
                _ => return Err(OperandError::MalformedOperand(span)),
            }
        }
        _ => return Err(OperandError::MalformedOperand(span)),
    };

    // Extract the identifier text by looking up in the source.
    // For phase-1, we use a lookup table matching register names to RegIds.
    let reg_id = match get_register_name(ast, actual_ident_node, source_map) {
        Some(name) => register_name_to_regid(&name)
            .ok_or_else(|| OperandError::UnknownRegister(name, span))?,
        None => {
            return Err(OperandError::MalformedOperand(span));
        }
    };

    Ok(Operand::Reg(reg_id))
}

/// Get the register name from an Ident node by looking at the source text.
pub(super) fn get_register_name(
    ast: &AstArena,
    ident_node: NodeId,
    source_map: &paideia_as_diagnostics::SourceMap,
) -> Option<String> {
    // Extract the register name from the span using the source map
    let node = ast.get(ident_node)?;
    let span = node.span;

    // Look up the file content in the source map
    let file_id = span.file();
    let source = source_map.content(file_id);

    // Extract the text from the span
    let start = span.byte_start() as usize;
    let end = start + span.byte_len() as usize;
    if end <= source.len() {
        Some(source[start..end].to_string())
    } else {
        None
    }
}

/// Check if an AST node is the "rip" identifier (special x86_64 register for RIP-relative addressing).
pub(super) fn is_rip_identifier(
    ast: &AstArena,
    node_id: NodeId,
    source_map: &paideia_as_diagnostics::SourceMap,
) -> bool {
    let node = match ast.get(node_id) {
        Some(n) => n,
        None => return false,
    };

    match node.kind {
        NodeKind::Ident => {
            matches!(
                get_register_name(ast, node_id, source_map).as_deref(),
                Some("rip")
            )
        }
        NodeKind::ExprPath => match ast.expr_data(node_id) {
            Some(ExprData::Path { segments }) if segments.len() == 1 => {
                matches!(
                    get_register_name(ast, segments[0], source_map).as_deref(),
                    Some("rip")
                )
            }
            _ => false,
        },
        _ => false,
    }
}

/// Map register names to RegId values.
///
/// Encoding (fits within u8):
/// - GPR (rax–r15): 0–15 (standard x86_64)
/// - Control registers (cr0–cr8): 16–24 (compact encoding for m2-005 bridge)
/// - Debug registers (dr0–dr7): 25–32 (compact encoding for m2-005 bridge)
/// - Extended low-byte registers (spl, bpl, sil, dil): 33–36
/// - YMM vector registers (ymm0–ymm15): 37–52 (AVX2 baseline per issue #1004)
/// - XMM scalar-float registers (xmm0–xmm15): 53–68 (paideia-os #1333, paideia-as#1333)
///
/// Phase 7 m2-001 (PA7C-m2-001): Sub-registers (32-bit, 16-bit, 8-bit) are supported
/// and resolve to the same RegId as their 64-bit form. For example, "eax", "ax", and "al"
/// all resolve to abi::RAX. This maintains width-agnostic register handling; the encoder
/// is responsible for width-aware MOV dispatch (follow-up issue PA7C-m2-001a).
///
/// The bridge in m2-005 will interpret values >= 16 as special registers and
/// extract the control/debug register index accordingly.
///
/// Phase R18 PA-R18-011 (issue #1004): YMM register band 37–52 introduced for AVX2 substrate.
/// YMM registers are used in raw-asm unsafe blocks; elaborator width contract returns None
/// (consistent with control/debug registers), deferring width-checking to a future safe-mode pass.
#[must_use]
pub(super) fn register_name_to_regid(name: &str) -> Option<RegId> {
    match name {
        // General-purpose registers (64-bit)
        "rax" => Some(abi::RAX),
        "rcx" => Some(abi::RCX),
        "rdx" => Some(abi::RDX),
        "rbx" => Some(abi::RBX),
        "rsp" => Some(abi::RSP),
        "rbp" => Some(abi::RBP),
        "rsi" => Some(abi::RSI),
        "rdi" => Some(abi::RDI),
        "r8" => Some(abi::R8),
        "r9" => Some(abi::R9),
        "r10" => Some(abi::R10),
        "r11" => Some(abi::R11),
        "r12" => Some(abi::R12),
        "r13" => Some(abi::R13),
        "r14" => Some(abi::R14),
        "r15" => Some(abi::R15),

        // 32-bit sub-registers (resolve to same RegId as 64-bit form)
        "eax" => Some(abi::RAX),
        "ecx" => Some(abi::RCX),
        "edx" => Some(abi::RDX),
        "ebx" => Some(abi::RBX),
        "esp" => Some(abi::RSP),
        "ebp" => Some(abi::RBP),
        "esi" => Some(abi::RSI),
        "edi" => Some(abi::RDI),
        "r8d" => Some(abi::R8),
        "r9d" => Some(abi::R9),
        "r10d" => Some(abi::R10),
        "r11d" => Some(abi::R11),
        "r12d" => Some(abi::R12),
        "r13d" => Some(abi::R13),
        "r14d" => Some(abi::R14),
        "r15d" => Some(abi::R15),

        // 16-bit sub-registers (resolve to same RegId as 64-bit form; r8w-r15w do not exist)
        "ax" => Some(abi::RAX),
        "cx" => Some(abi::RCX),
        "dx" => Some(abi::RDX),
        "bx" => Some(abi::RBX),
        "sp" => Some(abi::RSP),
        "bp" => Some(abi::RBP),
        "si" => Some(abi::RSI),
        "di" => Some(abi::RDI),

        // 8-bit sub-registers (resolve to same RegId as 64-bit form; al-r15b, but only al-bl exist)
        "al" => Some(abi::RAX),
        "cl" => Some(abi::RCX),
        "dl" => Some(abi::RDX),
        "bl" => Some(abi::RBX),

        // High-byte sub-registers: share 64-bit RegId of low-byte sibling.
        // Distinguished from spl/bpl/sil/dil by ABSENCE of REX prefix at encode time.
        "ah" => Some(abi::RSP),
        "ch" => Some(abi::RBP),
        "dh" => Some(abi::RSI),
        "bh" => Some(abi::RDI),

        // Extended low-byte sub-registers (require REX prefix for non-rsp/rsi/rbp/rdi).
        // PA-R13-013: spl/bpl/sil/dil require REX.B encoding (compact IDs 33–36).
        "spl" => Some(RegId(33)),
        "bpl" => Some(RegId(34)),
        "sil" => Some(RegId(35)),
        "dil" => Some(RegId(36)),

        // Extended low-byte sub-registers (require REX.B).
        "r8b" => Some(abi::R8),
        "r9b" => Some(abi::R9),
        "r10b" => Some(abi::R10),
        "r11b" => Some(abi::R11),
        "r12b" => Some(abi::R12),
        "r13b" => Some(abi::R13),
        "r14b" => Some(abi::R14),
        "r15b" => Some(abi::R15),

        // Control registers (compact encoding: 16 + index)
        "cr0" => Some(RegId(16)),
        "cr1" => Some(RegId(17)),
        "cr2" => Some(RegId(18)),
        "cr3" => Some(RegId(19)),
        "cr4" => Some(RegId(20)),
        "cr5" => Some(RegId(21)),
        "cr6" => Some(RegId(22)),
        "cr7" => Some(RegId(23)),
        "cr8" => Some(RegId(24)),

        // Debug registers (compact encoding: 25 + index)
        "dr0" => Some(RegId(25)),
        "dr1" => Some(RegId(26)),
        "dr2" => Some(RegId(27)),
        "dr3" => Some(RegId(28)),
        "dr4" => Some(RegId(29)),
        "dr5" => Some(RegId(30)),
        "dr6" => Some(RegId(31)),
        "dr7" => Some(RegId(32)),

        // YMM vector registers (compact encoding: 37 + index) — Phase R18 issue #1004
        "ymm0" => Some(RegId(37)),
        "ymm1" => Some(RegId(38)),
        "ymm2" => Some(RegId(39)),
        "ymm3" => Some(RegId(40)),
        "ymm4" => Some(RegId(41)),
        "ymm5" => Some(RegId(42)),
        "ymm6" => Some(RegId(43)),
        "ymm7" => Some(RegId(44)),
        "ymm8" => Some(RegId(45)),
        "ymm9" => Some(RegId(46)),
        "ymm10" => Some(RegId(47)),
        "ymm11" => Some(RegId(48)),
        "ymm12" => Some(RegId(49)),
        "ymm13" => Some(RegId(50)),
        "ymm14" => Some(RegId(51)),
        "ymm15" => Some(RegId(52)),

        // XMM scalar-float registers (compact encoding: 53 + index) —
        // paideia-os #1333, paideia-as#1333. Kept disjoint from the YMM band
        // (37-52) even though they name the same physical register file,
        // because XMM selects the legacy-SSE (non-VEX) encoder path.
        "xmm0" => Some(RegId(53)),
        "xmm1" => Some(RegId(54)),
        "xmm2" => Some(RegId(55)),
        "xmm3" => Some(RegId(56)),
        "xmm4" => Some(RegId(57)),
        "xmm5" => Some(RegId(58)),
        "xmm6" => Some(RegId(59)),
        "xmm7" => Some(RegId(60)),
        "xmm8" => Some(RegId(61)),
        "xmm9" => Some(RegId(62)),
        "xmm10" => Some(RegId(63)),
        "xmm11" => Some(RegId(64)),
        "xmm12" => Some(RegId(65)),
        "xmm13" => Some(RegId(66)),
        "xmm14" => Some(RegId(67)),
        "xmm15" => Some(RegId(68)),

        // Special register: RIP (Instruction Pointer) for RIP-relative addressing
        // PA10-006j: Must be recognized as a register to allow fallback to parse_address_to_sib
        // when try_parse_symbol_memory doesn't match [rip + symbol] pattern.
        // Returns sentinel value 0xFF which is outside all normal register ranges.
        "rip" => Some(RegId(0xFF)),

        _ => None,
    }
}

/// Map a GPR register name to the operand width its spelling implies.
///
/// PA8 m3-003 (#827): `register_name_to_regid` collapses every sub-register
/// spelling onto its 64-bit `RegId`, so the destination width of a
/// width-distinguishing `mov` (`mov al, …` vs `mov eax, …`) is lost by the
/// time operands reach the encoder. This helper recovers that width from the
/// register *name* alone, before the collapse, so a `mov reg, imm` site can be
/// retargeted to the width-aware `Mnemonic::MovSized` encoder path.
///
/// Returns:
/// - `W8`  for the 8-bit GPR low bytes (`al`–`bl`).
/// - `W16` for the 16-bit GPRs (`ax`–`di`).
/// - `W32` for the 32-bit GPRs (`eax`–`edi`, `r8d`–`r15d`).
/// - `W64` for the 64-bit GPRs (`rax`–`r15`).
///
/// Non-GPR names (control/debug registers) and unknown names return `None`;
/// callers treat that as "no width retarget" and keep the generic `mov` path.
#[must_use]
pub(super) fn register_name_width(name: &str) -> Option<IntWidth> {
    match name {
        // 64-bit GPRs.
        "rax" | "rcx" | "rdx" | "rbx" | "rsp" | "rbp" | "rsi" | "rdi" | "r8" | "r9" | "r10"
        | "r11" | "r12" | "r13" | "r14" | "r15" => Some(IntWidth::W64),

        // 32-bit sub-registers.
        "eax" | "ecx" | "edx" | "ebx" | "esp" | "ebp" | "esi" | "edi" | "r8d" | "r9d" | "r10d"
        | "r11d" | "r12d" | "r13d" | "r14d" | "r15d" => Some(IntWidth::W32),

        // 16-bit sub-registers.
        "ax" | "cx" | "dx" | "bx" | "sp" | "bp" | "si" | "di" => Some(IntWidth::W16),

        // 8-bit sub-registers (al–bl low-byte, ah–bh high-byte, and spl–dil extended low-byte).
        "al" | "cl" | "dl" | "bl" | "ah" | "ch" | "dh" | "bh" | "spl" | "bpl" | "sil" | "dil" | "r8b"
        | "r9b" | "r10b" | "r11b" | "r12b" | "r13b" | "r14b" | "r15b" => Some(IntWidth::W8),

        _ => None,
    }
}
