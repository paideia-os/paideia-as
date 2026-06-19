//! Module elaboration — structure → typed value.
//!
//! Elaborates an AST [`paideia_as_ast::Structure`] into a [`TypedValue`] by:
//! 1. Walking the structure's definitions directly (no AST reconstruction).
//! 2. Emitting [`SigDeclKind`] per arm of each definition.
//! 3. Threading linearity bindings through the scope via [`LinearityCtx`].
//!
//! Phase-2-m5-003 minimum: TypeId is a `usize` placeholder (0 = unresolved).
//! Linearity is inferred from expression string prefixes ("linear:...").

use paideia_as_ast::{Def, Structure};
use paideia_as_diagnostics::{Diagnostic, Span};
use paideia_as_ir::LinClass;
use paideia_as_types::{SigDeclKind, SignatureKind};

use crate::check_linearity::{S_OVERUSED, validate_scope};
use crate::env::Symbol;
use crate::linearity_ctx::LinearityCtx;

/// Diagnostic code alias for S0901 (overused binding).
///
/// Re-exports [`S_OVERUSED`] under the module-elaboration–specific name so that
/// callers do not need to import check_linearity directly.
pub const S_LINEAR_FIELD_OVERUSED: u16 = S_OVERUSED;

/// A typed value binding in a structure's elaboration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldBinding {
    /// Name of the field.
    pub name: String,
    /// Type ID placeholder: 0 = unresolved (phase-2-m5-003).
    pub ty_id: usize,
    /// The value reference.
    pub value: ValueRef,
    /// Linearity class of this binding.
    pub class: LinClass,
    /// Source location.
    pub span: Span,
}

/// Reference to a value within a structure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueRef {
    /// Mirrors [`Def::Val`] — a value expression as a string placeholder.
    Val(String),
    /// Mirrors [`Def::Type`] — a type expression as a string placeholder.
    Type(String),
    /// Mirrors [`Def::Module`] — a nested module as a recursive TypedValue.
    Module(Box<TypedValue>),
}

/// A typed value — the result of elaborating a [`Structure`].
///
/// Contains a flat list of field bindings and the computed signature kind.
/// The signature is inferred by walking the structure's definitions directly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedValue {
    /// Bindings for each field in the structure.
    pub bindings: Vec<FieldBinding>,
    /// Inferred signature kind.
    pub signature: SignatureKind,
    /// Source location of the structure.
    pub span: Span,
}

/// Elaborate a structure into a typed value with linearity checking.
///
/// Walks the structure's definitions directly, threading linearity bindings
/// through the scope via the provided [`LinearityCtx`]. Emits diagnostics for
/// linearity violations (e.g., S0900, S0901).
///
/// # Phase-2-m5-003
///
/// - TypeId is a placeholder (`0` = unresolved).
/// - Linearity is derived from expression string prefixes:
///   - If `expr` starts with `"linear:"`, use [`LinClass::Linear`].
///   - Otherwise, use [`LinClass::Unrestricted`] (default).
///
/// This is a **stand-in for parser integration** to allow the reject fixture
/// to specify linearity without parser changes. In phase-3, linearity will be
/// derived from the source syntax (e.g., `let x: Linear[T] = ...`).
pub fn elaborate_structure(
    s: &Structure,
    ctx: &mut LinearityCtx,
    diags: &mut Vec<Diagnostic>,
) -> TypedValue {
    let mut bindings = Vec::new();
    let mut sig_decls = Vec::new();

    // Walk definitions and build bindings + signature declarations.
    for def in &s.defs {
        match def {
            Def::Type { name, ty, span } => {
                // Type definition: emit Type declaration, add to bindings.
                bindings.push(FieldBinding {
                    name: name.clone(),
                    ty_id: 0,
                    value: ValueRef::Type(ty.clone()),
                    class: LinClass::Unrestricted,
                    span: *span,
                });

                sig_decls.push(SigDeclKind::Type {
                    name: name.clone(),
                    kind: LinClass::Unrestricted,
                });
            }

            Def::Val { name, expr, span } => {
                // Value definition: derive linearity class from expr string.
                let class = if expr.starts_with("linear:") {
                    LinClass::Linear
                } else {
                    LinClass::Unrestricted
                };

                // Compute Symbol: fold bytes with wrapping_mul and wrapping_add.
                let sym: Symbol = name
                    .bytes()
                    .fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));

                // Bind the symbol in the linearity context.
                ctx.bind(sym, class, *span);

                bindings.push(FieldBinding {
                    name: name.clone(),
                    ty_id: 0,
                    value: ValueRef::Val(expr.clone()),
                    class,
                    span: *span,
                });

                sig_decls.push(SigDeclKind::Val {
                    name: name.clone(),
                    ty_id: 0,
                });

                // Thread through: scan subsequent definitions for uses of this name.
                for subsequent_def in
                    s.defs[s.defs.iter().position(|d| d == def).unwrap() + 1..].iter()
                {
                    match subsequent_def {
                        Def::Val {
                            expr: next_expr, ..
                        } => {
                            // Whole-word substring check: name appears as a token in next_expr.
                            if contains_name_token(next_expr, name) {
                                ctx.use_(sym);
                            }
                        }
                        Def::Module { body, .. } => {
                            // Scan module body for uses.
                            if structure_contains_name_token(body, name) {
                                ctx.use_(sym);
                            }
                        }
                        _ => {}
                    }
                }
            }

            Def::Module {
                name, body, span, ..
            } => {
                // Module definition: enter scope, recurse, leave scope.
                // elaborate_structure validates its own innermost scope at return,
                // so we do NOT re-validate nested_scope here — that would double-emit.
                ctx.enter_scope();
                let nested_typed = elaborate_structure(body, ctx, diags);
                let _nested_scope = ctx.leave_scope();

                bindings.push(FieldBinding {
                    name: name.clone(),
                    ty_id: 0,
                    value: ValueRef::Module(Box::new(nested_typed.clone())),
                    class: LinClass::Unrestricted,
                    span: *span,
                });

                sig_decls.push(SigDeclKind::Module {
                    name: name.clone(),
                    kind: nested_typed.signature.clone(),
                });
            }
        }
    }

    // Validate the outer scope before returning.
    let scope = ctx.innermost().clone();
    let scope_diags = validate_scope(&scope);
    diags.extend(scope_diags);

    TypedValue {
        bindings,
        signature: SignatureKind { decls: sig_decls },
        span: s.span,
    }
}

