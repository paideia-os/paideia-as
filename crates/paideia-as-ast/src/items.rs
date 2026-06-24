//! Item-specific structured data.
//!
//! [`ItemData`] is an enum that carries the semantic payload for item nodes
//! (Module, Signature, Let, Effect, etc.). Each variant holds `NodeId`
//! references to child nodes that will be filled in by the parser.

use crate::{NodeId, exprs::GenericParam};

/// Value types for inner attributes.
///
/// Supports integer literals, string literals, and identifiers
/// for flexible attribute representation.
#[derive(Clone, Debug)]
pub enum AttrValue {
    /// Integer value (e.g., `#![bits = 32]`).
    Int(i64),
    /// String value (e.g., `#![desc = "..."]`).
    Str(NodeId),
    /// Identifier value (e.g., `#![mode = Default]`).
    Ident(NodeId),
}

/// Attribute applied to an item (e.g., struct, enum, function).
///
/// Attributes customize the behavior of declarations, such as
/// `#[derive(...)]` which synthesizes trait implementations,
/// or inner attributes like `#![bits = 32]` at module scope.
#[derive(Clone, Debug)]
pub enum ItemAttribute {
    /// Derive attribute: `#[derive(Trait1, Trait2, ...)]`
    ///
    /// Specifies traits whose implementations should be automatically
    /// synthesized for the decorated type.
    Derive {
        /// List of trait names as Ident nodes referring to traits (e.g., Eq, Hash, Debug).
        trait_names: Vec<NodeId>,
    },
    /// Inner attribute: `#![name = value]`
    ///
    /// Used for module-level or scope-level configuration (e.g., `#![bits = 32]`).
    InnerAttr {
        /// Attribute name (Ident node).
        name: NodeId,
        /// Attribute value.
        value: AttrValue,
    },
}

/// Impl block declaration.
///
/// `ImplDecl` represents a single impl block that provides implementations for a type,
/// either for a specific trait (trait impl) or inherent methods (inherent impl).
#[derive(Clone, Debug)]
pub struct ImplDecl {
    /// Generic parameters (type parameters with optional bounds).
    pub generic_params: Vec<GenericParam>,
    /// Optional trait name (Ident node). `None` for inherent impl.
    pub trait_name: Option<NodeId>,
    /// Generic arguments to the trait (Type nodes).
    pub trait_args: Vec<NodeId>,
    /// The type being impl'd (Type node).
    pub for_type: NodeId,
    /// Body items (Let or Fn nodes).
    pub methods: Vec<NodeId>,
}

/// Trait method declaration within a trait.
///
/// `TraitMethod` represents a single method signature (and optional default body)
/// within a trait declaration.
#[derive(Clone, Debug)]
pub struct TraitMethod {
    /// Name of the method (Ident node).
    pub name: NodeId,
    /// Generic parameters (type parameters with optional bounds).
    pub generic_params: Vec<GenericParam>,
    /// Method parameters: (name, type) pairs.
    pub params: Vec<(NodeId, NodeId)>,
    /// Return type (Type node).
    pub return_type: NodeId,
    /// Optional effect set constraint.
    pub effects: Option<NodeId>,
    /// Optional capability set constraint.
    pub capabilities: Option<NodeId>,
    /// Optional default body implementation (Expr node).
    /// When `None`, the method is abstract (ends with `;`).
    /// When `Some`, the method has a default body (ends with `{ expr }`).
    pub default_body: Option<NodeId>,
}

