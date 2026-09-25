//! `count` — counts input records. Sink command.
//!
//! `output_schema = None` — a scalar count is not a schemad record
//! stream. Once R223.M4 lands `reduce`, `count` is expressible as
//! `reduce { |acc, _| acc + 1 } 0`; keeping it as its own light
//! command matches the semantic-shell.md §6.2 enumeration and gives
//! the tab-completion (`co<TAB>`) surface an entry to point at.

use crate::schema::{SchemaRef, SchemasSig};
use crate::sig::{
    ArgSpec, CapSpec, CommandSig, EffectRow, ExecuteResult, FlagSpec, InvocationCtx,
};

/// Functor entry point — registered under the shell name `"count"`.
pub fn functor(schemas: &SchemasSig) -> CommandSig {
    let input = schemas
        .extras
        .first()
        .cloned()
        .unwrap_or_else(|| SchemaRef::of_name("Record@0.1"));

    CommandSig {
        name: "count".to_owned(),
        input_schema: Some(input),
        output_schema: None,
        arguments: vec![],
        flags: vec![FlagSpec {
            name: "unique".to_owned(),
            short: Some('u'),
            type_name: "Bool".to_owned(),
            default: Some("false".to_owned()),
            help: "Count only distinct records.".to_owned(),
        }],
        effects: EffectRow::pure(),
        required_capabilities: CapSpec::none(),
        execute,
    }
}

// FIXME(R223.M4): `count` really returns the count of records observed
// on its input stream. The stub returns argv.len() as a placeholder
// scalar so the R222.M3 canary can prove the CommandSig instance
// round-trips through dispatch without inventing a fake stream.
fn execute(ctx: &InvocationCtx) -> ExecuteResult {
    ExecuteResult::new(ctx.argv.len() as u64, &ctx.fingerprint)
}
