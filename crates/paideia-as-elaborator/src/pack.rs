//! Pack/unpack elaboration — existential module abstraction.
//!
//! This module implements pack/unpack operations for existential module abstraction:
//! - `pack M : S` — packages a module M matching signature S into an existential value.
//! - `unpack V` — opens a packed module, extracting the original module.
//! - `let module N = unpack V in body` — binds an unpacked module in a scope.
//!
//! Pack/unpack use a side-table convention (NOT a new ValueRef variant). A packed
//! TypedValue has exactly one binding named `_packed_module` with its value wrapped
//! in a `ValueRef::Module`, and a signature with a single type declaration
//! `_pack_{hash}` where hash is BLAKE3(signature debug format).
//!
//! # Phase-2-m5-010
//!
//! - Hash is BLAKE3 over `format!("{:?}", signature)`.
//! - First 8 bytes of hash are displayed in hex as the sentinel type name.
//! - `PackedValue` struct holds the elaborated packed representation.
//! - `is_packed` helper checks if a TypedValue conforms to pack convention.

use paideia_as_ast::Signature;
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_ir::LinClass;
use paideia_as_types::{SigDeclKind, SignatureKind};

use crate::modules::{FieldBinding, TypedValue, ValueRef};
use crate::sig_match::match_signature;

/// Diagnostic code for "unpack expects a packed value".
pub const M_UNPACK_NOT_PACKED: u16 = 304;

/// A packed module value representation.
///
/// Holds the elaborated result of `pack M : S`, with internal representation
/// using the side-table convention.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackedValue {
    /// The packed TypedValue (has `_packed_module` binding + `_pack_{hash}` sig decl).
    pub contents: TypedValue,
    /// Extracted signature for documentation (not used in unpack).
    pub signature: SignatureKind,
    /// Source location.
    pub span: Span,
}

/// Check if a TypedValue is a packed value.
///
/// Returns a reference to the inner module if `tv` has exactly 1 binding
/// named `_packed_module` with `ValueRef::Module(inner)`. Returns None otherwise.
pub fn is_packed(tv: &TypedValue) -> Option<&TypedValue> {
    if tv.bindings.len() == 1
        && let Some(binding) = tv.bindings.first()
        && binding.name == "_packed_module"
        && let ValueRef::Module(inner) = &binding.value
    {
        return Some(inner);
    }
    None
}

/// Elaborate `pack M : S` to a packed module value.
///
/// Steps:
/// 1. Check that module matches signature via [`match_signature`].
///    Return None on mismatch with diagnostics.
/// 2. Compute BLAKE3 hash of signature debug format.
///    Use first 8 bytes as u64 displayed in hex.
/// 3. Build TypedValue with:
///    - Single binding `_packed_module` containing the original module.
///    - Signature with one Type decl `_pack_{hash}`.
/// 4. Return Some(TypedValue).
pub fn elaborate_pack(
    module: &TypedValue,
    signature: &Signature,
    diags: &mut Vec<Diagnostic>,
) -> Option<TypedValue> {
    // Step 1: match_signature first.
    if !match_signature(module, signature, diags) {
        return None;
    }

    // Step 2: compute hash of signature.
    let mut hasher = blake3::Hasher::new();
    hasher.update(format!("{:?}", signature).as_bytes());
    let hash_bytes = hasher.finalize();
    let hash_u64 = u64::from_le_bytes([
        hash_bytes.as_bytes()[0],
        hash_bytes.as_bytes()[1],
        hash_bytes.as_bytes()[2],
        hash_bytes.as_bytes()[3],
        hash_bytes.as_bytes()[4],
        hash_bytes.as_bytes()[5],
        hash_bytes.as_bytes()[6],
        hash_bytes.as_bytes()[7],
    ]);
    let hash_hex = format!("{:016x}", hash_u64);

    // Step 3: build packed TypedValue.
    let packed_binding = FieldBinding {
        name: "_packed_module".to_string(),
        ty_id: 0,
        value: ValueRef::Module(Box::new(module.clone())),
        class: LinClass::Unrestricted,
        span: module.span,
    };

    let packed_signature = SignatureKind {
        decls: vec![SigDeclKind::Type {
            name: format!("_pack_{}", hash_hex),
            kind: LinClass::Unrestricted,
        }],
    };

    let packed_tv = TypedValue {
        bindings: vec![packed_binding],
        signature: packed_signature,
        span: module.span,
    };

    Some(packed_tv)
}