/// Structured payload for item nodes.
///
/// Each variant corresponds to a top-level item kind (Module, Let, Effect, etc.)
/// as specified in §8 of the syntax reference. Child `NodeId` fields point to
/// other nodes in the arena; those nodes' concrete kinds (Expr, Type, Pattern)
/// are introduced by later PRs.
///
/// Fields named `name` always point to an `Ident` node. Fields named `doc`
/// hold an optional `StringLit` node for documentation comments.
#[derive(Clone, Debug)]
pub enum ItemData {
    /// Module declaration: `module Name (: Sig)? = Body`
    Module {
        /// Name of the module (Ident node).
        name: NodeId,
        /// Optional signature ascription.
        sig: Option<NodeId>,
        /// Module body (Structure or Functor node).
        body: NodeId,
        /// Inner attributes (e.g., `#![bits = 32]`) at module scope.
        inner_attrs: Vec<ItemAttribute>,
        /// Optional documentation comment.
        doc: Option<NodeId>,
    },

    /// Signature declaration: analogous to Structure but introduces a signature.
    Signature {
        /// Name of the signature (Ident node).
        name: NodeId,
        /// Signature body (Structure node with type declarations).
        body: NodeId,
        /// Optional documentation comment.
        doc: Option<NodeId>,
    },

    /// Structure: `struct { ItemDecl* }`
    Structure {
        /// Item declarations in this structure.
        items: Vec<NodeId>,
        /// Inner attributes (e.g., `#![bits = 32]`) at structure scope.
        inner_attrs: Vec<ItemAttribute>,
        /// Optional documentation comment.
        doc: Option<NodeId>,
    },

    /// Functor: `functor (Param)+ -> struct { ItemDecl* }`
    Functor {
        /// Functor parameters (FunctorParam nodes).
        params: Vec<NodeId>,
        /// Functor body (Structure node).
        body: NodeId,
        /// Optional documentation comment.
        doc: Option<NodeId>,
    },

    /// Functor parameter: `Name : Sig`
    FunctorParam {
        /// Name of the parameter (Ident node).
        name: NodeId,
        /// Signature ascription (SignatureRef node).
        sig: NodeId,
    },

    /// Effect declaration: `effect Name { OpSig+ }`
    Effect {
        /// Name of the effect (Ident node).
        name: NodeId,
        /// Operation signatures (OpSig nodes).
        ops: Vec<NodeId>,
        /// Optional documentation comment.
        doc: Option<NodeId>,
    },

    /// Operation signature: `op Name : Type (!{ EffectSet })?`
    OpSig {
        /// Name of the operation (Ident node).
        name: NodeId,
        /// Type signature (Type node).
        ty: NodeId,
        /// Optional effect set constraint.
        effect_set: Option<NodeId>,
    },

    /// Capability declaration.
    Capability {
        /// Name of the capability (Ident node).
        name: NodeId,
        /// Capability body.
        body: NodeId,
        /// Optional documentation comment.
        doc: Option<NodeId>,
    },

    /// Let binding: `let [mut] Name <T> (: Type)? = Expr`
    Let {
        /// Whether this binding is mutable (`let mut`).
        mutable: bool,
        /// Name of the binding (Ident node).
        name: NodeId,
        /// Generic parameters (type parameters with optional bounds).
        /// Empty for non-generic bindings.
        generic_params: Vec<crate::exprs::GenericParam>,
        /// Optional type annotation (Type node).
        ty: Option<NodeId>,
        /// Value expression (Expr node).
        value: NodeId,
        /// Optional documentation comment.
        doc: Option<NodeId>,
    },

    /// Struct type declaration.
    Struct {
        /// Name of the struct (Ident node).
        name: NodeId,
        /// Generic parameters (type parameters with optional bounds).
        /// Empty for non-generic structs.
        generic_params: Vec<crate::exprs::GenericParam>,
        /// Struct fields.
        fields: Vec<NodeId>,
        /// Attributes applied to this struct (e.g., `#[derive(...)]`).
        attributes: Vec<ItemAttribute>,
        /// Optional documentation comment.
        doc: Option<NodeId>,
    },

