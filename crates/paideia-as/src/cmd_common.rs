//! Shared CLI plumbing across `paideia-as` subcommands.

use std::io::Write as _;
use std::path::Path;

use paideia_as_diagnostics::{Catalog, Diagnostic, SarifEmitter, SourceMap};

/// Write SARIF 2.1.0 output for `diagnostics` to `path`.
pub(crate) fn write_sarif(
    source_map: &SourceMap,
    catalog: &Catalog,
    diagnostics: &[Diagnostic],
    path: &Path,
) -> std::io::Result<()> {
    let emitter = SarifEmitter::new(source_map, catalog);
    let json = emitter.emit_string(diagnostics);
    let file = std::fs::File::create(path)?;
    let mut writer = std::io::BufWriter::new(file);
    writer.write_all(json.as_bytes())?;
    writer.flush()
}
