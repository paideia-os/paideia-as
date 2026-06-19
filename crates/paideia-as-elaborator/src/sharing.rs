//! Sharing constraint checking — structural type equality for functors.
//!
//! Checks that sharing constraints in functor applications hold by resolving
//! type paths and comparing resolved types. Emits M0303 diagnostics for
//! constraint violations.

use crate::modules::{TypedValue, ValueRef};
use paideia_as_ast::SharingConstraint;
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity};

/// Diagnostic code for "sharing constraint violated".
pub const M_SHARING_VIOLATED: u16 = 303;

/// Check that a value satisfies all sharing constraints.
///
/// Iterates through all constraints without short-circuit, accumulating
/// violations. For each constraint, resolves both paths against the value
/// and compares resolved types. Emits one M0303 diagnostic per violation.
///
/// Returns `true` if all constraints are satisfied, `false` if any violation
/// is found.
pub fn check_sharing_constraints(
    app_value: &TypedValue,
    constraints: &[SharingConstraint],
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let mut ok = true;

    for constraint in constraints {
        if !check_single_constraint(app_value, constraint, diags) {
            ok = false;
        }
    }

    ok
}

/// Check a single sharing constraint.
fn check_single_constraint(
    app_value: &TypedValue,
    constraint: &SharingConstraint,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let left_resolved = resolve_path(app_value, &constraint.left_path);
    let right_resolved = resolve_path(app_value, &constraint.right_path);

    let left_type = left_resolved.map(type_key);
    let right_type = right_resolved.map(type_key);

    // Constraint is violated if either path is unresolved or types differ.
    let is_satisfied = matches!((&left_type, &right_type), (Some(l), Some(r)) if l == r);

    if !is_satisfied {
        let left_str = constraint.left_path.join("::");
        let right_str = constraint.right_path.join("::");

        let left_display = left_type.as_deref().unwrap_or("<unresolved>");
        let right_display = right_type.as_deref().unwrap_or("<unresolved>");

        // Build "did you mean" suggestion if one path is unresolved.
        let suggestion = if left_resolved.is_none() {
            suggest_path(app_value, &left_str)
        } else if right_resolved.is_none() {
            suggest_path(app_value, &right_str)
        } else {
            None
        };

        let message = if let Some(suggestion) = suggestion {
            format!(
                "sharing constraint violated: {} = {}\n  left  resolves to {}\n  right resolves to {}\n  did you mean '{}'?",
                left_str, right_str, left_display, right_display, suggestion
            )
        } else {
            format!(
                "sharing constraint violated: {} = {}\n  left  resolves to {}\n  right resolves to {}",
                left_str, right_str, left_display, right_display
            )
        };

        diags.push(
            Diagnostic::error(m_code(M_SHARING_VIOLATED))
                .message(message)
                .with_span(constraint.span)
                .finish(),
        );

        return false;
    }

    true
}

/// Resolve a dotted path into the value tree.
///
/// An empty path resolves to None. A non-empty path walks through module
/// bindings by name, descending recursively until the path is exhausted.
///
/// Returns None if any segment cannot be found or if a non-module is
/// encountered before the path ends.
fn resolve_path<'a>(root: &'a TypedValue, path: &[String]) -> Option<&'a ValueRef> {
    if path.is_empty() {
        return None;
    }

    let mut current = root;
    for (i, segment) in path.iter().enumerate() {
        let is_last = i == path.len() - 1;

        // Find binding with matching name
        let binding = current.bindings.iter().find(|b| b.name == *segment)?;

        if is_last {
            // Last segment: return the value
            return Some(&binding.value);
        }

        // Not last: must be a module to continue
        match &binding.value {
            ValueRef::Module(inner) => {
                current = inner;
            }
            _ => return None,
        }
    }

    None
}

/// Extract the type key from a resolved value.
///
/// For Type variants, returns the type string. For Val or Module, returns
/// a generic placeholder.
fn type_key(v: &ValueRef) -> String {
    match v {
        ValueRef::Type(s) => s.clone(),
        ValueRef::Val(_) => "<val>".to_string(),
        ValueRef::Module(_) => "<module>".to_string(),
    }
}