    /// Enum type declaration.
    Enum {
        /// Name of the enum (Ident node).
        name: NodeId,
        /// Generic parameters (type parameters with optional bounds).
        /// Empty for non-generic enums.
        generic_params: Vec<crate::exprs::GenericParam>,
        /// Enum variants.
        variants: Vec<NodeId>,
        /// Attributes applied to this enum (e.g., `#[derive(...)]`).
        attributes: Vec<ItemAttribute>,
        /// Optional documentation comment.
        doc: Option<NodeId>,
    },

    /// Trait declaration: `trait Name<T> { type Item; method_sig; ... }`
    Trait {
        /// Name of the trait (Ident node).
        name: NodeId,
        /// Generic parameters (type parameters with optional bounds).
        generic_params: Vec<crate::exprs::GenericParam>,
        /// Associated type declarations (Ident nodes for type names).
        /// Each represents a `type Ident;` slot that concrete implementations must provide.
        associated_types: Vec<NodeId>,
        /// Trait methods (signatures and optional default bodies).
        methods: Vec<TraitMethod>,
        /// Optional documentation comment.
        doc: Option<NodeId>,
    },

    /// Impl block: `impl<T> (Trait<T>)? for Type { items }`
    Impl(ImplDecl),

    /// Unsafe block: `unsafe { effects: {...} capabilities: {...} justification: "..." block: {...} }`
    UnsafeBlock {
        /// Effects declared in the block.
        effects: Vec<NodeId>,
        /// Capabilities declared in the block.
        capabilities: Vec<NodeId>,
        /// Justification (StringLit node).
        justification: NodeId,
        /// Body statements.
        block: Vec<NodeId>,
    },

    /// Macro declaration: `macro Name(pattern) => template` or `macro Name { rule; ... }`.
    MacroDecl(crate::macros::MacroDeclData),

    /// Placeholder for non-item nodes (expressions, types, patterns, statements).
    /// Used by later PRs.
    NonItem,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_data_module_variant_constructs() {
        let name = NodeId::new(1).unwrap();
        let body = NodeId::new(2).unwrap();
        let item = ItemData::Module {
            name,
            sig: None,
            body,
            inner_attrs: vec![],
            doc: None,
        };
        match item {
            ItemData::Module {
                name: n,
                sig: s,
                body: b,
                inner_attrs: ia,
                doc: d,
            } => {
                assert_eq!(n, name);
                assert!(s.is_none());
                assert_eq!(b, body);
                assert!(ia.is_empty());
                assert!(d.is_none());
            }
            _ => panic!("expected Module variant"),
        }
    }

    #[test]
    fn item_data_let_with_type_constructs() {
        let name = NodeId::new(1).unwrap();
        let ty = NodeId::new(2).unwrap();
        let value = NodeId::new(3).unwrap();
        let item = ItemData::Let {
            mutable: false,
            name,
            generic_params: vec![],
            ty: Some(ty),
            value,
            doc: None,
        };
        match item {
            ItemData::Let {
                mutable: mut_flag,
                name: n,
                generic_params,
                ty: t,
                value: v,
                doc: d,
            } => {
                assert!(!mut_flag);
                assert_eq!(n, name);
                assert!(generic_params.is_empty());
                assert_eq!(t, Some(ty));
                assert_eq!(v, value);
                assert!(d.is_none());
            }
            _ => panic!("expected Let variant"),
        }
    }

    #[test]
    fn item_data_structure_with_items_constructs() {
        let item1 = NodeId::new(1).unwrap();
        let item2 = NodeId::new(2).unwrap();
        let item = ItemData::Structure {
            items: vec![item1, item2],
            inner_attrs: vec![],
            doc: None,
        };
        match item {
            ItemData::Structure {
                items: its,
                inner_attrs: ia,
                doc: d,
            } => {
                assert_eq!(its.len(), 2);
                assert_eq!(its[0], item1);
                assert_eq!(its[1], item2);
                assert!(ia.is_empty());
                assert!(d.is_none());
            }
            _ => panic!("expected Structure variant"),
        }
    }
}
