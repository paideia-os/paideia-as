//! `head` — keeps the first N records of an input stream.
//!
//! Schema-preserving. Argument is a numeric count `n`. R222.M3 wires
//! the shape; the stream slice is R223.M4's own landing.

use crate::schema::{SchemaRef, SchemasSig};
use crate::sig::{
    ArgSpec, CapSpec, CommandSig, EffectRow, ExecuteResult, FlagSpec, InvocationCtx,
};

/// Functor entry point — registered under the shell name `"head"`.
pub fn functor(schemas: &SchemasSig) -> CommandSig {
    let carried = schemas
        .extras
        .first()
        .cloned()
        .unwrap_or_else(|| SchemaRef::of_name("Record@0.1"));

    CommandSig {
        name: "head".to_owned(),
        input_schema: Some(carried.clone()),
        output_schema: Some(carried),
        arguments: vec![ArgSpec {
            name: "n".to_owned(),
            type_name: "U64".to_owned(),
            required: false,
            default: Some("10".to_owned()),
            help: "Number of records to keep (default 10).".to_owned(),
        }],
        flags: vec![],
        effects: EffectRow::pure(),
        required_capabilities: CapSpec::none(),
        execute,
    }
}

fn execute(ctx: &InvocationCtx) -> ExecuteResult {
    // Interpret argv[0] as the count if present; otherwise 10 (the
    // default in the spec).
    let n = ctx
        .argv
        .first()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(10);
    ExecuteResult::new(n, &ctx.fingerprint)
}
