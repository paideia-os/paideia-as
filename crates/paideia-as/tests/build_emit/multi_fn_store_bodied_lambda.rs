//! Regression fixture for #1168: Store-bodied lambda first_instr fix.
//!
//! Commit 3a384a6 fixed emit_visit_lambda.rs: Store-bodied lambdas now pass
//! body_id (not lambda_node_id) to record_lambda_entry, matching the id
//! emit_inst uses. This fixture verifies the fix across multiple Store-bodied
//! lambdas in a single module.
//!
//! Fixture: three lambdas in one module:
//!   - identity (non-Store): `fn (x: u64) -> x` — pure passthrough
//!   - set_first (Store): `fn (v: u64) -> counter1 = v` — writes counter1
//!   - set_second (Store): `fn (v: u64) -> counter2 = v` — writes counter2
//!
//! Expected .text (20 bytes total):
//!   - identity (4 bytes): `48 89 f8 c3` (mov rdi→rax; ret)
//!   - set_first (8 bytes): `48 89 3d 00 00 00 00 c3` (mov rdi→[rip+0]; ret)
//!   - set_second (8 bytes): `48 89 3d 00 00 00 00 c3` (mov rdi→[rip+0]; ret)
//!
//! Expected symbols:
//!   - identity: st_value=0, st_size=4
//!   - set_first: st_value=4, st_size=8
//!   - set_second: st_value=12, st_size=8
//!
//! Ground-truth verification: scan .text for `48 89 3d` (mov [rip+disp32], rdi)
//! 3-byte prefix; assert found positions = [4, 12] (set_first, set_second only).
//! If #3a384a6 were reverted, the elf.rs offset_map lookup would no-op on
//! lambda_node_id (wrong key), st_value would fall back to estimator, and this
//! assertion catches any divergence.

use object::{Object, ObjectSymbol};

use crate::common::elf::{assert_elf64_magic, text_bytes};
use crate::common::fixture::build_emit;
use crate::common::harness::run_build;

#[test]
fn store_bodied_lambdas_st_value_matches_encoder_ground_truth() {
    let out = run_build(build_emit("multi_fn_store_bodied_lambda.pdx"));
    out.assert_ok();

    let bytes = out.artifact_bytes();
    assert_elf64_magic(&bytes);

    let text = text_bytes(&bytes);
    // paideia-as#1276 phase 3: each fn grows by 8 bytes (prologue 4 +
    // epilogue 4). identity: 4→12, set_first: 8→16, set_second: 8→16.
    // Total: 12 + 16 + 16 = 44 bytes.
    assert_eq!(
        text.len(),
        44,
        ".text should be exactly 44 bytes (identity: 12, set_first: 16, set_second: 16); got {}",
        text.len()
    );

    // Assertion 1: Byte-exact .text matches expected sequence with frame prologue/epilogue.
    let expected = vec![
        // identity (12 bytes): push rbp; mov rbp,rsp; mov rax,rdi; mov rsp,rbp; pop rbp; ret
        0x55, 0x48, 0x89, 0xE5,
        0x48, 0x89, 0xf8,
        0x48, 0x89, 0xEC, 0x5D, 0xc3,
        // set_first (16 bytes): push rbp; mov rbp,rsp; mov [rip+0], rdi; mov rsp,rbp; pop rbp; ret
        0x55, 0x48, 0x89, 0xE5,
        0x48, 0x89, 0x3d, 0x00, 0x00, 0x00, 0x00,
        0x48, 0x89, 0xEC, 0x5D, 0xc3,
        // set_second (16 bytes)
        0x55, 0x48, 0x89, 0xE5,
        0x48, 0x89, 0x3d, 0x00, 0x00, 0x00, 0x00,
        0x48, 0x89, 0xEC, 0x5D, 0xc3,
    ];

    if text != expected {
        eprintln!(
            "MISMATCH: .text bytes do not match expected\n\
             Expected: {:02X?}\n\
             Got:      {:02X?}",
            expected, text
        );
        panic!(".text section mismatch");
    }
    eprintln!("✓ Assertion 1: .text bytes match expected sequence");

    // Parse ELF and verify symbols
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Assertion 2-4: Check st_value and st_size for each symbol
    let mut identity_found = false;
    let mut set_first_found = false;
    let mut set_second_found = false;

    eprintln!("=== Symbol table ===");
    for symbol in file.symbols() {
        if let Ok(name) = symbol.name() {
            let st_value = symbol.address() as usize;
            let st_size = symbol.size() as usize;
            eprintln!("  {}: st_value={}, st_size={}", name, st_value, st_size);

            match name {
                "identity" => {
                    assert_eq!(st_value, 0, "identity st_value must be 0");
                    assert_eq!(st_size, 12, "identity st_size must be 12 (paideia-as#1276: prologue+body+epilogue)");
                    identity_found = true;
                }
                "set_first" => {
                    assert_eq!(st_value, 12, "set_first st_value must be 12 (after identity's 12 bytes)");
                    assert_eq!(st_size, 16, "set_first st_size must be 16 (paideia-as#1276: prologue+body+epilogue)");
                    set_first_found = true;
                }
                "set_second" => {
                    assert_eq!(st_value, 28, "set_second st_value must be 28 (after identity + set_first: 12+16)");
                    assert_eq!(st_size, 16, "set_second st_size must be 16 (paideia-as#1276: prologue+body+epilogue)");
                    set_second_found = true;
                }
                _ => {}
            }
        }
    }

    assert!(identity_found, "Symbol identity not found");
    assert!(set_first_found, "Symbol set_first not found");
    assert!(set_second_found, "Symbol set_second not found");
    eprintln!(
        "✓ Assertions 2-4: identity (0/4), set_first (4/8), set_second (12/8) match expected"
    );

    // Assertion 5: Ground-truth cross-check — scan .text for mov [rip+disp32], rdi
    // 3-byte prefix: 48 89 3d
    eprintln!("=== Ground-truth scan: 48 89 3d pattern ===");
    let mut mov_positions = Vec::new();
    for i in 0..text.len().saturating_sub(2) {
        if text[i] == 0x48 && text[i + 1] == 0x89 && text[i + 2] == 0x3d {
            mov_positions.push(i);
            eprintln!("Found 48 89 3d at offset {}", i);
        }
    }

    // paideia-as#1276 phase 3: mov [rip+...] offsets shifted by prologue:
    //   set_first at fn base (12) + prologue (4) = 16
    //   set_second at fn base (28) + prologue (4) = 32
    assert_eq!(
        mov_positions, vec![16, 32],
        "Ground-truth scan: expected mov [rip+disp32] at offsets [16, 32], found {:?}",
        mov_positions
    );
    eprintln!("✓ Assertion 5: Ground-truth scan found mov opcodes at [16, 32]");
}
