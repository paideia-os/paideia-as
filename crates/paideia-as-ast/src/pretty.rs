//! Pretty-printing for AST trees.
//!
//! Functions in this module produce structured indented dumps of AST nodes,
//! showing the variant name and child NodeIds. Useful for snapshot testing
//! in the parser. Functions include [`print_item`], [`print_expr`], [`print_stmt`],
//! [`print_type`], and [`print_pattern`].

use crate::{
    AstArena, ExprData, GenericParam, HandlerArm, ItemData, NodeId, PatternData, StmtData, TypeData,
};

/// Format a single GenericParam for display.
fn format_generic_param(p: &GenericParam) -> String {
    match p {
        GenericParam::Type { name, .. } => format!("{}", name),
        GenericParam::Lifetime { name } => format!("'{}", name),
    }
}

/// Pretty-print an item node as a structured indented dump.
///
/// Produces output like:
/// ```text
/// Module { name: n1, sig: None, body: n2, doc: None }
///   Structure { items: [n3, n4], doc: None }
///     Let { name: n5, ty: None, value: n6, doc: None }
/// ```
///
/// Non-item child nodes (Placeholder, Expr, Type, etc.) are printed by ID only.
pub fn print_item(arena: &AstArena, id: NodeId) -> String {
    let mut output = String::new();
    print_item_internal(arena, id, 0, &mut output);
    output
}

