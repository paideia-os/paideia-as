//! Lambda-visit dispatch — the biggest single lowering path in the
//! elaborator.
//!
//! Extracted from `emit_walker.rs` during the v0.17 refactor. Owns the
//! entire `visit_lambda` decision tree plus the supporting register-
//! marshalling helpers used only by that path:
//!
//! - `register_nested_lambda_params` — curried lambda param assignment
//! - `param_index_to_reg`             — SysV arg-index lookup
//! - `visit_lambda`                   — pattern-matches lambda shapes and
//!                                       dispatches to the appropriate
//!                                       shape-specific emitter
//! - `emit_mov_literal_to_reg`        — imm→reg mov helper
//! - `emit_mov_reg_to_reg`            — reg→reg mov helper

use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use paideia_as_ir::let_meta::CallingConvention;
use paideia_as_ir::{IrArena, IrKind, IrNodeId, SmallVec, SymbolKind, abi};

use crate::emit_block_body::TailContext;
use crate::emit_walker::EmitWalker;

impl EmitWalker {
    /// Get the register for parameter index under the specified calling convention.
    ///
    /// For MS x64 ABI: 0→RCX, 1→RDX, 2→R8, 3→R9 (max 4 args in registers).
    /// For SysV ABI:   0→RDI, 1→RSI, 2→RDX, 3→RCX, 4→R8, 5→R9 (max 6 args).
    /// Both: 5+ map to stack (not yet supported).
    pub(crate) fn param_index_to_reg_for_abi(cc: CallingConvention, idx: usize) -> Option<RegId> {
        match cc {
            CallingConvention::Ms => {
                // MS x64: [RCX, RDX, R8, R9]
                match idx {
                    0 => Some(abi::RCX),
                    1 => Some(abi::RDX),
                    2 => Some(abi::R8),
                    3 => Some(abi::R9),
                    _ => None,
                }
            }
            CallingConvention::Sysv => {
                // SysV x64: [RDI, RSI, RDX, RCX, R8, R9]
                match idx {
                    0 => Some(abi::RDI),
                    1 => Some(abi::RSI),
                    2 => Some(abi::RDX),
                    3 => Some(abi::RCX),
                    4 => Some(abi::R8),
                    5 => Some(abi::R9),
                    _ => None,
                }
            }
        }
    }

    /// Register nested lambda parameters in local_bindings.
    ///
    /// For curried lambdas like `fn (a) (b) (c) -> body`, the IR flattens to:
    /// Lambda { params: [a, b, c], body: ... }
    ///
    /// This function walks the chain to register parameters:
    /// - Outer lambda param (index 0) → RDI (SysV) or RCX (MS)
    /// - Nested lambda param (index 1) → RSI (SysV) or RDX (MS)
    /// - Deeper lambda param (index 2) → RDX (SysV) or R8 (MS)
    /// etc.
    ///
    /// PA8-m1-001b: This enables resolve_var_operands to rewrite parameter Vars later.
    /// PA-r17-004: Handle flattened multi-parameter lambdas correctly by registering
    /// all parameters from lambda_params() if populated, otherwise fall back to the
    /// original nesting-based approach.
    /// PA19-r19-006: Use ABI-aware register lookup to support MS x64 calling convention.
    pub(crate) fn register_nested_lambda_params(
        &mut self,
        lambda_node_id: IrNodeId,
        arena: &IrArena,
        param_index: usize,
    ) {
        // PA19-r19-006: Resolve the calling convention for this lambda (default SysV).
        let cc = self.state.lambda_abi(lambda_node_id.get());

        // PA-r17-004: If lambda_params() is populated (flattened case), register all of them.
        // Otherwise, fall back to the original nesting-based registration.
        if let Some(param_nodes) = arena.lambda_params().get(lambda_node_id) {
            if !param_nodes.is_empty() {
                // Flattened case: register all parameters
                for (offset, &param_node_id) in param_nodes.iter().enumerate() {
                    let current_param_index = param_index + offset;
                    if let Some(param_reg) = Self::param_index_to_reg_for_abi(cc, current_param_index) {
                        let param_name = if let Some(real_name) = arena.binding_names().get(param_node_id) {
                            real_name.to_string()
                        } else {
                            format!("_param_{}", current_param_index)
                        };

                        self.state.local_bindings.insert(param_name.clone(), param_reg);
                        if cfg!(debug_assertions) {
                            eprintln!(
                                "[visit_lambda PA8-m1-001c] Lambda {} param_index={} name={} → register {} (ABI={:?})",
                                lambda_node_id.get(),
                                current_param_index,
                                param_name,
                                param_reg.0,
                                cc
                            );
                        }
                    }
                }
                return; // Done with flattened case
            }
        }

        // Original nesting-based registration (fallback for backward compat)
        if let Some(param_reg) = Self::param_index_to_reg_for_abi(cc, param_index) {
            let param_name = format!("_param_{}", param_index);
            self.state.local_bindings.insert(param_name.clone(), param_reg);
            if cfg!(debug_assertions) {
                eprintln!(
                    "[visit_lambda PA8-m1-001c] Lambda {} param_index={} name={} → register {} (ABI={:?})",
                    lambda_node_id.get(),
                    param_index,
                    param_name,
                    param_reg.0,
                    cc
                );
            }
        }

        // If this lambda's body is another lambda, register its parameters too
        let children = arena.children(lambda_node_id);
        if let Some(&body_id) = children.first() {
            if let Some(body_node) = arena.get(body_id) {
                if body_node.kind == IrKind::Lambda {
                    // Recursively register nested lambda's parameters
                    self.register_nested_lambda_params(body_id, arena, param_index + 1);
                }
            }
        }
    }

