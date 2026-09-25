//! `sort` — reorders an input stream by a key.
//!
//! Schema-preserving: input and output share the same schema. R222.M3
//! wires the shape; the actual sort implementation is R223.M4's
//! landing (`sort by size desc`).

use crate::schema::{SchemaRef, SchemasSig};
use crate::sig::{
    ArgSpec, CapSpec, CommandSig, EffectRow, ExecuteResult, FlagSpec, InvocationCtx,
};

/// Functor entry point — registered under the shell name `"sort"`.
pub fn functor(schemas: &SchemasSig) -> CommandSig {
    let carried = schemas
        .extras
        .first()
        .cloned()
        .unwrap_or_else(|| SchemaRef::of_name("Record@0.1"));

    CommandSig {
        name: "sort".to_owned(),
        input_schema: Some(carried.clone()),
        output_schema: Some(carried),
        arguments: vec![ArgSpec {
            name: "key".to_owned(),
            type_name: "FieldRef|Lambda".to_owned(),
            required: true,
            default: None,
            help: "Field name or lambda producing the sort key.".to_owned(),
        }],
        flags: vec![
            FlagSpec {
                name: "desc".to_owned(),
                short: Some('d'),
                type_name: "Bool".to_owned(),
                default: Some("false".to_owned()),
                help: "Sort in descending order.".to_owned(),
            },
            FlagSpec {
                name: "stable".to_owned(),
                short: None,
                type_name: "Bool".to_owned(),
                default: Some("true".to_owned()),
                help: "Preserve the relative order of equal keys.".to_owned(),
            },
        ],
        effects: EffectRow::pure(),
        required_capabilities: CapSpec::none(),
        execute,
    }
}

fn execute(ctx: &InvocationCtx) -> ExecuteResult {
    let scalar = if ctx.argv.is_empty() { 0 } else { 1 };
    ExecuteResult::new(scalar, &ctx.fingerprint)
}
