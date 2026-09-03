//! Item-specific structured data.
//!
//! [`ItemData`] is an enum that carries the semantic payload for item nodes
//! (Module, Signature, Let, Effect, etc.). Each variant holds `NodeId`
//! references to child nodes that will be filled in by the parser.

use crate::{NodeId, exprs::GenericParam};

/// Calling convention for function bindings.
///
/// Specifies the ABI (Application Binary Interface) calling convention
/// for function-shaped bindings. `None` on LetInfo means "paideia default" (unannotated).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum CallingConvention {
    /// Microsoft x64 calling convention (used for UEFI ABI compatibility).
    Ms,
    /// System V AMD64 ABI calling convention (used for Unix/Linux targets).
    Sysv,
}

/// Interrupt-service-routine attribute payload (paideia-as#1278, v0.21-002).
///
/// Carries the metadata for `@interrupt("vec_name")` and
/// `@interrupt_error("vec_name")` — sugar over `@no_frame` that a later
/// elaborator phase turns into a synthesized ISR prologue-epilogue
/// (spill RAX/RCX/RDX/RSI/RDI/R8..R15, execute body, restore, `iretq`,
/// with an extra `add rsp, 8` skip for the CPU-pushed error code in the
/// `_error` variant).
///
/// **Resolution:** the parser accepts a string literal that is either
/// a canonical vector name from the x86_64 exception table
/// (`page_fault`, `general_protection`, `breakpoint`, …) or the
/// decimal / hex spelling of a vector number in `0..=255`. Both are
/// normalised into `vector` here; `name` retains the original spelling
/// for diagnostics and doc output.
///
/// Phase-1 landing (parser + AST/IR plumbing) captures this struct and
/// propagates it through `ItemData::Let::interrupt` and IR
/// `LetInfo::interrupt`. The elaborator emit-side synthesis lands in a
/// phase-2 follow-up issue.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct InterruptAttr {
    /// `true` iff parsed from `@interrupt_error(...)` — the vector's
    /// CPU-pushed error code needs an extra 8-byte skip before `iretq`.
    /// `false` for the plain `@interrupt(...)` form.
    pub has_error_code: bool,
    /// The resolved vector number in `0..=255`.
    pub vector: u8,
    /// The original attribute-argument spelling as written in source.
    /// Kept so diagnostics can echo the user's word choice and so
    /// disassembly / DWARF output can label the ISR site by name.
    pub name: String,
}

/// Memory-ordering discipline for atomic bindings (paideia-as#1296, v0.21-003b).
///
/// Attached to a `let`-binding via the `@atomic(Ordering)` binding-position
/// attribute. Names the acquire/release/relaxed/seq-cst semantics that every
/// load and store of the binding must honour, so a code reviewer can see the
/// intended memory order at the declaration site without descending into an
/// `unsafe { }` block that hides the raw fence / lock-prefix sequence.
///
/// Encoding intent (x86_64 TSO, phase-2 elaborator emit — inert in phase-1):
/// - `Relaxed` — plain `mov` on both load and store.
/// - `Acquire` — plain `mov` on load (x86 TSO gives acquire for aligned loads); paired store site is `Release`.
/// - `Release` — plain `mov` on store (x86 TSO gives release for aligned stores); paired load site is `Acquire`.
/// - `SeqCst`  — `mfence`-bracketed load and `mov ; mfence` store (total order across cores).
///
/// The four variants match C11 `memory_order_*` and Rust `std::sync::atomic::Ordering`
/// so a reader familiar with either model can port intent one-to-one; the
/// `Consume` variant is deliberately omitted (folded into `Acquire`, same as
/// LLVM's x86 lowering) because no shipping x86_64 CPU distinguishes it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum AtomicOrdering {
    /// No inter-thread synchronisation. Reads/writes are atomic (indivisible)
    /// but carry no happens-before edge relative to other memory operations.
    Relaxed,
    /// Acquire ordering: no reads/writes in this thread may be reordered
    /// before this load. Pairs with a `Release` store on the producer side.
    Acquire,
    /// Release ordering: no reads/writes in this thread may be reordered
    /// after this store. Pairs with an `Acquire` load on the consumer side.
    Release,
    /// Sequentially-consistent ordering: a single total order across all
    /// SeqCst operations globally, in addition to Acquire+Release semantics.
    SeqCst,
}

