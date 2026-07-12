use super::harness::{Poison, RetTy};

pub fn synthesize_driver_c(entry: &str, ret_ty: RetTy, poison: &[Poison]) -> String {
    let ret_type_str = match ret_ty {
        RetTy::I32 => "int",
        RetTy::I64 => "long long",
    };

    let mut src = String::new();

    // Extern declaration
    src.push_str(&format!("extern {} {}(void);\n", ret_type_str, entry));
    src.push_str("int main(void) {\n");

    // Poison registers via inline asm
    if !poison.is_empty() {
        src.push_str("    __asm__ volatile (\n");
        for p in poison {
            src.push_str(&format!("        \"movabsq $0x{:x}, %%{}\\n\"\n", p.value, p.reg.name()));
        }
        src.push_str("        :: : ");
        let clobbers: Vec<String> = poison.iter().map(|p| format!("\"{}\"", p.reg.name())).collect();
        src.push_str(&clobbers.join(", "));
        src.push_str("\n    );\n");
    }

    // Call entry and capture result
    src.push_str(&format!("    {} v = {}();\n", ret_type_str, entry));
    src.push_str("    return (int)v;\n");
    src.push_str("}\n");

    src
}
