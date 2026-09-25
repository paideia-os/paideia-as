//! R227.M3 `.pds` import-resolution + module-graph fixture corpus.
//!
//! 12 tests, tagged `r227m3-imp-01` .. `r227m3-imp-12`:
//!
//!   01 — header with no imports resolves to an empty Vec.
//!   02 — a single import present in `project_root` resolves to it.
//!   03 — multiple imports, all in `project_root`, all resolve.
//!   04 — same import name in both project and system → project wins.
//!   05 — import missing from project falls through to system_paths.
//!   06 — 2-level DFS graph (root → A → B) records both edges.
//!   07 — a single missing import → NotFound with `tried = [project]`.
//!   08 — NotFound `tried` list carries project + every system_path.
//!   09 — direct self-cycle (A → A) → Cycle with 2-element path.
//!   10 — 2-cycle (A → B → A) → Cycle with 3-element path.
//!   11 — 3-cycle (A → B → C → A) → Cycle with 4-element path.
//!   12 — a parser callback error propagates as ParseError.

use std::fs;
use std::path::{Path, PathBuf};

use paideia_as_shell_pds::{
    parse_header, Import, ImportContext, ImportError, ModuleGraph, PdsHeader,
};
use tempfile::TempDir;

// ────────────────────────────────────────────────────────────────
// Test helpers
// ────────────────────────────────────────────────────────────────

/// Build a `PdsHeader` carrying an arbitrary `imports` list. Every
/// other field takes its `Default` value — the resolver only reads
/// `imports`.
fn header_with_imports(imports: Vec<Import>) -> PdsHeader {
    PdsHeader {
        imports,
        ..PdsHeader::default()
    }
}

/// Build an `Import { path, alias }` from two `&str`s.
fn imp(path: &str, alias: &str) -> Import {
    Import {
        path: path.to_owned(),
        alias: alias.to_owned(),
    }
}

/// Write an empty `.pds` file at `<root>/<rel>`, creating any parent
/// directories the `rel` walks through. Returns the resulting
/// [`PathBuf`].
fn touch(root: &Path, rel: &str) -> PathBuf {
    let full = root.join(rel);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).expect("create parent directory");
    }
    fs::write(&full, b"").expect("write empty .pds file");
    full
}

/// Write a `.pds` file whose only content is a header carrying the
/// given `#import "path" as alias` lines (one per (path, alias)).
/// Returns the resulting [`PathBuf`].
fn write_pds_with_imports(root: &Path, rel: &str, imports: &[(&str, &str)]) -> PathBuf {
    let full = root.join(rel);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).expect("create parent directory");
    }
    let mut src = String::new();
    for (path, alias) in imports {
        src.push_str(&format!("#import \"{path}\" as {alias}\n"));
    }
    src.push('\n'); // header terminator
    fs::write(&full, src.as_bytes()).expect("write .pds source");
    full
}

/// A filesystem-backed parser callback for
/// `ModuleGraph::build_from_root`. Reads `path`, runs
/// [`parse_header`], and wraps failures as [`ImportError::ParseError`].
fn fs_parser(path: &Path) -> Result<PdsHeader, ImportError> {
    let bytes = fs::read(path).map_err(|e| ImportError::ParseError {
        path: path.display().to_string(),
        reason: format!("read failed: {e}"),
    })?;
    let text = std::str::from_utf8(&bytes).map_err(|e| ImportError::ParseError {
        path: path.display().to_string(),
        reason: format!("utf-8 decode: {e}"),
    })?;
    parse_header(text).map_err(|e| ImportError::ParseError {
        path: path.display().to_string(),
        reason: e.to_string(),
    })
}

// ────────────────────────────────────────────────────────────────
// r227m3-imp-01 .. r227m3-imp-06 — accepting cases
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m3_imp_01_no_imports_resolves_empty() {
    let tmp = TempDir::new().expect("r227m3-imp-01: tempdir");
    let ctx = ImportContext::new(tmp.path().to_path_buf());
    let header = header_with_imports(vec![]);
    let resolved = header
        .imports_resolved(&ctx)
        .expect("r227m3-imp-01: empty imports must resolve to empty Vec");
    assert!(
        resolved.is_empty(),
        "r227m3-imp-01: expected empty resolved list, got {resolved:?}"
    );
}

#[test]
fn r227m3_imp_02_single_project_import_resolves() {
    let tmp = TempDir::new().expect("r227m3-imp-02: tempdir");
    let root = tmp.path().to_path_buf();
    let expected = touch(&root, "util.pds");
    let ctx = ImportContext::new(root.clone());
    let header = header_with_imports(vec![imp("util.pds", "util")]);
    let resolved = header
        .imports_resolved(&ctx)
        .expect("r227m3-imp-02: single project-local import must resolve");
    assert_eq!(
        resolved.len(),
        1,
        "r227m3-imp-02: exactly one resolved entry"
    );
    assert_eq!(
        resolved[0].path, expected,
        "r227m3-imp-02: resolved path equals project_root.join(path)"
    );
    assert_eq!(
        resolved[0].alias, "util",
        "r227m3-imp-02: alias copied verbatim from Import"
    );
}

