use super::super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn returns_42() {
    let poison = [
        Poison { reg: Reg::Rdi, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rsi, value: 0xDEADBEEF02 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF03 },
        Poison { reg: Reg::Rax, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "option_u64_some_extract.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 42,
    };
    run_and_verify(&case);
}
