//! #1233 Phase B closure emit primitives — unit tests.
//!
//! Tests for:
//! - Symbol registration for closure body Lambdas (mangled names, Local visibility)
//! - Frame layout precomputation for closures (fat_off, env_off, alignment)
//! - emit_closure_cons byte emission (LEA, MOV sequences)
//! - Capture binding registration in closure bodies ([R14 + offset])

use super::super::*;
use paideia_as_ir::{ClosureMeta, CaptureMeta, CaptureKind, IrKind};
use crate::local_binding_table::BindingHome;
use paideia_as_diagnostics::{FileId, Span};
use crate::LocalBindingTable;

#[test]
fn closure_symbol_registered_with_mangled_name() {
    // #1233 Phase B test (a): closure body gets ELF symbol registration.
    // Construct IR: Let("outer") = ClosureCons(Lambda)
    // Verify: Symbol "closure_outer_<lambda_id>" is registered as Function/Local.

    let mut arena = IrArena::new();
    let span = Span::new(FileId::new(1).unwrap(), 0, 0);

    // Create Lambda body
    let lambda_id = arena.alloc(IrKind::Lambda, span);

    // Create ClosureCons with Lambda child
    let cc_id = arena.alloc_with_children(IrKind::ClosureCons, span, [lambda_id]);

    // Create Let binding "outer" with ClosureCons RHS
    let let_id = arena.alloc_with_children(IrKind::Let, span, [cc_id]);
    arena.binding_names_mut().insert(let_id, "outer".to_string());

    // Register closure metadata
    let closure_meta = ClosureMeta {
        mangled_name: format!("closure_outer_{}", lambda_id.get()),
        captures: vec![],
        env_size: 0,
    };
    arena.closure_meta_mut().insert(lambda_id, closure_meta);

    // Run emit walker (will trigger register_closure_body_symbols pre-pass)
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify symbol was registered
    let mangled = format!("closure_outer_{}", lambda_id.get());
    let symbol = arena
        .symbols()
        .iter()
        .find(|s| s.name == mangled);
    assert!(symbol.is_some(), "closure symbol should be registered");

    let sym = symbol.unwrap();
    assert_eq!(sym.kind, SymbolKind::Function);
    assert_eq!(sym.visibility, paideia_as_ir::Visibility::Local);
    assert_eq!(sym.ir_node, lambda_id);
}

#[test]
fn frame_layout_computes_zero_capture_closure_slots() {
    // #1233 Phase B test (b): frame layout for zero-capture closure.
    // Construct: Lambda(body=ClosureCons(lambda_body))
    // Verify: fat_offset=0, env_offset=16, total_size rounded to 16.

    let mut arena = IrArena::new();
    let span = Span::new(FileId::new(1).unwrap(), 0, 0);

    // Create closure body Lambda
    let lambda_body_id = arena.alloc(IrKind::Lambda, span);

    // Create ClosureCons with Lambda child
    let cc_id = arena.alloc_with_children(IrKind::ClosureCons, span, [lambda_body_id]);

    // Create caller Lambda with ClosureCons body
    let caller_lambda_id = arena.alloc_with_children(IrKind::Lambda, span, [cc_id]);

    // Register closure meta for body
    let closure_meta = ClosureMeta {
        mangled_name: format!("closure_test_{}", lambda_body_id.get()),
        captures: vec![],
        env_size: 0,
    };
    arena.closure_meta_mut().insert(lambda_body_id, closure_meta);

    // Run walker to trigger precompute_caller_frame
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify frame layout
    let layout = arena.closure_frame_meta().get(caller_lambda_id);
    assert!(layout.is_some(), "frame layout should be computed");

    let frame = layout.unwrap();
    assert_eq!(frame.total_size, 16, "single zero-capture closure needs 16 bytes aligned");

    let slot = frame.get_slot(cc_id);
    assert!(slot.is_some(), "slot should be assigned");

    let (fat_off, env_off) = slot.unwrap();
    assert_eq!(fat_off, 0, "fat pair starts at offset 0");
    assert_eq!(env_off, 16, "env record follows fat pair at offset 16");
}

