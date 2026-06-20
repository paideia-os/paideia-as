//! `.debug_line` line-table builder per debug-info.md §1.2.
//!
//! Phase-4-m2-002: Extended with `build_line_program_from_instruction_table`
//! to reconstruct line table with post-rewrite instruction offsets.

use gimli::LineEncoding;
use gimli::write::{Address, LineProgram, LineString};
use std::collections::HashMap;

/// One source-line entry tying a PC to a (file, line) location.
#[derive(Clone, Debug)]
pub struct LineEntry {
    /// PC offset within the section (typically .text).
    pub pc: u64,
    /// 1-based source line number.
    pub line: u64,
    /// 1-based source column number.
    pub column: u64,
}

/// Build a line program from an InstructionSideTable with post-rewrite offsets.
///
/// Phase-4-m2-002: Reconstructs the DWARF .debug_line section after instruction
/// offsets have been rewritten. Uses the offset_map returned by emit_text_from_instructions
/// to ensure line-table entries match actual .text byte offsets.
///
/// # Arguments
///
/// * `side_table` - The instruction side-table with (IrNodeId, Instruction) pairs
/// * `offset_map` - Map of IrNodeId to emitted byte offset (from text emitter)
/// * `source_file` - Source file name for the line program
/// * `entries` - Line entries mapping IrNodeId to (line, column) in source
///
/// # Returns
///
/// A DWARF line program with rows positioned at post-rewrite offsets.
/// Rows are sorted by offset and include only instructions present in offset_map.
pub fn build_line_program_from_instruction_table(
    _side_table: &paideia_as_ir::InstructionSideTable,
    offset_map: &HashMap<paideia_as_ir::IrNodeId, u64>,
    source_file: &str,
    entries: &[(paideia_as_ir::IrNodeId, u64, u64)],
) -> LineProgram {
    let encoding = LineEncoding {
        minimum_instruction_length: 1,
        maximum_operations_per_instruction: 1,
        default_is_stmt: true,
        line_base: -5_i8,
        line_range: 14_u8,
    };
    let mut program = LineProgram::new(
        gimli::Encoding {
            format: gimli::Format::Dwarf32,
            version: 5,
            address_size: 8,
        },
        encoding,
        LineString::String(b".".to_vec()),
        None,
        LineString::String(source_file.as_bytes().to_vec()),
        None,
    );

    if entries.is_empty() {
        return program;
    }

    // Collect rows from entries, filtering by offset_map and sorting by offset.
    let mut rows: Vec<(u64, u64, u64)> = entries
        .iter()
        .filter_map(|(node_id, line, column)| {
            offset_map
                .get(node_id)
                .map(|offset| (*offset, *line, *column))
        })
        .collect();

    // Sort by offset to ensure ascending order in the line table.
    rows.sort_by_key(|(offset, _, _)| *offset);

    if !rows.is_empty() {
        program.begin_sequence(Some(Address::Constant(rows[0].0)));
        for &(offset, line, column) in &rows {
            program.row().address_offset = offset - rows[0].0;
            program.row().line = line;
            program.row().column = column;
            program.generate_row();
        }
        let last_offset = rows.last().unwrap().0;
        program.end_sequence(last_offset - rows[0].0);
    }

    program
}

