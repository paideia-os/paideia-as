//! Signature matching — structural subtyping for module ascription.
//!
//! Checks that a structure satisfies a signature by walking the signature's
//! declarations and verifying each is present in the structure with compatible
//! kind and type.
//!
//! # Phase-2-m5-004
//!
//! - Linearity is NOT re-walked; m5-003's [`elaborate_structure`] already
//!   validated linear bindings.
//! - TypeId comparison is placeholder (both 0 today, trivially match).
//! - Real kind/type unification is deferred; we use string-equality on
//!   concrete type definitions as a stand-in.
//! - `include` directives are deferred to m5-006+ when a signature registry
//!   is available.

use crate::modules::{FieldBinding, TypedValue, ValueRef};
use paideia_as_ast::{ModuleDecl, SigDecl, Signature, TypeAbstraction, TypeDecl, ValDecl};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity};
use std::collections::HashMap;

/// Diagnostic code for "structure missing signature declaration".
pub const M_SIG_MISSING_DECL: u16 = 301;

/// Diagnostic code for "structure binding kind mismatches signature".
pub const M_SIG_KIND_MISMATCH: u16 = 302;

/// Check that a structure satisfies a signature via structural subtyping.
///
/// Walks all declarations in `target` signature. The structure may have
/// additional bindings (not required by the signature) — these are silently
/// ignored. Emits one diagnostic per mismatch (missing field or kind/type
/// incompatibility) and returns `false` if any mismatch is found; returns
/// `true` if all required declarations are present and compatible.
///
/// # Linearity
///
/// Linearity is **not** re-checked. The structure's bindings have already
/// been validated for linearity in m5-003's [`elaborate_structure`]. This
/// function assumes linearity is already correct and focuses on kind and
/// type compatibility.
///
/// [`elaborate_structure`]: crate::modules::elaborate_structure
pub fn match_signature(
    structure: &TypedValue,
    target: &Signature,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    // Build fast lookup for structure bindings by name.
    let by_name: HashMap<&str, &FieldBinding> = structure
        .bindings
        .iter()
        .map(|b| (b.name.as_str(), b))
        .collect();

    let mut ok = true;

    // Walk every declaration in the target signature (no short-circuit).
    for decl in &target.decls {
        match decl {
            SigDecl::Type(type_decl) => {
                if !check_type_decl(&by_name, type_decl, diags) {
                    ok = false;
                }
            }
            SigDecl::Val(val_decl) => {
                if !check_val_decl(&by_name, val_decl, diags) {
                    ok = false;
                }
            }
            SigDecl::Module(module_decl) => {
                if !check_module_decl(&by_name, module_decl, diags) {
                    ok = false;
                }
            }
            SigDecl::Include(_) => {
                // phase-2-m5-004 deferral: resolution wires through a signature registry in m5-006+.
            }
        }
    }

    ok
}

/// Check a type declaration against structure bindings.
fn check_type_decl(
    by_name: &HashMap<&str, &FieldBinding>,
    decl: &TypeDecl,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let name = &decl.name;

    match by_name.get(name.as_str()) {
        None => {
            // Missing declaration: emit M0301.
            diags.push(
                Diagnostic::error(m_code(M_SIG_MISSING_DECL))
                    .message(format!(
                        "signature declares '{name}' but structure does not provide it"
                    ))
                    .with_span(decl.span)
                    .finish(),
            );
            false
        }
        Some(binding) => {
            // Check kind: must be ValueRef::Type.
            match &binding.value {
                ValueRef::Type(s) => {
                    // If signature has a concrete type definition, compare strings.
                    if let Some(definition) = &decl.definition {
                        let TypeAbstraction::Concrete(t) = &**definition;
                        if s != t {
                            diags.push(
                                Diagnostic::error(m_code(M_SIG_KIND_MISMATCH))
                                    .message(format!(
                                        "structure binding '{name}' has type {s:?}, but signature requires {t:?}"
                                    ))
                                    .with_span(decl.span)
                                    .finish(),
                            );
                            return false;
                        }
                    }
                    true
                }
                ValueRef::Val(_) | ValueRef::Module(_) => {
                    // Kind mismatch: signature expects Type, structure provides something else.
                    diags.push(
                        Diagnostic::error(m_code(M_SIG_KIND_MISMATCH))
                            .message(format!(
                                "signature declares '{name}' as a type, but structure provides a value/module"
                            ))
                            .with_span(decl.span)
                            .finish(),
                    );
                    false
                }
            }
        }
    }
}

