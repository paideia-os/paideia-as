//! Arena allocator for AST nodes.
//!
//! Every node lives in `AstArena.nodes: Vec<NodeData>`, indexed by
//! [`NodeId`]. Parent/child traversal uses arena indices, not Rust
//! references — this keeps the AST `Copy`-friendly and avoids the
//! borrow-checker tax of tree shapes.
//!
//! [`NodeId`]: crate::NodeId

use paideia_as_diagnostics::Span;
use static_assertions::const_assert;
use std::mem::size_of;
use std::ops::Index;

use crate::node_id::NodeId;

/// Discriminant for an AST node's variant.
///
/// Variants are partitioned into items (Module, Let, etc., from PR-16),
/// and category-specific nodes (Expressions, Statements, Types, Patterns).
/// Storage is `#[repr(u32)]` so the per-node size budget is predictable.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u32)]
#[non_exhaustive]
pub enum NodeKind {
    /// Placeholder kind for non-item nodes (expressions, types, patterns, etc.).
    Placeholder,
    /// Identifier node.
    Ident,
    /// Module declaration.
    Module,
    /// Signature declaration.
    Signature,
    /// Structure (module body).
    Structure,
    /// Functor (parameterized module body).
    Functor,
    /// Functor parameter.
    FunctorParam,
    /// Effect declaration.
    Effect,
    /// Operation signature within an effect.
    OpSig,
    /// Capability declaration.
    Capability,
    /// Let binding.
    Let,
    /// Struct type declaration.
    Struct,
    /// Enum type declaration.
    Enum,
    /// Unsafe block.
    UnsafeBlock,
    /// Macro declaration.
    MacroDecl,

    // Expressions (§8 Expr: LambdaExpr | ActionBlock | WithHandlerExpr | UnsafeExpr | InfixExpr | ...)
    /// `fn/λ params -> body` or `|x, y| body`.
    ExprLambda,
    /// `action !{eff} @{caps} { stmts }`.
    ExprActionBlock,
    /// `with handler-expr handle name block`.
    ExprWithHandler,
    /// `unsafe { effects: …, capabilities: …, justification: …, block: … }`.
    ExprUnsafe,
    /// `lhs op rhs` (infix operator expression).
    ExprInfix,
    /// `op expr` (prefix operator expression).
    ExprPrefix,
    /// `expr op` (postfix operator expression: `.field`, `[idx]`, `?`, etc.).
    ExprPostfix,
    /// Literal (Int/Float/Char/String/Byte/ByteString/Unit/Bool).
    ExprLiteral,
    /// `path::to::name` or simple `name`.
    ExprPath,
    /// `f(args)`.
    ExprCall,
    /// `{ stmts; expr? }`.
    ExprBlock,
    /// `match scrutinee { arms }`.
    ExprMatch,
    /// `if cond then else?`.
    ExprIf,
    /// `loop block` or `while cond block` or `for pat in iter block`.
    ExprLoop,
    /// `perform Effect::op(args)`.
    ExprPerform,
    /// `resume value`.
    ExprResume,
    /// `handle Effect { arms }` — handler-value construction.
    ExprHandlerValue,
    /// `quote { ... }` (code quotation).
    ExprQuote,
    /// `~(...)` (antiquotation / unquote).
    ExprAntiquote,
    /// `F(M)(N) sharing (...)` (functor application).
    ExprFunctorApp,
    /// `pack M : S` (pack expression).
    ExprPack,
    /// `unpack v` (unpack expression).
    ExprUnpack,
    /// `let module N = unpack v in <expr>` (let-module binding).
    ExprLetModule,
    /// `TypeName { field1: expr1, ... }` (record constructor).
    ExprRecordCons,
    /// `receiver.field` (field access).
    ExprFieldAccess,

    // Statements (§8 Stmt: LetStmt | ExprStmt | InstructionStmt | ReturnStmt)
    /// `let name: ty? = expr;`.
    StmtLet,
    /// `expr;`.
    StmtExpr,
    /// `return expr?;`.
    StmtReturn,
    /// `mnemonic operand, operand, ...` (assembly instruction).
    StmtInstruction,