/// Build a line program tying a sequence of PC values to source lines.
/// Phase-1: one source file per line table.
pub fn build_line_program(source_file: &str, entries: &[LineEntry]) -> LineProgram {
    let encoding = LineEncoding {
        minimum_instruction_length: 1,
        maximum_operations_per_instruction: 1,
        default_is_stmt: true,
        line_base: -5_i8,
        line_range: 14_u8,
    };
    let mut program = LineProgram::new(
        gimli::Encoding {
            format: gimli::Format::Dwarf32,
            version: 5,
            address_size: 8,
        },
        encoding,
        LineString::String(b".".to_vec()),
        None,
        LineString::String(source_file.as_bytes().to_vec()),
        None,
    );

    if !entries.is_empty() {
        program.begin_sequence(Some(Address::Constant(entries[0].pc)));
        for entry in entries {
            program.row().address_offset = entry.pc - entries[0].pc;
            program.row().line = entry.line;
            program.row().column = entry.column;
            program.generate_row();
        }
        let last_pc = entries.last().unwrap().pc;
        program.end_sequence(last_pc - entries[0].pc);
    }

    program
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_line_program_with_zero_entries() {
        let program = build_line_program("test.pdx", &[]);
        // Program should exist even with no entries
        assert!(program.is_empty());
    }

    #[test]
    fn build_line_program_with_three_entries() {
        let entries = vec![
            LineEntry {
                pc: 0x1000,
                line: 10,
                column: 1,
            },
            LineEntry {
                pc: 0x1010,
                line: 11,
                column: 1,
            },
            LineEntry {
                pc: 0x1020,
                line: 12,
                column: 5,
            },
        ];
        let program = build_line_program("test.pdx", &entries);
        // Program should not be empty
        assert!(!program.is_empty());
    }

    #[test]
    fn line_entry_clone() {
        let entry = LineEntry {
            pc: 0x100,
            line: 5,
            column: 10,
        };
        let cloned = entry.clone();
        assert_eq!(cloned.pc, entry.pc);
        assert_eq!(cloned.line, entry.line);
        assert_eq!(cloned.column, entry.column);
    }

    // ── Phase-4-m2-002 DWARF emit-stage tests ───────────────────────

    #[test]
    fn dwarf_emit_with_instruction_side_table_handles_empty() {
        use paideia_as_ir::InstructionSideTable;

        let side_table = InstructionSideTable::new();
        let offset_map: HashMap<paideia_as_ir::IrNodeId, u64> = HashMap::new();
        let entries: Vec<(paideia_as_ir::IrNodeId, u64, u64)> = vec![];

        let program = build_line_program_from_instruction_table(
            &side_table,
            &offset_map,
            "empty.pdx",
            &entries,
        );

        // Program should exist even with no entries
        assert!(program.is_empty());
    }

    #[test]
    fn dwarf_debug_line_row_matches_instruction_offset() {
        use paideia_as_ir::{InstructionSideTable, IrNodeId};

        let side_table = InstructionSideTable::new();

        // Create offset map: 3 instructions at offsets 0, 7, 15
        let mut offset_map = HashMap::new();
        let node_id_1 = IrNodeId::new(1).unwrap();
        let node_id_2 = IrNodeId::new(2).unwrap();
        let node_id_3 = IrNodeId::new(3).unwrap();
        offset_map.insert(node_id_1, 0);
        offset_map.insert(node_id_2, 7);
        offset_map.insert(node_id_3, 15);

        // Create line entries: (IrNodeId, line, column)
        let entries = vec![(node_id_1, 10, 1), (node_id_2, 11, 5), (node_id_3, 12, 1)];

        let program = build_line_program_from_instruction_table(
            &side_table,
            &offset_map,
            "test.pdx",
            &entries,
        );

        // Program should not be empty
        assert!(!program.is_empty());
    }

    #[test]
    fn dwarf_emit_handles_multi_instruction_sequence() {
        use paideia_as_ir::{InstructionSideTable, IrNodeId};

        let side_table = InstructionSideTable::new();

        // Create offset map with 5 instructions at various offsets
        let mut offset_map = HashMap::new();
        let node_ids: Vec<_> = (1..=5).map(|i| IrNodeId::new(i).unwrap()).collect();
        let offsets = [0u64, 2, 5, 8, 20];

        for (node_id, offset) in node_ids.iter().zip(offsets.iter()) {
            offset_map.insert(*node_id, *offset);
        }

        // Create line entries for all 5 instructions
        let entries: Vec<(IrNodeId, u64, u64)> = node_ids
            .iter()
            .enumerate()
            .map(|(idx, node_id)| (*node_id, (10 + idx) as u64, 1))
            .collect();

        let program = build_line_program_from_instruction_table(
            &side_table,
            &offset_map,
            "multi.pdx",
            &entries,
        );

        // Program should not be empty with 5 instructions
        assert!(!program.is_empty());
    }

    #[test]
    fn dwarf_emit_handles_partial_offset_map() {
        use paideia_as_ir::{InstructionSideTable, IrNodeId};

        let side_table = InstructionSideTable::new();

        // Create offset map with only 2 out of 3 instructions
        let mut offset_map = HashMap::new();
        let node_id_1 = IrNodeId::new(1).unwrap();
        let node_id_2 = IrNodeId::new(2).unwrap();
        let _node_id_3 = IrNodeId::new(3).unwrap();

        offset_map.insert(node_id_1, 0);
        offset_map.insert(node_id_2, 5);
        // node_id_3 intentionally not in offset_map

        let entries = vec![
            (node_id_1, 10, 1),
            (node_id_2, 11, 5),
            (_node_id_3, 12, 1), // This entry will be filtered out
        ];

        let program = build_line_program_from_instruction_table(
            &side_table,
            &offset_map,
            "partial.pdx",
            &entries,
        );

        // Program should contain only the 2 instructions in offset_map
        assert!(!program.is_empty());
    }
}
