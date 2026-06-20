//! Derive-macro synthesis for Eq / Hash / Debug.
//!
//! Phase 4 m9-008 minimum: each derive emits a synthetic impl block.
//! The body is "pseudo-IR" — the elaborator pattern-walks the type
//! and emits the canonical implementation:
//! - Eq: per-field equality + AND.
//! - Hash: per-field hash + combine.
//! - Debug: per-field field-name = field-value rendering.
//!
//! ## Phase 4 Scope
//!
//! Parse-only phase: elaborator wiring (m9 walker integration) documented as TODO.
//! - Attribute parsing stores trait names as NodeIds for resolver validation
//! - Synthesis functions generate trait implementations from type metadata
//! - Awaits m9 walker completion for downstream integration

/// Enumeration of supported derive macro kinds (phase-1 support).
///
/// Matches the derive traits that can be synthesized automatically:
/// - `Eq`: Equality comparison (per-field AND of field comparisons)
/// - `Hash`: Hashing (per-field hash combination)
/// - `Debug`: Pretty-printing (per-field formatted output)
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum DeriveKind {
    /// Derive `Eq` trait: generates equality comparison method
    Eq,
    /// Derive `Hash` trait: generates hashing method
    Hash,
    /// Derive `Debug` trait: generates formatting method
    Debug,
}

impl DeriveKind {
    /// Get the trait name as a string
    #[must_use]
    pub fn trait_name(&self) -> &'static str {
        match self {
            DeriveKind::Eq => "Eq",
            DeriveKind::Hash => "Hash",
            DeriveKind::Debug => "Debug",
        }
    }
}

/// Synthetic implementation of a derived trait.
///
/// Stores the trait being implemented, the type it applies to,
/// and a collection of method implementations as pseudo-IR text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntheticImpl {
    /// Name of the trait being derived (e.g., "Eq", "Hash", "Debug").
    pub trait_name: String,
    /// Name of the type being derived for.
    pub type_name: String,
    /// Method implementations as (method_name, body_text) pairs.
    /// For Eq: vec![("eq", "per-field comparisons")]
    /// For Hash: vec![("hash", "per-field hash combination")]
    /// For Debug: vec![("fmt", "per-field formatting")]
    pub method_bodies: Vec<(String, String)>,
}

/// Result type for derive synthesis operations.
pub type SynthesisResult = Result<SyntheticImpl, String>;

/// Synthesize a trait implementation for a derived trait.
///
/// Takes a `DeriveKind`, type name, and optional field metadata, and generates
/// the corresponding trait implementation with method bodies.
///
/// # Arguments
/// - `derive`: The trait to be derived
/// - `type_name`: The name of the type (for code generation)
/// - `fields`: Optional field names (for Eq, Hash, Debug synthesis)
///
/// # Returns
/// A `SyntheticImpl` containing the trait name and method implementations,
/// or an error string if synthesis fails.
///
/// # Examples
///
/// ```ignore
/// let eq_impl = synthesise_derive(DeriveKind::Eq, "Point", Some(vec!["x", "y"]))?;
/// assert_eq!(eq_impl.trait_name, "Eq");
/// ```
pub fn synthesise_derive(
    derive: DeriveKind,
    type_name: &str,
    fields: Option<&[&str]>,
) -> SynthesisResult {
    match derive {
        DeriveKind::Eq => synthesise_eq(type_name, fields),
        DeriveKind::Hash => synthesise_hash(type_name, fields),
        DeriveKind::Debug => synthesise_debug(type_name, fields),
    }
}

/// Synthesize an `Eq` trait implementation.
///
/// For records with fields, generates a method that compares each field
/// and combines the results with AND logic.
///
/// # Arguments
/// - `type_name`: Name of the type
/// - `fields`: Optional field names
///
/// # Returns
/// A `SyntheticImpl` with trait_name="Eq" and method_bodies containing the "eq" method
fn synthesise_eq(type_name: &str, fields: Option<&[&str]>) -> SynthesisResult {
    let method_body = if let Some(field_list) = fields {
        if field_list.is_empty() {
            "self == other".to_string()
        } else {
            let field_comparisons = field_list
                .iter()
                .map(|f| format!("self.{} == other.{}", f, f))
                .collect::<Vec<_>>()
                .join(" && ");
            format!("({})", field_comparisons)
        }
    } else {
        "self == other".to_string()
    };

    Ok(SyntheticImpl {
        trait_name: "Eq".to_string(),
        type_name: type_name.to_string(),
        method_bodies: vec![("eq".to_string(), method_body)],
    })
}

/// Synthesize a `Hash` trait implementation.
///
/// For records with fields, generates a method that hashes each field
/// and combines the results (via XOR or tuple hashing pattern).
///
/// # Arguments
/// - `type_name`: Name of the type
/// - `fields`: Optional field names
///
/// # Returns
/// A `SyntheticImpl` with trait_name="Hash" and method_bodies containing the "hash" method
fn synthesise_hash(type_name: &str, fields: Option<&[&str]>) -> SynthesisResult {
    let method_body = if let Some(field_list) = fields {
        if field_list.is_empty() {
            "// empty tuple pattern".to_string()
        } else {
            let field_hashes = field_list
                .iter()
                .map(|f| format!("hash(&self.{}, state)", f))
                .collect::<Vec<_>>()
                .join("; ");
            format!("{{ {} }}", field_hashes)
        }
    } else {
        "// type hash".to_string()
    };

    Ok(SyntheticImpl {
        trait_name: "Hash".to_string(),
        type_name: type_name.to_string(),
        method_bodies: vec![("hash".to_string(), method_body)],
    })
}

