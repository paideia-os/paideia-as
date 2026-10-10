# FMT-12 (paideia-as#1576): `fmt` CLI — glob expansion, `--list-different`, workspace default

## Summary

Three ergonomic upgrades to `paideia-as fmt`, all additive (no existing
flag or exit-code semantics change):

1. **Glob expansion** on positional args. `paideia-as fmt 'src/**/*.pdx'`
   now expands via the `glob` crate (0.3) and formats every match.
   Args containing `*`, `?`, or `[` are treated as globs; literal paths
   pass through unchanged, so the pre-FMT-12 `fmt a.pdx b.pdx` call
   shape still works bit-for-bit.
2. **`--list-different` flag**. Prints one path per line to stdout for
   each file whose formatted output would differ. Unlike `--check`, it
   exits 0 regardless of change state — the designed consumer is a
   pipeline (`paideia-as fmt --list-different | xargs ...`). Conflicts
   with `--check` and `--diff` at the clap layer.
3. **Workspace default**. `paideia-as fmt` with no positional args and
   no `--stdin` walks upward from CWD looking for a `.pdxfmt` marker
   (preferred) or `Cargo.toml` (fallback), then formats every `.pdx`
   under `src/` and `tests/` of that root. Mirrors rustfmt's "no args
   = workspace fmt" ergonomic.

## Files changed

- `Cargo.toml` — add `glob = "0.3"` to `[workspace.dependencies]`.
- `crates/paideia-as/Cargo.toml` — add `glob = { workspace = true }`.
- `crates/paideia-as/src/cli.rs` — switch `Fmt.inputs` to `Vec<String>`
  (globs are expanded in `cmd_fmt`, not clap), add `--list-different`
  flag with 3-way clap `conflicts_with_all` wiring against `--check`
  and `--diff`.
- `crates/paideia-as/src/main.rs` — pass the new `list_different` flag
  to `cmd_fmt::run`.
- `crates/paideia-as/src/cmd_fmt.rs` — new `expand_inputs`,
  `collect_workspace_pdx_files`, `find_workspace_root`,
  `walk_pdx_files`, and `is_glob_pattern` helpers; `run` now takes
  `&[String]` and dispatches to glob expansion or the workspace walk;
  `run_one_file` / `run_stdin` grow a `list_different` branch that
  prints paths to stdout and returns `Changed`/`Unchanged` without
  mutating the exit code; existing unit tests kept via a `run_paths`
  shim that re-wraps `&[PathBuf]` into the new `&[String]` signature.
- `crates/paideia-as/tests/cmd_fmt_cli_12.rs` — new integration-test
  module (assert_cmd, 8 tests).

## Glob corner-cases captured

- **Hidden files/dirs** (`.`-prefix): `glob 0.3` defaults to
  `require_literal_leading_dot = true`, so `src/**/*.pdx` deliberately
  does NOT descend into `.git/`, `.cache/`, etc. Explicitly covered by
  `fmt_glob_skips_hidden_dir_contents_by_default`.
- **Symlinks**: not deduplicated via canonicalization — a cyclic
  symlink in a user's layout can cause the same file to be formatted
  twice. This is idempotent (second pass is a no-op) and matches
  `rustfmt`'s own posture; we chose not to add a `canonicalize()` call
  since it would turn a benign double-format into a hard error on
  broken symlinks. Documented in the module header.
- **Directories matched by glob**: filtered out before
  `read_to_string`, so a pattern like `src/*` doesn't crash on
  `src/sub/`.
- **Zero-match glob**: produces an explicit stderr diagnostic and
  exit 2, so a CI typo (`.pdxx`) is loud rather than silent.
- **Workspace walk**: `.pdxfmt` beats `Cargo.toml` at any depth so a
  repo embedded in a larger Cargo workspace can pin the fmt root
  independently.

## Test coverage

- 8 integration tests under `tests/cmd_fmt_cli_12.rs`:
  glob `**` expansion, `--list-different` prints dirty paths,
  `--list-different` all-clean prints nothing, `--list-different`
  conflicts with `--check`, workspace default from `.pdxfmt`,
  workspace default + `--list-different`, empty-glob exit code,
  hidden-dir exclusion.
- 6 new unit tests inside `cmd_fmt.rs` covering
  `is_glob_pattern`, `find_workspace_root` priority, hidden-dir
  filtering in `walk_pdx_files`, mixed literal + glob dedupe,
  `--list-different` file preservation, `--list-different` zero exit.
- All pre-FMT-12 `cmd_fmt` unit tests kept, re-plumbed through a
  `run_paths` shim so they still exercise `run()` end-to-end under
  the new `&[String]` signature.
