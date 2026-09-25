//! `where` — stream filter. Named `where_` in Rust because `where` is
//! a Rust keyword; the shell name (and the `CommandSig::name` field
//! value) is still `"where"`.
//!
//! Input and output share the same schema — `where` is a schema-
//! preserving filter. R222.M3 wires the identity relationship by
//! reading both sides from the same session slot (`extras[0]` if
//! present, else a fabricated `Record@0.1` sentinel).

use crate::schema::{SchemaRef, SchemasSig};
use crate::sig::{
    ArgSpec, CapSpec, CommandSig, CommandWeight, EffectRow, ExecuteResult, FlagSpec,
    InvocationCtx,
};

/// Functor entry point — registered under the shell name `"where"`.
pub fn functor(schemas: &SchemasSig) -> CommandSig {
    // Schema-preserving: pick the session's first extra schema, else
    // fabricate the generic `Record@0.1` sentinel. The eventual .pdx
    // port makes this an explicit `?a` type variable on `SchemasSig`.
    let carried = schemas
        .extras
        .first()
        .cloned()
        .unwrap_or_else(|| SchemaRef::of_name("Record@0.1"));

    CommandSig {
        name: "where".to_owned(),
        input_schema: Some(carried.clone()),
        output_schema: Some(carried),
        arguments: vec![ArgSpec {
            name: "predicate".to_owned(),
            type_name: "Lambda".to_owned(),
            required: true,
            default: None,
            help: "Predicate lambda applied to each input record.".to_owned(),
        }],
        flags: vec![FlagSpec {
            name: "invert".to_owned(),
            short: Some('v'),
            type_name: "Bool".to_owned(),
            default: Some("false".to_owned()),
            help: "Keep records where the predicate is false.".to_owned(),
        }],
        effects: EffectRow::pure(),
        required_capabilities: CapSpec::none(),
        execute,
        // R222.M6: pure schema-preserving filter over already-streamed
        // records — in-process functor call.
        weight: CommandWeight::Light,
    }
}

fn execute(ctx: &InvocationCtx) -> ExecuteResult {
    // Stub — a real predicate application arrives with R223.M3
    // (`where` / `filter` with lambda predicate). Report scalar = 1
    // when a predicate arg was passed, 0 otherwise.
    let scalar = if ctx.argv.is_empty() { 0 } else { 1 };
    ExecuteResult::new(scalar, &ctx.fingerprint)
}
