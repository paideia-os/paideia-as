use std::io::Write;
use std::process::Stdio;

#[test]
fn cli_stdin_normalises_input() {
    let input = "let x = 1 \n\n\n\n\nlet y = 2";
    let expected = "let x = 1\n\n\nlet y = 2\n";

    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_paideia-fmt"))
        .arg("--stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn paideia-fmt");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        stdin
            .write_all(input.as_bytes())
            .expect("Failed to write to stdin");
    }

    let output = child.wait_with_output().expect("Failed to wait on child");
    assert!(output.status.success(), "paideia-fmt exited with error");

    let stdout = String::from_utf8(output.stdout).expect("Invalid UTF-8 in output");
    assert_eq!(stdout, expected);
}
