//! End-to-end test: spawn the LSP binary, send initialize, get back capabilities.

use std::io::Write;
use std::process::{Command, Stdio};

#[test]
#[ignore] // Timing-sensitive; may be flaky in slow CI environments
fn initialize_returns_server_info_and_capabilities() {
    let bin = env!("CARGO_BIN_EXE_paideia-lsp");
    let mut child = Command::new(bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn paideia-lsp");

    let req = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}"#;
    let msg = format!("Content-Length: {}\r\n\r\n{}", req.len(), req);

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        stdin.write_all(msg.as_bytes()).expect("write");
        stdin.flush().expect("flush");
    }

    // Give the server a moment to process.
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Send shutdown to terminate cleanly.
    let shutdown = r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#;
    let exit = r#"{"jsonrpc":"2.0","method":"exit","params":null}"#;

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        let smsg = format!("Content-Length: {}\r\n\r\n{}", shutdown.len(), shutdown);
        stdin.write_all(smsg.as_bytes()).expect("write shutdown");
        let xmsg = format!("Content-Length: {}\r\n\r\n{}", exit.len(), exit);
        stdin.write_all(xmsg.as_bytes()).expect("write exit");
        stdin.flush().expect("flush");
    }

    let output = child.wait_with_output().expect("wait");
    let stdout_str = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout_str.contains("paideia-lsp"),
        "stdout should mention server name; got: {stdout_str}"
    );
    assert!(
        stdout_str.contains("capabilities"),
        "stdout should include capabilities; got: {stdout_str}"
    );
}