#[test]
fn r227m3_imp_03_multi_project_imports_all_resolve() {
    let tmp = TempDir::new().expect("r227m3-imp-03: tempdir");
    let root = tmp.path().to_path_buf();
    let a = touch(&root, "lib/a.pds");
    let b = touch(&root, "lib/b.pds");
    let c = touch(&root, "lib/c.pds");
    let ctx = ImportContext::new(root.clone());
    let header = header_with_imports(vec![
        imp("lib/a.pds", "a"),
        imp("lib/b.pds", "b"),
        imp("lib/c.pds", "c"),
    ]);
    let resolved = header
        .imports_resolved(&ctx)
        .expect("r227m3-imp-03: all three project-local imports must resolve");
    assert_eq!(
        resolved.len(),
        3,
        "r227m3-imp-03: three resolved entries, one per Import"
    );
    assert_eq!(resolved[0].path, a, "r227m3-imp-03: entry 0 → lib/a.pds");
    assert_eq!(resolved[0].alias, "a", "r227m3-imp-03: alias 0 → a");
    assert_eq!(resolved[1].path, b, "r227m3-imp-03: entry 1 → lib/b.pds");
    assert_eq!(resolved[1].alias, "b", "r227m3-imp-03: alias 1 → b");
    assert_eq!(resolved[2].path, c, "r227m3-imp-03: entry 2 → lib/c.pds");
    assert_eq!(resolved[2].alias, "c", "r227m3-imp-03: alias 2 → c");
}

#[test]
fn r227m3_imp_04_project_shadows_system() {
    let project = TempDir::new().expect("r227m3-imp-04: project tempdir");
    let system = TempDir::new().expect("r227m3-imp-04: system tempdir");
    let project_root = project.path().to_path_buf();
    let system_root = system.path().to_path_buf();
    // Both roots offer "std/prelude.pds"; project must win.
    let project_hit = touch(&project_root, "std/prelude.pds");
    let system_hit = touch(&system_root, "std/prelude.pds");
    assert_ne!(
        project_hit, system_hit,
        "r227m3-imp-04: sanity — two distinct on-disk locations"
    );
    let ctx = ImportContext::new(project_root.clone())
        .with_system_paths(vec![system_root.clone()]);
    let header = header_with_imports(vec![imp("std/prelude.pds", "prelude")]);
    let resolved = header
        .imports_resolved(&ctx)
        .expect("r227m3-imp-04: must resolve against project first");
    assert_eq!(
        resolved.len(),
        1,
        "r227m3-imp-04: exactly one resolved entry"
    );
    assert_eq!(
        resolved[0].path, project_hit,
        "r227m3-imp-04: project_root wins over system_paths on shadow"
    );
    assert_ne!(
        resolved[0].path, system_hit,
        "r227m3-imp-04: system entry must not be the winner"
    );
}

#[test]
fn r227m3_imp_05_system_fallback_when_project_missing() {
    let project = TempDir::new().expect("r227m3-imp-05: project tempdir");
    let system = TempDir::new().expect("r227m3-imp-05: system tempdir");
    let project_root = project.path().to_path_buf();
    let system_root = system.path().to_path_buf();
    // Only the system root has "core/list.pds".
    let system_hit = touch(&system_root, "core/list.pds");
    assert!(
        !project_root.join("core/list.pds").exists(),
        "r227m3-imp-05: sanity — project does not carry this import"
    );
    let ctx = ImportContext::new(project_root)
        .with_system_paths(vec![system_root]);
    let header = header_with_imports(vec![imp("core/list.pds", "list")]);
    let resolved = header
        .imports_resolved(&ctx)
        .expect("r227m3-imp-05: system fallback must succeed");
    assert_eq!(
        resolved.len(),
        1,
        "r227m3-imp-05: exactly one resolved entry"
    );
    assert_eq!(
        resolved[0].path, system_hit,
        "r227m3-imp-05: resolved to the system-path candidate"
    );
    assert_eq!(
        resolved[0].alias, "list",
        "r227m3-imp-05: alias preserved through system fallback"
    );
}