fn print_item_internal(arena: &AstArena, id: NodeId, depth: usize, output: &mut String) {
    let indent = "  ".repeat(depth);

    let Some(item) = arena.item_data(id) else {
        // Not an item node; just print the ID.
        use std::fmt::Write;
        let _ = writeln!(output, "{}(non-item: {})", indent, id);
        return;
    };

    let line = match item {
        ItemData::Module {
            name,
            sig,
            body,
            inner_attrs,
            doc,
        } => {
            let attrs_str = if inner_attrs.is_empty() {
                "[]".to_string()
            } else {
                format!("[{}]", inner_attrs.len())
            };
            format!(
                "Module {{ name: {}, sig: {:?}, body: {}, inner_attrs: {}, doc: {:?} }}",
                name, sig, body, attrs_str, doc
            )
        }
        ItemData::Signature { name, body, doc } => {
            format!(
                "Signature {{ name: {}, body: {}, doc: {:?} }}",
                name, body, doc
            )
        }
        ItemData::Structure {
            items,
            inner_attrs,
            doc,
        } => {
            let items_str = items
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let attrs_str = if inner_attrs.is_empty() {
                "[]".to_string()
            } else {
                format!("[{}]", inner_attrs.len())
            };
            format!(
                "Structure {{ items: [{}], inner_attrs: {}, doc: {:?} }}",
                items_str, attrs_str, doc
            )
        }
        ItemData::Functor { params, body, doc } => {
            let params_str = params
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Functor {{ params: [{}], body: {}, doc: {:?} }}",
                params_str, body, doc
            )
        }
        ItemData::FunctorParam { name, sig } => {
            format!("FunctorParam {{ name: {}, sig: {} }}", name, sig)
        }
        ItemData::Effect { name, ops, doc } => {
            let ops_str = ops
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Effect {{ name: {}, ops: [{}], doc: {:?} }}",
                name, ops_str, doc
            )
        }
        ItemData::OpSig {
            name,
            ty,
            effect_set,
        } => {
            format!(
                "OpSig {{ name: {}, ty: {}, effect_set: {:?} }}",
                name, ty, effect_set
            )
        }
        ItemData::Capability { name, body, doc } => {
            format!(
                "Capability {{ name: {}, body: {}, doc: {:?} }}",
                name, body, doc
            )
        }
        ItemData::Let {
            public,
            mutable,
            name,
            generic_params,
            ty,
            value,
            doc,
        } => {
            format!(
                "Let {{ public: {}, mutable: {}, name: {}, generic_params: [{}], ty: {:?}, value: {}, doc: {:?} }}",
                public,
                mutable,
                name,
                generic_params
                    .iter()
                    .map(format_generic_param)
                    .collect::<Vec<_>>()
                    .join(", "),
                ty,
                value,
                doc
            )
        }
        ItemData::Struct {
            name,
            generic_params,
            fields,
            attributes,
            doc,
        } => {
            let fields_str = fields
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Struct {{ name: {}, generic_params: [{}], fields: [{}], attributes: {}, doc: {:?} }}",
                name,
                generic_params
                    .iter()
                    .map(format_generic_param)
                    .collect::<Vec<_>>()
                    .join(", "),
                fields_str,
                attributes.len(),
                doc
            )
        }
        ItemData::Enum {
            name,
            generic_params,
            variants,
            attributes,
            doc,
        } => {
            let variants_str = variants
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Enum {{ name: {}, generic_params: [{}], variants: [{}], attributes: {}, doc: {:?} }}",
                name,
                generic_params
                    .iter()
                    .map(format_generic_param)
                    .collect::<Vec<_>>()
                    .join(", "),
                variants_str,
                attributes.len(),
                doc
            )
        }
        ItemData::Trait {
            name,
            generic_params,
            associated_types,
            methods,
            doc,
        } => {
            let assoc_types_str = associated_types
                .iter()
                .map(|t| format!("{}", t))
                .collect::<Vec<_>>()
                .join(", ");
            let methods_str = methods
                .iter()
                .map(|m| format!("method(name: {}, params: {})", m.name, m.params.len()))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Trait {{ name: {}, generic_params: [{}], associated_types: [{}], methods: [{}], doc: {:?} }}",
                name,
                generic_params
                    .iter()
                    .map(format_generic_param)
                    .collect::<Vec<_>>()
                    .join(", "),
                assoc_types_str,
                methods_str,
                doc
            )
        }
        ItemData::Impl(impl_decl) => {
            let trait_str = impl_decl
                .trait_name
                .map(|t| format!("{}", t))
                .unwrap_or_else(|| "None".to_string());
            let trait_args_str = impl_decl
                .trait_args
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let methods_str = impl_decl
                .methods
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Impl {{ generic_params: [{}], trait_name: {}, trait_args: [{}], for_type: {}, methods: [{}] }}",
                impl_decl
                    .generic_params
                    .iter()
                    .map(format_generic_param)
                    .collect::<Vec<_>>()
                    .join(", "),
                trait_str,
                trait_args_str,
                impl_decl.for_type,
                methods_str
            )
        }
        ItemData::UnsafeBlock {
            effects,
            capabilities,
            justification,
            block,
        } => {
            let effects_str = effects
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let caps_str = capabilities
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let block_str = block
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "UnsafeBlock {{ effects: [{}], capabilities: [{}], justification: {}, block: [{}] }}",
                effects_str, caps_str, justification, block_str
            )
        }
        ItemData::MacroDecl(macro_data) => {
            let rules_str = macro_data
                .rules
                .iter()
                .map(|rule| format!("rule(pat: {}, tmpl: {})", rule.pattern, rule.template))
                .collect::<Vec<_>>()
                .join("; ");
            format!(
                "MacroDecl {{ name: {}, rules: [{}], doc: {:?} }}",
                macro_data.name, rules_str, macro_data.doc
            )
        }
        ItemData::NonItem => "NonItem".to_string(),
    };

    use std::fmt::Write;
    let _ = writeln!(output, "{}{}", indent, line);

    // Recurse into child items.
    match item {
        ItemData::Module { body, .. } => {
            if let Some(body_data) = arena.item_data(*body)
                && !matches!(body_data, ItemData::NonItem)
            {
                print_item_internal(arena, *body, depth + 1, output);
            }
        }
        ItemData::Signature { body, .. } => {
            if let Some(body_data) = arena.item_data(*body)
                && !matches!(body_data, ItemData::NonItem)
            {
                print_item_internal(arena, *body, depth + 1, output);
            }
        }
        ItemData::Structure { items, .. } => {
            for &item_id in items {
                if let Some(item_data) = arena.item_data(item_id)
                    && !matches!(item_data, ItemData::NonItem)
                {
                    print_item_internal(arena, item_id, depth + 1, output);
                }
            }
        }
        ItemData::Functor { params, body, .. } => {
            for &param_id in params {
                if let Some(param_data) = arena.item_data(param_id)
                    && !matches!(param_data, ItemData::NonItem)
                {
                    print_item_internal(arena, param_id, depth + 1, output);
                }
            }
            if let Some(body_data) = arena.item_data(*body)
                && !matches!(body_data, ItemData::NonItem)
            {
                print_item_internal(arena, *body, depth + 1, output);
            }
        }
        ItemData::Effect { ops, .. } => {
            for &op_id in ops {
                if let Some(op_data) = arena.item_data(op_id)
                    && !matches!(op_data, ItemData::NonItem)
                {
                    print_item_internal(arena, op_id, depth + 1, output);
                }
            }
        }
        // Other variants don't have nested item children.
        _ => {}
    }
}

