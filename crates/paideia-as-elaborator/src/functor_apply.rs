//! Applicative functor application as an elaboration form.
//!
//! This module implements functor application: given a functor F and an argument module M,
//! elaborates F(M) to a structure with M's bindings visible under the parameter name.
//!
//! # Phase-2-m5-005
//!
//! - Parameter visibility is implemented via a stand-in: the parameter binding is prepended
//!   to the result's bindings at index 0.
//! - Path equality uses BLAKE3 over parameter name, functor body span, and argument signature
//!   to produce cache keys. This is temporary; m5-006 will replace with structural identity
//!   once the module registry exists.
//! - No real substitution of parameter name references in body type expressions.
//! - No cross-functor identity (BLAKE3 over span is the stand-in).
//! - No interaction with ModuleKind::Pi from m5-002 — that's m5-007.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use paideia_as_ast::{Functor, SigDecl};
use paideia_as_diagnostics::Diagnostic;
use paideia_as_ir::LinClass;

use crate::linearity_ctx::LinearityCtx;
use crate::modules::{FieldBinding, TypedValue, ValueRef, elaborate_structure, symbol_of};
use crate::sig_match::match_signature;

/// Diagnostic code for when the argument fails signature matching.
/// Reuses M_SIG_MISSING_DECL (301) and M_SIG_KIND_MISMATCH (302) from sig_match.
pub const M_SIG_MISSING_DECL: u16 = 301;
/// Diagnostic code for when a binding kind mismatches signature requirement.
pub const M_SIG_KIND_MISMATCH: u16 = 302;

/// Applicative path equality cache key.
///
/// Derived from BLAKE3 hash over: functor parameter name, functor body span,
/// and argument signature (as Debug format).
///
/// Phase-2-m5-005: identity stands in for the (functor-id, arg-hash) Leroy applicative key;
/// m5-006 replaces with structural identity once the module registry exists.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct ApplyKey(pub [u8; 32]);

/// Module-level cache for applicative functor application results.
///
/// Maps from (parameter name, body span, argument signature hash) to the elaborated result.
/// This cache ensures that F(M) == F(M) along the same path (path equality).
static APPLY_CACHE: OnceLock<Mutex<HashMap<ApplyKey, TypedValue>>> = OnceLock::new();

/// Compute the applicative cache key.
///
/// Hashes: functor parameter name bytes, functor body span (file_id + start + end LE),
/// and argument signature Debug format.
fn compute_apply_key(functor: &Functor, argument: &TypedValue) -> ApplyKey {
    let mut hasher = blake3::Hasher::new();

    // Hash parameter name
    hasher.update(functor.param_name.as_bytes());

    // Hash span: file_id as u32, byte_start, byte_end in little-endian
    let span = functor.body.span;
    hasher.update(&span.file().get().to_le_bytes());
    hasher.update(&span.byte_start().to_le_bytes());
    hasher.update(&span.byte_end().to_le_bytes());

    // Hash argument signature
    hasher.update(format!("{:?}", argument.signature).as_bytes());

    ApplyKey(hasher.finalize().into())
}