#[test]
fn r227m3_imp_06_module_graph_two_level_dfs() {
    let tmp = TempDir::new().expect("r227m3-imp-06: tempdir");
    let root_dir = tmp.path().to_path_buf();
    // Layout: root.pds → a.pds → b.pds (a leaf).
    let root = write_pds_with_imports(&root_dir, "root.pds", &[("a.pds", "a_alias")]);
    let a = write_pds_with_imports(&root_dir, "a.pds", &[("b.pds", "b_alias")]);
    let b = write_pds_with_imports(&root_dir, "b.pds", &[]);
    let ctx = ImportContext::new(root_dir.clone());
    let graph = ModuleGraph::build_from_root(&root, &ctx, fs_parser)
        .expect("r227m3-imp-06: two-level DFS must succeed");
    assert_eq!(
        graph.edges.len(),
        2,
        "r227m3-imp-06: exactly two edges recorded, got {:?}",
        graph.edges
    );
    // Edge 0: root → a_alias.
    assert_eq!(
        graph.edges[0],
        (root.display().to_string(), "a_alias".to_owned()),
        "r227m3-imp-06: first edge is root → a_alias"
    );
    // Edge 1: a → b_alias.
    assert_eq!(
        graph.edges[1],
        (a.display().to_string(), "b_alias".to_owned()),
        "r227m3-imp-06: second edge is a → b_alias"
    );
    assert!(
        graph.resolved.contains_key(&root.display().to_string()),
        "r227m3-imp-06: resolved map records root"
    );
    assert!(
        graph.resolved.contains_key(&a.display().to_string()),
        "r227m3-imp-06: resolved map records a"
    );
    assert!(
        graph.resolved.contains_key(&b.display().to_string()),
        "r227m3-imp-06: resolved map records b (leaf)"
    );
}

// ────────────────────────────────────────────────────────────────
// r227m3-imp-07 .. r227m3-imp-12 — rejecting cases
// ────────────────────────────────────────────────────────────────

#[test]
fn r227m3_imp_07_single_missing_import_notfound() {
    let tmp = TempDir::new().expect("r227m3-imp-07: tempdir");
    let root = tmp.path().to_path_buf();
    let ctx = ImportContext::new(root.clone());
    let header = header_with_imports(vec![imp("missing.pds", "m")]);
    let err = header
        .imports_resolved(&ctx)
        .expect_err("r227m3-imp-07: must fail with NotFound");
    match err {
        ImportError::NotFound { path, tried } => {
            assert_eq!(path, "missing.pds", "r227m3-imp-07: echoed pragma path");
            assert_eq!(
                tried,
                vec![root.join("missing.pds")],
                "r227m3-imp-07: tried list contains just the project candidate"
            );
        }
        other => panic!("r227m3-imp-07: expected NotFound, got {other:?}"),
    }
}

#[test]
fn r227m3_imp_08_notfound_lists_project_plus_all_system_paths() {
    let project = TempDir::new().expect("r227m3-imp-08: project tempdir");
    let s1 = TempDir::new().expect("r227m3-imp-08: s1 tempdir");
    let s2 = TempDir::new().expect("r227m3-imp-08: s2 tempdir");
    let s3 = TempDir::new().expect("r227m3-imp-08: s3 tempdir");
    let project_root = project.path().to_path_buf();
    let s1_root = s1.path().to_path_buf();
    let s2_root = s2.path().to_path_buf();
    let s3_root = s3.path().to_path_buf();
    let ctx = ImportContext::new(project_root.clone()).with_system_paths(vec![
        s1_root.clone(),
        s2_root.clone(),
        s3_root.clone(),
    ]);
    let header = header_with_imports(vec![imp("nope.pds", "nope")]);
    let err = header
        .imports_resolved(&ctx)
        .expect_err("r227m3-imp-08: must fail with NotFound");
    match err {
        ImportError::NotFound { path, tried } => {
            assert_eq!(path, "nope.pds", "r227m3-imp-08: echoed pragma path");
            assert_eq!(
                tried,
                vec![
                    project_root.join("nope.pds"),
                    s1_root.join("nope.pds"),
                    s2_root.join("nope.pds"),
                    s3_root.join("nope.pds"),
                ],
                "r227m3-imp-08: tried list is project then every system_path in order"
            );
        }
        other => panic!("r227m3-imp-08: expected NotFound, got {other:?}"),
    }
}

#[test]
fn r227m3_imp_09_self_cycle_detected() {
    let tmp = TempDir::new().expect("r227m3-imp-09: tempdir");
    let root_dir = tmp.path().to_path_buf();
    // a.pds imports itself.
    let a = write_pds_with_imports(&root_dir, "a.pds", &[("a.pds", "a")]);
    let ctx = ImportContext::new(root_dir);
    let err = ModuleGraph::build_from_root(&a, &ctx, fs_parser)
        .expect_err("r227m3-imp-09: self-cycle must fail");
    match err {
        ImportError::Cycle { path } => {
            assert_eq!(
                path.len(),
                2,
                "r227m3-imp-09: cycle path has 2 elements (a on stack, then a as back-edge), got {path:?}"
            );
            let a_str = a.display().to_string();
            assert_eq!(
                path[0], a_str,
                "r227m3-imp-09: first cycle element is a"
            );
            assert_eq!(
                path[1], a_str,
                "r227m3-imp-09: second cycle element is a (the back-edge)"
            );
        }
        other => panic!("r227m3-imp-09: expected Cycle, got {other:?}"),
    }
}

