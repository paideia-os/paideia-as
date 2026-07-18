//! API freeze test for paideia-as-emit.
//!
//! This test captures the public API surface of paideia-as-emit via syn parsing
//! and insta snapshots. Any change to the public surface will cause a snapshot diff,
//! which surfaces in PR review. Breaking changes require an explicit, reviewer-approved
//! snapshot update.

use std::path::Path;
use syn::{Item, ItemEnum, ItemFn, ItemStruct, ItemTrait, ItemType, ItemConst, ItemUse, Visibility};
use quote::ToTokens;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct RenderedItem {
    kind: ItemKind,
    name: String,
    body: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum ItemKind {
    Const,
    Enum,
    Fn,
    Struct,
    Trait,
    Type,
    Use,
}

/// Extract the public API surface of a crate by parsing lib.rs and walking pub mod declarations.
fn extract_surface(crate_src_dir: &Path) -> String {
    let mut items: Vec<RenderedItem> = Vec::new();
    walk_module(crate_src_dir, "lib.rs", &mut items);
    items.sort_by_key(|r| (r.kind.clone(), r.name.clone()));
    items
        .iter()
        .map(|r| r.body.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn walk_module(dir: &Path, file_name: &str, out: &mut Vec<RenderedItem>) {
    let src = std::fs::read_to_string(dir.join(file_name))
        .unwrap_or_else(|e| panic!("failed to read {}/{}: {}", dir.display(), file_name, e));
    let file: syn::File = syn::parse_str(&src)
        .unwrap_or_else(|e| panic!("failed to parse {}/{}: {}", dir.display(), file_name, e));

    for item in &file.items {
        match item {
            // Descend into pub mod declarations (without inline content)
            Item::Mod(m) if is_pub(&m.vis) && m.content.is_none() => {
                walk_module(dir, &format!("{}.rs", m.ident), out);
            }
            Item::Enum(e) if is_pub(&e.vis) => out.push(render_enum(e)),
            Item::Struct(s) if is_pub(&s.vis) => out.push(render_struct(s)),
            Item::Fn(f) if is_pub(&f.vis) => out.push(render_fn(f)),
            Item::Trait(t) if is_pub(&t.vis) => out.push(render_trait(t)),
            Item::Type(t) if is_pub(&t.vis) => out.push(render_type_alias(t)),
            Item::Const(c) if is_pub(&c.vis) => out.push(render_const(c)),
            Item::Use(u) if is_pub(&u.vis) => out.push(render_use(u)),
            _ => {}
        }
    }
}

fn is_pub(v: &Visibility) -> bool {
    matches!(v, Visibility::Public(_))
}

fn collect_stability_attrs(attrs: &[syn::Attribute]) -> String {
    let mut result = Vec::new();
    for attr in attrs {
        let path_str = attr.path().segments.iter()
            .map(|s| s.ident.to_string())
            .collect::<Vec<_>>()
            .join("::");

        if matches!(path_str.as_str(), "non_exhaustive" | "repr" | "must_use" | "derive" | "deprecated") {
            result.push(attr.to_token_stream().to_string());
        }
    }
    if result.is_empty() {
        "(none)".to_string()
    } else {
        result.join("\n")
    }
}

fn render_enum(e: &ItemEnum) -> RenderedItem {
    let attrs = collect_stability_attrs(&e.attrs);
    let mut variants: Vec<String> = e.variants
        .iter()
        .map(|v| {
            let fields_str = render_variant_fields(&v.fields);
            format!("{}{}", v.ident, fields_str)
        })
        .collect();
    variants.sort();

    let body = if attrs == "(none)" {
        format!(
            "enum {}\n  variants:\n{}",
            e.ident,
            indent(&variants.join("\n"), "    ")
        )
    } else {
        format!(
            "enum {}\n  attrs:\n{}\n  variants:\n{}",
            e.ident,
            indent(&attrs, "    "),
            indent(&variants.join("\n"), "    ")
        )
    };

    RenderedItem {
        kind: ItemKind::Enum,
        name: e.ident.to_string(),
        body,
    }
}

fn render_variant_fields(fields: &syn::Fields) -> String {
    match fields {
        syn::Fields::Unit => String::new(),
        syn::Fields::Unnamed(f) => {
            let types: Vec<_> = f.unnamed.iter()
                .map(|f| f.ty.to_token_stream().to_string())
                .collect();
            format!("({})", types.join(", "))
        }
        syn::Fields::Named(f) => {
            let mut fields: Vec<_> = f.named.iter()
                .map(|f| {
                    let name = f.ident.as_ref().unwrap();
                    let ty = f.ty.to_token_stream().to_string();
                    format!("{}: {}", name, ty)
                })
                .collect();
            fields.sort();
            format!(" {{ {} }}", fields.join(", "))
        }
    }
}

fn render_struct(s: &ItemStruct) -> RenderedItem {
    let attrs = collect_stability_attrs(&s.attrs);
    let fields_str = match &s.fields {
        syn::Fields::Unit => "(unit)".to_string(),
        syn::Fields::Unnamed(f) => {
            let types: Vec<_> = f.unnamed.iter()
                .map(|f| f.ty.to_token_stream().to_string())
                .collect();
            format!("({})", types.join(", "))
        }
        syn::Fields::Named(f) => {
            let mut fields: Vec<_> = f.named.iter()
                .map(|f| {
                    let name = f.ident.as_ref().unwrap();
                    let ty = f.ty.to_token_stream().to_string();
                    format!("{}: {}", name, ty)
                })
                .collect();
            fields.sort();
            format!("{{ {} }}", fields.join(", "))
        }
    };

    let body = if attrs == "(none)" {
        format!("struct {}\n  fields: {}", s.ident, fields_str)
    } else {
        format!(
            "struct {}\n  attrs:\n{}\n  fields: {}",
            s.ident,
            indent(&attrs, "    "),
            fields_str
        )
    };

    RenderedItem {
        kind: ItemKind::Struct,
        name: s.ident.to_string(),
        body,
    }
}

fn render_fn(f: &ItemFn) -> RenderedItem {
    let attrs = collect_stability_attrs(&f.attrs);
    let sig = f.sig.to_token_stream().to_string();

    let body = if attrs == "(none)" {
        format!("fn {}\n  sig: {}", f.sig.ident, sig)
    } else {
        format!(
            "fn {}\n  attrs:\n{}\n  sig: {}",
            f.sig.ident,
            indent(&attrs, "    "),
            sig
        )
    };

    RenderedItem {
        kind: ItemKind::Fn,
        name: f.sig.ident.to_string(),
        body,
    }
}

fn render_trait(t: &ItemTrait) -> RenderedItem {
    let attrs = collect_stability_attrs(&t.attrs);
    let bounds = t.supertraits.iter()
        .map(|b| b.to_token_stream().to_string())
        .collect::<Vec<_>>()
        .join(", ");

    let body = if attrs == "(none)" {
        if bounds.is_empty() {
            format!("trait {}", t.ident)
        } else {
            format!("trait {}: {}", t.ident, bounds)
        }
    } else {
        if bounds.is_empty() {
            format!("trait {}\n  attrs:\n{}", t.ident, indent(&attrs, "    "))
        } else {
            format!(
                "trait {}: {}\n  attrs:\n{}",
                t.ident,
                bounds,
                indent(&attrs, "    ")
            )
        }
    };

    RenderedItem {
        kind: ItemKind::Trait,
        name: t.ident.to_string(),
        body,
    }
}

fn render_type_alias(t: &ItemType) -> RenderedItem {
    let attrs = collect_stability_attrs(&t.attrs);
    let ty = t.ty.to_token_stream().to_string();

    let body = if attrs == "(none)" {
        format!("type {} = {}", t.ident, ty)
    } else {
        format!(
            "type {} = {}\n  attrs:\n{}",
            t.ident,
            ty,
            indent(&attrs, "    ")
        )
    };

    RenderedItem {
        kind: ItemKind::Type,
        name: t.ident.to_string(),
        body,
    }
}

fn render_const(c: &ItemConst) -> RenderedItem {
    let attrs = collect_stability_attrs(&c.attrs);
    let ty = c.ty.to_token_stream().to_string();

    let body = if attrs == "(none)" {
        format!("const {}: {}", c.ident, ty)
    } else {
        format!(
            "const {}: {}\n  attrs:\n{}",
            c.ident,
            ty,
            indent(&attrs, "    ")
        )
    };

    RenderedItem {
        kind: ItemKind::Const,
        name: c.ident.to_string(),
        body,
    }
}

fn render_use(u: &ItemUse) -> RenderedItem {
    let use_str = u.to_token_stream().to_string();
    RenderedItem {
        kind: ItemKind::Use,
        name: format!("{:?}", u.tree),  // Use tree debug representation for ordering
        body: use_str,
    }
}

fn indent(s: &str, prefix: &str) -> String {
    s.lines()
        .map(|line| if line.is_empty() { line.to_string() } else { format!("{}{}", prefix, line) })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn emit_public_api_surface_snapshot() {
    let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let surface = extract_surface(&src_dir);
    insta::assert_snapshot!("emit_surface", surface);
}

#[test]
#[deny(unreachable_patterns)]
fn emit_error_is_non_exhaustive_wildcard_arm_reachable() {
    use paideia_as_emit::EmitError;

    let e = EmitError::InvalidOperand;
    let msg = match e {
        EmitError::OperandCount { .. } => "count",
        EmitError::OperandShape { .. } => "shape",
        EmitError::InvalidOperand => "invalid",
        EmitError::Unsupported => "unsupported",
        EmitError::UnresolvedRelocation => "reloc",
        _ => "future",   // ← proves EmitError is #[non_exhaustive]
    };
    assert_eq!(msg, "invalid");
}

#[test]
#[deny(unreachable_patterns)]
fn resolve_error_is_non_exhaustive_wildcard_arm_reachable() {
    use paideia_as_runtime::ResolveError;

    let e = ResolveError::UnknownSymbol {
        instr_index: 0,
        operand_index: 0,
        name: String::from("_"),
    };
    let _ = match e {
        ResolveError::UnknownSymbol { .. } => 0,
        ResolveError::UnknownLabel { .. } => 1,
        ResolveError::OutOfRange { .. } => 2,
        ResolveError::NotEncodableForMnemonic { .. } => 3,
        _ => 4,  // ← proves ResolveError is #[non_exhaustive]
    };
}

#[test]
fn wasm_example_consumer_import_shape_compiles() {
    use paideia_as_emit::{emit_instruction, CodeBuffer, EmitError};
    use paideia_as_runtime::{Instruction, InstrMode, Mnemonic, Operand, RegId, Scale};

    /// Compile-only witness that mirrors the #1023 wasm_add example's import + call surface.
    /// If any of these identifiers moves or changes signature, this test fails to compile.
    fn _witness_emit(buf: &mut CodeBuffer, ins: Instruction) -> Result<(), EmitError> {
        emit_instruction(buf, ins)
    }

    fn _witness_instr_ctor() -> Instruction {
        Instruction {
            mnemonic: Mnemonic::Nop,
            operands: smallvec::SmallVec::new(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::Mode64,
            emission_order: 0,
        }
    }

    // Force MemSib/Scale usage so those types are frozen too.
    let _ = Operand::MemSib {
        base: RegId(4),
        index: None,
        scale: Scale::X1,
        disp: 16,
    };
}
