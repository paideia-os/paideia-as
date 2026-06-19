//! Signature, Structure, Functor AST nodes for ML-style modules.
//!
//! This module provides the scaffolding for phase-2 module system support,
//! including signature interfaces, structure implementations, and functors
//! (parameterized structures).

use paideia_as_diagnostics::Span;

/// A signature — the interface a structure must satisfy.
///
/// Signatures define the declarations that a structure must implement,
/// similar to interfaces or abstract base classes in other languages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    /// Declarations in the signature.
    pub decls: Vec<SigDecl>,
    /// Source location.
    pub span: Span,
}

/// A single declaration within a signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SigDecl {
    /// `type t : Kind` or `type t = T`.
    ///
    /// Type member declaration: either abstract (no definition) or
    /// concrete (transparent type abbreviation).
    Type(TypeDecl),
    /// `val x : T`.
    ///
    /// Value member declaration: specifies the type of a value.
    Val(ValDecl),
    /// `module M : S`.
    ///
    /// Nested module member with signature ascription.
    Module(ModuleDecl),
    /// `include S`.
    ///
    /// Signature inclusion: re-exports all declarations from S.
    Include(IncludeDecl),
}

/// A type declaration within a signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeDecl {
    /// Name of the type member.
    pub name: String,
    /// Optional explicit definition for transparent type abbreviation.
    /// `None` indicates an abstract type.
    pub definition: Option<Box<TypeAbstraction>>,
    /// Source location.
    pub span: Span,
}

/// Type definition in a signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeAbstraction {
    /// A concrete type definition.
    Concrete(String),
}

/// A value declaration within a signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValDecl {
    /// Name of the value member.
    pub name: String,
    /// Type of the value.
    pub ty: String,
    /// Source location.
    pub span: Span,
}

/// A nested module declaration within a signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleDecl {
    /// Name of the nested module.
    pub name: String,
    /// Signature that the module must satisfy.
    pub signature: Box<Signature>,
    /// Source location.
    pub span: Span,
}

/// A signature inclusion declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IncludeDecl {
    /// Name of the signature being included.
    pub signature_name: String,
    /// Source location.
    pub span: Span,
}

/// A structure — implementations of a signature's declarations.
///
/// Structures provide implementations for the members declared in a signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Structure {
    /// Definitions in the structure.
    pub defs: Vec<Def>,
    /// Source location.
    pub span: Span,
}

/// A single definition within a structure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Def {
    /// `type t = T`.
    ///
    /// Type member definition.
    Type {
        /// Name of the type.
        name: String,
        /// Type definition.
        ty: String,
        /// Source location.
        span: Span,
    },
    /// `val x = e` or `let x = e`.
    ///
    /// Value member definition.
    Val {
        /// Name of the value.
        name: String,
        /// Value expression.
        expr: String,
        /// Source location.
        span: Span,
    },
    /// `module M = S` or `module M : Sig = S`.
    ///
    /// Nested module definition, optionally with signature ascription.
    Module {
        /// Name of the nested module.
        name: String,
        /// Optional signature ascription.
        ascription: Option<Box<Signature>>,
        /// Module body.
        body: Box<Structure>,
        /// Source location.
        span: Span,
    },
}