/// Pretty-print an expression node as a structured indented dump.
pub fn print_expr(arena: &AstArena, id: NodeId) -> String {
    let mut output = String::new();
    print_expr_internal(arena, id, 0, &mut output);
    output
}

fn print_expr_internal(arena: &AstArena, id: NodeId, depth: usize, output: &mut String) {
    let indent = "  ".repeat(depth);

    let Some(expr) = arena.expr_data(id) else {
        use std::fmt::Write;
        let _ = writeln!(output, "{}(non-expr: {})", indent, id);
        return;
    };

    let line = match expr {
        ExprData::Lambda {
            generic_params,
            params,
            body,
            pipe_form,
        } => {
            let params_str = params
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let gen_params_str = generic_params
                .iter()
                .map(format_generic_param)
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Lambda {{ generic_params: [{}], params: [{}], body: {}, pipe_form: {} }}",
                gen_params_str, params_str, body, pipe_form
            )
        }
        ExprData::ActionBlock {
            effects,
            capabilities,
            body,
        } => {
            let body_str = body
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "ActionBlock {{ effects: {:?}, capabilities: {:?}, body: [{}] }}",
                effects, capabilities, body_str
            )
        }
        ExprData::WithHandler {
            handler,
            bind,
            block,
            finally,
        } => {
            format!(
                "WithHandler {{ handler: {}, bind: {}, block: {}, finally: {:?} }}",
                handler, bind, block, finally
            )
        }
        ExprData::Unsafe {
            effects,
            capabilities,
            justification,
            block,
        } => {
            let eff_str = effects
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let cap_str = capabilities
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let block_str = block
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Unsafe {{ effects: [{}], capabilities: [{}], justification: {}, block: [{}] }}",
                eff_str, cap_str, justification, block_str
            )
        }
        ExprData::Infix { lhs, op, rhs } => {
            format!("Infix {{ lhs: {}, op: {}, rhs: {} }}", lhs, op, rhs)
        }
        ExprData::Prefix { op, expr: e, kind } => {
            format!("Prefix {{ op: {}, expr: {}, kind: {:?} }}", op, e, kind)
        }
        ExprData::Postfix { expr: e, op } => {
            format!("Postfix {{ expr: {}, op: {} }}", e, op)
        }
        ExprData::Cast { expr: e, target_ty } => {
            format!("Cast {{ expr: {}, target_ty: {} }}", e, target_ty)
        }
        ExprData::Literal { lit } => {
            format!("Literal {{ lit: {} }}", lit)
        }
        ExprData::Path { segments } => {
            let seg_str = segments
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("Path {{ segments: [{}] }}", seg_str)
        }
        ExprData::Call { callee, args } => {
            let args_str = args
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("Call {{ callee: {}, args: [{}] }}", callee, args_str)
        }
        ExprData::Block { stmts, tail } => {
            let stmts_str = stmts
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("Block {{ stmts: [{}], tail: {:?} }}", stmts_str, tail)
        }
        ExprData::Match { scrutinee, arms } => {
            let arms_str = arms
                .iter()
                .map(|arm| format!("({}, {:?}, {})", arm.pattern, arm.guard, arm.body))
                .collect::<Vec<_>>()
                .join("; ");
            format!("Match {{ scrutinee: {}, arms: [{}] }}", scrutinee, arms_str)
        }
        ExprData::If {
            cond,
            then_block,
            else_block,
        } => {
            format!(
                "If {{ cond: {}, then_block: {}, else_block: {:?} }}",
                cond, then_block, else_block
            )
        }
        ExprData::Loop { kind, header, body } => {
            format!(
                "Loop {{ kind: {:?}, header: {:?}, body: {} }}",
                kind, header, body
            )
        }
        ExprData::For {
            pattern,
            iterable,
            body,
        } => {
            format!(
                "For {{ pattern: {}, iterable: {}, body: {} }}",
                pattern, iterable, body
            )
        }
        ExprData::Break => "Break".to_string(),
        ExprData::Continue => "Continue".to_string(),
        ExprData::OperandRegister { reg } => {
            format!("OperandRegister {{ reg: {} }}", reg)
        }
        ExprData::OperandImmediate { expr: e } => {
            format!("OperandImmediate {{ expr: {} }}", e)
        }
        ExprData::OperandMemoryRef { addr } => {
            format!("OperandMemoryRef {{ addr: {} }}", addr)
        }
        ExprData::Perform { op_path, args } => {
            let args_str = args
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("Perform {{ op_path: {}, args: [{}] }}", op_path, args_str)
        }
        ExprData::Resume { value } => {
            format!("Resume {{ value: {} }}", value)
        }
        ExprData::HandlerValue { effect, arms } => {
            let arms_str = arms
                .iter()
                .map(|arm| match arm {
                    HandlerArm::Op { op, handler } => {
                        format!("Op {{ op: {}, handler: {} }}", op, handler)
                    }
                    HandlerArm::Finally { cleanup } => {
                        format!("Finally {{ cleanup: {} }}", cleanup)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "HandlerValue {{ effect: {}, arms: [{}] }}",
                effect, arms_str
            )
        }
        ExprData::Quote { body } => {
            format!("Quote {{ body: {} }}", body)
        }
        ExprData::Antiquote { value } => {
            format!("Antiquote {{ value: {} }}", value)
        }
        ExprData::FunctorApp {
            functor,
            arguments,
            sharing,
        } => {
            let args_str = arguments
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let sharing_str = sharing
                .iter()
                .map(|c| format!("({} = {})", c.left_path.join("::"), c.right_path.join("::")))
                .collect::<Vec<_>>()
                .join(", ");
            if sharing_str.is_empty() {
                format!(
                    "FunctorApp {{ functor: {}, arguments: [{}] }}",
                    functor, args_str
                )
            } else {
                format!(
                    "FunctorApp {{ functor: {}, arguments: [{}], sharing: [{}] }}",
                    functor, args_str, sharing_str
                )
            }
        }
        ExprData::Pack {
            module_path,
            signature_path,
        } => {
            format!(
                "Pack {{ module_path: {}, signature_path: {} }}",
                module_path, signature_path
            )
        }
        ExprData::Unpack { value } => {
            format!("Unpack {{ value: {} }}", value)
        }
        ExprData::LetModule { name, body, rest } => {
            format!(
                "LetModule {{ name: {}, body: {}, rest: {} }}",
                name, body, rest
            )
        }
        ExprData::RecordCons { type_name, fields } => {
            let fields_str = fields
                .iter()
                .map(|(name, expr)| format!("{}: {}", name, expr))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "RecordCons {{ type_name: {}, fields: [{}] }}",
                type_name, fields_str
            )
        }
        ExprData::FieldAccess { receiver, field } => {
            format!("FieldAccess {{ receiver: {}, field: {} }}", receiver, field)
        }
        ExprData::StringLiteral(s) => {
            format!("StringLiteral({:?})", s)
        }
        ExprData::ByteStringLiteral(b) => {
            format!("ByteStringLiteral({:?})", b)
        }
        ExprData::Borrow { expr, mutable } => {
            format!("Borrow {{ expr: {}, mutable: {} }}", expr, mutable)
        }
        ExprData::Deref { expr } => {
            format!("Deref {{ expr: {} }}", expr)
        }
        ExprData::ArrayLit(elements) => {
            let elements_str = elements
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("ArrayLit([{}])", elements_str)
        }

        ExprData::ArrayRepeat { expr, count } => {
            format!("ArrayRepeat {{ expr: {}, count: {} }}", expr, count)
        }

        ExprData::Uninit => "Uninit".to_string(),
    };

    use std::fmt::Write;
    let _ = writeln!(output, "{}{}", indent, line);
}

