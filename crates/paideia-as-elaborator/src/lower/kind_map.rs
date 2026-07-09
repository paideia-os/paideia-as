//! AST `NodeKind` → IR `IrKind` classifier for phase-1 structural lowering.
//!
//! Split out of `lower.rs` (2026-07-08). Pure function; no external state.
//! The mapping table lives in `lower.rs`'s module docs.

use paideia_as_ast::NodeKind;
use paideia_as_ir::IrKind;

/// Map an AST NodeKind to an IR IrKind per the lowering table.
///
/// This function is the heart of phase-1 structural lowering. The mapping
/// is deliberately coarse: every Placeholder, Module, Action, Var category
/// is a bucket for nodes that will be refined with proper IR variants in
/// later PRs.
pub(super) fn map_node_kind(kind: NodeKind) -> IrKind {
    match kind {
        // Identifiers and references
        NodeKind::Ident | NodeKind::ExprPath => IrKind::Var,

        // Literals
        NodeKind::ExprLiteral => IrKind::Literal,

        // String and byte string literals (PA10-002): lowered to StringLiteral IR nodes
        // with byte payloads stored in the literal_bytes side-table.
        NodeKind::ExprString | NodeKind::ExprByteString => IrKind::StringLiteral,

        // Inline bytes literals (Issue #1012): @guid("...") and @include_bytes("...") lower
        // to IrKind::InlineBytes with byte payloads stored in the literal_bytes side-table.
        NodeKind::ExprInlineBytes => IrKind::InlineBytes,

        // Operators (all desugared to applications)
        NodeKind::ExprInfix | NodeKind::ExprPrefix | NodeKind::ExprPostfix => IrKind::App,

        // Cast `expr as type` lowers to a dedicated IrKind::Cast (Phase 7 m4-002).
        // The target type is recorded separately in the CastSideTable; the emit
        // pass chooses movsx/movzx/mov per the source and destination widths.
        NodeKind::ExprCast => IrKind::Cast,

        // Function application
        NodeKind::ExprCall => IrKind::App,

        // Lambda abstraction
        NodeKind::ExprLambda => IrKind::Lambda,

        // Handler installation
        NodeKind::ExprWithHandler => IrKind::Handle,

        // Effect operations
        NodeKind::ExprPerform => IrKind::Perform,

        // Resume expressions (phase-1 placeholder)
        NodeKind::ExprResume => IrKind::App,

        // Handler-value construction (phase-1: placeholder mapped to Action)
        // TODO: phase-2 will introduce a dedicated IrKind::HandlerValue when elaborator
        // validates handler arm coverage and parameter binding.
        NodeKind::ExprHandlerValue => IrKind::Action,

        // Unsafe block escape hatch
        NodeKind::ExprUnsafe => IrKind::Unsafe,

        // Blocks and sequences (all mapped to Action placeholders for phase-1)
        NodeKind::ExprBlock | NodeKind::ExprActionBlock | NodeKind::StmtExpr => IrKind::Action,

        // Assembly instruction: mnemonic + operands persisted through lowering
        NodeKind::StmtInstruction => IrKind::RawInstruction,

        // Control flow: phase-4-m1-002 adds Match to IR; phase-4-m1-004 adds Branch
        NodeKind::ExprMatch => IrKind::Match,
        NodeKind::ExprIf => IrKind::Branch,

        // Control flow placeholders (phase-1 does not model these in IR yet)
        NodeKind::ExprLoop | NodeKind::StmtReturn => IrKind::Action,

        // Array literal (Phase 8 m2-002): sequence of element expressions.
        // cmd_build walks children, packs to bytes per element width.
        NodeKind::ExprArrayLit => IrKind::ArrayLit,

        // Array repeat (Phase 9 m1-002): `[expr; count]` → ArrayLit with N copies of expr.
        // During lowering, this is expanded by extract_repeat_count and expand_array_repeat.
        NodeKind::ExprArrayRepeat => IrKind::ArrayLit,

        // Record constructor (Phase 8 m2-003): instantiates a record type with field values.
        // At module level, populate_data_table walks fields and encodes to DataEntry.
        // At runtime, emit_walker lowering dispatches per context.
        NodeKind::ExprRecordCons => IrKind::RecordCons,

        // Let bindings
        NodeKind::StmtLet | NodeKind::Let => IrKind::Let,

        // Module-like constructs (items and declarations)
        NodeKind::Module
        | NodeKind::Signature
        | NodeKind::Structure
        | NodeKind::Effect
        | NodeKind::Capability
        | NodeKind::Struct
        | NodeKind::Enum => IrKind::Module,

        // Functor (parameterized module)
        NodeKind::Functor => IrKind::Functor,

        // Functor parameters and operation signatures (mapped to Var)
        NodeKind::FunctorParam | NodeKind::OpSig => IrKind::Var,

        // Unsafe block item
        NodeKind::UnsafeBlock => IrKind::Unsafe,

        // Placeholders and unknown nodes
        NodeKind::Placeholder => IrKind::Placeholder,

        // Borrow (phase-4-m5): & and &mut expressions → IrKind::Borrow.
        // PA10-006u: in static-init context, address-of-symbol constants are handled specially
        // by the elaborator (populate AddrOfSideTable); elsewhere they are runtime borrows.
        // Note: ExprBorrow covers both immutable (&) and mutable (&mut); the mutable flag
        // is in ExprData::Borrow { expr, mutable }, not a separate NodeKind.
        NodeKind::ExprBorrow => IrKind::Borrow,

        // Deref (*expr): dereference a reference.
        NodeKind::ExprDeref => IrKind::Deref,

        // Field access (receiver.field): access a named field of a record.
        // Phase 6 m3-002: Lowered to dedicated IrKind::FieldAccess with side-table
        // metadata (FieldAccessInfo: type_id, field_index) populated by
        // populate_field_access_info pass.
        NodeKind::ExprFieldAccess => IrKind::FieldAccess,

        // Operands (OperandRegister, OperandImmediate, OperandMemoryRef)
        // These do not appear as top-level nodes in phase-1, but map to Var
        // as a conservative default.
        NodeKind::OperandRegister | NodeKind::OperandImmediate | NodeKind::OperandMemoryRef => {
            IrKind::Var
        }

        // Types (TypeName, TypeFnPtr, TypeTuple, TypeLinearClass, TypeEffectRow)
        // These are not lowered to IR in phase-1 (they stay in the type table).
        // If they appear as top-level nodes, map to Placeholder.
        NodeKind::TypeName
        | NodeKind::TypeFnPtr
        | NodeKind::TypeTuple
        | NodeKind::TypeLinearClass
        | NodeKind::TypeEffectRow => IrKind::Placeholder,

        // Patterns (PatWildcard, PatIdent, PatLiteral, etc.)
        // These are not lowered to IR in phase-1 (they stay in the pattern table).
        // If they appear as top-level nodes, map to Placeholder.
        NodeKind::PatWildcard
        | NodeKind::PatIdent
        | NodeKind::PatLiteral
        | NodeKind::PatTuple
        | NodeKind::PatStruct
        | NodeKind::PatEnumVariant
        | NodeKind::PatOr
        | NodeKind::PatBinding => IrKind::Placeholder,

        // Wildcard for future variants added to NodeKind after phase-1.
        _ => IrKind::Placeholder,
    }
}