/// Apply a functor to an argument module.
///
/// Steps:
/// 1. Check that the argument matches the functor's parameter signature via [`match_signature`].
///    If it fails, emit diagnostics and return None.
/// 2. Check the cache; if hit, return the cached TypedValue clone.
/// 3. Otherwise, elaborate the functor body into a fresh LinearityCtx.
/// 4. Prepend a parameter binding (stand-in for parameter visibility) at index 0.
/// 5. Insert the result into the cache.
/// 6. Return Some(body_tv).
pub fn apply_functor(
    functor: &Functor,
    argument: &TypedValue,
    diags: &mut Vec<Diagnostic>,
) -> Option<TypedValue> {
    // Step 1: match_signature first.
    if !match_signature(argument, &functor.param_signature, diags) {
        return None;
    }

    // Step 2: check cache.
    let key = compute_apply_key(functor, argument);
    {
        let cache = APPLY_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Ok(cache_guard) = cache.lock()
            && let Some(cached) = cache_guard.get(&key)
        {
            return Some(cached.clone());
        }
    }

    // Step 3: elaborate the functor body.
    let mut ctx = LinearityCtx::new();
    let mut body_tv = elaborate_structure(&functor.body, &mut ctx, diags);

    // Step 4: prepend parameter binding (phase-2-m5-005 stand-in for parameter visibility).
    let param_binding = FieldBinding {
        name: functor.param_name.clone(),
        ty_id: 0,
        value: ValueRef::Module(Box::new(argument.clone())),
        class: paideia_as_ir::LinClass::Unrestricted,
        span: functor.span,
    };
    body_tv.bindings.insert(0, param_binding);

    // Step 5: insert into cache.
    {
        let cache = APPLY_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Ok(mut cache_guard) = cache.lock() {
            cache_guard.insert(key, body_tv.clone());
        }
    }

    // Step 6: return result.
    Some(body_tv)
}

/// Derive a linearity class from a type string's prefix.
///
/// Reuses the m5-003 `"linear:"` convention and adds `"affine:"` for AC 2.
/// Any other type string gets [`LinClass::Unrestricted`].
fn class_from_ty_prefix(ty: &str) -> LinClass {
    if ty.strip_prefix("linear:").is_some() {
        LinClass::Linear
    } else if ty.strip_prefix("affine:").is_some() {
        LinClass::Affine
    } else {
        LinClass::Unrestricted
    }
}

/// Count how many times a name token appears in a structure.
fn count_name_occurrences(s: &paideia_as_ast::Structure, name: &str) -> usize {
    s.defs.iter().fold(0, |acc, def| {
        acc + match def {
            paideia_as_ast::Def::Val { expr, .. } => {
                // Count all occurrences of name as a token in expr.
                let mut count = 0;
                let bytes = expr.as_bytes();
                let name_bytes = name.as_bytes();
                let name_len = name_bytes.len();

                for start in 0..bytes.len() {
                    if bytes[start..].starts_with(name_bytes) {
                        let end = start + name_len;
                        let before_ok = start == 0 || !is_word_char(bytes[start - 1]);
                        let after_ok = end == bytes.len() || !is_word_char(bytes[end]);
                        if before_ok && after_ok {
                            count += 1;
                        }
                    }
                }
                count
            }
            paideia_as_ast::Def::Type { ty, .. } => {
                // Count in type definitions too
                let mut count = 0;
                let bytes = ty.as_bytes();
                let name_bytes = name.as_bytes();
                let name_len = name_bytes.len();

                for start in 0..bytes.len() {
                    if bytes[start..].starts_with(name_bytes) {
                        let end = start + name_len;
                        let before_ok = start == 0 || !is_word_char(bytes[start - 1]);
                        let after_ok = end == bytes.len() || !is_word_char(bytes[end]);
                        if before_ok && after_ok {
                            count += 1;
                        }
                    }
                }
                count
            }
            paideia_as_ast::Def::Module { body, .. } => {
                // Recurse into nested modules
                count_name_occurrences(body, name)
            }
        }
    })
}

