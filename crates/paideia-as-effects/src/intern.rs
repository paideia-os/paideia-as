use std::collections::HashMap;

use paideia_as_ir::EffectRowId;

use crate::row::EffectRow;

/// An interned table of effect rows.
///
/// Maps each unique `EffectRow` to a stable `EffectRowId`. The empty row is
/// always pre-seeded at `EffectRowId::EMPTY (0)`.
pub struct EffectInterner {
    rows: Vec<EffectRow>,
    by_value: HashMap<EffectRow, EffectRowId>,
}

impl EffectInterner {
    /// Construct a new interner with the empty row pre-seeded.
    pub fn new() -> Self {
        let mut me = Self {
            rows: vec![],
            by_value: HashMap::new(),
        };
        // EffectRowId::EMPTY (=0) is the empty-row sentinel — pre-seed.
        me.rows.push(EffectRow::empty());
        me.by_value.insert(EffectRow::empty(), EffectRowId::EMPTY);
        me
    }

    /// Intern a row, returning its stable id.
    ///
    /// If the row is already interned, returns its existing id.
    /// Otherwise, allocates a new id.
    pub fn intern(&mut self, row: EffectRow) -> EffectRowId {
        if let Some(id) = self.by_value.get(&row) {
            return *id;
        }
        let id = EffectRowId(self.rows.len() as u32);
        self.by_value.insert(row.clone(), id);
        self.rows.push(row);
        id
    }

    /// Retrieve the row for a given id.
    ///
    /// Panics if the id is out of bounds.
    pub fn get(&self, id: EffectRowId) -> &EffectRow {
        &self.rows[id.0 as usize]
    }

    /// Retrieve the empty row's id.
    pub fn empty(&self) -> EffectRowId {
        EffectRowId::EMPTY
    }

    /// The number of rows currently interned.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// `true` if no rows are interned (only possible before any intern calls).
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

impl Default for EffectInterner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::row::EffectId;

    #[test]
    fn empty_is_canonical() {
        let mut interner = EffectInterner::new();
        let empty_id = interner.intern(EffectRow::empty());
        assert_eq!(empty_id, EffectRowId::EMPTY);
    }

    #[test]
    fn equal_rows_share_id() {
        let mut interner = EffectInterner::new();
        let e1 = EffectId::new(1).unwrap();

        let row = EffectRow::from_ids(vec![e1], None);
        let id1 = interner.intern(row.clone());
        let id2 = interner.intern(row.clone());

        assert_eq!(id1, id2);
    }

    #[test]
    fn distinct_rows_get_distinct_ids() {
        let mut interner = EffectInterner::new();
        let e1 = EffectId::new(1).unwrap();
        let e2 = EffectId::new(2).unwrap();

        let row1 = EffectRow::from_ids(vec![e1], None);
        let row2 = EffectRow::from_ids(vec![e2], None);

        let id1 = interner.intern(row1);
        let id2 = interner.intern(row2);

        assert_ne!(id1, id2);
    }
}
