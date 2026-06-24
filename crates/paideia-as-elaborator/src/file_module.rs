//! File-to-module mapping validation for phase-1.
//!
//! Implements §7.6 constraint: each `.pdx` source file declares exactly one
//! top-level module (structure or functor) whose name matches the file basename
//! in PascalCase.
//!
//! Three diagnostic codes:
//! - M0305: file basename does not match top-level module name.
//! - M0306: multiple top-level modules (parser already emits; we guard against double-emit).
//! - M0313: file contains no top-level module.

use std::path::Path;

use paideia_as_ast::{AstArena, ItemData, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity};

/// Diagnostic code for "file basename does not match module name".
pub const M_FILE_NAME_MISMATCH: u16 = 305;

/// Diagnostic code for "multiple top-level modules in file".
/// (Already emitted by parser at parse_item.rs:124; we avoid double-emit.)
pub const M_MULTIPLE_TOP_MODULES: u16 = 306;

/// Diagnostic code for "file contains no top-level module".
pub const M_NO_TOP_MODULE: u16 = 313;

/// Validate the file-to-module mapping for a parsed source file.
///
/// Given the synthetic root Structure node from `parse_source_file`, walks its
/// item list to identify Module declarations. Enforces:
/// - Exactly one module per file (M0313 if none; M0306 if >1).
/// - Module name matches file basename in PascalCase (M0305 if mismatch).
///
/// Returns `true` if validation passes (no errors); `false` if any diagnostic
/// was emitted. The returned boolean can be used to gate downstream lowering.
///
/// # Arguments
///
/// - `file_path`: The `.pdx` file being validated (used to extract basename).
/// - `root`: The synthetic Structure NodeId from `parse_source_file`.
/// - `arena`: The AST arena for accessing node data and spans.
/// - `content`: The source file content (for extracting Ident lexemes).
/// - `diags`: Accumulator for diagnostic messages.
///
/// # Panics
///
/// Panics if `root` is not a Structure node or if accessed NodeIds are invalid.
pub fn validate_file_module_mapping(
    file_path: &Path,
    root: NodeId,
    arena: &AstArena,
    content: &str,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let expected_name =
        expected_module_name(file_path.file_stem().and_then(|s| s.to_str()).unwrap_or(""));

    // Extract the synthetic root Structure node's items.
    let root_node = *arena.get(root).expect("root node must exist");

    if root_node.kind != NodeKind::Structure {
        // This should not happen if parse_source_file works correctly,
        // but we handle it gracefully.
        return false;
    }

    let items = arena
        .item_data(root)
        .and_then(|data| match data {
            ItemData::Structure { items, .. } => Some(items.clone()),
            _ => None,
        })
        .unwrap_or_default();

    // Collect all Module items.
    let mut module_ids = Vec::new();
    for item_id in items {
        if let Some(node_data) = arena.get(item_id)
            && node_data.kind == NodeKind::Module
        {
            module_ids.push(item_id);
        }
    }

    let mut ok = true;

    // Check count: 0 → M0313, >1 → M0306.
    match module_ids.len() {
        0 => {
            let code = m_code(M_NO_TOP_MODULE);
            let diag = Diagnostic::error(code)
                .message("file contains no top-level module; expected exactly one")
                .finish();
            diags.push(diag);
            ok = false;
        }
        1 => {
            // Proceed to name check.
        }
        _ => {
            // Multiple modules: The parser already emits M0306 when it encounters the
            // second module (at parse_item.rs:124). We do not re-emit here to avoid
            // double-emission. Just mark as error and proceed.
            ok = false;
        }
    }

    // If exactly one module, check name match.
    if module_ids.len() == 1 {
        let module_id = module_ids[0];
        if let Some(ItemData::Module { name: name_id, .. }) = arena.item_data(module_id)
            && let Some(name_node) = arena.get(*name_id)
        {
            let name_span = name_node.span;
            let name_start = name_span.byte_start() as usize;
            let name_end = name_span.byte_end() as usize;
            let actual_name = if name_end <= content.len() {
                &content[name_start..name_end]
            } else {
                ""
            };

            if actual_name != expected_name {
                let code = m_code(M_FILE_NAME_MISMATCH);
                let diag = Diagnostic::error(code)
                    .message(format!(
                        "module name '{}' does not match file basename '{}' (expected '{}')",
                        actual_name,
                        file_path.file_stem().and_then(|s| s.to_str()).unwrap_or(""),
                        expected_name
                    ))
                    .with_span(name_span)
                    .finish();
                diags.push(diag);
                ok = false;
            }
        }
    }

    ok
}