/// Pretty-print a statement node as a structured indented dump.
pub fn print_stmt(arena: &AstArena, id: NodeId) -> String {
    let mut output = String::new();
    print_stmt_internal(arena, id, 0, &mut output);
    output
}

fn print_stmt_internal(arena: &AstArena, id: NodeId, depth: usize, output: &mut String) {
    let indent = "  ".repeat(depth);

    let Some(stmt) = arena.stmt_data(id) else {
        use std::fmt::Write;
        let _ = writeln!(output, "{}(non-stmt: {})", indent, id);
        return;
    };

    let line = match stmt {
        StmtData::Let {
            mutable,
            name,
            ty,
            value,
        } => {
            format!(
                "Let {{ mutable: {}, name: {}, ty: {:?}, value: {} }}",
                mutable, name, ty, value
            )
        }
        StmtData::Expr { expr } => {
            format!("Expr {{ expr: {} }}", expr)
        }
        StmtData::Return { value } => {
            format!("Return {{ value: {:?} }}", value)
        }
        StmtData::Instruction { mnemonic, operands } => {
            let ops_str = operands
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Instruction {{ mnemonic: {}, operands: [{}] }}",
                mnemonic, ops_str
            )
        }
        StmtData::Label { name } => {
            format!("Label {{ name: {} }}", name)
        }
    };

    use std::fmt::Write;
    let _ = writeln!(output, "{}{}", indent, line);
}