    // Operands (§8 Operand: Register | ImmediateExpr | MemoryRef)
    /// Register operand (Ident-shaped, e.g., `rax`, `r8`).
    OperandRegister,
    /// Immediate operand (any Expr).
    OperandImmediate,
    /// Memory reference operand (`[addr]`).
    OperandMemoryRef,

    // Types (§8 Type: TypeName | Arrow | Tuple | LinearClass | EffectRowType)
    /// `TypeName` or `TypeName(args)`.
    TypeName,
    /// `(T1, T2, ...) -> T !{...} @{...}`.
    TypeArrow,
    /// `(T1, T2, ...)`.
    TypeTuple,
    /// `<LinClass> T` (linear/ordered/affine/unrestricted).
    TypeLinearClass,
    /// `eff1, eff2 | rest` or `ε`.
    TypeEffectRow,
    /// `*T`.
    TypePtr,
    /// `record { field1: T1, ... }` (record type).
    TypeRecord,

    // Patterns (§8 Pattern)
    /// `_` (wildcard).
    PatWildcard,
    /// Named pattern (identifier).
    PatIdent,
    /// Literal pattern.
    PatLiteral,
    /// Tuple pattern.
    PatTuple,
    /// Struct pattern.
    PatStruct,
    /// Enum variant pattern.
    PatEnumVariant,
    /// `p1 | p2` (or-pattern).
    PatOr,
    /// `name @ pat` (binding pattern).
    PatBinding,
}

/// Per-node arena entry: variant discriminant and source position.
#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct NodeData {
    /// Variant discriminant.
    pub kind: NodeKind,
    /// Source span this node covers.
    pub span: Span,
}

// AC: `size_of::<NodeData>() <= 32 bytes`. Currently 16 with alignment.
const_assert!(size_of::<NodeData>() <= 32);