/// Struct-level attribute (paideia-as#1373, v0.28-M1-004).
///
/// Attached to a struct-decl node via the [`crate::StructAttrTable`]
/// side-table rather than by growing every construction/destructuring
/// site of [`ItemData::Struct`]. Sparse — most struct declarations have
/// no entry. New primitives extend this enum by appending a variant.
///
/// **Composition:** primitives that constrain _how_ a struct is laid out
/// (packing, per-field endianness, …) are designed to compose. A struct
/// tagged [`StructAttr::Packed`] may still hold fields carrying the
/// field-level `@endian(be|le)` annotation from paideia-as#1374 (b2-05):
/// packing fixes the byte offset of each field, and the endian
/// annotation fixes each field's byte order at load/store sites. The
/// two rules stack without ordering ambiguity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructAttr {
    /// `@packed_struct` or `@packed_struct(align=<n>)` (paideia-as#1373).
    ///
    /// **Layout:** dense — the elaborator emits fields at their
    /// no-padding offsets (each field's offset is the running sum of
    /// preceding field sizes, honouring per-field endianness).
    ///
    /// **Alignment:** forced to `align` when `Some(n)`, else 1. When
    /// `Some(n)`, `n` is a positive power of two — the parser rejects
    /// zero and non-powers-of-two at parse time (P0293) so downstream
    /// phases can trust the invariant.
    Packed {
        /// Forced alignment in bytes (positive power of two), or `None`
        /// for `align = 1` (the packed default).
        align: Option<u32>,
    },
}

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

    /// Let binding: `let [mut] Name <T> (: Type)? = Expr @align(N)? @ring(slots=M, slot_size=K)? @link_section("name")? @abi("ms"|"sysv")? @no_frame?`
    Let {
        /// Whether this binding is public (`pub let`).
        public: bool,
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
        /// Optional alignment directive `@align(N)` for per-symbol alignment (PA10-006y).
        /// When `Some(N)`, the symbol must be aligned to N bytes (power of 2, 1..2^30).
        align: Option<u32>,
        /// Optional ring buffer directive `@ring(slots=M, slot_size=K)` for ring buffers (PA14-r14-008).
        /// When `Some((M, K))`, allocates ring structures with M slots of K bytes each.
        ring: Option<(u32, u32)>,
        /// Optional link_section directive `@link_section("name")` (PA19-r19-010).
        /// When `Some(name)`, emits data into a custom-named ELF section.
        link_section: Option<String>,
        /// Optional ABI calling convention directive `@abi("ms"|"sysv")` (PA19-r19-001).
        /// When `Some(cc)`, specifies the calling convention for function-shaped bindings.
        /// `None` means paideia default (unannotated), not explicitly `Sysv`.
        abi: Option<CallingConvention>,
        /// Frame-prologue opt-out directive `@no_frame` (paideia-as#1276, unblocks paideia-os#716).
        /// When `true`, the emitter is instructed to skip the default SysV prologue/epilogue
        /// (`push rbp; mov rbp, rsp` / `leave; ret`) for function-shaped bindings — used for
        /// hand-crafted trampolines, ISR entries, syscall stubs, and any function that
        /// manipulates `rsp` directly. Only meaningful on lambda-shaped Lets; the annotation
        /// is inert on non-function bindings until later phases add a P02xx placement check.
        ///
        /// Phase-1 landing (parser + AST/IR plumbing): the flag is parsed and propagated into
        /// [`crate::items::ItemData::Let`] and eventually into IR `LetInfo::no_frame`, but
        /// the elaborator emit pass still ignores it — no prologue/epilogue emission changes.
        no_frame: bool,
        /// Interrupt-service-routine sugar `@interrupt("vec")` / `@interrupt_error("vec")`
        /// (paideia-as#1278, v0.21-002). When `Some(attr)`, the binding is an ISR entry
        /// and the elaborator (phase-2) synthesises the ISR prologue/epilogue instead
        /// of the SysV frame. Composes with `@no_frame` (which is implied when
        /// `interrupt.is_some()`).
        ///
        /// Phase-1 landing: parser + AST/IR plumbing only; the elaborator emit-side
        /// synthesis lands in a phase-2 follow-up issue.
        interrupt: Option<InterruptAttr>,
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
        /// Struct fields: each is (field_name_node, field_type_node).
        fields: Vec<(NodeId, NodeId)>,
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
        /// Enum variants: each can be unit-shaped, tuple-shaped, or record-shaped.
        variants: Vec<crate::types::EnumVariant>,
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
            public: false,
            mutable: false,
            name,
            generic_params: vec![],
            ty: Some(ty),
            value,
            align: None,
            ring: None,
            link_section: None,
            abi: None,
            no_frame: false,
            interrupt: None,
            doc: None,
        };
        match item {
            ItemData::Let {
                public: _,
                mutable: mut_flag,
                name: n,
                generic_params,
                ty: t,
                value: v,
                align: a,
                ring: r,
                link_section: ls,
                abi,
                no_frame: nf,
                interrupt: intr,
                doc: d,
            } => {
                assert!(!mut_flag);
                assert_eq!(n, name);
                assert!(generic_params.is_empty());
                assert_eq!(t, Some(ty));
                assert_eq!(v, value);
                assert_eq!(a, None);
                assert_eq!(r, None);
                assert_eq!(ls, None);
                assert_eq!(abi, None);
                assert!(!nf);
                assert!(intr.is_none());
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
