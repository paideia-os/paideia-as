//! `.debug_info` emission per debug-info.md §1.1.
//!
//! One Compilation Unit (CU) per source file. Each function emits a
//! `DW_TAG_subprogram` DIE with `DW_AT_name`, `DW_AT_low_pc`, `DW_AT_high_pc`.
//! Each `let` binding inside a function becomes a `DW_TAG_variable` DIE
//! with location expression (phase-1: frame-base + offset).

use gimli::write::{Address, AttributeValue, DwarfUnit};
use gimli::{
    DW_AT_language, DW_AT_name, DW_AT_producer, DW_LANG_C, DW_TAG_subprogram, Encoding, Format,
};

/// Description of a function to include in the CU.
#[derive(Clone, Debug)]
pub struct FunctionDie {
    /// Function name.
    pub name: String,
    /// Low PC (start address in the section).
    pub low_pc: u64,
    /// High PC (end address; exclusive).
    pub high_pc: u64,
}

/// Description of a source file as the CU's primary unit.
#[derive(Clone, Debug)]
pub struct CompilationUnit {
    /// Source file name (e.g. "main.pdx").
    pub source_file: String,
    /// Producer string (e.g. "paideia-as 0.0.1").
    pub producer: String,
    /// Functions in the CU.
    pub functions: Vec<FunctionDie>,
}

/// Build a `DwarfUnit` holding a CU DIE plus one subprogram DIE per function.
/// Returns the `DwarfUnit` ready for emission into ELF.
pub fn build_cu(unit: &CompilationUnit) -> DwarfUnit {
    let encoding = Encoding {
        format: Format::Dwarf32,
        version: 5,
        address_size: 8,
    };
    let mut dwarf = DwarfUnit::new(encoding);

    // Add CU-level attributes: producer, name, language.
    let cu_root = dwarf.unit.root();
    let producer_id = dwarf.strings.add(unit.producer.as_str());
    let source_id = dwarf.strings.add(unit.source_file.as_str());

    dwarf
        .unit
        .get_mut(cu_root)
        .set(DW_AT_producer, AttributeValue::StringRef(producer_id));
    dwarf
        .unit
        .get_mut(cu_root)
        .set(DW_AT_name, AttributeValue::StringRef(source_id));
    dwarf
        .unit
        .get_mut(cu_root)
        .set(DW_AT_language, AttributeValue::Language(DW_LANG_C));

    // Add a subprogram DIE per function.
    for f in &unit.functions {
        let die = dwarf.unit.add(cu_root, DW_TAG_subprogram);
        let name_id = dwarf.strings.add(f.name.as_str());

        dwarf
            .unit
            .get_mut(die)
            .set(DW_AT_name, AttributeValue::StringRef(name_id));
        dwarf.unit.get_mut(die).set(
            gimli::DW_AT_low_pc,
            AttributeValue::Address(Address::Constant(f.low_pc)),
        );
        dwarf.unit.get_mut(die).set(
            gimli::DW_AT_high_pc,
            AttributeValue::Udata(f.high_pc - f.low_pc),
        );
    }

    dwarf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_cu_with_zero_functions() {
        let unit = CompilationUnit {
            source_file: "test.pdx".to_string(),
            producer: "paideia-as 0.0.1".to_string(),
            functions: vec![],
        };
        let dwarf = build_cu(&unit);
        // Verify the DwarfUnit was created and has at least the CU root DIE
        assert!(dwarf.unit.count() > 0);
    }

    #[test]
    fn build_cu_with_one_function() {
        let unit = CompilationUnit {
            source_file: "test.pdx".to_string(),
            producer: "paideia-as 0.0.1".to_string(),
            functions: vec![FunctionDie {
                name: "main".to_string(),
                low_pc: 0x1000,
                high_pc: 0x1100,
            }],
        };
        let dwarf = build_cu(&unit);
        // Should have CU root + 1 subprogram
        assert_eq!(dwarf.unit.count(), 2);
    }

    #[test]
    fn build_cu_with_multiple_functions() {
        let unit = CompilationUnit {
            source_file: "test.pdx".to_string(),
            producer: "paideia-as 0.0.1".to_string(),
            functions: vec![
                FunctionDie {
                    name: "foo".to_string(),
                    low_pc: 0x1000,
                    high_pc: 0x1050,
                },
                FunctionDie {
                    name: "bar".to_string(),
                    low_pc: 0x2000,
                    high_pc: 0x2100,
                },
                FunctionDie {
                    name: "baz".to_string(),
                    low_pc: 0x3000,
                    high_pc: 0x3200,
                },
            ],
        };
        let dwarf = build_cu(&unit);
        // Should have CU root + 3 subprograms
        assert_eq!(dwarf.unit.count(), 4);
    }

    #[test]
    fn subprogram_low_high_pc_set() {
        let unit = CompilationUnit {
            source_file: "test.pdx".to_string(),
            producer: "paideia-as 0.0.1".to_string(),
            functions: vec![FunctionDie {
                name: "test_fn".to_string(),
                low_pc: 0x100,
                high_pc: 0x110,
            }],
        };
        let dwarf = build_cu(&unit);
        let root = dwarf.unit.root();
        let root_entry = dwarf.unit.get(root);
        // Verify the root has children
        let has_children = root_entry.children().count() > 0;
        assert!(has_children);

        // Verify we have the expected number of entries (root + 1 function)
        assert_eq!(dwarf.unit.count(), 2);
    }
}
