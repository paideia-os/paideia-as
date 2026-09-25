//! paideia-as-shell-pds — R227.M1 (`.pds` script header pragma parser).
//!
//! Consumes a `.pds` source string, walks the leading `#…` pragma
//! lines (after an optional `#!` shebang), and returns a typed
//! [`PdsHeader`] plus the byte offset of the body — the point the
//! shell-lex / shell-ast layer takes over.
//!
//! # Position in the pipeline
//!
//! ```text
//!     .pds source
//!         │
//!         ▼
//!   paideia-as-shell-pds::parse_header
//!         │  ── PdsHeader { capabilities, requires_paideia,
//!         │                 imports, schemas, ascii,
//!         │                 body_offset }
//!         ▼
//!   src[header.body_offset ..]   → paideia-as-shell-lex
//!         │
//!         ▼
//!   Vec<Token> → paideia-as-shell-ast → paideia-as-shell-eval
//! ```
//!
//! The header parser is deliberately self-contained: it does not
//! import the shell tokenizer. Header pragmas do not participate in
//! Unicode-general shell surface syntax (no NFC normalisation, no
//! interpolation, no context switching) — they are ASCII pragma
//! lines, and pulling in the shell lexer would put an NFC pass on
//! the hot path of every `.pds` load.
//!
//! # Grammar (M1)
//!
//! ```text
//!   file       := shebang? header body?
//!   shebang    := '#!' <any-except-newline> '\n'
//!   header     := ( pragma_line )* blank_or_eof
//!   pragma_line := '#' pragma_name pragma_args '\n'
//!   pragma_name := 'capability' | 'requires-paideia' | 'import'
//!               |  'schema'     | 'ascii'
//! ```
//!
//! A `pragma_line` that fails to match any recognised pragma name
//! yields [`PdsHeaderError::UnknownPragma`]. The header terminates on
//! the first blank line (a line whose trimmed content is empty) or
//! the first line whose first non-whitespace byte is not `#` — that
//! line is *not* consumed and becomes part of the body.
//!
//! # Fingerprints
//!
//! The R227.M1 test corpus tags each fixture with `r227m1-header-NN`
//! so the R220.M10 `@fingerprint` correlator can attribute pass/fail
//! to a specific fixture without re-parsing its name.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod cap_check;
pub mod header;
pub mod load_fingerprint;
pub mod version_check;

pub use cap_check::{check_subset, CapCheckError, CapabilitySet};
pub use header::{parse_header, Import, PdsHeader, PdsHeaderError, SchemaRef, Version};
pub use load_fingerprint::{emit_load, CollectingLoadSink, LoadSink, NullLoadSink};
pub use version_check::{check_requires_paideia, VersionCheckError, SYSTEM_VERSION};
