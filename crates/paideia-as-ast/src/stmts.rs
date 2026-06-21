//! Statement-specific structured data (§8 Stmt grammar).
//!
//! [`StmtData`] is an enum carrying the semantic payload for statement nodes.
//! Statements are the building blocks of block bodies and unsafe blocks.
//! Categories: LetStmt, ExprStmt, ReturnStmt, InstructionStmt.

use crate::NodeId;

/// Structured payload for statement nodes.
///
/// Each variant corresponds to a statement kind as specified in §8 of the
/// syntax reference. Child `NodeId` fields point to other nodes in the arena.
#[derive(Clone, Debug)]
pub enum StmtData {
    /// `let [mut] name: ty? = expr;`.
    ///
    /// Local let-binding (statement form). Distinct from top-level `ItemData::Let`.
    /// The `mutable` flag indicates whether this is a mutable binding (`let mut ...`).
    Let {
        /// Whether this is a mutable binding (true for `let mut ...`, false for `let ...`).
        mutable: bool,
        /// Binding name (Ident node).
        name: NodeId,
        /// Optional type annotation (Type node).
        ty: Option<NodeId>,
        /// Value expression.
        value: NodeId,
    },

    /// `expr;`.
    ///
    /// Expression statement: any expression can be a statement if followed by `;`.
    Expr {
        /// Expression.
        expr: NodeId,
    },

    /// `return expr?;`.
    ///
    /// Return statement with optional value.
    Return {
        /// Optional return value.
        value: Option<NodeId>,
    },

    /// `mnemonic operand, operand, ...`.
    ///
    /// Assembly instruction statement. The mnemonic is interned via the arena's
    /// mnemonic table; operand nodes hold OperandRegister, OperandImmediate, or
    /// OperandMemoryRef data.
    Instruction {
        /// Interned mnemonic ID (index into arena.mnemonic_table).
        mnemonic: u32,
        /// Operand nodes.
        operands: Vec<NodeId>,
    },

    /// `label_name:`.
    ///
    /// Label declaration statement (Phase 6 m4-002). Labels are targets for Jcc/Jmp
    /// instructions within unsafe blocks. The label name is stored as an identifier node.
    /// Duplicate labels → U1609; unknown references → U1610.
    Label {
        /// Label name (Ident node).
        name: NodeId,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_nodeid(n: u32) -> NodeId {
        NodeId::new(n).unwrap()
    }

    #[test]
    fn stmt_let_constructs() {
        let name = make_nodeid(1);
        let ty = make_nodeid(2);
        let value = make_nodeid(3);
        let stmt = StmtData::Let {
            mutable: false,
            name,
            ty: Some(ty),
            value,
        };
        match stmt {
            StmtData::Let {
                mutable: m,
                name: n,
                ty: t,
                value: v,
            } => {
                assert!(!m);
                assert_eq!(n, name);
                assert_eq!(t, Some(ty));
                assert_eq!(v, value);
            }
            _ => panic!("expected Let variant"),
        }
    }

    #[test]
    fn stmt_expr_constructs() {
        let expr = make_nodeid(1);
        let stmt = StmtData::Expr { expr };
        match stmt {
            StmtData::Expr { expr: e } => {
                assert_eq!(e, expr);
            }
            _ => panic!("expected Expr variant"),
        }
    }

    #[test]
    fn stmt_return_constructs() {
        let value = make_nodeid(1);
        let stmt = StmtData::Return { value: Some(value) };
        match stmt {
            StmtData::Return { value: v } => {
                assert_eq!(v, Some(value));
            }
            _ => panic!("expected Return variant"),
        }
    }

    #[test]
    fn stmt_instruction_constructs() {
        let op1 = make_nodeid(1);
        let op2 = make_nodeid(2);
        let stmt = StmtData::Instruction {
            mnemonic: 42,
            operands: vec![op1, op2],
        };
        match stmt {
            StmtData::Instruction {
                mnemonic: m,
                operands: ops,
            } => {
                assert_eq!(m, 42);
                assert_eq!(ops.len(), 2);
            }
            _ => panic!("expected Instruction variant"),
        }
    }

    #[test]
    fn stmt_return_no_value_constructs() {
        let stmt = StmtData::Return { value: None };
        match stmt {
            StmtData::Return { value } => {
                assert!(value.is_none());
            }
            _ => panic!("expected Return variant"),
        }
    }

    #[test]
    fn stmt_let_mutable_constructs() {
        let name = make_nodeid(1);
        let ty = make_nodeid(2);
        let value = make_nodeid(3);
        let stmt = StmtData::Let {
            mutable: true,
            name,
            ty: Some(ty),
            value,
        };
        match stmt {
            StmtData::Let {
                mutable: m,
                name: n,
                ty: t,
                value: v,
            } => {
                assert!(m);
                assert_eq!(n, name);
                assert_eq!(t, Some(ty));
                assert_eq!(v, value);
            }
            _ => panic!("expected Let variant"),
        }
    }
}
