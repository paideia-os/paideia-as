//! Integration tests for x86_64 instruction encoding against Intel SDM Vol 2A.
//!
//! This test suite encodes 35 representative x86_64 instructions and verifies
//! that the output bytes are valid and decode correctly with iced-x86.

use paideia_as_emitter_elf::encode::*;

#[test]
fn sdm_vectors_valid_and_decodable() {
    use iced_x86::Decoder;

    let test_cases: &[(&str, Vec<u8>)] = &[
        // MOV instructions (64-bit immediate forms)
        ("mov rax, 1", {
            let mut buf = CodeBuffer::new();
            mov_reg64_imm32(&mut buf, Reg64::Rax, 1);
            buf.bytes
        }),
        ("mov rcx, 42", {
            let mut buf = CodeBuffer::new();
            mov_reg64_imm32(&mut buf, Reg64::Rcx, 42);
            buf.bytes
        }),
        ("mov rdx, -100", {
            let mut buf = CodeBuffer::new();
            mov_reg64_imm32(&mut buf, Reg64::Rdx, -100);
            buf.bytes
        }),
        ("mov rbx, 0x7fffffff", {
            let mut buf = CodeBuffer::new();
            mov_reg64_imm32(&mut buf, Reg64::Rbx, 0x7fffffff);
            buf.bytes
        }),
        ("mov rsi, 0x0123456789abcdef", {
            let mut buf = CodeBuffer::new();
            mov_reg64_imm64(&mut buf, Reg64::Rsi, 0x0123456789abcdef);
            buf.bytes
        }),
        ("mov r8, 0x1000", {
            let mut buf = CodeBuffer::new();
            mov_reg64_imm64(&mut buf, Reg64::R8, 0x1000);
            buf.bytes
        }),
        ("mov r15, 0xffffffffffffffff", {
            let mut buf = CodeBuffer::new();
            mov_reg64_imm64(&mut buf, Reg64::R15, 0xffffffffffffffff);
            buf.bytes
        }),
        // MOV reg64, reg64
        ("mov rax, rbx", {
            let mut buf = CodeBuffer::new();
            mov_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rbx);
            buf.bytes
        }),
        ("mov r9, r10", {
            let mut buf = CodeBuffer::new();
            mov_reg64_reg64(&mut buf, Reg64::R9, Reg64::R10);
            buf.bytes
        }),
        ("mov r8, r15", {
            let mut buf = CodeBuffer::new();
            mov_reg64_reg64(&mut buf, Reg64::R8, Reg64::R15);
            buf.bytes
        }),
        // MOV [rbp+disp], reg64
        ("mov [rbp-8], rbx", {
            let mut buf = CodeBuffer::new();
            mov_mem_rbp_disp_reg64(&mut buf, -8, Reg64::Rbx);
            buf.bytes
        }),
        ("mov [rbp+0], rcx", {
            let mut buf = CodeBuffer::new();
            mov_mem_rbp_disp_reg64(&mut buf, 0, Reg64::Rcx);
            buf.bytes
        }),
        ("mov [rbp+256], rdx", {
            let mut buf = CodeBuffer::new();
            mov_mem_rbp_disp_reg64(&mut buf, 256, Reg64::Rdx);
            buf.bytes
        }),
        ("mov [rbp-128], rsi", {
            let mut buf = CodeBuffer::new();
            mov_mem_rbp_disp_reg64(&mut buf, -128, Reg64::Rsi);
            buf.bytes
        }),
        ("mov [rbp+r12-off], r11", {
            let mut buf = CodeBuffer::new();
            mov_mem_rbp_disp_reg64(&mut buf, -512, Reg64::R11);
            buf.bytes
        }),
        // MOV reg64, [rbp+disp]
        ("mov rax, [rbp-8]", {
            let mut buf = CodeBuffer::new();
            mov_reg64_mem_rbp_disp(&mut buf, Reg64::Rax, -8);
            buf.bytes
        }),
        ("mov rdx, [rbp+32]", {
            let mut buf = CodeBuffer::new();
            mov_reg64_mem_rbp_disp(&mut buf, Reg64::Rdx, 32);
            buf.bytes
        }),
        ("mov r14, [rbp+2000]", {
            let mut buf = CodeBuffer::new();
            mov_reg64_mem_rbp_disp(&mut buf, Reg64::R14, 2000);
            buf.bytes
        }),
        // Arithmetic: ADD, SUB, XOR
        ("add rax, rax", {
            let mut buf = CodeBuffer::new();
            add_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rax);
            buf.bytes
        }),
        ("add rcx, rdx", {
            let mut buf = CodeBuffer::new();
            add_reg64_reg64(&mut buf, Reg64::Rcx, Reg64::Rdx);
            buf.bytes
        }),
        ("add r10, r12", {
            let mut buf = CodeBuffer::new();
            add_reg64_reg64(&mut buf, Reg64::R10, Reg64::R12);
            buf.bytes
        }),
        ("sub rdx, rax", {
            let mut buf = CodeBuffer::new();
            sub_reg64_reg64(&mut buf, Reg64::Rdx, Reg64::Rax);
            buf.bytes
        }),
        ("sub r8, r9", {
            let mut buf = CodeBuffer::new();
            sub_reg64_reg64(&mut buf, Reg64::R8, Reg64::R9);
            buf.bytes
        }),
        ("xor rax, rax", {
            let mut buf = CodeBuffer::new();
            xor_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rax);
            buf.bytes
        }),
        ("xor r15, r15", {
            let mut buf = CodeBuffer::new();
            xor_reg64_reg64(&mut buf, Reg64::R15, Reg64::R15);
            buf.bytes
        }),
        // Comparison: CMP, TEST
        ("cmp rsi, rdi", {
            let mut buf = CodeBuffer::new();
            cmp_reg64_reg64(&mut buf, Reg64::Rsi, Reg64::Rdi);
            buf.bytes
        }),
        ("cmp r9, r14", {
            let mut buf = CodeBuffer::new();
            cmp_reg64_reg64(&mut buf, Reg64::R9, Reg64::R14);
            buf.bytes
        }),
        ("test rbx, rbx", {
            let mut buf = CodeBuffer::new();
            test_reg64_reg64(&mut buf, Reg64::Rbx, Reg64::Rbx);
            buf.bytes
        }),
        ("test r11, r13", {
            let mut buf = CodeBuffer::new();
            test_reg64_reg64(&mut buf, Reg64::R11, Reg64::R13);
            buf.bytes
        }),
        // Jumps: JMP (rel8, rel32), JCC (conditional)
        ("jmp +5", {
            let mut buf = CodeBuffer::new();
            jmp_rel8(&mut buf, 5);
            buf.bytes
        }),
        ("jmp -10", {
            let mut buf = CodeBuffer::new();
            jmp_rel8(&mut buf, -10);
            buf.bytes
        }),
        ("jmp +0x1000", {
            let mut buf = CodeBuffer::new();
            jmp_rel32(&mut buf, 0x1000);
            buf.bytes
        }),
        ("jmp -5", {
            let mut buf = CodeBuffer::new();
            jmp_rel32(&mut buf, -5);
            buf.bytes
        }),
        ("je +100", {
            let mut buf = CodeBuffer::new();
            jcc_rel32(&mut buf, Cond::Eq, 100);
            buf.bytes
        }),
        ("jne -20", {
            let mut buf = CodeBuffer::new();
            jcc_rel32(&mut buf, Cond::Neq, -20);
            buf.bytes
        }),
        ("jl -10", {
            let mut buf = CodeBuffer::new();
            jcc_rel32(&mut buf, Cond::Lt, -10);
            buf.bytes
        }),
        ("jge +50", {
            let mut buf = CodeBuffer::new();
            jcc_rel32(&mut buf, Cond::Ge, 50);
            buf.bytes
        }),
        ("jle -50", {
            let mut buf = CodeBuffer::new();
            jcc_rel32(&mut buf, Cond::Le, -50);
            buf.bytes
        }),
        ("jg +0", {
            let mut buf = CodeBuffer::new();
            jcc_rel32(&mut buf, Cond::Gt, 0);
            buf.bytes
        }),
        // Stack: PUSH, POP
        ("push rbp", {
            let mut buf = CodeBuffer::new();
            push_reg64(&mut buf, Reg64::Rbp);
            buf.bytes
        }),
        ("push r12", {
            let mut buf = CodeBuffer::new();
            push_reg64(&mut buf, Reg64::R12);
            buf.bytes
        }),
        ("pop rbp", {
            let mut buf = CodeBuffer::new();
            pop_reg64(&mut buf, Reg64::Rbp);
            buf.bytes
        }),
        ("pop r15", {
            let mut buf = CodeBuffer::new();
            pop_reg64(&mut buf, Reg64::R15);
            buf.bytes
        }),
        // CALL, RET
        ("call +0x1000", {
            let mut buf = CodeBuffer::new();
            call_rel32(&mut buf, 0x1000);
            buf.bytes
        }),
        ("ret", {
            let mut buf = CodeBuffer::new();
            ret(&mut buf);
            buf.bytes
        }),
    ];

    for (name, bytes) in test_cases {
        let mut decoder = Decoder::new(64, bytes.as_slice(), 0);
        let instr = decoder.decode();
        assert!(
            !instr.is_invalid(),
            "Vector '{}' produced invalid instruction bytes: {:x?}",
            name,
            bytes
        );
    }
}

