/// v15_survey_witness.rs: Witness test for 32-bit mode instruction survey.
///
/// This test validates the catalogue in design/encoding/32bit-mode-survey.md:
/// - Parses the markdown file via HTML anchors
/// - Extracts the data table rows
/// - Verifies row structure, byte format, SDM references, status codes, and issue links
/// - Ensures row count >= 34
///
/// The test uses inline markdown parsing (no external dependencies).

#[test]
fn test_survey_catalogue_structure() {
    // Read the survey markdown (relative to workspace root)
    let survey_path = "../../design/encoding/32bit-mode-survey.md";
    let survey_content =
        std::fs::read_to_string(survey_path)
            .expect("Failed to read 32bit-mode-survey.md");

    // Extract catalogue section via HTML anchor markers
    let begin_marker = "<!-- catalogue:begin -->";
    let end_marker = "<!-- catalogue:end -->";

    let begin_idx = survey_content
        .find(begin_marker)
        .expect("Missing catalogue:begin anchor");
    let end_idx = survey_content[begin_idx..]
        .find(end_marker)
        .expect("Missing catalogue:end anchor");

    let catalogue_section = &survey_content[begin_idx + begin_marker.len()
        ..begin_idx + end_idx];

    // Split by lines and collect rows (skip header separators)
    let mut rows = Vec::new();
    for line in catalogue_section.lines() {
        let trimmed = line.trim();
        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }
        // Skip header line (contains "# |" pattern)
        if trimmed.contains("# | gas_line") {
            continue;
        }
        // Skip separator lines (all dashes/pipes)
        if trimmed.starts_with('|') && trimmed.chars().all(|c| c == '|' || c == '-' || c == ' ') {
            continue;
        }
        // Data rows start with |
        if trimmed.starts_with('|') {
            rows.push(trimmed);
        }
    }

    // Verify row count
    assert!(
        rows.len() >= 34,
        "Expected >= 34 catalogue rows, found {}",
        rows.len()
    );

    // Validate each row structure
    for (idx, row) in rows.iter().enumerate() {
        let cells: Vec<&str> = row.split('|').map(|s| s.trim()).collect();
        // Expect 10-11 cells (trailing | may create empty cell): [empty, col1, col2, ..., col9, empty?]
        assert!(
            cells.len() == 10 || cells.len() == 11,
            "Row {} has {} cells, expected 10 or 11: {}",
            idx,
            cells.len(),
            row
        );
        // Ensure we have at least 10 non-empty cells worth of content
        let effective_cells = if cells.len() == 11 && cells[10].is_empty() {
            &cells[0..10]
        } else {
            &cells
        };
        assert!(
            effective_cells.len() >= 10,
            "Row {}: not enough columns",
            idx
        );

        // Handle potential trailing empty cell
        let actual_cells = if cells.len() == 11 && cells[10].is_empty() {
            &cells[0..10]
        } else {
            &cells
        };

        let row_num = actual_cells[1]; // # column
        let gas_line = actual_cells[2];
        let _att_disasm = actual_cells[3];
        let _intel_disasm = actual_cells[4];
        let bytes = actual_cells[5];
        let sdm_ref = actual_cells[6];
        let encoder_fn = actual_cells[7];
        let status = actual_cells[8];
        let issue = actual_cells[9];

        // Verify row number is numeric or placeholder
        if !row_num.is_empty() {
            assert!(
                row_num.chars().all(|c| c.is_numeric()),
                "Row {}: invalid # column: {}",
                idx,
                row_num
            );
        }

        // Verify gas_line is numeric (source line reference)
        if !gas_line.is_empty() && gas_line != "–" {
            assert!(
                gas_line.chars().all(|c| c.is_numeric() || c == '–' || c == '-'),
                "Row {}: invalid gas_line: {}",
                idx,
                gas_line
            );
        }

        // Verify bytes: hex digits only, no angle brackets
        if !bytes.is_empty() && bytes != "–" {
            assert!(
                bytes.chars().all(|c| c.is_ascii_hexdigit() || c == ' ' || c == '-'),
                "Row {}: bytes contain invalid chars: {}",
                idx,
                bytes
            );
            assert!(
                !bytes.contains('<') && !bytes.contains('>'),
                "Row {}: bytes must not contain angle brackets: {}",
                idx,
                bytes
            );
        }

        // Verify SDM reference format
        if !sdm_ref.is_empty() && sdm_ref != "–" {
            assert!(
                sdm_ref.contains("Vol") && sdm_ref.contains('§'),
                "Row {}: SDM ref missing 'Vol' or '§': {}",
                idx,
                sdm_ref
            );
        }

        // Verify encoder_fn is valid identifier
        if !encoder_fn.is_empty() && encoder_fn != "–" {
            assert!(
                encoder_fn.chars().all(|c| c.is_alphanumeric() || c == '_'),
                "Row {}: invalid encoder_fn: {}",
                idx,
                encoder_fn
            );
        }

        // Verify status is one of: ✅, ⚠, ❌
        assert!(
            status == "✅" || status == "⚠" || status == "❌",
            "Row {}: invalid status '{}', expected ✅, ⚠, or ❌",
            idx,
            status
        );

        // Verify issue link format
        if !issue.is_empty() && issue != "–" {
            // Should be a GitHub issue link like #880, #881, etc.
            // or "–" for complete rows
            assert!(
                issue.starts_with('#') && issue[1..].chars().all(|c| c.is_numeric()),
                "Row {}: invalid issue format: {}",
                idx,
                issue
            );
            // Verify issue is in v1.5 range (#879–#892)
            if issue != "–" {
                let issue_num: u32 = issue[1..].parse()
                    .expect("Failed to parse issue number");
                assert!(
                    issue_num >= 879 && issue_num <= 892,
                    "Row {}: issue {} out of v1.5 range [879,892]",
                    idx,
                    issue_num
                );
            }
        }

        // Constraint: if status != ✅, then issue must be non-"–"
        if status != "✅" {
            assert!(
                issue != "–" && !issue.is_empty(),
                "Row {}: status is {} but issue is empty or '–'",
                idx,
                status
            );
        }
    }

    // Final sanity check: at least one ✅ and one ❌ or ⚠
    let completed_count = rows.iter()
        .filter(|r| r.contains(" ✅ "))
        .count();
    let gap_count = rows.iter()
        .filter(|r| r.contains(" ❌ ") || r.contains(" ⚠ "))
        .count();

    assert!(
        completed_count > 0,
        "Expected at least one completed (✅) instruction"
    );
    assert!(
        gap_count > 0,
        "Expected at least one gap (❌) or refinement (⚠) row"
    );
}