/// Pretty-print a type node as a structured indented dump.
pub fn print_type(arena: &AstArena, id: NodeId) -> String {
    let mut output = String::new();
    print_type_internal(arena, id, 0, &mut output);
    output
}

fn print_type_internal(arena: &AstArena, id: NodeId, depth: usize, output: &mut String) {
    let indent = "  ".repeat(depth);

    let Some(ty) = arena.type_data(id) else {
        use std::fmt::Write;
        let _ = writeln!(output, "{}(non-type: {})", indent, id);
        return;
    };

    let line = match ty {
        TypeData::Name { name, args } => {
            let args_str = args
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("Name {{ name: {}, args: [{}] }}", name, args_str)
        }
        TypeData::Arrow {
            params,
            ret,
            effects,
            capabilities,
        } => {
            let params_str = params
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Arrow {{ params: [{}], ret: {}, effects: {:?}, capabilities: {:?} }}",
                params_str, ret, effects, capabilities
            )
        }
        TypeData::Tuple { elements } => {
            let elem_str = elements
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("Tuple {{ elements: [{}] }}", elem_str)
        }
        TypeData::LinearClass { class, inner } => {
            format!("LinearClass {{ class: {:?}, inner: {} }}", class, inner)
        }
        TypeData::EffectRow { items, rest } => {
            let items_str = items
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("EffectRow {{ items: [{}], rest: {:?} }}", items_str, rest)
        }
        TypeData::Ptr { pointee } => {
            format!("Ptr {{ pointee: {} }}", pointee)
        }
        TypeData::Ref { pointee, mutable } => {
            format!("Ref {{ pointee: {}, mutable: {} }}", pointee, mutable)
        }
        TypeData::Record { fields } => {
            let fields_str = fields
                .iter()
                .map(|(name, ty)| format!("{}: {}", name, ty))
                .collect::<Vec<_>>()
                .join(", ");
            format!("Record {{ fields: [{}] }}", fields_str)
        }
        TypeData::Enum { variants } => {
            let variants_str = variants
                .iter()
                .map(|v| match v {
                    crate::EnumVariant::Unit { name } => format!("{}", name),
                    crate::EnumVariant::Tuple { name, payload } => {
                        let payload_str = payload
                            .iter()
                            .map(|p| format!("{}", p))
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("{}({})", name, payload_str)
                    }
                    crate::EnumVariant::Record { name, fields } => {
                        let fields_str = fields
                            .iter()
                            .map(|(fname, fty)| format!("{}: {}", fname, fty))
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("{} {{ {} }}", name, fields_str)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("Enum {{ variants: [{}] }}", variants_str)
        }
        TypeData::SelfQualifiedPath { item } => {
            format!("SelfQualifiedPath {{ item: {} }}", item)
        }
        TypeData::Array { element, length } => {
            format!("Array {{ element: {}, length: {} }}", element, length)
        }
    };

    use std::fmt::Write;
    let _ = writeln!(output, "{}{}", indent, line);
}

/// Pretty-print a pattern node as a structured indented dump.
pub fn print_pattern(arena: &AstArena, id: NodeId) -> String {
    let mut output = String::new();
    print_pattern_internal(arena, id, 0, &mut output);
    output
}

fn print_pattern_internal(arena: &AstArena, id: NodeId, depth: usize, output: &mut String) {
    let indent = "  ".repeat(depth);

    let Some(pat) = arena.pattern_data(id) else {
        use std::fmt::Write;
        let _ = writeln!(output, "{}(non-pattern: {})", indent, id);
        return;
    };

    let line = match pat {
        PatternData::Wildcard => "Wildcard".to_string(),
        PatternData::Ident { name, mutable } => {
            format!("Ident {{ name: {}, mutable: {} }}", name, mutable)
        }
        PatternData::Literal { lit } => {
            format!("Literal {{ lit: {} }}", lit)
        }
        PatternData::Tuple { elements } => {
            let elem_str = elements
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("Tuple {{ elements: [{}] }}", elem_str)
        }
        PatternData::Struct { path, fields } => {
            let fields_str = fields
                .iter()
                .map(|f| format!("({}, {})", f.name, f.pattern))
                .collect::<Vec<_>>()
                .join("; ");
            format!("Struct {{ path: {}, fields: [{}] }}", path, fields_str)
        }
        PatternData::EnumVariant { path, args } => {
            let args_str = args
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("EnumVariant {{ path: {}, args: [{}] }}", path, args_str)
        }
        PatternData::Or { alternatives } => {
            let alt_str = alternatives
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("Or {{ alternatives: [{}] }}", alt_str)
        }
        PatternData::Binding { name, inner } => {
            format!("Binding {{ name: {}, inner: {} }}", name, inner)
        }
    };

    use std::fmt::Write;
    let _ = writeln!(output, "{}{}", indent, line);
}

/// Dump every item node in the arena as a top-level S-expression-style tree.
///
/// Walks `arena` in NodeId order and pretty-prints each item-kind node it
/// finds; non-item nodes are skipped (they're emitted as children when the
/// containing item recurses into them).
///
/// Output begins with a header line `(ast-arena nodes=<N>)` so the
/// snapshot is easy to grep. Each subsequent line is one tree from
/// [`print_item`].
///
/// Use this for snapshot tests of the parser output: the output is
/// **idempotent** (re-pretty-printing the same arena yields the same
/// string).
#[must_use]
pub fn dump_arena(arena: &AstArena) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    let _ = writeln!(output, "(ast-arena nodes={})", arena.len());

    // Walk all nodes in allocation order; emit only items here. Items
    // recurse into their children themselves (via print_item).
    for index in 0..arena.len() {
        let id = NodeId::new((index + 1) as u32).expect("non-zero");
        if arena.item_data(id).is_some() {
            output.push_str(&print_item(arena, id));
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ItemData, NodeKind};
    use paideia_as_diagnostics::{FileId, Span};

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn print_module_with_body() {
        let mut arena = AstArena::new();

        // Allocate identifiers and structure
        let module_name = arena.alloc(NodeKind::Ident, span());
        let let_name = arena.alloc(NodeKind::Ident, span());
        let let_value = arena.alloc(NodeKind::Placeholder, span()); // Expression placeholder

        let let_id = arena.alloc_item(
            NodeKind::Let,
            span(),
            ItemData::Let {
                public: false,
                mutable: false,
                name: let_name,
                generic_params: vec![],
                ty: None,
                value: let_value,
                doc: None,
            },
        );

        let structure_id = arena.alloc_item(
            NodeKind::Structure,
            span(),
            ItemData::Structure {
                items: vec![let_id],
                inner_attrs: vec![],
                doc: None,
            },
        );

        let module_id = arena.alloc_item(
            NodeKind::Module,
            span(),
            ItemData::Module {
                name: module_name,
                sig: None,
                body: structure_id,
                inner_attrs: vec![],
                doc: None,
            },
        );

        let output = print_item(&arena, module_id);
        assert!(output.contains("Module {"));
        assert!(output.contains("Structure {"));
        assert!(output.contains("Let {"));
    }

    #[test]
    fn print_non_item_node() {
        let mut arena = AstArena::new();
        let placeholder_id = arena.alloc(NodeKind::Placeholder, span());
        let output = print_item(&arena, placeholder_id);
        assert!(output.contains("non-item"));
    }

    #[test]
    fn print_out_of_range_id() {
        let arena = AstArena::new();
        let stray_id = NodeId::new(999).unwrap();
        let output = print_item(&arena, stray_id);
        assert!(output.contains("non-item"));
    }

    #[test]
    fn print_expr_path() {
        use crate::{ExprData, NodeKind};
        let mut arena = AstArena::new();
        let path_id = arena.alloc(NodeKind::Ident, span());
        let expr_id = arena.alloc_expr(
            NodeKind::ExprPath,
            span(),
            ExprData::Path {
                segments: vec![path_id],
            },
        );
        let output = print_expr(&arena, expr_id);
        assert!(output.contains("Path"));
    }

    #[test]
    fn print_stmt_expr() {
        use crate::{NodeKind, StmtData};
        let mut arena = AstArena::new();
        let inner_expr = arena.alloc(NodeKind::Placeholder, span());
        let stmt_id = arena.alloc_stmt(
            NodeKind::StmtExpr,
            span(),
            StmtData::Expr { expr: inner_expr },
        );
        let output = print_stmt(&arena, stmt_id);
        assert!(output.contains("Expr"));
    }

    #[test]
    fn print_type_name() {
        use crate::{NodeKind, TypeData};
        let mut arena = AstArena::new();
        let name_id = arena.alloc(NodeKind::Ident, span());
        let type_id = arena.alloc_type(
            NodeKind::TypeName,
            span(),
            TypeData::Name {
                name: name_id,
                args: vec![],
            },
        );
        let output = print_type(&arena, type_id);
        assert!(output.contains("Name"));
    }

    #[test]
    fn print_pattern_wildcard() {
        use crate::{NodeKind, PatternData};
        let mut arena = AstArena::new();
        let pat_id = arena.alloc_pattern(NodeKind::PatWildcard, span(), PatternData::Wildcard);
        let output = print_pattern(&arena, pat_id);
        assert!(output.contains("Wildcard"));
    }
}