/// Elaborate `unpack V` to extract the original module from a packed value.
///
/// Steps:
/// 1. Check that `packed` is a packed value via [`is_packed`].
///    If None, emit M0304 (M_UNPACK_NOT_PACKED) with binding count → return None.
/// 2. Return Some(cloned inner module).
pub fn elaborate_unpack(packed: &TypedValue, diags: &mut Vec<Diagnostic>) -> Option<TypedValue> {
    // Step 1: check is_packed.
    match is_packed(packed) {
        Some(inner) => {
            // Step 2: return cloned inner module.
            Some(inner.clone())
        }
        None => {
            // Emit M0304.
            let message = format!(
                "unpack expects a packed value; got a structure with {} bindings",
                packed.bindings.len()
            );
            diags.push(
                Diagnostic::error(m_code(M_UNPACK_NOT_PACKED))
                    .message(message)
                    .with_span(packed.span)
                    .finish(),
            );
            None
        }
    }
}

/// Elaborate `let module N = unpack V in body` to bind an unpacked module in scope.
///
/// Steps:
/// 1. Unpack the packed value via [`elaborate_unpack`].
///    Return None on failure (diagnostic already emitted).
/// 2. Elaborate the body structure.
/// 3. Prepend a FieldBinding for the unpacked module at index 0
///    with name, value ValueRef::Module(inner), class Unrestricted.
/// 4. Return Some(body_tv).
pub fn elaborate_let_module(
    name: &str,
    packed: &TypedValue,
    body_tv: &mut TypedValue,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    // Step 1: unpack.
    let unpacked = match elaborate_unpack(packed, diags) {
        Some(u) => u,
        None => return false,
    };

    // Step 3: prepend binding at index 0.
    let module_binding = FieldBinding {
        name: name.to_string(),
        ty_id: 0,
        value: ValueRef::Module(Box::new(unpacked)),
        class: LinClass::Unrestricted,
        span: body_tv.span,
    };
    body_tv.bindings.insert(0, module_binding);

    true
}

