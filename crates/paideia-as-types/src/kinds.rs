//! Type lattice classes (kinds).
//!
//! The kind of a type determines its substructural properties: linearity,
//! affinity, etc. This module types those properties and computes the kind
//! of any interned type via structural examination.

use paideia_as_ir::LinClass;

use crate::types::{Type, TypeId};

/// The lattice class assigned to a type.
///
/// This is an alias to [`paideia_as_ir::LinClass`], but typed separately
/// here so future type-level refinements (e.g., kinds beyond linearity)
/// don't drag the IR crate. Phase-1 uses only `Unrestricted` and `Linear`.
pub type Kind = LinClass;

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
        _ => Kind::Unrestricted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CapSetId, Type};
    use paideia_as_ir::EffectRowId;

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
}