/// Helper to check if a byte is a word character.
fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Elaborate the functor body with abstract argument parameter binding.
///
/// # Returns
///
/// A [`TypedValue`] representing the elaborated body. No parameter-name prepending
/// occurs here; that is [`apply_functor`]'s concern.
///
/// # Diagnostics
///
/// Emits S0900 (never used) and S0901 (overused) diagnostics for parameter
/// fields that violate their linearity class constraints.
pub fn elaborate_functor_body(functor: &Functor, diags: &mut Vec<Diagnostic>) -> TypedValue {
    let mut ctx = LinearityCtx::new();
    let mut param_syms: Vec<(u32, LinClass, paideia_as_diagnostics::Span, String)> = Vec::new();

    // Bind each parameter field from the signature.
    for decl in &functor.param_signature.decls {
        if let SigDecl::Val(vd) = decl {
            let class = class_from_ty_prefix(&vd.ty);
            let sym = symbol_of(&vd.name);
            ctx.bind(sym, class, vd.span);
            param_syms.push((sym, class, vd.span, vd.name.clone()));
        }
    }

    // Scan the body for all occurrences of each parameter field.
    for (sym, _class, _span, name) in &param_syms {
        let count = count_name_occurrences(&functor.body, name);
        for _ in 0..count {
            ctx.use_(*sym);
        }
    }

    // Elaborate the functor body.
    // elaborate_structure will validate the scope (which includes param bindings and body defs).
    elaborate_structure(&functor.body, &mut ctx, diags)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::{Signature, Structure, ValDecl};
    use paideia_as_diagnostics::{FileId, Span};
    use paideia_as_types::{SigDeclKind, SignatureKind};

    use crate::check_linearity::{S_NEVER_USED, S_OVERUSED};

    fn span(start: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), start, 1)
    }

    fn empty_structure() -> Structure {
        Structure {
            defs: vec![],
            span: span(0),
        }
    }

    fn empty_signature() -> Signature {
        Signature {
            decls: vec![],
            span: span(0),
        }
    }

    fn empty_typed_value() -> TypedValue {
        TypedValue {
            bindings: vec![],
            signature: SignatureKind::default(),
            span: span(0),
        }
    }

    /// Test 1: empty functor body yields param binding only.
    #[test]
    fn empty_functor_body_yields_param_binding_only() {
        let functor = Functor {
            param_name: "M".to_string(),
            param_signature: Box::new(empty_signature()),
            body: Box::new(empty_structure()),
            span: span(100), // Use a different span to avoid cache collision.
        };

        let argument = empty_typed_value();
        let mut diags = Vec::new();

        let result = apply_functor(&functor, &argument, &mut diags);
        assert!(result.is_some());
        let result = result.unwrap();

        assert_eq!(result.bindings.len(), 1);
        assert_eq!(result.bindings[0].name, "M");
        assert!(matches!(result.bindings[0].value, ValueRef::Module(_)));
        assert!(diags.is_empty());
    }

    /// Test 2: body with three fields preserves them plus param.
    #[test]
    fn body_with_three_fields_preserves_them_plus_param() {
        use paideia_as_ast::Def as AstDef;

        let structure = Structure {
            defs: vec![
                AstDef::Val {
                    name: "x".to_string(),
                    expr: "10".to_string(),
                    span: span(1),
                },
                AstDef::Val {
                    name: "y".to_string(),
                    expr: "20".to_string(),
                    span: span(2),
                },
                AstDef::Val {
                    name: "z".to_string(),
                    expr: "30".to_string(),
                    span: span(3),
                },
            ],
            span: span(101), // Use a different span to avoid cache collision.
        };

        let functor = Functor {
            param_name: "M".to_string(),
            param_signature: Box::new(empty_signature()),
            body: Box::new(structure),
            span: span(101), // Match body span.
        };

        let argument = empty_typed_value();
        let mut diags = Vec::new();

        let result = apply_functor(&functor, &argument, &mut diags);
        assert!(result.is_some());
        let result = result.unwrap();

        assert_eq!(result.bindings.len(), 4);
        assert_eq!(result.bindings[0].name, "M");
        assert_eq!(result.bindings[1].name, "x");
        assert_eq!(result.bindings[2].name, "y");
        assert_eq!(result.bindings[3].name, "z");
        assert!(diags.is_empty());
    }

    /// Test 3: argument failing signature returns None and emits.
    #[test]
    fn argument_failing_signature_returns_none_and_emits() {
        let signature = Signature {
            decls: vec![SigDecl::Val(ValDecl {
                name: "required".to_string(),
                ty: "int".to_string(),
                span: span(10),
            })],
            span: span(0),
        };

        let functor = Functor {
            param_name: "M".to_string(),
            param_signature: Box::new(signature),
            body: Box::new(empty_structure()),
            span: span(102), // Use a different span to avoid cache collision.
        };

        // Argument is empty (no "required" binding).
        let argument = empty_typed_value();
        let mut diags = Vec::new();

        let result = apply_functor(&functor, &argument, &mut diags);
        assert!(result.is_none());
        assert!(!diags.is_empty());
        // Check that at least one diagnostic has code 301 or 302.
        let has_relevant_code = diags.iter().any(|d| {
            let code = d.code();
            let n = code.number();
            n == M_SIG_MISSING_DECL || n == M_SIG_KIND_MISMATCH
        });
        assert!(has_relevant_code);
    }

    /// Test 4: same argument twice yields same apply key.
    #[test]
    fn same_argument_twice_yields_same_apply_key() {
        let functor = Functor {
            param_name: "M".to_string(),
            param_signature: Box::new(empty_signature()),
            body: Box::new(empty_structure()),
            span: span(103), // Use a different span.
        };

        let argument = empty_typed_value();

        let key1 = compute_apply_key(&functor, &argument);
        let key2 = compute_apply_key(&functor, &argument);

        assert_eq!(key1, key2);
    }

    /// Test 5: different argument yields different apply key.
    #[test]
    fn different_argument_yields_different_apply_key() {
        let functor = Functor {
            param_name: "M".to_string(),
            param_signature: Box::new(empty_signature()),
            body: Box::new(empty_structure()),
            span: span(104), // Use a different span.
        };

        let arg1 = empty_typed_value();

        let mut arg2 = empty_typed_value();
        arg2.bindings.push(FieldBinding {
            name: "extra".to_string(),
            ty_id: 0,
            value: ValueRef::Val("123".to_string()),
            class: LinClass::Unrestricted,
            span: span(5),
        });
        // Update signature to match the new binding (so it's a different structure signature).
        arg2.signature = SignatureKind {
            decls: vec![SigDeclKind::Val {
                name: "extra".to_string(),
                ty_id: 0,
            }],
        };

        let key1 = compute_apply_key(&functor, &arg1);
        let key2 = compute_apply_key(&functor, &arg2);

        assert_ne!(key1, key2);
    }

    /// Test 6: argument with extras still matches (subsumption).
    #[test]
    fn argument_with_extras_still_matches() {
        let signature = empty_signature(); // no requirements

        let functor = Functor {
            param_name: "M".to_string(),
            param_signature: Box::new(signature),
            body: Box::new(empty_structure()),
            span: span(105), // Use a different span.
        };

        // Argument has an extra binding not in signature (subsumption).
        let mut argument = empty_typed_value();
        argument.bindings.push(FieldBinding {
            name: "extra".to_string(),
            ty_id: 0,
            value: ValueRef::Val("456".to_string()),
            class: LinClass::Unrestricted,
            span: span(5),
        });

        let mut diags = Vec::new();
        let result = apply_functor(&functor, &argument, &mut diags);

        assert!(result.is_some());
        assert!(diags.is_empty());
    }

    // Test suite for elaborate_functor_body

    /// Test 1: empty body and empty signature yields no diagnostics.
    #[test]
    fn empty_body_empty_sig_no_diags() {
        let functor = Functor {
            param_name: "M".to_string(),
            param_signature: Box::new(empty_signature()),
            body: Box::new(empty_structure()),
            span: span(200),
        };

        let mut diags = Vec::new();
        let result = elaborate_functor_body(&functor, &mut diags);

        assert!(result.bindings.is_empty());
        assert!(diags.is_empty());
    }

    /// Test 2: linear parameter field used once yields no diagnostics.
    #[test]
    fn linear_param_field_used_once_no_diags() {
        use paideia_as_ast::Def;

        let sig = Signature {
            decls: vec![SigDecl::Val(ValDecl {
                name: "r".to_string(),
                ty: "linear:Res".to_string(),
                span: span(10),
            })],
            span: span(0),
        };

        let body = Structure {
            defs: vec![Def::Val {
                name: "y".to_string(),
                expr: "use r".to_string(),
                span: span(20),
            }],
            span: span(15),
        };

        let functor = Functor {
            param_name: "M".to_string(),
            param_signature: Box::new(sig),
            body: Box::new(body),
            span: span(201),
        };

        let mut diags = Vec::new();
        let result = elaborate_functor_body(&functor, &mut diags);

        assert_eq!(result.bindings.len(), 1); // only "y" from body
        assert!(diags.is_empty());
    }

    /// Test 3: linear parameter field unused emits S0900.
    #[test]
    fn linear_param_field_unused_emits_s0900() {
        use paideia_as_ast::Def;

        let sig = Signature {
            decls: vec![SigDecl::Val(ValDecl {
                name: "r".to_string(),
                ty: "linear:Res".to_string(),
                span: span(10),
            })],
            span: span(0),
        };

        let body = Structure {
            defs: vec![Def::Val {
                name: "y".to_string(),
                expr: "42".to_string(),
                span: span(20),
            }],
            span: span(15),
        };

        let functor = Functor {
            param_name: "M".to_string(),
            param_signature: Box::new(sig),
            body: Box::new(body),
            span: span(202),
        };

        let mut diags = Vec::new();
        let result = elaborate_functor_body(&functor, &mut diags);

        assert_eq!(result.bindings.len(), 1); // only "y" from body
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_NEVER_USED);
    }

    /// Test 4: affine parameter field used twice emits S0901.
    #[test]
    fn affine_param_field_used_twice_emits_s0901() {
        use paideia_as_ast::Def;

        let sig = Signature {
            decls: vec![SigDecl::Val(ValDecl {
                name: "a".to_string(),
                ty: "affine:Res".to_string(),
                span: span(10),
            })],
            span: span(0),
        };

        let body = Structure {
            defs: vec![
                Def::Val {
                    name: "x".to_string(),
                    expr: "use a".to_string(),
                    span: span(20),
                },
                Def::Val {
                    name: "y".to_string(),
                    expr: "use a again".to_string(),
                    span: span(30),
                },
            ],
            span: span(15),
        };

        let functor = Functor {
            param_name: "M".to_string(),
            param_signature: Box::new(sig),
            body: Box::new(body),
            span: span(203),
        };

        let mut diags = Vec::new();
        let result = elaborate_functor_body(&functor, &mut diags);

        assert_eq!(result.bindings.len(), 2); // "x" and "y" from body
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_OVERUSED);
    }

    /// Test 5: mixed linear (unused) and unrestricted parameter fields.
    #[test]
    fn mixed_linear_unrestricted_only_linear_flagged() {
        use paideia_as_ast::Def;

        let sig = Signature {
            decls: vec![
                SigDecl::Val(ValDecl {
                    name: "r".to_string(),
                    ty: "linear:Res".to_string(),
                    span: span(10),
                }),
                SigDecl::Val(ValDecl {
                    name: "k".to_string(),
                    ty: "int".to_string(), // Unrestricted (no prefix)
                    span: span(11),
                }),
            ],
            span: span(0),
        };

        let body = Structure {
            defs: vec![Def::Val {
                name: "y".to_string(),
                expr: "42".to_string(),
                span: span(20),
            }],
            span: span(15),
        };

        let functor = Functor {
            param_name: "M".to_string(),
            param_signature: Box::new(sig),
            body: Box::new(body),
            span: span(204),
        };

        let mut diags = Vec::new();
        let result = elaborate_functor_body(&functor, &mut diags);

        assert_eq!(result.bindings.len(), 1); // only "y" from body
        // Only "r" (linear, unused) should emit S0900; "k" is Unrestricted so no diagnostic.
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_NEVER_USED);
    }
}