/// Check a val declaration against structure bindings.
fn check_val_decl(
    by_name: &HashMap<&str, &FieldBinding>,
    decl: &ValDecl,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let name = &decl.name;

    match by_name.get(name.as_str()) {
        None => {
            // Missing declaration: emit M0301.
            diags.push(
                Diagnostic::error(m_code(M_SIG_MISSING_DECL))
                    .message(format!(
                        "signature declares '{name}' but structure does not provide it"
                    ))
                    .with_span(decl.span)
                    .finish(),
            );
            false
        }
        Some(binding) => {
            // Check kind: must be ValueRef::Val.
            match &binding.value {
                ValueRef::Val(_) => {
                    // In phase-2-m5-003, both binding.ty_id and the sig val type id are 0 (placeholder).
                    // Trivially match. Real type checking deferred to later phases.
                    true
                }
                ValueRef::Type(_) | ValueRef::Module(_) => {
                    // Kind mismatch: signature expects Val, structure provides something else.
                    diags.push(
                        Diagnostic::error(m_code(M_SIG_KIND_MISMATCH))
                            .message(format!(
                                "signature declares '{name}' as a value, but structure provides a type/module"
                            ))
                            .with_span(decl.span)
                            .finish(),
                    );
                    false
                }
            }
        }
    }
}

/// Check a module declaration against structure bindings.
fn check_module_decl(
    by_name: &HashMap<&str, &FieldBinding>,
    decl: &ModuleDecl,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let name = &decl.name;

    match by_name.get(name.as_str()) {
        None => {
            // Missing declaration: emit M0301.
            diags.push(
                Diagnostic::error(m_code(M_SIG_MISSING_DECL))
                    .message(format!(
                        "signature declares '{name}' but structure does not provide it"
                    ))
                    .with_span(decl.span)
                    .finish(),
            );
            false
        }
        Some(binding) => {
            // Check kind: must be ValueRef::Module.
            match &binding.value {
                ValueRef::Module(inner) => {
                    // Recursively check the nested module.
                    match_signature(inner, &decl.signature, diags)
                }
                ValueRef::Type(_) | ValueRef::Val(_) => {
                    // Kind mismatch: signature expects Module, structure provides something else.
                    diags.push(
                        Diagnostic::error(m_code(M_SIG_KIND_MISMATCH))
                            .message(format!(
                                "signature declares '{name}' as a module, but structure provides a type/value"
                            ))
                            .with_span(decl.span)
                            .finish(),
                    );
                    false
                }
            }
        }
    }
}