/// A functor — a parameterized structure.
///
/// Functors allow structures to be parameterized over other structures
/// satisfying a given signature, enabling generic module composition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Functor {
    /// Name of the formal parameter.
    pub param_name: String,
    /// Signature that the parameter must satisfy.
    pub param_signature: Box<Signature>,
    /// Functor body structure.
    pub body: Box<Structure>,
    /// Source location.
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn test_span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    /// Test roundtrip of empty signature.
    #[test]
    fn empty_signature_roundtrip() {
        let sig = Signature {
            decls: vec![],
            span: test_span(),
        };
        let cloned = sig.clone();
        assert_eq!(sig, cloned);
    }

    /// Test signature with type, value, module, and include declarations.
    #[test]
    fn signature_with_type_val_module_include_decls() {
        let ty_decl = SigDecl::Type(TypeDecl {
            name: "t".to_string(),
            definition: None,
            span: test_span(),
        });

        let val_decl = SigDecl::Val(ValDecl {
            name: "x".to_string(),
            ty: "int".to_string(),
            span: test_span(),
        });

        let mod_decl = SigDecl::Module(ModuleDecl {
            name: "M".to_string(),
            signature: Box::new(Signature {
                decls: vec![],
                span: test_span(),
            }),
            span: test_span(),
        });

        let inc_decl = SigDecl::Include(IncludeDecl {
            signature_name: "S".to_string(),
            span: test_span(),
        });

        let sig = Signature {
            decls: vec![ty_decl, val_decl, mod_decl, inc_decl],
            span: test_span(),
        };

        assert_eq!(sig.decls.len(), 4);
        assert!(matches!(sig.decls[0], SigDecl::Type(_)));
        assert!(matches!(sig.decls[1], SigDecl::Val(_)));
        assert!(matches!(sig.decls[2], SigDecl::Module(_)));
        assert!(matches!(sig.decls[3], SigDecl::Include(_)));

        let cloned = sig.clone();
        assert_eq!(sig, cloned);
    }

    /// Test roundtrip of empty structure.
    #[test]
    fn empty_structure_roundtrip() {
        let s = Structure {
            defs: vec![],
            span: test_span(),
        };
        let cloned = s.clone();
        assert_eq!(s, cloned);
    }

    /// Test structure with type, value, and module definitions.
    #[test]
    fn structure_with_type_val_module_defs() {
        let ty_def = Def::Type {
            name: "t".to_string(),
            ty: "int".to_string(),
            span: test_span(),
        };

        let val_def = Def::Val {
            name: "x".to_string(),
            expr: "42".to_string(),
            span: test_span(),
        };

        let mod_def = Def::Module {
            name: "M".to_string(),
            ascription: None,
            body: Box::new(Structure {
                defs: vec![],
                span: test_span(),
            }),
            span: test_span(),
        };

        let s = Structure {
            defs: vec![ty_def, val_def, mod_def],
            span: test_span(),
        };

        assert_eq!(s.defs.len(), 3);
        assert!(matches!(s.defs[0], Def::Type { .. }));
        assert!(matches!(s.defs[1], Def::Val { .. }));
        assert!(matches!(s.defs[2], Def::Module { .. }));

        let cloned = s.clone();
        assert_eq!(s, cloned);
    }

    /// Test functor with parameter and body.
    #[test]
    fn functor_with_param_and_body() {
        let param_sig = Box::new(Signature {
            decls: vec![],
            span: test_span(),
        });

        let body = Box::new(Structure {
            defs: vec![],
            span: test_span(),
        });

        let functor = Functor {
            param_name: "Arg".to_string(),
            param_signature: param_sig,
            body,
            span: test_span(),
        };

        assert_eq!(functor.param_name, "Arg");
        assert_eq!(functor.param_signature.decls.len(), 0);
        assert_eq!(functor.body.defs.len(), 0);
    }

    /// Test nested module declarations at two levels deep.
    #[test]
    fn nested_module_decl_recurses_signature() {
        let inner_sig = Signature {
            decls: vec![],
            span: test_span(),
        };

        let inner_mod = ModuleDecl {
            name: "Inner".to_string(),
            signature: Box::new(inner_sig),
            span: test_span(),
        };

        let outer_sig = Signature {
            decls: vec![SigDecl::Module(inner_mod)],
            span: test_span(),
        };

        let outer_mod = ModuleDecl {
            name: "Outer".to_string(),
            signature: Box::new(outer_sig),
            span: test_span(),
        };

        let root_sig = Signature {
            decls: vec![SigDecl::Module(outer_mod)],
            span: test_span(),
        };

        assert_eq!(root_sig.decls.len(), 1);
        if let SigDecl::Module(mod_decl) = &root_sig.decls[0] {
            assert_eq!(mod_decl.name, "Outer");
            assert_eq!(mod_decl.signature.decls.len(), 1);
            if let SigDecl::Module(inner_decl) = &mod_decl.signature.decls[0] {
                assert_eq!(inner_decl.name, "Inner");
            } else {
                panic!("expected nested ModuleDecl");
            }
        } else {
            panic!("expected Module variant");
        }
    }

    /// Acceptance test 3: signature with 3 specific declarations in order.
    #[test]
    fn snapshot_signature_with_3_decls_shape() {
        let decls = vec![
            SigDecl::Type(TypeDecl {
                name: "t".to_string(),
                definition: Some(Box::new(TypeAbstraction::Concrete("int".to_string()))),
                span: test_span(),
            }),
            SigDecl::Val(ValDecl {
                name: "value".to_string(),
                ty: "t".to_string(),
                span: test_span(),
            }),
            SigDecl::Include(IncludeDecl {
                signature_name: "Base".to_string(),
                span: test_span(),
            }),
        ];

        let sig = Signature {
            decls,
            span: test_span(),
        };

        assert_eq!(sig.decls.len(), 3);
        match &sig.decls[0] {
            SigDecl::Type(ty_decl) => {
                assert_eq!(ty_decl.name, "t");
                assert!(ty_decl.definition.is_some());
            }
            _ => panic!("expected Type variant"),
        }
        match &sig.decls[1] {
            SigDecl::Val(val_decl) => {
                assert_eq!(val_decl.name, "value");
                assert_eq!(val_decl.ty, "t");
            }
            _ => panic!("expected Val variant"),
        }
        match &sig.decls[2] {
            SigDecl::Include(inc_decl) => {
                assert_eq!(inc_decl.signature_name, "Base");
            }
            _ => panic!("expected Include variant"),
        }
    }

    /// Acceptance test 3: signature containing only an Include.
    #[test]
    fn snapshot_signature_with_include_only() {
        let sig = Signature {
            decls: vec![SigDecl::Include(IncludeDecl {
                signature_name: "S".to_string(),
                span: test_span(),
            })],
            span: test_span(),
        };

        assert_eq!(sig.decls.len(), 1);
        match &sig.decls[0] {
            SigDecl::Include(inc_decl) => {
                assert_eq!(inc_decl.signature_name, "S");
            }
            _ => panic!("expected Include variant"),
        }
    }

    /// Acceptance test 3: signature with a single ModuleDecl.
    #[test]
    fn snapshot_signature_with_module_decl_only() {
        let nested_sig = Signature {
            decls: vec![SigDecl::Val(ValDecl {
                name: "x".to_string(),
                ty: "int".to_string(),
                span: test_span(),
            })],
            span: test_span(),
        };

        let sig = Signature {
            decls: vec![SigDecl::Module(ModuleDecl {
                name: "M".to_string(),
                signature: Box::new(nested_sig),
                span: test_span(),
            })],
            span: test_span(),
        };

        assert_eq!(sig.decls.len(), 1);
        match &sig.decls[0] {
            SigDecl::Module(mod_decl) => {
                assert_eq!(mod_decl.name, "M");
                assert_eq!(mod_decl.signature.decls.len(), 1);
                match &mod_decl.signature.decls[0] {
                    SigDecl::Val(val_decl) => {
                        assert_eq!(val_decl.name, "x");
                        assert_eq!(val_decl.ty, "int");
                    }
                    _ => panic!("expected Val variant in nested signature"),
                }
            }
            _ => panic!("expected Module variant"),
        }
    }
}