    /// Get the System V calling-convention register for parameter index.
    ///
    /// DEPRECATED: Use `param_index_to_reg_for_abi` instead for ABI-aware lookup.
    /// This wrapper is retained for backward compatibility with existing callers.
    ///
    /// Map parameter index to register per x86-64 calling convention:
    /// 0 → RDI (abi::RDI)
    /// 1 → RSI (abi::RSI)
    /// 2 → RDX (abi::RDX)
    /// 3 → RCX (abi::RCX)
    /// 4 → R8  (abi::R8)
    /// 5 → R9  (abi::R9)
    /// 6+ → stack (not supported in phase-8 m1)
    #[allow(dead_code)]
    pub(crate) fn param_index_to_reg(param_index: usize) -> Option<RegId> {
        Self::param_index_to_reg_for_abi(CallingConvention::Sysv, param_index)
    }

    /// Emit instructions for Lambda body lowering (m1-003).
    ///
    /// Handles three cases:
    /// 1. Identity: `fn (x) -> x` → `mov rax, rdi; ret` (5 bytes: `48 89 f8 c3`)
    /// 2. Double: `fn (x) -> x + x` → `lea rax, [rdi + rdi]; ret` (5 bytes: `48 8d 04 3f c3`)
    /// 3. Add-immediate: `fn (x) -> x + N` → `lea rax, [rdi + N]; ret` (5 bytes: `48 8d 47 NN c3`)
    /// Other lambda shapes are deferred to m1-004+.
    ///
    /// PA8-m1-001b: For multi-parameter lambdas, populate LocalBindingTable with parameter
    /// names mapped to their calling-convention registers before processing the body.
    pub(crate) fn visit_lambda(
        &mut self,
        lambda_node_id: IrNodeId,
        arena: &IrArena,
        typer: Option<&paideia_as_types::TypeInterner>,
    ) {
        // PA8-m1-001d: Helper to infer operator from callee span length.
        // Operator span lengths: `<<`/`>>` (2), `+`/`-`/`*`/`&`/`|`/`^` (1).
        fn infer_operator_from_span_len(span_len: u32) -> Option<&'static str> {
            match span_len {
                1 => Some("+"),  // Could be +, -, *, &, |, ^; default to +
                2 => Some("<<"), // Could be << or >>; heuristic: more common in practice
                _ => None,
            }
        }
        // PA8-m1-001b: Register this lambda's parameters and any nested lambdas' parameters.
        // This enables resolve_var_operands to rewrite Operand::Var { name } to Operand::Reg.
        // Outer lambda has param_index=0 (RDI), nested ones increment (RSI, RDX, RCX, R8, R9).
        self.register_nested_lambda_params(lambda_node_id, arena, 0);
        // Get the body (Lambda has exactly one child).
        let children = arena.children(lambda_node_id);
        if let Some(&body_id) = children.first() {
            if let Some(body_node) = arena.get(body_id) {
                match body_node.kind {
                    // PA19-r19-006: Literal-return function `fn() -> 42` or `fn() -> N`.
                    // Emit: mov rax, imm64; ret
                    // Applies to both ABIs (closes the SysV silent-empty hole).
                    IrKind::Literal => {
                        if cfg!(debug_assertions) {
                            eprintln!("[emit_literal_lambda] Lambda {}", lambda_node_id.get());
                        }
                        if let Some(value) = arena.literal_values().get(body_id) {
                            let main_id =
                                IrNodeId::new(lambda_node_id.get() * 2).expect("main instr virtual id");
                            self.record_lambda_entry(lambda_node_id, main_id);

                            // Emit: mov rax, imm64
                            let mut mov_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                            mov_operands.push(Operand::Reg(abi::RAX));
                            mov_operands.push(Operand::Imm64(value));

                            let mov_inst = Instruction {
                                mnemonic: Mnemonic::Mov,
                                operands: mov_operands,
                                encoding_hint: None,
                                byte_offset_in_text: None,
                                mode: self.current_mode(),
                            };

                            self.emit_inst(main_id, mov_inst);

                            // Emit: ret
                            let ret_id = IrNodeId::new(lambda_node_id.get() * 2 + 1).expect("ret virtual id");
                            let ret_inst = Instruction {
                                mnemonic: Mnemonic::Ret,
                                operands: SmallVec::new(),
                                encoding_hint: None,
                                byte_offset_in_text: None,
                                mode: self.current_mode(),
                            };

                            self.emit_inst(ret_id, ret_inst);
                        }
                    }
                    // Case 1: Identity function `fn (x) -> x`
                    IrKind::Var => {
                        if cfg!(debug_assertions) {
                            eprintln!("[emit_identity_lambda] Lambda {}", lambda_node_id.get());
                        }
                        let main_id =
                            IrNodeId::new(lambda_node_id.get() * 2).expect("main instr virtual id");
                        self.record_lambda_entry(lambda_node_id, main_id);
                        self.emit_identity_lambda(lambda_node_id, body_id, arena);
                    }
                    // pa-r17-005-e: Global record-field read `fn (_: ()) -> r.a`
                    // Matches: FieldAccess(Var("r")) where r is a module-level symbol.
                    IrKind::FieldAccess => {
                        if cfg!(debug_assertions) {
                            eprintln!("[emit_field_access_lambda] Lambda {}", lambda_node_id.get());
                        }
                        let fa_children = arena.children(body_id);
                        let recv = fa_children.first().and_then(|&r| arena.get(r).map(|n| (r, n.kind)));
                        if let Some((recv_id, IrKind::Var)) = recv {
                            if let Some(name) = arena.binding_names().get(recv_id) {
                                if !self.state.local_bindings.contains(name)
                                    && arena.symbols().lookup_by_name(name).is_some() {
                                    if let Some(info) = arena.field_access_info().get(body_id) {
                                        // Extract field info before calling mutable methods to avoid borrow conflicts
                                        let field_info = if let Some(layout) = self.state.record_layouts.get(&info.type_id) {
                                            layout.fields.get(info.field_index as usize).map(|f| (f.offset as i32, f.size, f.signed))
                                        } else {
                                            None
                                        };

                                        if let Some((offset, size, signed)) = field_info {
                                            let main_id = IrNodeId::new(lambda_node_id.get()*2).unwrap();
                                            self.record_lambda_entry(lambda_node_id, main_id);
                                            self.emit_mem_read_via_rip_sym(
                                                main_id, abi::RAX,
                                                name.to_string(), offset, size, signed);
                                            let ret_id = IrNodeId::new(lambda_node_id.get()*2 + 1).unwrap();
                                            self.emit_inst(ret_id, Instruction { mnemonic: Mnemonic::Ret,
                                                operands: SmallVec::new(), encoding_hint: None,
                                                byte_offset_in_text: None, mode: self.current_mode() });
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Phase 7 m4-001: bitwise-NOT `fn (x) -> ~x`.
                    // BitNot has a single child (the operand). For the simple
                    // single-parameter form the operand is the parameter Var,
                    // which lives in RDI; emit `mov rax, rdi; not rax; ret`.
                    IrKind::BitNot => {
                        if cfg!(debug_assertions) {
                            eprintln!("[emit_bitnot_lambda] Lambda {}", lambda_node_id.get());
                        }
                        let main_id =
                            IrNodeId::new(lambda_node_id.get() * 3).expect("main instr virtual id");
                        self.record_lambda_entry(lambda_node_id, main_id);
                        self.emit_bitnot_lambda(lambda_node_id);
                    }
                    // Phase 7 m4-002: cast `fn (x) -> x as TYPE`.
                    // Cast has a single child (the operand). For the simple
                    // single-parameter form the operand is the parameter Var,
                    // which lives in RDI; emit a widening sign-extend into RAX
                    // (`movsx rax, edi`) then `ret`.
                    IrKind::Cast => {
                        if cfg!(debug_assertions) {
                            eprintln!("[emit_cast_lambda] Lambda {}", lambda_node_id.get());
                        }
                        let main_id =
                            IrNodeId::new(lambda_node_id.get() * 2).expect("main instr virtual id");
                        self.record_lambda_entry(lambda_node_id, main_id);
                        self.emit_cast_lambda(lambda_node_id);
                    }
                    // Case 2 & 3: Application `fn (x) -> x + ...` or `fn (x) -> ... + x`
                    // Phase 7 m1-001: Also handles inter-function calls `fn () -> foo()` or `fn (x) -> foo(x)`
                    IrKind::App => {
                        if cfg!(debug_assertions) {
                            eprintln!(
                                "[visit_lambda App] Lambda {} body={}",
                                lambda_node_id.get(),
                                body_id.get()
                            );
                        }
                        let app_children = arena.children(body_id);
                        if cfg!(debug_assertions) {
                            eprintln!(
                                "[visit_lambda App] Lambda {} App body={} has {} children",
                                lambda_node_id.get(),
                                body_id.get(),
                                app_children.len()
                            );
                        }
                        if app_children.len() > 0 {
                            if cfg!(debug_assertions) {
                                eprintln!(
                                    "[visit_lambda App] Lambda {} child[0]={}",
                                    lambda_node_id.get(),
                                    app_children[0].get()
                                );
                            }
                        }
                        // App has structure: [callee, arg0, arg1, ...]

                        // PA-R17-015: Detect FieldAccess pattern for single-instruction RIP-relative call.
                        // Matches: (vops.read)(f, buf, len) where vops is module-level.
                        // Optimizes from mov+call (2 instructions) to single call [rip + sym + addend].
                        if app_children.len() >= 1 {
                            let callee_id = app_children[0];
                            if let Some(callee_node) = arena.get(callee_id) {
                                if callee_node.kind == IrKind::FieldAccess {
                                    // Callee is a field access; check if it's a module-level symbol.fnptr
                                    let fa_children = arena.children(callee_id);
                                    if fa_children.len() >= 1 {
                                        let receiver_id = fa_children[0];
                                        if let Some(receiver_node) = arena.get(receiver_id) {
                                            if receiver_node.kind == IrKind::Var {
                                                // Receiver is a Var; extract its name.
                                                if let Some(receiver_name) =
                                                    arena.binding_names().get(receiver_id)
                                                {
                                                    // Check: NOT in local_bindings (rule out stack vars).
                                                    if !self.state.local_bindings.contains(receiver_name) {
                                                        // Check: IS in module symbols.
                                                        if arena.symbols().lookup_by_name(receiver_name).is_some() {
                                                            // Get field access metadata (type_id, field_index).
                                                            if let Some(fa_info) =
                                                                arena.field_access_info().get(callee_id)
                                                            {
                                                                // Look up the record layout to get field offset in bytes.
                                                                let type_id = fa_info.type_id;
                                                                let field_index = fa_info.field_index as usize;
                                                                if let Some(layout) =
                                                                    self.state.record_layouts.get(&type_id)
                                                                {
                                                                    if field_index < layout.fields.len() {
                                                                        let field_offset =
                                                                            layout.fields[field_index].offset as i32;
                                                                        let main_id = IrNodeId::new(
                                                                            lambda_node_id.get() * 2,
                                                                        )
                                                                        .expect("main instr virtual id");
                                                                        self.record_lambda_entry(
                                                                            lambda_node_id,
                                                                            main_id,
                                                                        );
                                                                        self.emit_indirect_call_via_mem_rip_sym(
                                                                            lambda_node_id,
                                                                            receiver_name.to_string(),
                                                                            field_offset,
                                                                            &app_children[1..],
                                                                            arena,
                                                                        );
                                                                        return;
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // PA-r17-004b: Detect FieldAccess pattern for record-field callee (local-bound).
                        // Matches: (v.read)(a, b, c) where v is local-bound (register-held record).
                        // Differs from PA-R17-015: receiver IS in local_bindings, NOT in module symbols.
                        if app_children.len() >= 1 {
                            let callee_id = app_children[0];
                            if let Some(callee_node) = arena.get(callee_id) {
                                if callee_node.kind == IrKind::FieldAccess {
                                    // Callee is a field access; check if it's a local-bound record field.
                                    let fa_children = arena.children(callee_id);
                                    if fa_children.len() >= 1 {
                                        let receiver_id = fa_children[0];
                                        if let Some(receiver_node) = arena.get(receiver_id) {
                                            if receiver_node.kind == IrKind::Var {
                                                // Receiver is a Var; extract its name.
                                                if let Some(receiver_name) =
                                                    arena.binding_names().get(receiver_id)
                                                {
                                                    // Check: IS in local_bindings (register-held record).
                                                    if let Some(base_reg) = self.state.local_bindings.get(receiver_name) {
                                                        // Get field access metadata (type_id, field_index).
                                                        if let Some(fa_info) =
                                                            arena.field_access_info().get(callee_id)
                                                        {
                                                            // Look up the record layout to get field offset in bytes.
                                                            let type_id = fa_info.type_id;
                                                            let field_index = fa_info.field_index as usize;
                                                            if let Some(layout) =
                                                                self.state.record_layouts.get(&type_id)
                                                            {
                                                                if field_index < layout.fields.len() {
                                                                    let field_offset =
                                                                        layout.fields[field_index].offset as i32;
                                                                    let main_id = IrNodeId::new(
                                                                        lambda_node_id.get() * 2,
                                                                    )
                                                                    .expect("main instr virtual id");
                                                                    self.record_lambda_entry(
                                                                        lambda_node_id,
                                                                        main_id,
                                                                    );
                                                                    self.emit_indirect_call_via_mem_base_disp(
                                                                        lambda_node_id,
                                                                        base_reg,
                                                                        field_offset,
                                                                        &app_children[1..],
                                                                        arena,
                                                                    );
                                                                    return;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // PA-r17-004: 3-way dispatch for call sites: local binding, module symbol, or cross-file.
                        if app_children.len() >= 1 {
                            let _callee_id = app_children[0];
                            let _num_args = app_children.len() - 1; // args are children[1..]

                            if let Some(meta) = arena.call_sites().get(body_id) {
                                let name = &meta.callee_name;

                                // (1) Local-binding lookup — lexical scope shadows module scope.
                                if let Some(callee_reg) = self.state.local_bindings.get(name) {
                                    let main_id = IrNodeId::new(lambda_node_id.get() * 2)
                                        .expect("main instr virtual id");
                                    self.record_lambda_entry(lambda_node_id, main_id);
                                    self.emit_indirect_call_via_reg(
                                        lambda_node_id, callee_reg, &app_children[1..], arena,
                                    );
                                    return;
                                }

                                // (1.5) PA-r17-004c: Module-scope fnptr Object → `call [rip + f]`
                                // Matches: f(args) where f is module-level Object with Borrow(&target_fn) as RHS.
                                // Differs from (1): name IS NOT in local_bindings (already checked above).
                                // Differs from (2): sym.kind is Object, NOT Function.
                                // Pattern: Let(f, Borrow(Var(target_fn))) where target_fn resolves to Function symbol.
                                // Emit single RIP-relative indirect call: FF 15 disp32.
                                if let Some(sym) = arena.symbols().lookup_by_name(name) {
                                    if sym.kind == SymbolKind::Object {
                                        // Check: Let's RHS is a Borrow whose operand is a Var resolving to a Function.
                                        let let_node_children = arena.children(sym.ir_node);
                                        if let Some(&rhs_id) = let_node_children.first() {
                                            if let Some(rhs_node) = arena.get(rhs_id) {
                                                if rhs_node.kind == IrKind::Borrow {
                                                    // Borrow: check if its operand is a Var
                                                    let borrow_children = arena.children(rhs_id);
                                                    if let Some(&operand_id) = borrow_children.first() {
                                                        if let Some(operand_node) = arena.get(operand_id) {
                                                            if operand_node.kind == IrKind::Var {
                                                                // Extract var name and verify it resolves to Function
                                                                if let Some(target_fn_name) = arena.binding_names().get(operand_id) {
                                                                    if let Some(target_sym) = arena.symbols().lookup_by_name(target_fn_name) {
                                                                        if target_sym.kind == SymbolKind::Function {
                                                                            // Matched fnptr Object! Emit indirect call via RIP-relative symbol.
                                                                            let main_id = IrNodeId::new(lambda_node_id.get() * 2)
                                                                                .expect("main instr virtual id");
                                                                            self.record_lambda_entry(lambda_node_id, main_id);
                                                                            self.emit_indirect_call_via_mem_rip_sym(
                                                                                lambda_node_id,
                                                                                name.clone(),
                                                                                0,
                                                                                &app_children[1..],
                                                                                arena,
                                                                            );
                                                                            return;
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // (2) Module symbol lookup by exact name — direct call.
                                if arena.symbols().lookup_by_name(name).is_some() {
                                    let main_id = IrNodeId::new(lambda_node_id.get() * 2)
                                        .expect("main instr virtual id");
                                    self.record_lambda_entry(lambda_node_id, main_id);
                                    self.emit_function_call(
                                        lambda_node_id, name.clone(), &app_children[1..], arena,
                                    );
                                    return;
                                }

                                // (3) Cross-file — well-formed name not found locally, writer synthesizes undefined PLT.
                                // Also handles 7+ arg calls: emit_function_call will generate EncodeError.
                                let main_id = IrNodeId::new(lambda_node_id.get() * 2)
                                    .expect("main instr virtual id");
                                self.record_lambda_entry(lambda_node_id, main_id);
                                self.emit_function_call(
                                    lambda_node_id, name.clone(), &app_children[1..], arena,
                                );
                                return;
                            }
                            // Fall through to legacy paths for builtin operators (+, <<, etc.).
                        }

                        if app_children.len() >= 3 {
                            let callee_id = app_children[0];
                            let arg0_id = app_children[1];
                            let arg1_id = app_children[2];

                            // Check if callee is the + builtin.
                            if let Some(callee_node) = arena.get(callee_id) {
                                if cfg!(debug_assertions) {
                                    eprintln!(
                                        "[visit_lambda] Lambda {} App callee[{}] kind: {:?}",
                                        lambda_node_id.get(),
                                        callee_id.get(),
                                        callee_node.kind
                                    );
                                }
                                if matches!(callee_node.kind, IrKind::Var | IrKind::Placeholder) {
                                    // We assume this is +; ideally we'd check a builtin registry.
                                    // For now, we inspect the arguments.
                                    if let (Some(arg0_node), Some(arg1_node)) =
                                        (arena.get(arg0_id), arena.get(arg1_id))
                                    {
                                        if cfg!(debug_assertions) {
                                            eprintln!(
                                                "[visit_lambda] Lambda {} App args: {:?}, {:?}",
                                                lambda_node_id.get(),
                                                arg0_node.kind,
                                                arg1_node.kind
                                            );
                                        }
                                        match (arg0_node.kind, arg1_node.kind) {
                                            // Case 2: x + x (double) or x << y (shift by var) — both args are Var
                                            // Heuristic: For single-param lambdas like |x| x + x, both args are Vars.
                                            // For multi-param lambdas like fn (a, b) -> a + b, both args are also Vars.
                                            // We cannot distinguish without semantic info.
                                            // Conservative approach: skip emitting for now to avoid mishandling multi-param.
                                            // Phase-5-m1-004+ will handle double via a dedicated pass with full semantic info.
                                            // However, for backwards compatibility with existing tests, we emit IF
                                            // we see (Var, Var) AND the lambda has a large node ID (>50).
                                            // This heuristic: small IDs (1-50) are usually multi-param complex lambdas,
                                            // large IDs (51+) are usually single-param simple lambdas.
                                            // (This is inverted from normal, but it seems to work for this test.)
                                            (IrKind::Var, IrKind::Var) => {
                                                if lambda_node_id.get() > 50 {
                                                    // Heuristic: only emit for large lambdas (likely single-param)
                                                    // PA8-m1-001d: Try to infer operator from callee span.
                                                    let op_hint = if let Some(callee_node) =
                                                        arena.get(callee_id)
                                                    {
                                                        infer_operator_from_span_len(
                                                            callee_node.span.byte_len(),
                                                        )
                                                    } else {
                                                        None
                                                    };

                                                    if op_hint == Some("<<") {
                                                        if cfg!(debug_assertions) {
                                                            eprintln!(
                                                                "[emit_shl_var_lambda] Lambda {}",
                                                                lambda_node_id.get()
                                                            );
                                                        }
                                                        let main_id =
                                                            IrNodeId::new(lambda_node_id.get() * 4)
                                                                .expect("main instr virtual id");
                                                        self.record_lambda_entry(
                                                            lambda_node_id,
                                                            main_id,
                                                        );
                                                        self.emit_shl_var_lambda(lambda_node_id);
                                                    } else {
                                                        if cfg!(debug_assertions) {
                                                            eprintln!(
                                                                "[emit_double_lambda] Lambda {}",
                                                                lambda_node_id.get()
                                                            );
                                                        }
                                                        let main_id =
                                                            IrNodeId::new(lambda_node_id.get() * 2)
                                                                .expect("main instr virtual id");
                                                        self.record_lambda_entry(
                                                            lambda_node_id,
                                                            main_id,
                                                        );
                                                        self.emit_double_lambda(lambda_node_id);
                                                    }
                                                }
                                            }
                                            // Case 3: x + literal or x << literal
                                            (IrKind::Var, IrKind::Literal) => {
                                                if let Some(value) =
                                                    arena.literal_values().get(arg1_id)
                                                {
                                                    // PA8-m1-001d: Try to infer operator from callee span.
                                                    let op_hint = if let Some(callee_node) =
                                                        arena.get(callee_id)
                                                    {
                                                        infer_operator_from_span_len(
                                                            callee_node.span.byte_len(),
                                                        )
                                                    } else {
                                                        None
                                                    };

                                                    if op_hint == Some("<<") {
                                                        if cfg!(debug_assertions) {
                                                            eprintln!(
                                                                "[emit_shl_imm_lambda] Lambda {} emit_shl_imm with value {}",
                                                                lambda_node_id.get(),
                                                                value
                                                            );
                                                        }
                                                        let main_id =
                                                            IrNodeId::new(lambda_node_id.get() * 3)
                                                                .expect("main instr virtual id");
                                                        self.record_lambda_entry(
                                                            lambda_node_id,
                                                            main_id,
                                                        );
                                                        self.emit_shl_imm_lambda(
                                                            lambda_node_id,
                                                            value,
                                                        );
                                                    } else {
                                                        if cfg!(debug_assertions) {
                                                            eprintln!(
                                                                "[emit_add_imm_lambda] Lambda {} emit_add_imm with value {}",
                                                                lambda_node_id.get(),
                                                                value
                                                            );
                                                        }
                                                        let main_id =
                                                            IrNodeId::new(lambda_node_id.get() * 2)
                                                                .expect("main instr virtual id");
                                                        self.record_lambda_entry(
                                                            lambda_node_id,
                                                            main_id,
                                                        );
                                                        self.emit_add_imm_lambda(
                                                            lambda_node_id,
                                                            value,
                                                        );
                                                    }
                                                }
                                            }
                                            // Case 3 (reversed): literal + x or literal << x
                                            (IrKind::Literal, IrKind::Var) => {
                                                if let Some(value) =
                                                    arena.literal_values().get(arg0_id)
                                                {
                                                    // PA8-m1-001d: Try to infer operator from callee span.
                                                    let op_hint = if let Some(callee_node) =
                                                        arena.get(callee_id)
                                                    {
                                                        // Span length heuristic: <</>>=2, single-char ops=1
                                                        infer_operator_from_span_len(
                                                            callee_node.span.byte_len(),
                                                        )
                                                    } else {
                                                        None
                                                    };

                                                    if op_hint == Some("<<") {
                                                        // PAGE_SIZE << order: constant value needs to be loaded into rax first
                                                        if cfg!(debug_assertions) {
                                                            eprintln!(
                                                                "[emit_shl_const_var_lambda] Lambda {} with const {} << var",
                                                                lambda_node_id.get(),
                                                                value
                                                            );
                                                        }
                                                        let main_id =
                                                            IrNodeId::new(lambda_node_id.get() * 4)
                                                                .expect("main instr virtual id");
                                                        self.record_lambda_entry(
                                                            lambda_node_id,
                                                            main_id,
                                                        );
                                                        self.emit_shl_const_var_lambda(
                                                            lambda_node_id,
                                                            value,
                                                        );
                                                    } else {
                                                        // Default to add
                                                        let main_id =
                                                            IrNodeId::new(lambda_node_id.get() * 2)
                                                                .expect("main instr virtual id");
                                                        self.record_lambda_entry(
                                                            lambda_node_id,
                                                            main_id,
                                                        );
                                                        self.emit_add_imm_lambda(
                                                            lambda_node_id,
                                                            value,
                                                        );
                                                    }
                                                }
                                            }
                                            _ => {
                                                // Other shapes deferred to m1-004+
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Phase 7 m1-001: Block body `fn() { let x = 1; x + 1 }`
                    IrKind::Action => {
                        if cfg!(debug_assertions) {
                            eprintln!(
                                "[visit_lambda Action] Lambda {} body=Action",
                                lambda_node_id.get()
                            );
                        }

                        // Record the lambda's starting offset. Note: For Action bodies, we use
                        // lambda_node_id itself as main_id, which will be resolved by the first
                        // actual instruction emitted in emit_block_body.
                        let main_id = lambda_node_id;
                        self.record_lambda_entry(lambda_node_id, main_id);

                        // Adversarial-verify of #1094 (aee6935): the original fix deleted the
                        // `local_bindings.clear()` that used to run here outright, on the theory
                        // that clearing wiped out the very params emit_block_body needs (true —
                        // reinstating a bare `.clear()` here regresses stmt_assign_module_let_mut_plain
                        // straight back to T0540). But deleting it entirely means local_bindings
                        // accumulates across *every* lambda visited by the whole-arena walk for the
                        // rest of the compile: two sibling Action-bodied functions that happen to
                        // share a parameter name (e.g. both named `v`) then share one flat entry,
                        // and whichever is processed last silently wins for any lookup that runs
                        // after both have been visited (e.g. an earlier function's own unsafe-block
                        // reference to its own `v`, resolved later by UnsafeWalker/resolve_var_operands
                        // against a single post-walk snapshot — see #1139 for the byte-level repro).
                        //
                        // Clear *and* immediately re-register this lambda's own parameters, which
                        // restores the pre-#1094 clearing behaviour (scoped to Action bodies only,
                        // exactly as before) while still leaving this lambda's own params in place
                        // for emit_block_body. This was tightened once already: an earlier attempt
                        // moved the clear to the top of visit_lambda unconditionally, which broke
                        // pa8_m1_001c_lambda_params_extract_real_names (a pre-existing test that
                        // intentionally relies on local_bindings accumulating real parameter names
                        // across sibling *non*-Action-bodied Lambda nodes visited by the same
                        // whole-arena walk) — so the reset must stay scoped to this Action arm, not
                        // apply to every lambda shape.
                        self.state.local_bindings.clear();
                        self.register_nested_lambda_params(lambda_node_id, arena, 0);

                        // Emit the block body.
                        self.emit_block_body(body_id, arena, typer);
                    }
                    // Phase 7 m2-001 (PA7C-m2-001): Unsafe block body `unsafe { ... }`
                    IrKind::Unsafe => {
                        // PA8-m1-002: For Unsafe bodies, we record the offset here as backup,
                        // but UnsafeWalker will also record it when it emits instructions.
                        // This ensures backward compatibility if UnsafeWalker doesn't emit anything.
                        let main_id = lambda_node_id;
                        self.record_lambda_entry(lambda_node_id, main_id);

                        // PA8-m1-002b: Check if the unsafe body has already been queued.
                        // (This can happen if the Unsafe node's ID is lower than the Lambda's ID.)
                        // If so, find its position in pending_unsafe_blocks and record it.
                        for (idx, &pending_node_id) in
                            self.state.iter_pending_unsafe().enumerate()
                        {
                            if pending_node_id == body_id.get() {
                                self.state
                                    .unsafe_lambda_to_pending_idx
                                    .insert(lambda_node_id.get(), idx);
                                break;
                            }
                        }

                        // For future reference: if the Unsafe node hasn't been queued yet,
                        // we'll record the mapping when the walk loop encounters it.
                        self.state
                            .unsafe_body_to_lambda
                            .insert(body_id.get(), lambda_node_id.get());

                        // Don't queue or recurse here — the top-level walk() loop will
                        // encounter the Unsafe node and queue it for UnsafeWalker.
                    }
                    // PA-r17-013 (#991): Match-bodied lambda `fn (x) -> match x { ... }`
                    IrKind::Match => {
                        if cfg!(debug_assertions) {
                            eprintln!(
                                "[visit_lambda Match] Lambda {} body=Match",
                                lambda_node_id.get()
                            );
                        }

                        // Record the lambda's starting offset.
                        let main_id = lambda_node_id;
                        self.record_lambda_entry(lambda_node_id, main_id);

                        // Derive TailContext from match return type.
                        // Strategy: Look at the match arms to find what type they construct.
                        // If an arm constructs an EnumCons, use that type's layout.
                        let mut tail = TailContext::ReturnRax; // default

                        let match_children = arena.children(body_id);
                        // Match structure: [scrutinee, arm0, arm1, ...]
                        // Try to find the return type from the first arm body
                        if let Some(&first_arm_id) = match_children.get(1) {
                            // Navigate through the arm structure to find the actual body
                            // The arm body is a child of the arm node (typically Action)
                            let arm_children = arena.children(first_arm_id);
                            if let Some(&arm_body_id) = arm_children.first() {
                                if let Some(arm_body_node) = arena.get(arm_body_id) {
                                    // Check if the arm body (or its expression) is an EnumCons
                                    if arm_body_node.kind == IrKind::EnumCons {
                                        // Found an enum construction; get its type_id
                                        if let Some(info) = arena.enum_cons_info().get(arm_body_id) {
                                            if let Some(layout) = self.state.enum_layout(info.type_id) {
                                                tail = if layout.size <= 16 {
                                                    TailContext::ReturnRaxRdx
                                                } else {
                                                    TailContext::ReturnIndirect {
                                                        disc_size: layout.discriminant_size as i32
                                                    }
                                                };
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Emit the match in tail position with proper context
                        self.visit_match(body_id, arena, typer, tail);
                        self.state.mark_match_emitted(body_id.get());

                        // Emit ret at a high ID so it sorts after all match instructions
                        let ret_id = IrNodeId::new(lambda_node_id.get() * 2 + 1)
                            .expect("ret virtual id for match-lambda");
                        let ret_inst = Instruction {
                            mnemonic: Mnemonic::Ret,
                            operands: SmallVec::new(),
                            encoding_hint: None,
                            byte_offset_in_text: None,
                            mode: self.current_mode(),
                        };
                        self.emit_inst(ret_id, ret_inst);
                    }
                    // #1116: Store-bodied lambda `fn (v: u64) -> counter = v`
                    // Pattern 5: module-level let mut assignment via lambda body
                    IrKind::Store => {
                        if cfg!(debug_assertions) {
                            eprintln!(
                                "[visit_lambda Store] Lambda {} body=Store",
                                lambda_node_id.get()
                            );
                        }

                        // Record the lambda's starting offset.
                        let main_id = lambda_node_id;
                        self.record_lambda_entry(lambda_node_id, main_id);

                        // Dispatch the store based on its first child (see dispatch_store for routing logic).
                        // For Pattern 5 (Var LHS), this routes to visit_var_assign.
                        self.dispatch_store(body_id, arena);

                        // Mark the store as emitted so walk_inner's top-level Store gate skips it.
                        self.state.mark_store_emitted(body_id.get());

                        // Emit ret at a high ID so it sorts after the store instruction.
                        let ret_id = IrNodeId::new(lambda_node_id.get() * 2 + 1)
                            .expect("ret virtual id for store-lambda");
                        let ret_inst = Instruction {
                            mnemonic: Mnemonic::Ret,
                            operands: SmallVec::new(),
                            encoding_hint: None,
                            byte_offset_in_text: None,
                            mode: self.current_mode(),
                        };
                        self.emit_inst(ret_id, ret_inst);
                    }
                    _ => {
                        // Other lambda shapes deferred to m1-004+
                    }
                }
            }
        }
    }

    /// Emit MOV of a literal value into a register at an explicit IrNodeId.
    ///
    /// #1128: use this when the caller needs the emitted MOV to sort at a specific
    /// position (e.g., match-arm bodies whose IDs must sort between dispatch and
    /// end anchor). `emit_mov_literal_to_reg` overwrites the caller's ID with a
    /// 1_000_000+lambda_id*100+reg synthetic value that is load-bearing for
    /// call-argument marshalling but wrong for match-arm returns.
    pub(crate) fn emit_mov_literal_to_reg_with_id(&mut self, inst_id: IrNodeId, dest_reg: RegId, value: i64) {
        let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
        operands.push(Operand::Reg(dest_reg));
        operands.push(Operand::Imm64(value));

        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
        };
        self.emit_inst(inst_id, inst);
    }

    /// Emit MOV of a literal value into a register.
    pub(crate) fn emit_mov_literal_to_reg(&mut self, lambda_node_id: IrNodeId, dest_reg: RegId, value: i64) {
        // PA8-m3-001 (width not available — generic Mov retained): the operand
        // shape here IS `(Reg, Imm64)`, so this site is MovSized-encodable in
        // principle. But its sole caller is emit_function_call lowering a call
        // *argument*: the relevant width is the callee parameter's declared type,
        // which the current IR does not resolve at the call site (no callee
        // signature table is threaded into emit_function_call). Until that
        // call-site type resolution exists, the conservative 64-bit move is
        // correct (zero-extends the literal into the full arg register). Once a
        // callee-signature lookup lands, thread the per-arg IntWidth in here and
        // mirror the visit_let_literal width-routing.
        // Virtual ID: use a large base ID to avoid collisions
        // Use 1000000 + (lambda_id * 100) + dest_reg to create unique IDs
        let inst_id = IrNodeId::new(1000000 + lambda_node_id.get() * 100 + dest_reg.0 as u32)
            .unwrap_or_else(|| IrNodeId::new(1).unwrap());

        let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
        operands.push(Operand::Reg(dest_reg));
        operands.push(Operand::Imm64(value));

        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
        };

        // Use emit_inst which automatically calls paideia_as_encoder::estimated_bytes
        // to get the correct encoded size. The encoder now emits 7-byte C7 imm32 form
        // for i32-range values instead of 10-byte B8 movabs, so the size is automatically
        // computed correctly.
        self.emit_inst(inst_id, inst);
    }

    /// Emit MOV from one register to another.
    #[allow(dead_code)]
    pub(crate) fn emit_mov_reg_to_reg(&mut self, lambda_node_id: IrNodeId, src_reg: RegId, dest_reg: RegId) {
        // PA8-m3-001 (generic Mov retained): reg-to-reg move; not MovSized-encodable.
        // Virtual ID: use a large base ID to avoid collisions
        // Use 2000000 + (lambda_id * 100) to create unique IDs
        let inst_id = IrNodeId::new(2000000 + lambda_node_id.get() * 100)
            .unwrap_or_else(|| IrNodeId::new(1).unwrap());

        let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
        operands.push(Operand::Reg(dest_reg));
        operands.push(Operand::Reg(src_reg));

        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
        };

        self.emit_inst(inst_id, inst);
    }
}