impl NodeData {
    /// Construct a `NodeData` directly. Most callers should use
    /// [`AstArena::alloc`] instead.
    #[must_use]
    pub fn new(kind: NodeKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Slab-allocated AST storage for one source file.
///
/// `AstArena` is the owner of every AST node; nodes are referenced by
/// [`NodeId`] for the arena's lifetime. The arena is single-pass write,
/// many-pass read: parsers and lowering passes mint new ids in order,
/// then later passes index into the arena read-only.
///
/// The parallel vectors (`items`, `exprs`, `stmts`, `types`, `patterns`)
/// store optional category-specific data. All vectors are kept in sync:
/// each `alloc*` call appends to the `nodes` vector and pushes `None`
/// onto all OTHER category vectors (or the appropriate data for its own
/// category).
///
/// [`ItemData`]: crate::ItemData
#[derive(Debug, Default)]
pub struct AstArena {
    nodes: Vec<NodeData>,
    items: Vec<Option<Box<crate::ItemData>>>,
    exprs: Vec<Option<Box<crate::ExprData>>>,
    stmts: Vec<Option<Box<crate::StmtData>>>,
    types: Vec<Option<Box<crate::TypeData>>>,
    patterns: Vec<Option<Box<crate::PatternData>>>,
    /// Interned mnemonic strings (for assembly instruction names).
    /// Index 0 is unused; valid IDs start at 1.
    mnemonic_table: Vec<String>,
}

impl AstArena {
    /// Construct an empty arena.
    #[must_use]
    pub fn new() -> Self {
        let mut s = Self::default();
        // Reserve index 0 in mnemonic_table so that valid IDs start at 1.
        s.mnemonic_table.push(String::new());
        s
    }

    /// Construct an arena with capacity for `n` nodes pre-reserved.
    #[must_use]
    pub fn with_capacity(n: usize) -> Self {
        let mut s = Self {
            nodes: Vec::with_capacity(n),
            items: Vec::with_capacity(n),
            exprs: Vec::with_capacity(n),
            stmts: Vec::with_capacity(n),
            types: Vec::with_capacity(n),
            patterns: Vec::with_capacity(n),
            mnemonic_table: Vec::new(),
        };
        // Reserve index 0 in mnemonic_table so that valid IDs start at 1.
        s.mnemonic_table.push(String::new());
        s
    }

    /// Allocate a new node with the given kind and span, returning its
    /// stable [`NodeId`]. IDs are monotonically increasing.
    ///
    /// For non-item nodes, the corresponding slot in `items` is set to
    /// `None`, and similarly for other category vectors. Use [`alloc_item`]
    /// for item nodes; use category-specific allocators (`alloc_expr`, etc.)
    /// for nodes with category-specific data.
    ///
    /// # Panics
    ///
    /// Panics if the arena would exceed `u32::MAX` nodes — a 4 G AST
    /// is not a realistic target.
    ///
    /// [`alloc_item`]: Self::alloc_item
    pub fn alloc(&mut self, kind: NodeKind, span: Span) -> NodeId {
        let next = self
            .nodes
            .len()
            .checked_add(1)
            .expect("AST node count overflow");
        let id = NodeId::new(u32::try_from(next).expect("more than u32::MAX nodes"))
            .expect("node count + 1 is non-zero");
        self.nodes.push(NodeData::new(kind, span));
        self.items.push(None);
        self.exprs.push(None);
        self.stmts.push(None);
        self.types.push(None);
        self.patterns.push(None);
        id
    }

    /// Number of nodes allocated so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// `true` iff no nodes have been allocated.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Borrow the underlying slice of node data.
    #[must_use]
    pub fn as_slice(&self) -> &[NodeData] {
        &self.nodes
    }

    /// Return `None` if `id` was not minted by this arena (i.e., its
    /// index is past the current size).
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&NodeData> {
        self.nodes.get(id.index())
    }

    /// Allocate an item node with its structured payload, returning its
    /// stable [`NodeId`].
    ///
    /// The `data` is stored in the parallel `items` vector; other
    /// category vectors are filled with `None`.
    pub fn alloc_item(&mut self, kind: NodeKind, span: Span, data: crate::ItemData) -> NodeId {
        let next = self
            .nodes
            .len()
            .checked_add(1)
            .expect("AST node count overflow");
        let id = NodeId::new(u32::try_from(next).expect("more than u32::MAX nodes"))
            .expect("node count + 1 is non-zero");
        self.nodes.push(NodeData::new(kind, span));
        self.items.push(Some(Box::new(data)));
        self.exprs.push(None);
        self.stmts.push(None);
        self.types.push(None);
        self.patterns.push(None);
        id
    }

    /// Allocate an expression node with its structured payload, returning its
    /// stable [`NodeId`].
    ///
    /// The `data` is stored in the parallel `exprs` vector; other
    /// category vectors are filled with `None`.
    pub fn alloc_expr(&mut self, kind: NodeKind, span: Span, data: crate::ExprData) -> NodeId {
        let next = self
            .nodes
            .len()
            .checked_add(1)
            .expect("AST node count overflow");
        let id = NodeId::new(u32::try_from(next).expect("more than u32::MAX nodes"))
            .expect("node count + 1 is non-zero");
        self.nodes.push(NodeData::new(kind, span));
        self.items.push(None);
        self.exprs.push(Some(Box::new(data)));
        self.stmts.push(None);
        self.types.push(None);
        self.patterns.push(None);
        id
    }

    /// Allocate a statement node with its structured payload, returning its
    /// stable [`NodeId`].
    ///
    /// The `data` is stored in the parallel `stmts` vector; other
    /// category vectors are filled with `None`.
    pub fn alloc_stmt(&mut self, kind: NodeKind, span: Span, data: crate::StmtData) -> NodeId {
        let next = self
            .nodes
            .len()
            .checked_add(1)
            .expect("AST node count overflow");
        let id = NodeId::new(u32::try_from(next).expect("more than u32::MAX nodes"))
            .expect("node count + 1 is non-zero");
        self.nodes.push(NodeData::new(kind, span));
        self.items.push(None);
        self.exprs.push(None);
        self.stmts.push(Some(Box::new(data)));
        self.types.push(None);
        self.patterns.push(None);
        id
    }

    /// Allocate a type node with its structured payload, returning its
    /// stable [`NodeId`].
    ///
    /// The `data` is stored in the parallel `types` vector; other
    /// category vectors are filled with `None`.
    pub fn alloc_type(&mut self, kind: NodeKind, span: Span, data: crate::TypeData) -> NodeId {
        let next = self
            .nodes
            .len()
            .checked_add(1)
            .expect("AST node count overflow");
        let id = NodeId::new(u32::try_from(next).expect("more than u32::MAX nodes"))
            .expect("node count + 1 is non-zero");
        self.nodes.push(NodeData::new(kind, span));
        self.items.push(None);
        self.exprs.push(None);
        self.stmts.push(None);
        self.types.push(Some(Box::new(data)));
        self.patterns.push(None);
        id
    }

    /// Allocate a pattern node with its structured payload, returning its
    /// stable [`NodeId`].
    ///
    /// The `data` is stored in the parallel `patterns` vector; other
    /// category vectors are filled with `None`.
    pub fn alloc_pattern(
        &mut self,
        kind: NodeKind,
        span: Span,
        data: crate::PatternData,
    ) -> NodeId {
        let next = self
            .nodes
            .len()
            .checked_add(1)
            .expect("AST node count overflow");
        let id = NodeId::new(u32::try_from(next).expect("more than u32::MAX nodes"))
            .expect("node count + 1 is non-zero");
        self.nodes.push(NodeData::new(kind, span));
        self.items.push(None);
        self.exprs.push(None);
        self.stmts.push(None);
        self.types.push(None);
        self.patterns.push(Some(Box::new(data)));
        id
    }

    /// Look up the item-data for a node, returning `None` for non-item
    /// nodes or out-of-range ids.
    #[must_use]
    pub fn item_data(&self, id: NodeId) -> Option<&crate::ItemData> {
        self.items.get(id.index())?.as_ref().map(|b| b.as_ref())
    }

    /// Look up the expression-data for a node, returning `None` for non-expression
    /// nodes or out-of-range ids.
    #[must_use]
    pub fn expr_data(&self, id: NodeId) -> Option<&crate::ExprData> {
        self.exprs.get(id.index())?.as_ref().map(|b| b.as_ref())
    }

    /// Look up the statement-data for a node, returning `None` for non-statement
    /// nodes or out-of-range ids.
    #[must_use]
    pub fn stmt_data(&self, id: NodeId) -> Option<&crate::StmtData> {
        self.stmts.get(id.index())?.as_ref().map(|b| b.as_ref())
    }

    /// Look up the type-data for a node, returning `None` for non-type
    /// nodes or out-of-range ids.
    #[must_use]
    pub fn type_data(&self, id: NodeId) -> Option<&crate::TypeData> {
        self.types.get(id.index())?.as_ref().map(|b| b.as_ref())
    }

    /// Look up the pattern-data for a node, returning `None` for non-pattern
    /// nodes or out-of-range ids.
    #[must_use]
    pub fn pattern_data(&self, id: NodeId) -> Option<&crate::PatternData> {
        self.patterns.get(id.index())?.as_ref().map(|b| b.as_ref())
    }

    /// Intern a mnemonic string (assembly instruction name) and return its ID.
    ///
    /// IDs are stable within the arena's lifetime. IDs start at 1;
    /// index 0 is reserved.
    pub fn intern_mnemonic(&mut self, s: &str) -> u32 {
        if let Some(idx) = self.mnemonic_table.iter().position(|t| t == s) {
            return idx as u32;
        }
        let id = self.mnemonic_table.len() as u32;
        self.mnemonic_table.push(s.to_owned());
        id
    }

    /// Retrieve the mnemonic string for a given interned ID.
    ///
    /// Returns an empty string if the ID is 0 or out of range.
    #[must_use]
    pub fn mnemonic_str(&self, id: u32) -> &str {
        self.mnemonic_table
            .get(id as usize)
            .map(|s| s.as_str())
            .unwrap_or("")
    }
}

impl Index<NodeId> for AstArena {
    type Output = NodeData;

    fn index(&self, id: NodeId) -> &Self::Output {
        &self.nodes[id.index()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn alloc_returns_increasing_ids() {
        let mut a = AstArena::new();
        let a1 = a.alloc(NodeKind::Placeholder, span());
        let a2 = a.alloc(NodeKind::Placeholder, span());
        let a3 = a.alloc(NodeKind::Placeholder, span());
        assert_eq!(a1.get(), 1);
        assert_eq!(a2.get(), 2);
        assert_eq!(a3.get(), 3);
    }

    #[test]
    fn index_returns_node_data() {
        let mut a = AstArena::new();
        let id = a.alloc(NodeKind::Placeholder, span());
        assert_eq!(a[id].kind, NodeKind::Placeholder);
        assert_eq!(a[id].span, span());
    }

    #[test]
    fn get_returns_none_for_out_of_range() {
        let a = AstArena::new();
        let stray = NodeId::new(7).unwrap();
        assert!(a.get(stray).is_none());
    }

    #[test]
    fn len_and_empty_reflect_state() {
        let mut a = AstArena::new();
        assert!(a.is_empty());
        assert_eq!(a.len(), 0);
        a.alloc(NodeKind::Placeholder, span());
        assert!(!a.is_empty());
        assert_eq!(a.len(), 1);
    }

    #[test]
    fn with_capacity_pre_reserves() {
        // No assertion about Vec internals; just verify the constructor
        // does not panic and produces an empty arena.
        let a = AstArena::with_capacity(64);
        assert_eq!(a.len(), 0);
    }

    #[test]
    fn one_million_allocs_completes() {
        // Informational: the AC mentions <200ms; we do not measure here
        // (no bench harness yet) but we do verify correctness at scale.
        let mut a = AstArena::with_capacity(1_000_000);
        for _ in 0..1_000_000 {
            a.alloc(NodeKind::Placeholder, span());
        }
        assert_eq!(a.len(), 1_000_000);
        let last = NodeId::new(1_000_000).unwrap();
        assert_eq!(a[last].kind, NodeKind::Placeholder);
    }

    #[test]
    fn node_data_size_is_within_budget() {
        // §AC: size_of::<NodeData>() <= 32 bytes. const_assert above is
        // the binding gate; runtime check mirrors it for visibility.
        assert!(size_of::<NodeData>() <= 32);
    }

    #[test]
    fn alloc_for_non_item_does_not_populate_item_data() {
        let mut a = AstArena::new();
        let id = a.alloc(NodeKind::Placeholder, span());
        assert!(a.item_data(id).is_none());
    }

    #[test]
    fn alloc_item_populates_item_data() {
        use crate::ItemData;
        let mut a = AstArena::new();
        // Allocate a Module with a non-existent name and body as a test.
        // In real parsing, these would point to actual Ident and Structure nodes.
        let name_id = NodeId::new(1).unwrap(); // Pretend this is an Ident node
        let body_id = NodeId::new(2).unwrap(); // Pretend this is a Structure node
        let module_id = a.alloc_item(
            NodeKind::Module,
            span(),
            ItemData::Module {
                name: name_id,
                sig: None,
                body: body_id,
                doc: None,
            },
        );
        let item = a.item_data(module_id).expect("item data should exist");
        match item {
            ItemData::Module {
                name,
                sig,
                body,
                doc,
            } => {
                assert_eq!(*name, name_id);
                assert!(sig.is_none());
                assert_eq!(*body, body_id);
                assert!(doc.is_none());
            }
            _ => panic!("expected Module variant"),
        }
    }

    #[test]
    fn alloc_expr_populates_expr_data() {
        use crate::ExprData;
        let mut a = AstArena::new();
        let path_id = NodeId::new(1).unwrap();
        let expr_id = a.alloc_expr(
            NodeKind::ExprPath,
            span(),
            ExprData::Path {
                segments: vec![path_id],
            },
        );
        let expr = a.expr_data(expr_id).expect("expr data should exist");
        match expr {
            ExprData::Path { segments } => {
                assert_eq!(segments.len(), 1);
                assert_eq!(segments[0], path_id);
            }
            _ => panic!("expected Path variant"),
        }
    }

    #[test]
    fn alloc_stmt_populates_stmt_data() {
        use crate::StmtData;
        let mut a = AstArena::new();
        let expr_id = NodeId::new(1).unwrap();
        let stmt_id = a.alloc_stmt(NodeKind::StmtExpr, span(), StmtData::Expr { expr: expr_id });
        let stmt = a.stmt_data(stmt_id).expect("stmt data should exist");
        match stmt {
            StmtData::Expr { expr } => {
                assert_eq!(*expr, expr_id);
            }
            _ => panic!("expected Expr variant"),
        }
    }

    #[test]
    fn alloc_type_populates_type_data() {
        use crate::TypeData;
        let mut a = AstArena::new();
        let name_id = NodeId::new(1).unwrap();
        let type_id = a.alloc_type(
            NodeKind::TypeName,
            span(),
            TypeData::Name {
                name: name_id,
                args: vec![],
            },
        );
        let ty = a.type_data(type_id).expect("type data should exist");
        match ty {
            TypeData::Name { name, args } => {
                assert_eq!(*name, name_id);
                assert!(args.is_empty());
            }
            _ => panic!("expected Name variant"),
        }
    }

    #[test]
    fn alloc_pattern_populates_pattern_data() {
        use crate::PatternData;
        let mut a = AstArena::new();
        let pat_id = a.alloc_pattern(NodeKind::PatWildcard, span(), PatternData::Wildcard);
        let pat = a.pattern_data(pat_id).expect("pattern data should exist");
        match pat {
            PatternData::Wildcard => {}
            _ => panic!("expected Wildcard variant"),
        }
    }

    #[test]
    fn intern_mnemonic_returns_stable_ids() {
        let mut a = AstArena::new();
        let id1 = a.intern_mnemonic("mov");
        let id2 = a.intern_mnemonic("add");
        let id1_again = a.intern_mnemonic("mov");
        assert_eq!(id1, id1_again);
        assert_ne!(id1, id2);
    }

    #[test]
    fn mnemonic_str_retrieves_interned_values() {
        let mut a = AstArena::new();
        let mov_id = a.intern_mnemonic("mov");
        let add_id = a.intern_mnemonic("add");
        assert_eq!(a.mnemonic_str(mov_id), "mov");
        assert_eq!(a.mnemonic_str(add_id), "add");
    }

    #[test]
    fn parallel_vectors_stay_in_sync() {
        use crate::{ExprData, ItemData};
        let mut a = AstArena::new();
        // Allocate one of each kind
        let _id1 = a.alloc(NodeKind::Placeholder, span());
        let _id2 = a.alloc_item(
            NodeKind::Let,
            span(),
            ItemData::Let {
                name: NodeId::new(1).unwrap(),
                ty: None,
                value: NodeId::new(2).unwrap(),
                doc: None,
            },
        );
        let _id3 = a.alloc_expr(
            NodeKind::ExprLiteral,
            span(),
            ExprData::Literal {
                lit: NodeId::new(3).unwrap(),
            },
        );
        // All vectors should have the same length
        assert_eq!(a.len(), 3);
        // Check that each category data is only in its own vector
        assert!(a.expr_data(_id1).is_none());
        assert!(a.expr_data(_id2).is_none());
        assert!(a.expr_data(_id3).is_some());
    }
}
