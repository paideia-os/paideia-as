# Tooling (Phase 4 m12)

**Status:** Phase 4 m12 closure appendix.
**Scope:** Documents the three `paideia-as` CLI subcommands shipped in m12 plus the deferred package manager.

## 0. What m12 adds

Phase 4 m12 closes three of the four user-facing CLI gaps that surfaced during Phase 3:

- `paideia-as test` — discovers + runs `#[test]`-annotated functions (m12-001).
- `paideia-as fmt` — formats `.pdx` source in place / via stdin / `--check` (m12-002).
- `paideia-as doc` — generates HTML documentation from doc-comments + signatures (m12-003).

The fourth gap — a package manager — is documented in §5 but is deferred to a Phase 5+ standalone milestone.

## 1. paideia-as test (m12-001)

`crates/paideia-as-test` ships the test runner library; `crates/paideia-as/src/cmd_test.rs` wires the CLI.

```
paideia-as test [OPTIONS] [PATHS]...

Options:
  --filter <regex>  Restrict to tests matching the regex.
  --list            Print discovered tests; don't execute.
  [PATHS]...        Override default scan paths (default: tests/, src/).
```

API surface:

```rust
pub struct TestEntry { pub name: String, pub source_path: String }
pub struct TestSummary { pub discovered: usize, pub passed: usize, pub failed: usize, pub filtered: usize }
pub struct TestRunner { /* opts */ }

impl TestRunner {
    pub fn new() -> Self;
    pub fn with_filter(self, pattern: &str) -> Result<Self, String>;
    pub fn list_only(self) -> Self;
    pub fn discover(&self, paths: &[PathBuf]) -> Vec<TestEntry>;
    pub fn run(&self, entries: &[TestEntry]) -> TestSummary;
}
```

Phase-4-m12-001 honest scope: **discovery + listing work end-to-end**. Actual test execution gates on the elaborator's lower path + a runtime evaluator — m13 self-hosting territory. Today the runner reports all discovered tests as "passed" (parse-only smoke). The 6 unit tests cover discovery, filter, list-mode, summary shape.

## 2. paideia-as fmt (m12-002)

`crates/paideia-fmt` (existing from Phase 2 m8-010) provides the `format(source, opts) -> String` core. `crates/paideia-as/src/cmd_fmt.rs` wires the CLI.

```
paideia-as fmt [OPTIONS] [FILE]

Options:
  --check    Exit 1 if the file would change; 0 if already formatted.
  --stdin    Read from stdin, write to stdout.
  [FILE]     Path to .pdx file (required unless --stdin).
```

Behaviours:

- Default: read FILE, format, write in place.
- `--check`: read FILE (or stdin), format, exit 1 if formatted ≠ source; no write.
- `--stdin`: read stdin, format, write to stdout.
- `--stdin --check`: read stdin, format, exit 1 if changes needed.

Existing formatter (m8-010) handles: trailing-whitespace strip, tab/space normalisation (default 4-space indent), blank-line cap (max 2 consecutive), ASCII↔Unicode glyph conversion (→, ⇒, λ, etc.) by config.

3 CLI tests cover already-formatted, needs-formatting, in-place write paths.

## 3. paideia-as doc (m12-003)

`crates/paideia-as-doc` ships the extractor + HTML renderer; `crates/paideia-as/src/cmd_doc.rs` wires the CLI.

```
paideia-as doc <INPUT> --output <OUTPUT>
```

Pipeline:

1. Read the `.pdx` source.
2. Walk line-by-line: accumulate `///` doc-comments, detect item declarations (let / fn / struct / enum / trait / impl / effect / capability / module).
3. Emit `DocCorpus { items: Vec<DocItem { name, kind, signature, doc } > }`.
4. Render `<!DOCTYPE html>` with one `<section id="name">` per item, monospace headings, minimal styling.
5. Cross-reference resolution: `[Name]` in doc text becomes `<a href="#name">Name</a>` when `Name` matches a corpus item.

Phase-4-m12-003 honest scope: minimum-viable styling (inline CSS), no search, no multi-file aggregation. Per the AC bullet "Minimum-viable styling; no search."

13 tests across the library + CLI cover extraction, rendering, cross-references, edge cases.

## 4. CLI summary

After m12, `paideia-as` has 7 subcommands:

| Subcommand | Source                                | Phase  |
|------------|---------------------------------------|--------|
| `check`    | parse + diagnostics                   | m1     |
| `build`    | parse → IR → emit (ELF64/PE/PAX)      | Phase 1+ |
| `lsp`      | language server (tower-lsp)           | m8 (Phase 2) |
| `dump-ast` | debug-print parsed AST                | m1     |
| `test`     | discover + run #[test] functions      | **m12-001** |
| `fmt`      | format .pdx source                    | **m12-002** |
| `doc`      | generate HTML docs from source        | **m12-003** |

The `hsm` subcommand tree (m6 / m7 / m8 / m11 from Phase 3 / m3-001/002 from Phase 4) adds further verbs under `paideia-as hsm pkcs11 init` etc.

## 5. Deferred: package manager

The fourth user-facing tool gap — a package manager — is **not** shipped in Phase 4. The minimum-viable shape would be:

- `paideia-as add <pkg>` — add a dep to the local manifest.
- `paideia-as build` resolves deps recursively.
- A central registry (or git-URL-direct).
- Lockfile for reproducible builds.

This is a Phase 5+ standalone milestone because:

1. **Stdlib bootstrap**: PaideiaOS may want to ship its own kernel-oriented "stdlib" parallel to `paideia-stdlib`. The package manager design depends on whether crates form a single tree (Cargo-style) or multiple trees per execution target.
2. **PaideiaOS subsystem composition**: how does a kernel module "depend on" a userspace library when the two run with different ABIs / allocators? The answer affects manifest design.
3. **Trust + signing integration**: every published package must be PQ-signed (m7 / m8 from Phase 3). The registry's role in the trust chain hasn't been designed.

None of these block Phase 4 closure; they shape the m12-005-or-Phase-5 package-manager design when it begins.

## 6. PaideiaOS impact

With m12 closed, PaideiaOS subsystem authors can:

- **Test discipline**: write `#[test]` functions in `.pdx` source; `paideia-as test --filter '^kernel'` exercises a subset. Execution awaits m13 runtime evaluator.
- **Fmt-on-save**: editor recipes (m8-014 from Phase 2) integrate `paideia-as fmt --stdin` for save hooks.
- **Documentation site**: `paideia-as doc kernel/*.pdx --output build/docs/kernel.html` generates per-subsystem HTML. Multi-file aggregation is m12-005 follow-up.

The package manager gap means PaideiaOS uses git submodules or vendored dependencies until Phase 5+.

## 7. Forward links

- **m13 self-hosting groundwork**: enables real test execution by providing the runtime evaluator that `paideia-as test` calls into.
- **m14 documentation closure**: consolidates m12's docs alongside the rest of Phase 4.
- **Phase 5 package manager**: the natural follow-up to m12.
- **PaideiaOS m1**: uses `paideia-as test` for kernel subsystem unit tests; `paideia-as fmt` in CI to enforce style.
