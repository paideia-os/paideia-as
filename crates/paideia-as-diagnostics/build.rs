//! Build-time validator for `catalog.toml`.

use serde::Deserialize;
use std::collections::BTreeMap;

/// Maps category names to their letter codes.
fn category_letter(name: &str) -> Option<char> {
    match name {
        "lexer" => Some('E'),
        "elaborator" => Some('E'),
        "parser" => Some('P'),
        "module" => Some('M'),
        "type" => Some('T'),
        "substructural" => Some('S'),
        "effect" => Some('F'),
        "capability" => Some('C'),
        "optimization" => Some('O'),
        "unsafe" => Some('U'),
        "binary" => Some('B'),
        "dwarf" => Some('D'),
        "lint" => Some('L'),
        "workspace" => Some('W'),
        "runtime" => Some('R'),
        "post-quantum" => Some('Q'),
        "experimental" => Some('Z'),
        _ => None,
    }
}

#[derive(Deserialize)]
struct CatalogHeader {
    version: String,
    #[serde(default)]
    #[allow(dead_code)]
    last_updated: String,
}

#[derive(Deserialize)]
struct CatalogEntry {
    #[allow(dead_code)]
    severity: String,
    category: String,
}

#[derive(Deserialize)]
struct RawCatalog {
    catalog: CatalogHeader,
    diagnostic: BTreeMap<String, CatalogEntry>,
}

fn main() {
    println!("cargo:rerun-if-changed=catalog.toml");

    let text = std::fs::read_to_string("catalog.toml").expect("read catalog.toml");
    let raw: RawCatalog = toml::from_str(&text).expect("parse catalog.toml");

    for (key, entry) in &raw.diagnostic {
        // Verify key starts with an uppercase letter.
        let first = key.chars().next().expect("non-empty key");
        if !first.is_ascii_uppercase() {
            panic!(
                "catalog key {} does not start with an uppercase letter",
                key
            );
        }

        // Verify category name maps to a known letter.
        let expected = category_letter(&entry.category).unwrap_or_else(|| {
            panic!(
                "catalog key {} declares unknown category '{}'",
                key, entry.category
            )
        });

        // Verify the declared letter matches the category letter.
        if expected != first {
            panic!(
                "catalog key {} has letter '{}' but category '{}' maps to '{}'",
                key, first, entry.category, expected
            );
        }
    }

    let _ = raw.catalog.version; // touched, currently unused
}
