//! Type lattice classes (kinds).
//!
//! The kind of a type determines its substructural properties: linearity,
//! affinity, etc. This module types those properties and computes the kind
//! of any interned type via structural examination.
//!
//! Module-level kinds capture the structure of module types (signatures)
//! and functor types (Π-kinds over signatures).

use paideia_as_ast::{Functor, Signature};
use paideia_as_ir::LinClass;

use crate::types::{Type, TypeId};

/// The lattice class assigned to a type.
///
/// This is an alias to [`paideia_as_ir::LinClass`], but typed separately
/// here so future type-level refinements (e.g., kinds beyond linearity)
/// don't drag the IR crate. Phase-1 uses only `Unrestricted` and `Linear`.
pub type Kind = LinClass;

/// The kind of a module's type — either a structure's signature
/// (concrete kind) or a functor's Π-kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleKind {
    /// A structure's kind is its signature's kind.
    Sig(SignatureKind),
    /// A functor's kind is Π(x : sig_in). sig_out.
    /// `dependent` indicates whether the body refers to the argument.
    Pi {
        /// Name of the formal parameter.
        param_name: String,
        /// Kind of the parameter signature.
        param_kind: SignatureKind,
        /// Kind of the functor body.
        body_kind: Box<ModuleKind>,
        /// Whether the body refers to the parameter.
        dependent: bool,
    },
}

/// The kind of a signature — its declarations + their kinds + linearity.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SignatureKind {
    /// Declarations in the signature kind.
    pub decls: Vec<SigDeclKind>,
}

/// A single declaration kind within a signature kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SigDeclKind {
    /// `type t : K` — abstract type with kind K.
    Type {
        /// Name of the type declaration.
        name: String,
        /// Kind of the type.
        kind: Kind,
    },
    /// `val x : T` — value of type T.
    Val {
        /// Name of the value declaration.
        name: String,
        /// Type ID placeholder (m5-003 wires real type ids).
        ty_id: usize,
    },
    /// `module M : S` — nested module of signature kind S.
    Module {
        /// Name of the nested module.
        name: String,
        /// Kind of the nested module's signature.
        kind: SignatureKind,
    },
}

/// Determine the kind of an interned type.
///
/// Phase-1 rules:
/// - Primitives (`Unit`, `Bool`, `Char`, `UInt`, `SInt`, `Float`), tuples,
///   named types, function arrows, `Bot` → `Unrestricted` (freely copyable).
/// - `Top` → `Unrestricted` (the universal type is always free).
/// - `Named { name: 1, .. }` (the capability placeholder) → `Linear`
///   (capabilities cannot be copied). The AC mentions a `Cap<T>` family;
///   phase-1 encodes this as reserved name index `1`.
///
/// Note: The AC says "Type::kind(t) returns … Top for Top". Since the
/// lattice doesn't currently have a `Top` kind variant, phase-1 maps
/// `Top` (the type) → `Unrestricted` (the kind). Alternative: extend
/// `LinClass` with a `Top` variant. **Phase-1 decision: keep `LinClass`
/// unchanged; document the divergence.**
pub fn type_kind(_type_id: TypeId, ty: &Type) -> Kind {
    match ty {
        Type::Named { name: 1, .. } => Kind::Linear,
        Type::Var(_) => Kind::Unrestricted,
        _ => Kind::Unrestricted,
    }
}

/// Infer the kind of a Signature.
///
/// Phase-2-m5-002 minimum: walks the SigDecl list and produces a
/// SignatureKind whose declarations mirror the AST in shape. Type
/// declarations get kind Kind::Star (or whatever the AST records).
/// Val declarations get a placeholder ty_id = 0; m5-003 wires real
/// type ids.
pub fn kind_signature(sig: &Signature) -> SignatureKind {
    let decls = sig
        .decls
        .iter()
        .filter_map(|decl| {
            use paideia_as_ast::SigDecl;
            match decl {
                SigDecl::Type(ty_decl) => {
                    Some(SigDeclKind::Type {
                        name: ty_decl.name.clone(),
                        kind: Kind::Unrestricted, // Default to Unrestricted for phase-2
                    })
                }
                SigDecl::Val(val_decl) => {
                    Some(SigDeclKind::Val {
                        name: val_decl.name.clone(),
                        ty_id: 0, // Placeholder; m5-003 wires real type ids
                    })
                }
                SigDecl::Module(mod_decl) => {
                    let nested_kind = kind_signature(&mod_decl.signature);
                    Some(SigDeclKind::Module {
                        name: mod_decl.name.clone(),
                        kind: nested_kind,
                    })
                }
                SigDecl::Include(_) => {
                    // Include declarations are not reflected in the kind;
                    // they are resolved during elaboration.
                    None
                }
            }
        })
        .collect();

    SignatureKind { decls }
}