#[test]
fn r227m3_imp_10_two_cycle_detected() {
    let tmp = TempDir::new().expect("r227m3-imp-10: tempdir");
    let root_dir = tmp.path().to_path_buf();
    // a.pds → b.pds → a.pds
    let a = write_pds_with_imports(&root_dir, "a.pds", &[("b.pds", "b")]);
    let b = write_pds_with_imports(&root_dir, "b.pds", &[("a.pds", "a")]);
    let ctx = ImportContext::new(root_dir);
    let err = ModuleGraph::build_from_root(&a, &ctx, fs_parser)
        .expect_err("r227m3-imp-10: 2-cycle must fail");
    match err {
        ImportError::Cycle { path } => {
            assert_eq!(
                path.len(),
                3,
                "r227m3-imp-10: 2-cycle path has 3 elements (a, b, a), got {path:?}"
            );
            let a_str = a.display().to_string();
            let b_str = b.display().to_string();
            assert_eq!(path[0], a_str, "r227m3-imp-10: element 0 is a");
            assert_eq!(path[1], b_str, "r227m3-imp-10: element 1 is b");
            assert_eq!(
                path[2], a_str,
                "r227m3-imp-10: element 2 is a (back-edge closes cycle)"
            );
        }
        other => panic!("r227m3-imp-10: expected Cycle, got {other:?}"),
    }
}

#[test]
fn r227m3_imp_11_three_cycle_detected() {
    let tmp = TempDir::new().expect("r227m3-imp-11: tempdir");
    let root_dir = tmp.path().to_path_buf();
    // a.pds → b.pds → c.pds → a.pds
    let a = write_pds_with_imports(&root_dir, "a.pds", &[("b.pds", "b")]);
    let b = write_pds_with_imports(&root_dir, "b.pds", &[("c.pds", "c")]);
    let c = write_pds_with_imports(&root_dir, "c.pds", &[("a.pds", "a")]);
    let ctx = ImportContext::new(root_dir);
    let err = ModuleGraph::build_from_root(&a, &ctx, fs_parser)
        .expect_err("r227m3-imp-11: 3-cycle must fail");
    match err {
        ImportError::Cycle { path } => {
            assert_eq!(
                path.len(),
                4,
                "r227m3-imp-11: 3-cycle path has 4 elements (a, b, c, a), got {path:?}"
            );
            let a_str = a.display().to_string();
            let b_str = b.display().to_string();
            let c_str = c.display().to_string();
            assert_eq!(path[0], a_str, "r227m3-imp-11: element 0 is a");
            assert_eq!(path[1], b_str, "r227m3-imp-11: element 1 is b");
            assert_eq!(path[2], c_str, "r227m3-imp-11: element 2 is c");
            assert_eq!(
                path[3], a_str,
                "r227m3-imp-11: element 3 is a (back-edge closes cycle)"
            );
        }
        other => panic!("r227m3-imp-11: expected Cycle, got {other:?}"),
    }
}

#[test]
fn r227m3_imp_12_parser_error_propagates() {
    // The parser callback deliberately returns a ParseError for the
    // root node. `build_from_root` must surface it verbatim (path and
    // reason echoed), not swallow it or reinterpret it as NotFound.
    let tmp = TempDir::new().expect("r227m3-imp-12: tempdir");
    let root_dir = tmp.path().to_path_buf();
    let root = touch(&root_dir, "root.pds");
    let ctx = ImportContext::new(root_dir);

    let broken_parser = |p: &Path| -> Result<PdsHeader, ImportError> {
        Err(ImportError::ParseError {
            path: p.display().to_string(),
            reason: "synthetic parser failure".to_owned(),
        })
    };

    let err = ModuleGraph::build_from_root(&root, &ctx, broken_parser)
        .expect_err("r227m3-imp-12: parser error must propagate");
    match err {
        ImportError::ParseError { path, reason } => {
            assert_eq!(
                path,
                root.display().to_string(),
                "r227m3-imp-12: echoed the offending node's path"
            );
            assert_eq!(
                reason, "synthetic parser failure",
                "r227m3-imp-12: reason forwarded verbatim from the parser callback"
            );
        }
        other => panic!("r227m3-imp-12: expected ParseError, got {other:?}"),
    }
}