/// Helper to construct a diagnostic code for category M.
fn m_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::M, Severity::Error, n).expect("valid M code")
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::{ModuleDecl, SigDecl, Signature, TypeDecl, ValDecl};
    use paideia_as_diagnostics::{FileId, Span};
    use paideia_as_ir::LinClass;

    fn span(start: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), start, 1)
    }

    /// Test 1: empty signature matches empty structure.
    #[test]
    fn empty_signature_matches_empty_structure() {
        let structure = TypedValue {
            bindings: vec![],
            signature: Default::default(),
            span: span(0),
        };

        let target = Signature {
            decls: vec![],
            span: span(0),
        };

        let mut diags = Vec::new();
        let result = match_signature(&structure, &target, &mut diags);

        assert!(result);
        assert!(diags.is_empty());
    }

    /// Test 2: signature matches structure exactly.
    #[test]
    fn signature_matches_structure_exactly() {
        let structure = TypedValue {
            bindings: vec![
                FieldBinding {
                    name: "t".to_string(),
                    ty_id: 0,
                    value: ValueRef::Type("int".to_string()),
                    class: LinClass::Unrestricted,
                    span: span(1),
                },
                FieldBinding {
                    name: "x".to_string(),
                    ty_id: 0,
                    value: ValueRef::Val("42".to_string()),
                    class: LinClass::Unrestricted,
                    span: span(2),
                },
                FieldBinding {
                    name: "M".to_string(),
                    ty_id: 0,
                    value: ValueRef::Module(Box::new(TypedValue {
                        bindings: vec![],
                        signature: Default::default(),
                        span: span(3),
                    })),
                    class: LinClass::Unrestricted,
                    span: span(3),
                },
            ],
            signature: Default::default(),
            span: span(0),
        };

        let target = Signature {
            decls: vec![
                SigDecl::Type(TypeDecl {
                    name: "t".to_string(),
                    definition: None,
                    span: span(10),
                }),
                SigDecl::Val(ValDecl {
                    name: "x".to_string(),
                    ty: "int".to_string(),
                    span: span(11),
                }),
                SigDecl::Module(ModuleDecl {
                    name: "M".to_string(),
                    signature: Box::new(Signature {
                        decls: vec![],
                        span: span(12),
                    }),
                    span: span(12),
                }),
            ],
            span: span(10),
        };

        let mut diags = Vec::new();
        let result = match_signature(&structure, &target, &mut diags);

        assert!(result);
        assert!(diags.is_empty());
    }

    /// Test 3 (AC 1): signature satisfied by structure with extras.
    #[test]
    fn signature_satisfied_by_structure_with_extras() {
        let structure = TypedValue {
            bindings: vec![
                FieldBinding {
                    name: "t".to_string(),
                    ty_id: 0,
                    value: ValueRef::Type("int".to_string()),
                    class: LinClass::Unrestricted,
                    span: span(1),
                },
                FieldBinding {
                    name: "x".to_string(),
                    ty_id: 0,
                    value: ValueRef::Val("42".to_string()),
                    class: LinClass::Unrestricted,
                    span: span(2),
                },
                FieldBinding {
                    name: "M".to_string(),
                    ty_id: 0,
                    value: ValueRef::Module(Box::new(TypedValue {
                        bindings: vec![],
                        signature: Default::default(),
                        span: span(3),
                    })),
                    class: LinClass::Unrestricted,
                    span: span(3),
                },
            ],
            signature: Default::default(),
            span: span(0),
        };

        // Signature only requires x; extras (t, M) are ignored.
        let target = Signature {
            decls: vec![SigDecl::Val(ValDecl {
                name: "x".to_string(),
                ty: "int".to_string(),
                span: span(10),
            })],
            span: span(10),
        };

        let mut diags = Vec::new();
        let result = match_signature(&structure, &target, &mut diags);

        assert!(result);
        assert!(diags.is_empty());
    }

    /// Test 4 (AC 2): missing field emits M0301.
    #[test]
    fn missing_field_emits_m0301() {
        let structure = TypedValue {
            bindings: vec![FieldBinding {
                name: "y".to_string(),
                ty_id: 0,
                value: ValueRef::Val("99".to_string()),
                class: LinClass::Unrestricted,
                span: span(1),
            }],
            signature: Default::default(),
            span: span(0),
        };

        let target = Signature {
            decls: vec![SigDecl::Val(ValDecl {
                name: "x".to_string(),
                ty: "int".to_string(),
                span: span(10),
            })],
            span: span(10),
        };

        let mut diags = Vec::new();
        let result = match_signature(&structure, &target, &mut diags);

        assert!(!result);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), M_SIG_MISSING_DECL);
    }

    /// Test 5 (AC 3): kind mismatch emits M0302.
    #[test]
    fn kind_mismatch_emits_m0302() {
        let structure = TypedValue {
            bindings: vec![FieldBinding {
                name: "x".to_string(),
                ty_id: 0,
                value: ValueRef::Type("int".to_string()),
                class: LinClass::Unrestricted,
                span: span(1),
            }],
            signature: Default::default(),
            span: span(0),
        };

        let target = Signature {
            decls: vec![SigDecl::Val(ValDecl {
                name: "x".to_string(),
                ty: "int".to_string(),
                span: span(10),
            })],
            span: span(10),
        };

        let mut diags = Vec::new();
        let result = match_signature(&structure, &target, &mut diags);

        assert!(!result);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), M_SIG_KIND_MISMATCH);
    }

    /// Test 6: nested module declaration recurses.
    #[test]
    fn nested_module_decl_recurses() {
        let inner_structure = TypedValue {
            bindings: vec![FieldBinding {
                name: "y".to_string(),
                ty_id: 0,
                value: ValueRef::Val("10".to_string()),
                class: LinClass::Unrestricted,
                span: span(2),
            }],
            signature: Default::default(),
            span: span(0),
        };

        let structure = TypedValue {
            bindings: vec![FieldBinding {
                name: "M".to_string(),
                ty_id: 0,
                value: ValueRef::Module(Box::new(inner_structure)),
                class: LinClass::Unrestricted,
                span: span(1),
            }],
            signature: Default::default(),
            span: span(0),
        };

        let target = Signature {
            decls: vec![SigDecl::Module(ModuleDecl {
                name: "M".to_string(),
                signature: Box::new(Signature {
                    decls: vec![SigDecl::Val(ValDecl {
                        name: "y".to_string(),
                        ty: "int".to_string(),
                        span: span(20),
                    })],
                    span: span(19),
                }),
                span: span(19),
            })],
            span: span(19),
        };

        let mut diags = Vec::new();
        let result = match_signature(&structure, &target, &mut diags);

        assert!(result);
        assert!(diags.is_empty());
    }

    /// Test 6b: nested module mismatch propagates M0301 with inner decl span.
    #[test]
    fn nested_module_mismatch_propagates() {
        let inner_structure = TypedValue {
            bindings: vec![FieldBinding {
                name: "z".to_string(), // Wrong name, doesn't match sig's "y"
                ty_id: 0,
                value: ValueRef::Val("10".to_string()),
                class: LinClass::Unrestricted,
                span: span(2),
            }],
            signature: Default::default(),
            span: span(0),
        };

        let structure = TypedValue {
            bindings: vec![FieldBinding {
                name: "M".to_string(),
                ty_id: 0,
                value: ValueRef::Module(Box::new(inner_structure)),
                class: LinClass::Unrestricted,
                span: span(1),
            }],
            signature: Default::default(),
            span: span(0),
        };

        let inner_y_span = span(20);
        let target = Signature {
            decls: vec![SigDecl::Module(ModuleDecl {
                name: "M".to_string(),
                signature: Box::new(Signature {
                    decls: vec![SigDecl::Val(ValDecl {
                        name: "y".to_string(),
                        ty: "int".to_string(),
                        span: inner_y_span,
                    })],
                    span: span(19),
                }),
                span: span(19),
            })],
            span: span(19),
        };

        let mut diags = Vec::new();
        let result = match_signature(&structure, &target, &mut diags);

        assert!(!result);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), M_SIG_MISSING_DECL);
        // Verify the diagnostic has the inner decl's span.
        let diag_span = diags[0].primary_span().expect("should have primary span");
        assert_eq!(diag_span.byte_start(), inner_y_span.byte_start());
    }
}