/// Kind-check a Functor: produces the Π kind.
pub fn kind_functor(functor: &Functor) -> ModuleKind {
    let param_kind = kind_signature(&functor.param_signature);
    let body_kind = Box::new(ModuleKind::Sig(kind_signature(&Signature {
        decls: vec![], // Placeholder: actual body signature inference happens in elaboration
        span: functor.span,
    })));

    ModuleKind::Pi {
        param_name: functor.param_name.clone(),
        param_kind,
        body_kind,
        dependent: false, // Default to non-dependent; elaboration refines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CapSetId, Type};
    use paideia_as_ast::{Functor, ModuleDecl, SigDecl, Signature, TypeDecl, ValDecl};
    use paideia_as_diagnostics::Span;
    use paideia_as_ir::EffectRowId;

    fn test_span() -> Span {
        use paideia_as_diagnostics::FileId;
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn primitives_are_unrestricted() {
        let unit = Type::Unit;
        let bool_ty = Type::Bool;
        let char_ty = Type::Char;
        let u8_ty = Type::UInt(8);
        let i8_ty = Type::SInt(8);
        let f32_ty = Type::Float(32);

        assert_eq!(
            type_kind(TypeId::new(1).unwrap(), &unit),
            Kind::Unrestricted
        );
        assert_eq!(
            type_kind(TypeId::new(1).unwrap(), &bool_ty),
            Kind::Unrestricted
        );
        assert_eq!(
            type_kind(TypeId::new(1).unwrap(), &char_ty),
            Kind::Unrestricted
        );
        assert_eq!(
            type_kind(TypeId::new(1).unwrap(), &u8_ty),
            Kind::Unrestricted
        );
        assert_eq!(
            type_kind(TypeId::new(1).unwrap(), &i8_ty),
            Kind::Unrestricted
        );
        assert_eq!(
            type_kind(TypeId::new(1).unwrap(), &f32_ty),
            Kind::Unrestricted
        );
    }

    #[test]
    fn top_and_bot_are_unrestricted() {
        let top = Type::Top;
        let bot = Type::Bot;

        assert_eq!(type_kind(TypeId::new(1).unwrap(), &top), Kind::Unrestricted);
        assert_eq!(type_kind(TypeId::new(1).unwrap(), &bot), Kind::Unrestricted);
    }

    #[test]
    fn tuples_are_unrestricted() {
        let tuple = Type::Tuple(vec![]);

        assert_eq!(
            type_kind(TypeId::new(1).unwrap(), &tuple),
            Kind::Unrestricted
        );
    }

    #[test]
    fn function_types_are_unrestricted() {
        let fn_ty = Type::Fn {
            params: vec![],
            ret: TypeId::new(1).unwrap(),
            effects: EffectRowId::EMPTY,
            caps: CapSetId::EMPTY,
        };

        assert_eq!(
            type_kind(TypeId::new(1).unwrap(), &fn_ty),
            Kind::Unrestricted
        );
    }

    #[test]
    fn named_other_types_are_unrestricted() {
        let named = Type::Named {
            name: 42,
            args: vec![],
        };

        assert_eq!(
            type_kind(TypeId::new(1).unwrap(), &named),
            Kind::Unrestricted
        );
    }

    #[test]
    fn cap_placeholder_is_linear() {
        let cap_placeholder = Type::Named {
            name: 1,
            args: vec![],
        };

        assert_eq!(
            type_kind(TypeId::new(1).unwrap(), &cap_placeholder),
            Kind::Linear
        );
    }

    #[test]
    fn cap_placeholder_with_args_is_linear() {
        let cap_with_arg = Type::Named {
            name: 1,
            args: vec![TypeId::new(1).unwrap()],
        };

        assert_eq!(
            type_kind(TypeId::new(1).unwrap(), &cap_with_arg),
            Kind::Linear
        );
    }

    #[test]
    fn type_var_is_unrestricted() {
        let ty_var = Type::Var(crate::types::TyVar::new(1).unwrap());

        assert_eq!(
            type_kind(TypeId::new(1).unwrap(), &ty_var),
            Kind::Unrestricted
        );
    }

    #[test]
    fn term_type_is_unrestricted() {
        let term = Type::Term;

        assert_eq!(
            type_kind(TypeId::new(1).unwrap(), &term),
            Kind::Unrestricted
        );
    }

    // Module and signature kind tests

    #[test]
    fn empty_signature_kinds_to_empty() {
        let sig = Signature {
            decls: vec![],
            span: test_span(),
        };

        let kind = kind_signature(&sig);
        assert_eq!(kind.decls.len(), 0);
        assert_eq!(kind, SignatureKind::default());
    }

    #[test]
    fn signature_with_type_decl_kinds_correctly() {
        let sig = Signature {
            decls: vec![SigDecl::Type(TypeDecl {
                name: "t".to_string(),
                definition: None,
                span: test_span(),
            })],
            span: test_span(),
        };

        let kind = kind_signature(&sig);
        assert_eq!(kind.decls.len(), 1);
        match &kind.decls[0] {
            SigDeclKind::Type { name, kind: k } => {
                assert_eq!(name, "t");
                assert_eq!(*k, Kind::Unrestricted);
            }
            _ => panic!("expected Type variant"),
        }
    }

    #[test]
    fn signature_with_val_decl_uses_placeholder_ty_id() {
        let sig = Signature {
            decls: vec![SigDecl::Val(ValDecl {
                name: "x".to_string(),
                ty: "int".to_string(),
                span: test_span(),
            })],
            span: test_span(),
        };

        let kind = kind_signature(&sig);
        assert_eq!(kind.decls.len(), 1);
        match &kind.decls[0] {
            SigDeclKind::Val { name, ty_id } => {
                assert_eq!(name, "x");
                assert_eq!(*ty_id, 0); // placeholder
            }
            _ => panic!("expected Val variant"),
        }
    }

    #[test]
    fn signature_with_nested_module_decl_recurses() {
        let inner_sig = Signature {
            decls: vec![SigDecl::Type(TypeDecl {
                name: "t".to_string(),
                definition: None,
                span: test_span(),
            })],
            span: test_span(),
        };

        let sig = Signature {
            decls: vec![SigDecl::Module(ModuleDecl {
                name: "M".to_string(),
                signature: Box::new(inner_sig),
                span: test_span(),
            })],
            span: test_span(),
        };

        let kind = kind_signature(&sig);
        assert_eq!(kind.decls.len(), 1);
        match &kind.decls[0] {
            SigDeclKind::Module { name, kind: nested } => {
                assert_eq!(name, "M");
                assert_eq!(nested.decls.len(), 1);
                match &nested.decls[0] {
                    SigDeclKind::Type {
                        name: inner_name, ..
                    } => {
                        assert_eq!(inner_name, "t");
                    }
                    _ => panic!("expected nested Type variant"),
                }
            }
            _ => panic!("expected Module variant"),
        }
    }

    #[test]
    fn functor_kinds_to_pi() {
        let param_sig = Signature {
            decls: vec![],
            span: test_span(),
        };

        let body = paideia_as_ast::Structure {
            defs: vec![],
            span: test_span(),
        };

        let functor = Functor {
            param_name: "Arg".to_string(),
            param_signature: Box::new(param_sig),
            body: Box::new(body),
            span: test_span(),
        };

        let kind = kind_functor(&functor);
        match kind {
            ModuleKind::Pi {
                param_name,
                param_kind,
                body_kind,
                dependent,
            } => {
                assert_eq!(param_name, "Arg");
                assert_eq!(param_kind.decls.len(), 0);
                assert!(!dependent);
                match *body_kind {
                    ModuleKind::Sig(sig_kind) => {
                        assert_eq!(sig_kind.decls.len(), 0);
                    }
                    _ => panic!("expected Sig variant in body"),
                }
            }
            _ => panic!("expected Pi variant"),
        }
    }

    #[test]
    fn functor_pi_records_param_name() {
        let param_sig = Signature {
            decls: vec![],
            span: test_span(),
        };

        let body = paideia_as_ast::Structure {
            defs: vec![],
            span: test_span(),
        };

        let functor = Functor {
            param_name: "MyParam".to_string(),
            param_signature: Box::new(param_sig),
            body: Box::new(body),
            span: test_span(),
        };

        let kind = kind_functor(&functor);
        match kind {
            ModuleKind::Pi { param_name, .. } => {
                assert_eq!(param_name, "MyParam");
            }
            _ => panic!("expected Pi variant"),
        }
    }

    #[test]
    fn functor_pi_records_param_signature_kind() {
        let param_sig = Signature {
            decls: vec![SigDecl::Val(ValDecl {
                name: "x".to_string(),
                ty: "int".to_string(),
                span: test_span(),
            })],
            span: test_span(),
        };

        let body = paideia_as_ast::Structure {
            defs: vec![],
            span: test_span(),
        };

        let functor = Functor {
            param_name: "Arg".to_string(),
            param_signature: Box::new(param_sig),
            body: Box::new(body),
            span: test_span(),
        };

        let kind = kind_functor(&functor);
        match kind {
            ModuleKind::Pi { param_kind, .. } => {
                assert_eq!(param_kind.decls.len(), 1);
                match &param_kind.decls[0] {
                    SigDeclKind::Val { name, .. } => {
                        assert_eq!(name, "x");
                    }
                    _ => panic!("expected Val variant"),
                }
            }
            _ => panic!("expected Pi variant"),
        }
    }

    #[test]
    fn snapshot_3_decl_signature_kind() {
        let sig = Signature {
            decls: vec![
                SigDecl::Type(TypeDecl {
                    name: "t".to_string(),
                    definition: None,
                    span: test_span(),
                }),
                SigDecl::Val(ValDecl {
                    name: "value".to_string(),
                    ty: "t".to_string(),
                    span: test_span(),
                }),
                SigDecl::Module(ModuleDecl {
                    name: "M".to_string(),
                    signature: Box::new(Signature {
                        decls: vec![],
                        span: test_span(),
                    }),
                    span: test_span(),
                }),
            ],
            span: test_span(),
        };

        let kind = kind_signature(&sig);
        assert_eq!(kind.decls.len(), 3);

        match &kind.decls[0] {
            SigDeclKind::Type { name, .. } => {
                assert_eq!(name, "t");
            }
            _ => panic!("expected Type variant"),
        }

        match &kind.decls[1] {
            SigDeclKind::Val { name, .. } => {
                assert_eq!(name, "value");
            }
            _ => panic!("expected Val variant"),
        }

        match &kind.decls[2] {
            SigDeclKind::Module { name, .. } => {
                assert_eq!(name, "M");
            }
            _ => panic!("expected Module variant"),
        }
    }

    #[test]
    fn snapshot_functor_pi_shape() {
        let param_sig = Signature {
            decls: vec![
                SigDecl::Type(TypeDecl {
                    name: "elem_t".to_string(),
                    definition: None,
                    span: test_span(),
                }),
                SigDecl::Val(ValDecl {
                    name: "length".to_string(),
                    ty: "usize".to_string(),
                    span: test_span(),
                }),
            ],
            span: test_span(),
        };

        let body = paideia_as_ast::Structure {
            defs: vec![],
            span: test_span(),
        };

        let functor = Functor {
            param_name: "Elem".to_string(),
            param_signature: Box::new(param_sig),
            body: Box::new(body),
            span: test_span(),
        };

        let kind = kind_functor(&functor);
        match kind {
            ModuleKind::Pi {
                param_name,
                param_kind,
                body_kind,
                dependent,
            } => {
                assert_eq!(param_name, "Elem");
                assert_eq!(param_kind.decls.len(), 2);
                assert!(!dependent);
                match *body_kind {
                    ModuleKind::Sig(_) => {
                        // OK
                    }
                    _ => panic!("expected Sig in body"),
                }
            }
            _ => panic!("expected Pi variant"),
        }
    }

    #[test]
    fn module_kind_clone_equality() {
        let sig_kind = SignatureKind {
            decls: vec![SigDeclKind::Type {
                name: "t".to_string(),
                kind: Kind::Unrestricted,
            }],
        };

        let module_kind = ModuleKind::Sig(sig_kind.clone());
        let cloned = module_kind.clone();

        assert_eq!(module_kind, cloned);
    }
}