/// Synthesize a `Debug` trait implementation.
///
/// For records with fields, generates a method that formats each field
/// with its name and value in struct-literal format.
/// For enums, generates match arms for each variant.
///
/// # Arguments
/// - `type_name`: Name of the type
/// - `fields`: Optional field names
///
/// # Returns
/// A `SyntheticImpl` with trait_name="Debug" and method_bodies containing the "fmt" method
fn synthesise_debug(type_name: &str, fields: Option<&[&str]>) -> SynthesisResult {
    let method_body = if let Some(field_list) = fields {
        if field_list.is_empty() {
            format!("f.debug_struct(\"{}\").finish()", type_name)
        } else {
            let field_formats = field_list
                .iter()
                .map(|f| format!("    .field(\"{}\", &self.{})", f, f))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "f.debug_struct(\"{}\")\n{}\n    .finish()",
                type_name, field_formats
            )
        }
    } else {
        format!("f.debug_struct(\"{}\").finish()", type_name)
    };

    Ok(SyntheticImpl {
        trait_name: "Debug".to_string(),
        type_name: type_name.to_string(),
        method_bodies: vec![("fmt".to_string(), method_body)],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesise_eq_for_record_emits_per_field_compare() {
        let result = synthesise_eq("Point", Some(&["x", "y"]));
        assert!(result.is_ok());

        let impl_block = result.unwrap();
        assert_eq!(impl_block.trait_name, "Eq");
        assert_eq!(impl_block.type_name, "Point");
        assert_eq!(impl_block.method_bodies.len(), 1);

        let (method_name, body) = &impl_block.method_bodies[0];
        assert_eq!(method_name, "eq");
        assert!(body.contains("self.x") && body.contains("self.y"));
        assert!(body.contains("=="));
        assert!(body.contains("&&"));
    }

    #[test]
    fn synthesise_eq_for_empty_record_succeeds() {
        let result = synthesise_eq("Empty", Some(&[]));
        assert!(result.is_ok());

        let impl_block = result.unwrap();
        assert_eq!(impl_block.trait_name, "Eq");
        assert_eq!(impl_block.type_name, "Empty");
        assert_eq!(impl_block.method_bodies.len(), 1);

        let (method_name, _body) = &impl_block.method_bodies[0];
        assert_eq!(method_name, "eq");
    }

    #[test]
    fn synthesise_hash_for_record_combines_field_hashes() {
        let result = synthesise_hash("Color", Some(&["r", "g", "b"]));
        assert!(result.is_ok());

        let impl_block = result.unwrap();
        assert_eq!(impl_block.trait_name, "Hash");
        assert_eq!(impl_block.type_name, "Color");
        assert_eq!(impl_block.method_bodies.len(), 1);

        let (method_name, body) = &impl_block.method_bodies[0];
        assert_eq!(method_name, "hash");
        assert!(body.contains("self.r") && body.contains("self.g") && body.contains("self.b"));
        assert!(body.contains("hash"));
    }

    #[test]
    fn synthesise_debug_for_record_emits_formatted_output() {
        let result = synthesise_debug("Rect", Some(&["width", "height"]));
        assert!(result.is_ok());

        let impl_block = result.unwrap();
        assert_eq!(impl_block.trait_name, "Debug");
        assert_eq!(impl_block.type_name, "Rect");
        assert_eq!(impl_block.method_bodies.len(), 1);

        let (method_name, body) = &impl_block.method_bodies[0];
        assert_eq!(method_name, "fmt");
        assert!(body.contains("debug_struct"));
        assert!(body.contains("field"));
        assert!(body.contains("width") && body.contains("height"));
    }

    #[test]
    fn synthesise_derive_for_all_kinds_succeeds() {
        let type_name = "Test";
        let fields = Some(&["a", "b"][..]);

        let eq_result = synthesise_derive(DeriveKind::Eq, type_name, fields);
        assert!(eq_result.is_ok());
        assert_eq!(eq_result.unwrap().trait_name, "Eq");

        let hash_result = synthesise_derive(DeriveKind::Hash, type_name, fields);
        assert!(hash_result.is_ok());
        assert_eq!(hash_result.unwrap().trait_name, "Hash");

        let debug_result = synthesise_derive(DeriveKind::Debug, type_name, fields);
        assert!(debug_result.is_ok());
        assert_eq!(debug_result.unwrap().trait_name, "Debug");
    }

    #[test]
    fn synthetic_impl_stores_multiple_methods() {
        let impl_block = SyntheticImpl {
            trait_name: "Clone".to_string(),
            type_name: "Data".to_string(),
            method_bodies: vec![
                ("clone".to_string(), "self.deep_copy()".to_string()),
                (
                    "clone_from".to_string(),
                    "*self = other.clone()".to_string(),
                ),
            ],
        };

        assert_eq!(impl_block.method_bodies.len(), 2);
        assert_eq!(impl_block.method_bodies[0].0, "clone");
        assert_eq!(impl_block.method_bodies[1].0, "clone_from");
    }

    #[test]
    fn derive_kind_trait_name_returns_correct_string() {
        assert_eq!(DeriveKind::Eq.trait_name(), "Eq");
        assert_eq!(DeriveKind::Hash.trait_name(), "Hash");
        assert_eq!(DeriveKind::Debug.trait_name(), "Debug");
    }
}
