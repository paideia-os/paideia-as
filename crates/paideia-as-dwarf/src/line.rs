//! `.debug_line` line-table builder per debug-info.md §1.2.

use gimli::LineEncoding;
use gimli::write::{Address, LineProgram, LineString};

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
}
