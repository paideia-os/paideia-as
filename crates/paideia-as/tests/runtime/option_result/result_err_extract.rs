use super::super::harness::{run_and_verify, Poison, Reg, RetTy, RuntimeCase};

#[test]
fn returns_7() {
    let poison = [
        Poison { reg: Reg::Rdi, value: 0xDEADBEEF01 },
        Poison { reg: Reg::Rsi, value: 0xDEADBEEF02 },
        Poison { reg: Reg::Rdx, value: 0xDEADBEEF03 },
        Poison { reg: Reg::Rax, value: 0xDEADBEEF04 },
    ];
    let case = RuntimeCase {
        fixture_pdx: "result_u64_u64_err_extract.pdx",
        entry: "entry",
        ret_ty: RetTy::I64,
        poison: &poison,
        expected: 7,
    };
    run_and_verify(&case);
}
