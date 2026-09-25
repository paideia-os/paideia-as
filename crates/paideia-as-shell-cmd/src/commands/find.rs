//! `find` — walks the filesystem, emits `FileSchema@0.1` records.
//!
//! Source command: `input_schema = None`. The R222.M3 canary
//! (`find .`) exercises this functor end-to-end — the shell splits
//! the line, resolves `find`, calls [`functor`] with the session's
//! [`SchemasSig`], gets back a [`CommandSig`], and would invoke
//! `execute` (stubbed in R222.M3; the FS walk itself is R223.M2's
//! landing when the pipeline stage integration lets records flow).

use crate::schema::{SchemaRef, SchemasSig};
use crate::sig::{
    ArgSpec, CapSpec, CommandSig, CommandWeight, EffectRow, ExecuteResult, FlagSpec,
    InvocationCtx,
};

/// Functor entry point — registered in the [`crate::CommandRegistry`]
/// under the name `"find"`.
pub fn functor(schemas: &SchemasSig) -> CommandSig {
    let output = schemas
        .extra_by_name("FileSchema@0.1")
        .cloned()
        .unwrap_or_else(|| SchemaRef::of_name("FileSchema@0.1"));

    CommandSig {
        name: "find".to_owned(),
        input_schema: None,
        output_schema: Some(output),
        arguments: vec![ArgSpec {
            name: "path".to_owned(),
            type_name: "Path".to_owned(),
            required: true,
            default: None,
            help: "Root directory to walk.".to_owned(),
        }],
        flags: vec![
            FlagSpec {
                name: "name".to_owned(),
                short: None,
                type_name: "String".to_owned(),
                default: None,
                help: "Match records whose name equals the given string.".to_owned(),
            },
            FlagSpec {
                name: "type".to_owned(),
                short: Some('t'),
                type_name: "FileType".to_owned(),
                default: None,
                help: "Restrict to files of the given type (file|dir|symlink).".to_owned(),
            },
            FlagSpec {
                name: "size".to_owned(),
                short: None,
                type_name: "ByteSize".to_owned(),
                default: None,
                help: "Restrict by size (e.g. `>1.MB`).".to_owned(),
            },
            FlagSpec {
                name: "recursive".to_owned(),
                short: Some('r'),
                type_name: "Bool".to_owned(),
                default: Some("true".to_owned()),
                help: "Recurse into subdirectories.".to_owned(),
            },
        ],
        effects: EffectRow::of(&["fs_read", "fs_enumerate"]),
        required_capabilities: CapSpec::of(&["fs_read_under_path"]),
        execute,
        // R222.M6: `find` walks the filesystem — the supervisor spawns
        // it as a substrate process rather than running an in-process
        // stub that would need direct fs syscall access on the host.
        weight: CommandWeight::Heavy,
    }
}

/// Stub execute — R222.M3 shape.
///
/// Returns `scalar = 1` to signal "would emit at least one record if
/// the FS were mounted". The real FS walk is R223.M2's landing when
/// the pipeline stage integration exists; changing this stub to a
/// real walk before then would encode assumptions about a stream API
/// that has not been designed yet.
fn execute(ctx: &InvocationCtx) -> ExecuteResult {
    // The stub honours the ctx's argv shape: an empty argv is a
    // caller bug (`find` requires a path argument), signalled by
    // scalar = 0 so the R222.M3 canary can spot the invariant break.
    let scalar = if ctx.argv.is_empty() { 0 } else { 1 };
    ExecuteResult::new(scalar, &ctx.fingerprint)
}