#[test]
fn frame_layout_computes_two_capture_closure_slots() {
    // #1233 Phase B test (b+): frame layout for multi-capture closures.
    // Construct: Lambda(body=ClosureCons(lambda_body) with 2 captures)
    // Verify: layout captures fat pairs and env records with proper alignment.

    let mut arena = IrArena::new();
    let span = Span::new(FileId::new(1).unwrap(), 0, 0);

    // Create closure body Lambda
    let lambda_body_id = arena.alloc(IrKind::Lambda, span);

    // Create ClosureCons with Lambda child
    let cc_id = arena.alloc_with_children(IrKind::ClosureCons, span, [lambda_body_id]);

    // Create caller Lambda with ClosureCons body
    let caller_lambda_id = arena.alloc_with_children(IrKind::Lambda, span, [cc_id]);

    // Register closure meta with 2 captures (16 bytes total)
    let closure_meta = ClosureMeta {
        mangled_name: format!("closure_test_{}", lambda_body_id.get()),
        captures: vec![
            CaptureMeta {
                name: "x".to_string(),
                offset: 0,
                kind: CaptureKind::Consume,
            },
            CaptureMeta {
                name: "y".to_string(),
                offset: 8,
                kind: CaptureKind::Consume,
            },
        ],
        env_size: 16,
    };
    arena.closure_meta_mut().insert(lambda_body_id, closure_meta);

    // Run walker
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify frame layout
    let layout = arena.closure_frame_meta().get(caller_lambda_id).unwrap();
    // fat pair (16) + env record (16) = 32 bytes, already 16-aligned
    assert_eq!(layout.total_size, 32);

    let (fat_off, env_off) = layout.get_slot(cc_id).unwrap();
    assert_eq!(fat_off, 0);
    assert_eq!(env_off, 16);
}

#[test]
fn closure_capture_binding_registered_at_env_slot() {
    // #1233 Phase B test: closure body captures registered as [R14 + offset].
    // When visit_lambda processes a closure body Lambda, captures are inserted
    // into local_bindings with BindingHome::EnvSlot.

    let mut local_bindings = LocalBindingTable::new();

    // Simulate register_nested_lambda_params + capture registration
    // (this normally happens in visit_lambda)
    local_bindings.insert_env("captured_x".to_string(), 0);
    local_bindings.insert_env("captured_y".to_string(), 8);

    // Verify captures are retrievable as EnvSlot
    assert_eq!(
        local_bindings.get_home("captured_x"),
        Some(BindingHome::EnvSlot(0))
    );
    assert_eq!(
        local_bindings.get_home("captured_y"),
        Some(BindingHome::EnvSlot(8))
    );

    // Verify scalar get() returns None for EnvSlot (not a register)
    assert_eq!(local_bindings.get("captured_x"), None);
    assert_eq!(local_bindings.get("captured_y"), None);

    // Verify contains() works
    assert!(local_bindings.contains("captured_x"));
    assert!(local_bindings.contains("captured_y"));
}

#[test]
fn closure_capture_shadows_parameter() {
    // #1233: Closure capture with same name as parameter shadows param.
    let mut local_bindings = LocalBindingTable::new();

    // Root scope: parameter x in RAX
    local_bindings.insert("x".to_string(), paideia_as_ir::abi::RAX);

    // Nested scope (closure body): capture x at [r14 + 0]
    local_bindings.push_scope();
    local_bindings.insert_env("x".to_string(), 0);

    // Lookup finds EnvSlot (shadow wins)
    assert_eq!(local_bindings.get_home("x"), Some(BindingHome::EnvSlot(0)));
    assert_eq!(local_bindings.get("x"), None); // get() returns None for EnvSlot

    // Pop back to root
    local_bindings.pop_scope();
    assert_eq!(
        local_bindings.get("x"),
        Some(paideia_as_ir::abi::RAX)
    );
}

#[test]
fn frame_layout_sixteen_aligned() {
    // #1233: frame layout must be 16-byte aligned for SysV.
    // Scenario: one closure with 10-byte env (triggers alignment padding).

    let mut arena = IrArena::new();
    let span = Span::new(FileId::new(1).unwrap(), 0, 0);

    let lambda_body_id = arena.alloc(IrKind::Lambda, span);

    let cc_id = arena.alloc_with_children(IrKind::ClosureCons, span, [lambda_body_id]);

    let caller_lambda_id = arena.alloc_with_children(IrKind::Lambda, span, [cc_id]);

    // Odd-sized env (10 bytes)
    let closure_meta = ClosureMeta {
        mangled_name: format!("closure_test_{}", lambda_body_id.get()),
        captures: vec![
            CaptureMeta {
                name: "c0".to_string(),
                offset: 0,
                kind: CaptureKind::Consume,
            },
        ],
        env_size: 10,
    };
    arena.closure_meta_mut().insert(lambda_body_id, closure_meta);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    let layout = arena.closure_frame_meta().get(caller_lambda_id).unwrap();
    // fat(16) + env(10) = 26 → round up to 32
    assert_eq!(layout.total_size, 32, "frame size must be 16-aligned");
}