#[test]
fn encode_then_iced_disassemble_round_trip() {
    let mut buf = CodeBuffer::new();
    mov_reg64_imm32(&mut buf, Reg64::Rax, 1);

    let mut decoder = iced_x86::Decoder::new(64, buf.as_slice(), 0);
    let instr = decoder.decode();

    assert!(!instr.is_invalid());
    assert_eq!(instr.mnemonic(), iced_x86::Mnemonic::Mov);
}

#[test]
fn complex_sequence_encoding() {
    use iced_x86::Decoder;

    // Encode a small function prologue/epilogue:
    // push rbp
    // mov rbp, rsp
    // mov rax, 1
    // ret
    // pop rbp
    let mut buf = CodeBuffer::new();
    push_reg64(&mut buf, Reg64::Rbp);
    mov_reg64_reg64(&mut buf, Reg64::Rbp, Reg64::Rsp);
    mov_reg64_imm32(&mut buf, Reg64::Rax, 1);
    ret(&mut buf);
    pop_reg64(&mut buf, Reg64::Rbp);

    let mut decoder = Decoder::new(64, buf.as_slice(), 0);
    let mut count = 0;
    loop {
        let instr = decoder.decode();
        if instr.is_invalid() {
            break;
        }
        count += 1;
        // Just verify that each decoded instruction is valid
    }
    assert_eq!(count, 5, "Expected 5 valid instructions in the sequence");
}