/// Helper to construct a diagnostic code for category M.
fn m_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::M, Severity::Error, n).expect("valid M code")
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::{SigDecl, Signature, TypeDecl};
    use paideia_as_diagnostics::FileId;

    fn span(start: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), start, 1)
    }

    /// Test 1: pack_matches_signature_returns_some
    /// Empty module + empty sig → Some(packed value with `_packed_module` binding).
    #[test]
    fn pack_matches_signature_returns_some() {
        let module = TypedValue {
            bindings: vec![],
            signature: Default::default(),
            span: span(0),
        };

        let signature = Signature {
            decls: vec![],
            span: span(1),
        };

        let mut diags = Vec::new();
        let result = elaborate_pack(&module, &signature, &mut diags);

        assert!(result.is_some());
        assert!(diags.is_empty());

        let packed = result.unwrap();
        assert_eq!(packed.bindings.len(), 1);
        assert_eq!(packed.bindings[0].name, "_packed_module");
        assert!(matches!(packed.bindings[0].value, ValueRef::Module(_)));
    }

    /// Test 2: pack_mismatch_returns_none_with_m0301_or_m0302
    /// Module missing val → None, diag with code 301 or 302.
    #[test]
    fn pack_mismatch_returns_none_with_m0301_or_m0302() {
        let module = TypedValue {
            bindings: vec![],
            signature: Default::default(),
            span: span(0),
        };

        // Signature requires a type "t" that module doesn't have.
        let signature = Signature {
            decls: vec![SigDecl::Type(TypeDecl {
                name: "t".to_string(),
                definition: None,
                span: span(1),
            })],
            span: span(1),
        };

        let mut diags = Vec::new();
        let result = elaborate_pack(&module, &signature, &mut diags);

        assert!(result.is_none());
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0].code().number() == 301 || diags[0].code().number() == 302,
            "Expected M0301 or M0302, got {}",
            diags[0].code().number()
        );
    }

    /// Test 3: unpack_round_trip_returns_original_bindings
    /// Pack then unpack, assert returned module's bindings equal original's.
    #[test]
    fn unpack_round_trip_returns_original_bindings() {
        let original_module = TypedValue {
            bindings: vec![FieldBinding {
                name: "x".to_string(),
                ty_id: 0,
                value: ValueRef::Val("42".to_string()),
                class: LinClass::Unrestricted,
                span: span(2),
            }],
            signature: Default::default(),
            span: span(0),
        };

        let signature = Signature {
            decls: vec![SigDecl::Val(paideia_as_ast::ValDecl {
                name: "x".to_string(),
                ty: "int".to_string(),
                span: span(1),
            })],
            span: span(1),
        };

        // Pack.
        let mut diags = Vec::new();
        let packed =
            elaborate_pack(&original_module, &signature, &mut diags).expect("pack should succeed");
        assert!(diags.is_empty());

        // Unpack.
        diags.clear();
        let unpacked = elaborate_unpack(&packed, &mut diags).expect("unpack should succeed");
        assert!(diags.is_empty());

        // Compare bindings.
        assert_eq!(unpacked.bindings, original_module.bindings);
    }

    /// Test 4: unpack_non_pack_returns_none_with_m0304
    /// Call unpack on a non-pack TypedValue → None, diag with code 304.
    #[test]
    fn unpack_non_pack_returns_none_with_m0304() {
        let not_packed = TypedValue {
            bindings: vec![
                FieldBinding {
                    name: "x".to_string(),
                    ty_id: 0,
                    value: ValueRef::Val("42".to_string()),
                    class: LinClass::Unrestricted,
                    span: span(1),
                },
                FieldBinding {
                    name: "y".to_string(),
                    ty_id: 0,
                    value: ValueRef::Val("99".to_string()),
                    class: LinClass::Unrestricted,
                    span: span(2),
                },
            ],
            signature: Default::default(),
            span: span(0),
        };

        let mut diags = Vec::new();
        let result = elaborate_unpack(&not_packed, &mut diags);

        assert!(result.is_none());
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), M_UNPACK_NOT_PACKED);
        assert!(diags[0].message().contains("2 bindings"));
    }

    /// Test 5: let_module_binds_unpacked_in_scope
    /// Assert body_tv.bindings[0].name == "N".
    #[test]
    fn let_module_binds_unpacked_in_scope() {
        let original_module = TypedValue {
            bindings: vec![],
            signature: Default::default(),
            span: span(0),
        };

        let signature = Signature {
            decls: vec![],
            span: span(1),
        };

        // Pack.
        let mut diags = Vec::new();
        let packed =
            elaborate_pack(&original_module, &signature, &mut diags).expect("pack should succeed");
        assert!(diags.is_empty());

        // Create body_tv with some bindings.
        let mut body_tv = TypedValue {
            bindings: vec![FieldBinding {
                name: "z".to_string(),
                ty_id: 0,
                value: ValueRef::Val("100".to_string()),
                class: LinClass::Unrestricted,
                span: span(2),
            }],
            signature: Default::default(),
            span: span(0),
        };

        // let_module.
        diags.clear();
        let result = elaborate_let_module("N", &packed, &mut body_tv, &mut diags);
        assert!(result);
        assert!(diags.is_empty());

        // Check binding at index 0.
        assert_eq!(body_tv.bindings.len(), 2);
        assert_eq!(body_tv.bindings[0].name, "N");
        assert!(matches!(body_tv.bindings[0].value, ValueRef::Module(_)));
        // Old binding moved to index 1.
        assert_eq!(body_tv.bindings[1].name, "z");
    }

    /// Test 6: pack_then_unpack_then_let_module_full_pipeline
    /// Pack(M) → packed; unpack(packed) → unpacked;
    /// let_module("N", packed, rest) → assert "N" at bindings[0]
    /// and its value is ValueRef::Module pointing at original M.
    #[test]
    fn pack_then_unpack_then_let_module_full_pipeline() {
        let original_module = TypedValue {
            bindings: vec![FieldBinding {
                name: "x".to_string(),
                ty_id: 0,
                value: ValueRef::Val("42".to_string()),
                class: LinClass::Unrestricted,
                span: span(2),
            }],
            signature: Default::default(),
            span: span(0),
        };

        let signature = Signature {
            decls: vec![SigDecl::Val(paideia_as_ast::ValDecl {
                name: "x".to_string(),
                ty: "int".to_string(),
                span: span(1),
            })],
            span: span(1),
        };

        let mut diags = Vec::new();

        // Pack M.
        let packed =
            elaborate_pack(&original_module, &signature, &mut diags).expect("pack should succeed");
        assert!(diags.is_empty());

        // Unpack to verify round-trip.
        let unpacked = elaborate_unpack(&packed, &mut diags).expect("unpack should succeed");
        assert!(diags.is_empty());
        assert_eq!(unpacked.bindings, original_module.bindings);

        // let_module with rest body.
        let mut rest_body = TypedValue {
            bindings: vec![],
            signature: Default::default(),
            span: span(3),
        };

        diags.clear();
        let result = elaborate_let_module("N", &packed, &mut rest_body, &mut diags);
        assert!(result);
        assert!(diags.is_empty());

        // Verify N is at bindings[0] and contains the original module.
        assert_eq!(rest_body.bindings.len(), 1);
        assert_eq!(rest_body.bindings[0].name, "N");

        // Extract inner module and verify it matches original.
        if let ValueRef::Module(inner) = &rest_body.bindings[0].value {
            assert_eq!(inner.bindings, original_module.bindings);
        } else {
            panic!("Expected ValueRef::Module");
        }
    }
}