/// Check if a string contains a name as a whole-word token.
///
/// Uses ASCII word-boundary semantics: the match is only accepted when neither
/// the character immediately before nor the character immediately after the
/// matched substring is an ASCII alphanumeric character or underscore.
/// This prevents field `x` from spuriously matching inside `x_other` or `extra`.
fn contains_name_token(expr: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let bytes = expr.as_bytes();
    let name_bytes = name.as_bytes();
    let name_len = name_bytes.len();

    for start in 0..bytes.len() {
        if bytes[start..].starts_with(name_bytes) {
            let end = start + name_len;
            let before_ok = start == 0 || !is_word_char(bytes[start - 1]);
            let after_ok = end == bytes.len() || !is_word_char(bytes[end]);
            if before_ok && after_ok {
                return true;
            }
        }
    }
    false
}

/// Returns true if the byte is an ASCII word character (alphanumeric or underscore).
#[inline]
fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Check if a structure's definitions contain a name token anywhere.
fn structure_contains_name_token(s: &Structure, name: &str) -> bool {
    s.defs.iter().any(|def| match def {
        Def::Val { expr, .. } => contains_name_token(expr, name),
        Def::Type { ty, .. } => contains_name_token(ty, name),
        Def::Module { body, .. } => structure_contains_name_token(body, name),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn span(start: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), start, 1)
    }

    /// AC 1 baseline: empty structure elaborates to empty typed value.
    #[test]
    fn empty_structure_elaborates_to_empty_typed_value() {
        let s = Structure {
            defs: vec![],
            span: span(0),
        };

        let mut ctx = LinearityCtx::new();
        let mut diags = Vec::new();

        let tv = elaborate_structure(&s, &mut ctx, &mut diags);

        assert!(tv.bindings.is_empty());
        assert!(tv.signature.decls.is_empty());
        assert!(diags.is_empty());
    }

    /// AC 1: three-field structure (type + val + module) elaborates cleanly.
    #[test]
    fn three_field_structure_elaborates_cleanly() {
        let s = Structure {
            defs: vec![
                Def::Type {
                    name: "t".to_string(),
                    ty: "int".to_string(),
                    span: span(10),
                },
                Def::Val {
                    name: "x".to_string(),
                    expr: "42".to_string(),
                    span: span(20),
                },
                Def::Module {
                    name: "M".to_string(),
                    ascription: None,
                    body: Box::new(Structure {
                        defs: vec![],
                        span: span(30),
                    }),
                    span: span(30),
                },
            ],
            span: span(0),
        };

        let mut ctx = LinearityCtx::new();
        let mut diags = Vec::new();

        let tv = elaborate_structure(&s, &mut ctx, &mut diags);

        assert_eq!(tv.bindings.len(), 3);
        assert_eq!(tv.signature.decls.len(), 3);

        // Check first binding is Type.
        assert_eq!(tv.bindings[0].name, "t");
        assert!(matches!(tv.bindings[0].value, ValueRef::Type(_)));

        // Check second binding is Val.
        assert_eq!(tv.bindings[1].name, "x");
        assert!(matches!(tv.bindings[1].value, ValueRef::Val(_)));

        // Check third binding is Module.
        assert_eq!(tv.bindings[2].name, "M");
        assert!(matches!(tv.bindings[2].value, ValueRef::Module(_)));

        assert!(diags.is_empty());
    }

    /// AC 2 positive: linear field declared and referenced once.
    #[test]
    fn linear_field_threaded_once_no_diagnostic() {
        let s = Structure {
            defs: vec![
                Def::Val {
                    name: "x".to_string(),
                    expr: "linear:res".to_string(),
                    span: span(10),
                },
                Def::Val {
                    name: "y".to_string(),
                    expr: "use x".to_string(),
                    span: span(20),
                },
            ],
            span: span(0),
        };

        let mut ctx = LinearityCtx::new();
        let mut diags = Vec::new();

        let tv = elaborate_structure(&s, &mut ctx, &mut diags);

        assert_eq!(tv.bindings.len(), 2);
        assert_eq!(tv.bindings[0].class, LinClass::Linear);

        // No diagnostics should be emitted (linear field used exactly once).
        assert!(diags.is_empty());
    }

    /// AC 2 negative: linear field used twice emits S0901.
    #[test]
    fn linear_field_used_twice_emits_s0901() {
        let s = Structure {
            defs: vec![
                Def::Val {
                    name: "x".to_string(),
                    expr: "linear:res".to_string(),
                    span: span(10),
                },
                Def::Val {
                    name: "y".to_string(),
                    expr: "use x".to_string(),
                    span: span(20),
                },
                Def::Val {
                    name: "z".to_string(),
                    expr: "use x again".to_string(),
                    span: span(30),
                },
            ],
            span: span(0),
        };

        let mut ctx = LinearityCtx::new();
        let mut diags = Vec::new();

        let tv = elaborate_structure(&s, &mut ctx, &mut diags);

        assert_eq!(tv.bindings.len(), 3);

        // Expect exactly one S0901 diagnostic (overused).
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_LINEAR_FIELD_OVERUSED);
    }

    /// AC 2 negative: linear field never used emits S0900.
    #[test]
    fn linear_field_never_used_emits_s0900() {
        let s = Structure {
            defs: vec![
                Def::Val {
                    name: "x".to_string(),
                    expr: "linear:res".to_string(),
                    span: span(10),
                },
                Def::Val {
                    name: "y".to_string(),
                    expr: "42".to_string(),
                    span: span(20),
                },
            ],
            span: span(0),
        };

        let mut ctx = LinearityCtx::new();
        let mut diags = Vec::new();

        let tv = elaborate_structure(&s, &mut ctx, &mut diags);

        assert_eq!(tv.bindings.len(), 2);

        // Expect exactly one S0900 diagnostic (never used).
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), 900);
    }

    /// AC 3: nested module def recurses and validates inner scope.
    #[test]
    fn nested_module_def_recurses_and_validates_inner_scope() {
        let inner_struct = Structure {
            defs: vec![Def::Val {
                name: "inner_x".to_string(),
                expr: "linear:res".to_string(),
                span: span(25),
            }],
            span: span(20),
        };

        let s = Structure {
            defs: vec![
                Def::Val {
                    name: "outer_x".to_string(),
                    expr: "10".to_string(),
                    span: span(10),
                },
                Def::Module {
                    name: "M".to_string(),
                    ascription: None,
                    body: Box::new(inner_struct),
                    span: span(20),
                },
            ],
            span: span(0),
        };

        let mut ctx = LinearityCtx::new();
        let mut diags = Vec::new();

        let tv = elaborate_structure(&s, &mut ctx, &mut diags);

        assert_eq!(tv.bindings.len(), 2);

        // Inner module's linear field is never used → S0900 from nested scope.
        // The outer_x is Unrestricted so it doesn't cause a diagnostic.
        // Total diagnostics: 1 (S0900 for inner_x).
        assert!(!diags.is_empty());
        assert!(diags.iter().any(|d| d.code().number() == 900));
    }
}
