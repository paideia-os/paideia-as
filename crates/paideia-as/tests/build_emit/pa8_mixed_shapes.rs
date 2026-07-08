/// PA8-m1-002: Multi-shape contiguity test.
/// Tests that functions using different emit shapes (different main_id multipliers)
/// still produce correct contiguous st_value and st_size across all shapes.
use object::{Object, ObjectSection, ObjectSymbol};
use std::env;
use std::process::Command;

#[test]
fn build_emit_pa8_mixed_shapes() {
    let temp_dir = env::temp_dir().join("pa8_mixed_shapes_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    // Fixture: Functions using different emit shapes:
    // - identity (node*2)
    // - bitwise-not (node*3)
    // - identity again (node*2)
    // This exercises the record_lambda_entry mechanism with different main_id multipliers.
    let source = "module Pa8mixedshapes = structure { \
        let id1 : (u64) -> u64 = fn (x: u64) -> x ; \
        let not1 : (u64) -> u64 = fn (x: u64) -> ~x ; \
        let id2 : (u64) -> u64 = fn (x: u64) -> x ; \
        let not2 : (u64) -> u64 = fn (x: u64) -> ~x \
    }";

    let source_file = temp_dir.join("Pa8mixedshapes.pdx");
    std::fs::write(&source_file, source).expect("write source file");

    // Build the source to object file using paideia-as.
    let output_obj = temp_dir.join("Pa8mixedshapes.o");
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run")
        .arg("--quiet")
        .arg("--")
        .arg("build")
        .arg(source_file.to_str().unwrap())
        .arg("--emit")
        .arg("elf64")
        .arg("-o")
        .arg(output_obj.to_str().unwrap());
    cmd.env("NO_COLOR", "1");
    let out = cmd.output().expect("paideia-as build");

    assert!(
        out.status.success(),
        "paideia-as build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(output_obj.exists(), "output .o not created");

    // Parse the object file.
    let obj_data = std::fs::read(&output_obj).expect("read .o");
    let obj = object::File::parse(&*obj_data).expect("parse .o");

    // Find .text size.
    let mut text_size = 0u64;
    for section in obj.sections() {
        if section.name().unwrap_or("") == ".text" {
            text_size = section.size();
            break;
        }
    }
    assert!(text_size > 0, ".text section must exist and have size > 0");

    // Collect symbols in order.
    let mut symbols = vec![];
    for symbol in obj.symbols() {
        if let Ok(name) = symbol.name() {
            if name == "id1" || name == "not1" || name == "id2" || name == "not2" {
                symbols.push((
                    name.to_string(),
                    symbol.address() as u32,
                    symbol.size() as u32,
                ));
            }
        }
    }

    assert_eq!(symbols.len(), 4, "expected 4 function symbols");

    // Assertions:
    // 1. All have size > 0
    for (name, addr, size) in &symbols {
        assert!(
            *size > 0,
            "symbol '{}' at {:#x} has st_size=0 (regression)",
            name,
            addr
        );
    }

    // 2. Verify contiguity and correct ordering.
    for i in 0..symbols.len() - 1 {
        let (name_i, addr_i, size_i) = symbols[i].clone();
        let (name_next, addr_next, _) = symbols[i + 1].clone();
        assert!(
            addr_i + size_i == addr_next,
            "symbols '{}' and '{}' not contiguous: {} + {} != {}",
            name_i,
            name_next,
            addr_i,
            size_i,
            addr_next
        );
    }

    // 3. Last symbol extends to .text end.
    if let Some((name, addr, size)) = symbols.last() {
        assert_eq!(
            *addr as u64 + *size as u64,
            text_size,
            "last symbol '{}' doesn't extend to .text end: {:#x} + {} != {:#x}",
            name,
            addr,
            size,
            text_size
        );
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}