/// Compute Levenshtein distance between two strings.
///
/// Uses classic 2-row dynamic programming.
fn levenshtein(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut prev = (0..=b_len).collect::<Vec<_>>();
    let mut curr = vec![0; b_len + 1];

    for i in 1..=a_len {
        curr[0] = i;
        for j in 1..=b_len {
            let cost = if a.chars().nth(i - 1) == b.chars().nth(j - 1) {
                0
            } else {
                1
            };
            curr[j] = std::cmp::min(
                std::cmp::min(prev[j] + 1, curr[j - 1] + 1),
                prev[j - 1] + cost,
            );
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b_len]
}

/// Suggest a close path from the value tree.
///
/// Performs DFS through all reachable paths, returning the closest one with
/// Levenshtein distance ≤ 2.
fn suggest_path(root: &TypedValue, missing: &str) -> Option<String> {
    let mut best_match: Option<(usize, String)> = None;

    fn dfs(
        current: &TypedValue,
        prefix: Vec<String>,
        target: &str,
        best: &mut Option<(usize, String)>,
    ) {
        for binding in &current.bindings {
            let mut path = prefix.clone();
            path.push(binding.name.clone());
            let path_str = path.join("::");

            let distance = levenshtein(&path_str, target);
            if distance <= 2 && (best.is_none() || distance < best.as_ref().unwrap().0) {
                *best = Some((distance, path_str));
            }

            // Recursively descend through modules
            if let ValueRef::Module(inner) = &binding.value {
                dfs(inner, path, target, best);
            }
        }
    }

    dfs(root, Vec::new(), missing, &mut best_match);
    best_match.map(|(_, path)| path)
}

/// Helper to construct a diagnostic code for category M.
fn m_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::M, Severity::Error, n).expect("valid M code")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::FieldBinding;
    use paideia_as_diagnostics::{FileId, Span};
    use paideia_as_ir::LinClass;

    fn span(start: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), start, 1)
    }

    /// Test 1: empty constraint list passes
    #[test]
    fn empty_constraint_list_passes() {
        let app_value = TypedValue {
            bindings: vec![],
            signature: Default::default(),
            span: span(0),
        };

        let constraints = vec![];
        let mut diags = Vec::new();

        let result = check_sharing_constraints(&app_value, &constraints, &mut diags);

        assert!(result);
        assert!(diags.is_empty());
    }

    /// Test 2: single satisfied constraint no diag (AC 1)
    #[test]
    fn single_satisfied_constraint_no_diag() {
        let app_value = TypedValue {
            bindings: vec![
                FieldBinding {
                    name: "M".to_string(),
                    ty_id: 0,
                    value: ValueRef::Module(Box::new(TypedValue {
                        bindings: vec![FieldBinding {
                            name: "t".to_string(),
                            ty_id: 0,
                            value: ValueRef::Type("int".to_string()),
                            class: LinClass::Unrestricted,
                            span: span(1),
                        }],
                        signature: Default::default(),
                        span: span(1),
                    })),
                    class: LinClass::Unrestricted,
                    span: span(1),
                },
                FieldBinding {
                    name: "N".to_string(),
                    ty_id: 0,
                    value: ValueRef::Module(Box::new(TypedValue {
                        bindings: vec![FieldBinding {
                            name: "t".to_string(),
                            ty_id: 0,
                            value: ValueRef::Type("int".to_string()),
                            class: LinClass::Unrestricted,
                            span: span(2),
                        }],
                        signature: Default::default(),
                        span: span(2),
                    })),
                    class: LinClass::Unrestricted,
                    span: span(2),
                },
            ],
            signature: Default::default(),
            span: span(0),
        };

        let constraints = vec![SharingConstraint {
            left_path: vec!["M".to_string(), "t".to_string()],
            right_path: vec!["N".to_string(), "t".to_string()],
            span: span(10),
        }];

        let mut diags = Vec::new();
        let result = check_sharing_constraints(&app_value, &constraints, &mut diags);

        assert!(result);
        assert!(diags.is_empty());
    }

    /// Test 3: single violated constraint emits M0303 (AC 2)
    #[test]
    fn single_violated_constraint_emits_m0303() {
        let app_value = TypedValue {
            bindings: vec![
                FieldBinding {
                    name: "M".to_string(),
                    ty_id: 0,
                    value: ValueRef::Module(Box::new(TypedValue {
                        bindings: vec![FieldBinding {
                            name: "t".to_string(),
                            ty_id: 0,
                            value: ValueRef::Type("int".to_string()),
                            class: LinClass::Unrestricted,
                            span: span(1),
                        }],
                        signature: Default::default(),
                        span: span(1),
                    })),
                    class: LinClass::Unrestricted,
                    span: span(1),
                },
                FieldBinding {
                    name: "N".to_string(),
                    ty_id: 0,
                    value: ValueRef::Module(Box::new(TypedValue {
                        bindings: vec![FieldBinding {
                            name: "t".to_string(),
                            ty_id: 0,
                            value: ValueRef::Type("string".to_string()),
                            class: LinClass::Unrestricted,
                            span: span(2),
                        }],
                        signature: Default::default(),
                        span: span(2),
                    })),
                    class: LinClass::Unrestricted,
                    span: span(2),
                },
            ],
            signature: Default::default(),
            span: span(0),
        };

        let constraints = vec![SharingConstraint {
            left_path: vec!["M".to_string(), "t".to_string()],
            right_path: vec!["N".to_string(), "t".to_string()],
            span: span(10),
        }];

        let mut diags = Vec::new();
        let result = check_sharing_constraints(&app_value, &constraints, &mut diags);

        assert!(!result);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), 303);
        assert!(diags[0].message().contains("M::t"));
        assert!(diags[0].message().contains("N::t"));
    }

    /// Test 4: mixed satisfied and violated emits one diag
    #[test]
    fn mixed_satisfied_and_violated_emits_one_diag() {
        let app_value = TypedValue {
            bindings: vec![
                FieldBinding {
                    name: "M".to_string(),
                    ty_id: 0,
                    value: ValueRef::Module(Box::new(TypedValue {
                        bindings: vec![
                            FieldBinding {
                                name: "t".to_string(),
                                ty_id: 0,
                                value: ValueRef::Type("int".to_string()),
                                class: LinClass::Unrestricted,
                                span: span(1),
                            },
                            FieldBinding {
                                name: "u".to_string(),
                                ty_id: 0,
                                value: ValueRef::Type("int".to_string()),
                                class: LinClass::Unrestricted,
                                span: span(1),
                            },
                        ],
                        signature: Default::default(),
                        span: span(1),
                    })),
                    class: LinClass::Unrestricted,
                    span: span(1),
                },
                FieldBinding {
                    name: "N".to_string(),
                    ty_id: 0,
                    value: ValueRef::Module(Box::new(TypedValue {
                        bindings: vec![
                            FieldBinding {
                                name: "t".to_string(),
                                ty_id: 0,
                                value: ValueRef::Type("int".to_string()),
                                class: LinClass::Unrestricted,
                                span: span(2),
                            },
                            FieldBinding {
                                name: "u".to_string(),
                                ty_id: 0,
                                value: ValueRef::Type("string".to_string()),
                                class: LinClass::Unrestricted,
                                span: span(2),
                            },
                        ],
                        signature: Default::default(),
                        span: span(2),
                    })),
                    class: LinClass::Unrestricted,
                    span: span(2),
                },
            ],
            signature: Default::default(),
            span: span(0),
        };

        let constraints = vec![
            SharingConstraint {
                left_path: vec!["M".to_string(), "t".to_string()],
                right_path: vec!["N".to_string(), "t".to_string()],
                span: span(10),
            },
            SharingConstraint {
                left_path: vec!["M".to_string(), "u".to_string()],
                right_path: vec!["N".to_string(), "u".to_string()],
                span: span(11),
            },
        ];

        let mut diags = Vec::new();
        let result = check_sharing_constraints(&app_value, &constraints, &mut diags);

        assert!(!result);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), 303);
    }

    /// Test 5: path miss treated as violation M0303 (AC 5)
    #[test]
    fn path_miss_treated_as_violation_m0303() {
        let app_value = TypedValue {
            bindings: vec![
                FieldBinding {
                    name: "M".to_string(),
                    ty_id: 0,
                    value: ValueRef::Module(Box::new(TypedValue {
                        bindings: vec![FieldBinding {
                            name: "t".to_string(),
                            ty_id: 0,
                            value: ValueRef::Type("int".to_string()),
                            class: LinClass::Unrestricted,
                            span: span(1),
                        }],
                        signature: Default::default(),
                        span: span(1),
                    })),
                    class: LinClass::Unrestricted,
                    span: span(1),
                },
                FieldBinding {
                    name: "N".to_string(),
                    ty_id: 0,
                    value: ValueRef::Module(Box::new(TypedValue {
                        bindings: vec![],
                        signature: Default::default(),
                        span: span(2),
                    })),
                    class: LinClass::Unrestricted,
                    span: span(2),
                },
            ],
            signature: Default::default(),
            span: span(0),
        };

        let constraints = vec![SharingConstraint {
            left_path: vec!["M".to_string(), "t".to_string()],
            right_path: vec!["N".to_string(), "t".to_string()],
            span: span(10),
        }];

        let mut diags = Vec::new();
        let result = check_sharing_constraints(&app_value, &constraints, &mut diags);

        assert!(!result);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), 303);
        assert!(diags[0].message().contains("unresolved"));
    }

    /// Test 6: did_you_mean suggests close path (AC 4)
    #[test]
    fn did_you_mean_suggests_close_path() {
        let app_value = TypedValue {
            bindings: vec![FieldBinding {
                name: "M".to_string(),
                ty_id: 0,
                value: ValueRef::Module(Box::new(TypedValue {
                    bindings: vec![FieldBinding {
                        name: "t".to_string(),
                        ty_id: 0,
                        value: ValueRef::Type("int".to_string()),
                        class: LinClass::Unrestricted,
                        span: span(1),
                    }],
                    signature: Default::default(),
                    span: span(1),
                })),
                class: LinClass::Unrestricted,
                span: span(1),
            }],
            signature: Default::default(),
            span: span(0),
        };

        let constraints = vec![SharingConstraint {
            left_path: vec!["M".to_string(), "tee".to_string()],
            right_path: vec!["M".to_string(), "t".to_string()],
            span: span(10),
        }];

        let mut diags = Vec::new();
        let result = check_sharing_constraints(&app_value, &constraints, &mut diags);

        assert!(!result);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message().contains("did you mean"));
        assert!(diags[0].message().contains("M::t"));
    }
}
