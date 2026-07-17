//! Unit tests for VEX prefix builder — Phase R18 PA-R18-011 (issue #1004).
//! These tests exercise the VEX encoding logic in isolation (not on the AC count).

use paideia_as_encoder::encode_vex::{Vex2, Vex3};

/// Test 2-byte VEX construction with no B/X bits set.
#[test]
fn vex2_construction_canonical() {
    // 2-byte VEX: C5 xx
    // pp=0x1 (66h), L=1 (256-bit), vvvv=0x0, R=1 (low reg)
    let vex = Vex2::new(true, 0x0, true, 0x1);
    // Expected: 10 0000 01 (R=1, vvvv=0, L=1, pp=1)
    assert_eq!(vex.byte, 0x85);
}

/// Test 3-byte VEX construction when B bit is high.
#[test]
fn vex3_construction_with_b_high() {
    // 3-byte VEX: C4 xx yy
    // R=1, X=1, B=0 (src2_id >= 8), map=1, W=0, vvvv=0x0, L=1, pp=0x1
    let vex = Vex3::new(true, true, false, 0x01, false, 0x0, true, 0x1);
    // byte0: R=1 (bit7=0), X=1 (bit6=0), B=0 (bit5=1), map=1
    // 0010 0001 = 0x21
    assert_eq!(vex.byte0, 0x21);
    // byte1: W=0, vvvv=0, L=1 (bit2=1), pp=1
    // 0000 0101 = 0x05
    assert_eq!(vex.byte1, 0x05);
}

/// Test 3-byte VEX construction when X bit is high.
#[test]
fn vex3_construction_with_x_high() {
    // 3-byte VEX with X=0 (index >= 8)
    let vex = Vex3::new(true, false, true, 0x01, false, 0x0, true, 0x1);
    // byte0: R=1 (bit7=0), X=0 (bit6=1), B=1 (bit5=0), map=1
    // 0100 0001 = 0x41
    assert_eq!(vex.byte0, 0x41);
}

/// Test 3-byte VEX construction with both B and X bits high.
#[test]
fn vex3_construction_with_b_and_x_high() {
    // 3-byte VEX with both R=0, X=0, B=0 (all high registers)
    let vex = Vex3::new(false, false, false, 0x01, false, 0x0, true, 0x1);
    // byte0: R=0 (bit7=1), X=0 (bit6=1), B=0 (bit5=1), map=1
    // 1110 0001 = 0xE1
    assert_eq!(vex.byte0, 0xE1);
}
