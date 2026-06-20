//! Smoke test: the gdb helper script is present + syntactically valid Python.

#[test]
fn gdb_script_exists() {
    let path = std::path::Path::new("../../scripts/gdb/paideia.py");
    let absolute = if path.exists() {
        path.to_path_buf()
    } else {
        // Fallback: walk up from CARGO_MANIFEST_DIR.
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../scripts/gdb/paideia.py");
        p
    };
    assert!(
        absolute.exists(),
        "scripts/gdb/paideia.py must exist: {}",
        absolute.display()
    );
}

#[test]
fn gdb_script_is_python_syntactically_valid() {
    // Spawn `python3 -c "import ast; ast.parse(open(path).read())"` to verify.
    // If python3 isn't available, skip the test (don't fail).
    let path = {
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../scripts/gdb/paideia.py");
        p
    };
    if !path.exists() {
        eprintln!("script missing; skipping");
        return;
    }
    let result = std::process::Command::new("python3")
        .arg("-c")
        .arg(format!(
            "import ast; ast.parse(open('{}').read())",
            path.display()
        ))
        .output();
    match result {
        Ok(output) if output.status.success() => {}
        Ok(output) => panic!(
            "python AST parse failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ),
        Err(_) => eprintln!("python3 not available; skipping syntactic check"),
    }
}