/// Convert a file stem to PascalCase module name.
///
/// Splits on `_` or `-`, skips numeric-only segments (for e.g. `01_hello` → `Hello`),
/// capitalizes each remaining segment, and concatenates.
///
/// # Examples
///
/// - `hello` → `Hello`
/// - `my_module` → `MyModule`
/// - `kebab-case` → `KebabCase`
/// - `snake_AND_kebab` → `SnakeAndKebab` (note: AND is lowercased then capitalized)
/// - `01_hello` → `Hello` (numeric prefix skipped)
/// - `02_functions` → `Functions` (numeric prefix skipped)
#[must_use]
pub fn expected_module_name(stem: &str) -> String {
    if stem.is_empty() {
        return String::new();
    }

    let segments: Vec<&str> = stem.split(['_', '-']).collect();

    segments
        .into_iter()
        .filter(|s| !s.is_empty() && !s.chars().all(|c| c.is_ascii_digit()))
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    let rest: String = chars.collect::<String>().to_lowercase();
                    first.to_uppercase().chain(rest.chars()).collect()
                }
            }
        })
        .collect()
}

/// Helper to construct a diagnostic code for category M.
fn m_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::M, Severity::Error, n).expect("valid M code")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test 1: PascalCase transformation with underscores.
    #[test]
    fn pascal_case_transform_handles_underscores() {
        assert_eq!(expected_module_name("hello"), "Hello");
        assert_eq!(expected_module_name("my_module"), "MyModule");
        assert_eq!(expected_module_name("my_small_module"), "MySmallModule");
    }

    /// Test 2: PascalCase transformation with hyphens.
    #[test]
    fn pascal_case_transform_handles_hyphens() {
        assert_eq!(expected_module_name("kebab-case"), "KebabCase");
        assert_eq!(expected_module_name("hello-world"), "HelloWorld");
    }

    /// Test 3: PascalCase with mixed delimiters.
    #[test]
    fn pascal_case_transform_handles_mixed() {
        assert_eq!(expected_module_name("hello_world-test"), "HelloWorldTest");
    }

    /// Test 4: Single module with matching name validates OK.
    #[test]
    fn single_module_with_matching_name_ok() {
        use paideia_as_diagnostics::FileId;

        let mut arena = AstArena::new();
        let span = paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 0, 5);

        // Create a fake Ident node for the module name.
        let name_id = arena.alloc(NodeKind::Ident, span);

        // Create a fake Structure for the module body.
        let body_id = arena.alloc(NodeKind::Structure, span);

        // Create a Module item.
        let module_id = arena.alloc_item(
            NodeKind::Module,
            span,
            ItemData::Module {
                name: name_id,
                sig: None,
                body: body_id,
                inner_attrs: vec![],
                doc: None,
            },
        );

        // Create the root Structure containing this module.
        let root_id = arena.alloc_item(
            NodeKind::Structure,
            span,
            ItemData::Structure {
                items: vec![module_id],
                inner_attrs: vec![],
                doc: None,
            },
        );

        // Validate with matching content.
        let content = "Hello";
        let file_path = Path::new("/tmp/Hello.pdx");
        let mut diags = Vec::new();

        let result = validate_file_module_mapping(file_path, root_id, &arena, content, &mut diags);

        assert!(result);
        assert!(diags.is_empty());
    }

    /// Test 5: Multiple modules causes validation to fail (M0306 emitted by parser, not elaborator).
    #[test]
    fn multiple_modules_causes_validation_to_fail() {
        use paideia_as_diagnostics::FileId;

        let mut arena = AstArena::new();
        let span = paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 0, 5);

        // Create two Module items with matching names.
        let name_id_1 = arena.alloc(NodeKind::Ident, span);
        let body_id_1 = arena.alloc(NodeKind::Structure, span);
        let module_id_1 = arena.alloc_item(
            NodeKind::Module,
            span,
            ItemData::Module {
                name: name_id_1,
                sig: None,
                body: body_id_1,
                inner_attrs: vec![],
                doc: None,
            },
        );

        let name_id_2 = arena.alloc(NodeKind::Ident, span);
        let body_id_2 = arena.alloc(NodeKind::Structure, span);
        let module_id_2 = arena.alloc_item(
            NodeKind::Module,
            span,
            ItemData::Module {
                name: name_id_2,
                sig: None,
                body: body_id_2,
                inner_attrs: vec![],
                doc: None,
            },
        );

        // Root structure contains both.
        let root_id = arena.alloc_item(
            NodeKind::Structure,
            span,
            ItemData::Structure {
                items: vec![module_id_1, module_id_2],
                inner_attrs: vec![],
                doc: None,
            },
        );

        let content = "Hello";
        let file_path = Path::new("/tmp/Hello.pdx");
        let mut diags = Vec::new();

        let result = validate_file_module_mapping(file_path, root_id, &arena, content, &mut diags);

        // Validation should fail (no diag emitted by elaborator; parser handles M0306).
        assert!(!result);
        assert!(diags.is_empty());
    }

    /// Test 6: Name mismatch emits M0305.
    #[test]
    fn name_mismatch_emits_m0305() {
        use paideia_as_diagnostics::FileId;

        let mut arena = AstArena::new();
        let span = paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 0, 7);

        let name_id = arena.alloc(NodeKind::Ident, span);
        let body_id = arena.alloc(NodeKind::Structure, span);
        let module_id = arena.alloc_item(
            NodeKind::Module,
            span,
            ItemData::Module {
                name: name_id,
                sig: None,
                body: body_id,
                inner_attrs: vec![],
                doc: None,
            },
        );

        let root_id = arena.alloc_item(
            NodeKind::Structure,
            span,
            ItemData::Structure {
                items: vec![module_id],
                inner_attrs: vec![],
                doc: None,
            },
        );

        let content = "WrongName";
        let file_path = Path::new("/tmp/my_thing.pdx");
        let mut diags = Vec::new();

        let result = validate_file_module_mapping(file_path, root_id, &arena, content, &mut diags);

        assert!(!result);
        assert!(
            diags
                .iter()
                .any(|d| d.code().number() == M_FILE_NAME_MISMATCH)
        );
    }

    /// Test 7: Empty file emits M0313.
    #[test]
    fn empty_file_emits_m0313() {
        use paideia_as_diagnostics::FileId;

        let mut arena = AstArena::new();
        let span = paideia_as_diagnostics::Span::new(FileId::new(1).unwrap(), 0, 0);

        // Root structure with no items.
        let root_id = arena.alloc_item(
            NodeKind::Structure,
            span,
            ItemData::Structure {
                items: vec![],
                inner_attrs: vec![],
                doc: None,
            },
        );

        let content = "";
        let file_path = Path::new("/tmp/MyModule.pdx");
        let mut diags = Vec::new();

        let result = validate_file_module_mapping(file_path, root_id, &arena, content, &mut diags);

        assert!(!result);
        assert!(diags.iter().any(|d| d.code().number() == M_NO_TOP_MODULE));
    }

    /// Test 8: Numeric prefixes are skipped in PascalCase transformation.
    #[test]
    fn pascal_case_transform_skips_numeric_prefixes() {
        assert_eq!(expected_module_name("01_hello"), "Hello");
        assert_eq!(expected_module_name("02_functions"), "Functions");
        assert_eq!(expected_module_name("03_records"), "Records");
        assert_eq!(expected_module_name("10_capabilities"), "Capabilities");
        assert_eq!(expected_module_name("01"), ""); // All numeric → empty
        assert_eq!(expected_module_name("123_456_abc"), "Abc"); // Numeric segments skipped
    }
}
