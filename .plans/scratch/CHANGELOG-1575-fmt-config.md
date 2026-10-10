# CHANGELOG — FMT-11 FormatOptions v2 + `.pdxfmt` TOML config (paideia-as#1575)

## Summary

Lifts formatter configuration out of hard-coded defaults into a
`.pdxfmt` TOML file at the workspace root. Introduces discovery
(walk-up from any start dir, nearest-ancestor wins), a strongly-typed
loader with structured `ConfigError`, and an `apply_to_options` overlay
that merges partial overrides onto `FormatOptions::default()`.

Extends `FormatOptions` with three new fields — `indent_width`,
`trailing_comma_policy`, `glyph_policy` — and introduces two
`#[non_exhaustive]` policy enums (`TrailingCommaPolicy`,
`GlyphPolicy`). Existing call sites continue to compile (new fields
default via `Default`).

## TOML schema (settled)

```toml
[fmt]
max_width           = 100       # usize, default 100
indent_width        = 2         # usize, default 2
align_match_arms    = true      # bool,  default true
align_unsafe_fields = true      # bool,  default true
trailing_comma      = "always"  # "always" | "never" | "preserve"
glyph               = "preserve" # "preserve" | "unicode" | "ascii"
```

Every key is optional; omitted keys inherit `FormatOptions::default()`.

## Changed files

- `crates/paideia-fmt/Cargo.toml`
  - Adds `serde = { workspace = true }` + `toml = { workspace = true }`
    (both already workspace deps, no new workspace entries).
  - Adds `[dev-dependencies] tempfile = { workspace = true }` for the
    filesystem-walking tests.
- `crates/paideia-fmt/src/lib.rs`
  - `pub mod config;` added.
  - `FormatOptions` grows three fields: `indent_width: usize` (default
    2), `trailing_comma_policy: TrailingCommaPolicy` (default
    `Always`), `glyph_policy: GlyphPolicy` (default `Preserve`).
  - Two new `#[non_exhaustive]` enums: `TrailingCommaPolicy`
    (`Always`/`Never`/`Preserve`) and `GlyphPolicy`
    (`Preserve`/`Unicode`/`Ascii`), both `#[derive(Deserialize)]` with
    `#[serde(rename_all = "lowercase")]` and `#[derive(Default)]`.
  - Legacy `GlyphStyle` + `glyph_style` field retained verbatim —
    `format_whitespace` continues to use them (FMT-9/FMT-10 will
    migrate across and deprecate `GlyphStyle`).
- `crates/paideia-fmt/src/config.rs` (new, ~370 LOC incl. tests)
  - `pub const CONFIG_FILENAME = ".pdxfmt"`.
  - `pub struct FmtConfig { fmt: FmtSection }` — serde `Deserialize`.
  - `pub struct FmtSection { max_width, indent_width, align_match_arms,
    align_unsafe_fields, trailing_comma, glyph }` — every field
    `Option<T>` so partials merge cleanly.
  - `pub enum ConfigError { Io, Parse, NotFound }` with
    `From<io::Error>` (NotFound-aware) + `From<toml::de::Error>` +
    `Display`/`Error` impls.
  - `pub fn load(path) -> Result<FmtConfig, ConfigError>`.
  - `pub fn discover(start) -> Option<(PathBuf, FmtConfig)>` — walks
    ancestors; parse errors collapse to `None` per documented
    semantics (callers that want errors call `load` directly).
  - `pub fn apply_to_options(cfg, opts)` — `if let Some` overlay.
- `crates/paideia-fmt/src/config.rs` (tests, 10 total):
  - `discover_returns_none_when_no_pdxfmt`
  - `load_parses_max_width_override_and_applies`
  - `load_rejects_invalid_toml`
  - `discover_walks_up_from_nested_dir`
  - `apply_to_options_partial_merges_with_defaults`
  - `load_parses_full_schema`
  - `load_notfound_when_file_missing`
  - `apply_to_options_empty_leaves_defaults`
  - `trailing_comma_policy_lowercase_variants`
  - `glyph_policy_lowercase_variants`
  - `discover_prefers_nearest_ancestor`

## Variants NOT covered

- No CLI wiring yet — `paideia-fmt --stdin` still uses
  `FormatOptions::default()`. A follow-up (likely FMT-11a) will thread
  `discover(cwd())` through `main.rs` + the LSP's
  `format_whitespace` call site in `paideia-lsp::server`.
- No consumer reads `indent_width` / `trailing_comma_policy` /
  `glyph_policy` yet; the FMT-4..8 structural formatters still return
  byte-identical fallbacks per the FMT-3 contract. These fields are
  pre-wired for the FMT-9/FMT-10 polish wave.
- Non-exhaustive policy enums intentionally have NO consumer `match`
  arms today — when FMT-9 wires them in, those `match` sites must end
  with a wildcard arm (crate note on both enums).

## Approx LOC

- `Cargo.toml`: +5
- `lib.rs`: +~75 (3 fields + 2 enums + defaults + docs)
- `config.rs`: +~370 (incl. 10 tests)
- `.plans/scratch/CHANGELOG-1575-fmt-config.md`: this file

## Build

Not run — main should invoke `bash tools/build.sh` and re-invoke me
with the error tail if it fails.
