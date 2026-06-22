//! PA8 m3-005 (#829): PaideiaOS four-file `.text` byte-snapshot canary.
//!
//! After the Phase 8 m3 encoder corrections land (m3-001 width-routed
//! `MovSized`, m3-002 cast dispatch table, m3-003 width-aware `mov reg, imm`,
//! m3-004 round-trip iced-x86), the encoded instruction stream emitted for the
//! four unquarantined PaideiaOS kernel files must be locked down so that any
//! future, *unintended* change to encoder output is caught immediately.
//!
//! Why `.text` bytes (not whole-file SHA256)?
//! -----------------------------------------
//! Empirically the whole `.o` is NOT byte-deterministic across rebuilds: the
//! symbol table / section-header metadata reorders run-to-run (verified: 17
//! bytes flip in the symtab region of exceptions.o between two builds), while
//! the `.text` section — the actual encoded machine code, which is exactly what
//! the m3 encoder work changes — is fully deterministic. So the gate is the
//! `.text` section bytes. This is the load-bearing artifact: shorter
//! `MovSized`, narrower `mov reg, imm`, and corrected cast mnemonics all show
//! up here and nowhere else.
//!
//! Procedure:
//! 1. Discover PaideiaOS at `../../PaideiaOS` (or via env `PAIDEIA_OS_PATH`);
//!    skipped cleanly if absent.
//! 2. Build each of: kernel_main.pdx, exceptions.pdx, idt.pdx, pt_walk.pdx with
//!    the release `paideia-as` binary; assert exit 0.
//! 3. Extract the `.text` section via the `object` crate.
//! 4. Assert the bytes match the locked-in baseline below.
//!
//! Updating the baseline:
//! ----------------------
//! If an encoder change *intentionally* alters output (e.g. a future m3 task
//! narrows the `movabs $imm, %rax` (10-byte `48 B8 ..`) forms still present in
//! these snapshots into 5/7-byte forms), regenerate the baseline with:
//!
//!   for n in kernel_main exceptions idt pt_walk; do \
//!     paideia-as build --emit elf64 <file> -o /tmp/$n.o && \
//!     objcopy -O binary --only-section=.text /tmp/$n.o /tmp/$n.text && \
//!     xxd -p /tmp/$n.text | tr -d '\n'; echo; done
//!
//! and paste the new bytes here, *with a commit message documenting why the
//! bytes changed* (which instruction got shorter / which mnemonic changed).
//! An unexplained baseline edit is a red flag in review.
//!
//! Linux-only, mirroring paideia_os_checkpoint2_m2_canary.rs and
//! paideia_os_r1_5_r2_5_rebuild.rs.

#![cfg(target_os = "linux")]

use object::{Object, ObjectSection};
use std::path::{Path, PathBuf};
use std::process::Command;

/// One file under test: relative path under `src/kernel`, a short name, and the
/// expected `.text` byte snapshot (post-m3 corrected encoder output).
struct Case {
    rel_path: &'static str,
    name: &'static str,
    text: &'static [u8],
}

// === Baseline `.text` snapshots ===
// Captured on branch topic/pa8-m3-829 from the release paideia-as binary after
// m3-001..004 landed (commit 5597b71). Each is the verbatim `.text` section.
//
// NOTE: these still contain 10-byte `movabs $imm, %rax` (`48 B8 ..`) and
// `movabs $imm, %rdi` (`48 BF ..`) forms for small immediates. That is the
// *current* corrected-encoder output and is intentional to lock; a later m3
// task that narrows these is an EXPECTED change and must update the baseline.

const KERNEL_MAIN_TEXT: &[u8] = &[
    0x48, 0x89, 0xc0, 0x48, 0x89, 0xc0, 0xe8, 0x00, 0x00, 0x00, 0x00, 0xe8, 0x00, 0x00, 0x00, 0x00,
    0x48, 0x89, 0xc0, 0x48, 0x89, 0xc0, 0xe8, 0x00, 0x00, 0x00, 0x00, 0xe8, 0x00, 0x00, 0x00, 0x00,
    0x48, 0x89, 0xc0, 0x48, 0x89, 0xc0, 0xe8, 0x00, 0x00, 0x00, 0x00, 0xe8, 0x00, 0x00, 0x00, 0x00,
];

const EXCEPTIONS_TEXT: &[u8] = &[
    0x48, 0xb8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xb8, 0x03, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x48, 0xb8, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xb8,
    0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xb8, 0x0d, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x48, 0xb8, 0x0e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xb8, 0x02, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xb8, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x48, 0xb8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xb8, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0xf4, 0x48, 0x89, 0xc0, 0xf4, 0x48, 0x89, 0xc0, 0xc3, 0xe8, 0x00, 0x00,
    0x00, 0x00, 0xc3, 0xe8, 0x00, 0x00, 0x00, 0x00, 0xc3, 0xe8, 0x00, 0x00, 0x00, 0x00, 0xc3, 0xe8,
    0x00, 0x00, 0x00, 0x00, 0xc3, 0xe8, 0x00, 0x00, 0x00, 0x00, 0xc3, 0xc3,
];

const IDT_TEXT: &[u8] = &[
    0x48, 0xb8, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xb8, 0x10, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x48, 0xb8, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xb8,
    0xff, 0x0f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xb8, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x48, 0xb8, 0x8e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xb8, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xb8, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x48, 0xb8, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xb8, 0x21, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0xc3, 0x48, 0xb8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48,
    0x89, 0xc0, 0x48, 0x89, 0xc0, 0x48, 0x89, 0xc0, 0x48, 0x89, 0xc0, 0xe8, 0x00, 0x00, 0x00, 0x00,
    0xc3, 0xe8, 0x00, 0x00, 0x00, 0x00, 0xc3, 0xe8, 0x00, 0x00, 0x00, 0x00, 0xc3, 0xe8, 0x00, 0x00,
    0x00, 0x00, 0xc3, 0xe8, 0x00, 0x00, 0x00, 0x00, 0xc3, 0xe8, 0x00, 0x00, 0x00, 0x00, 0xc3, 0xe8,
    0x00, 0x00, 0x00, 0x00, 0xc3, 0xe8, 0x00, 0x00, 0x00, 0x00, 0xc3, 0xc3, 0x48, 0xbf, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xbf, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x48, 0xbf, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xbf, 0x08, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x48, 0xbf, 0x0d, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xbf,
    0x0e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xbf, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x48, 0xbf, 0x21, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

const PT_WALK_TEXT: &[u8] = &[
    0x48, 0xb8, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xb8, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x48, 0xb8, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xb8,
    0xff, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xb8, 0x27, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x48, 0xb8, 0x1e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xb8, 0x15, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xb8, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x48, 0xb8, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xb8, 0x02, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x48, 0xb8, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xe8, 0x00,
    0x00, 0x00, 0x00, 0xc3, 0xe8, 0x00, 0x00, 0x00, 0x00, 0xc3, 0xe8, 0x00, 0x00, 0x00, 0x00, 0xc3,
    0xe8, 0x00, 0x00, 0x00, 0x00, 0xc3, 0x48, 0x8d, 0x04, 0x3f, 0xc3,
];

const CASES: &[Case] = &[
    Case {
        rel_path: "boot/kernel_main.pdx",
        name: "kernel_main",
        text: KERNEL_MAIN_TEXT,
    },
    Case {
        rel_path: "core/int/exceptions.pdx",
        name: "exceptions",
        text: EXCEPTIONS_TEXT,
    },
    Case {
        rel_path: "core/int/idt.pdx",
        name: "idt",
        text: IDT_TEXT,
    },
    Case {
        rel_path: "core/mm/pt_walk.pdx",
        name: "pt_walk",
        text: PT_WALK_TEXT,
    },
];

/// Discover the PaideiaOS repo: `PAIDEIA_OS_PATH` env, else `../../PaideiaOS`.
fn find_paideia_os() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PAIDEIA_OS_PATH") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }
    let relative = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .parent()?
        .parent()?
        .join("PaideiaOS");
    if relative.exists() {
        Some(relative)
    } else {
        None
    }
}

/// Locate the release `paideia-as` binary built into the workspace target dir.
fn find_paideia_as() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .parent()?
        .join("target/release/paideia-as");
    if p.exists() { Some(p) } else { None }
}

/// Build `src` to an elf64 object at `out`; return (success, stderr).
fn build(paideia_as: &Path, src: &Path, out: &Path) -> (bool, String) {
    let _ = std::fs::remove_file(out);
    let result = Command::new(paideia_as)
        .args(["build", "--emit", "elf64"])
        .arg(src)
        .args(["-o"])
        .arg(out)
        .output()
        .expect("run paideia-as");
    let ok = result.status.code() == Some(0) && out.exists();
    (ok, String::from_utf8_lossy(&result.stderr).into_owned())
}

/// Extract the `.text` section bytes from an ELF object on disk.
fn read_text_section(obj_path: &Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(obj_path).ok()?;
    let file = object::File::parse(bytes.as_slice()).ok()?;
    let text = file.section_by_name(".text")?;
    Some(text.data().ok()?.to_vec())
}

/// Render a byte slice as a compact hex string for diff messages.
fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// First differing offset between two slices, if any.
fn first_diff(a: &[u8], b: &[u8]) -> Option<usize> {
    let n = a.len().min(b.len());
    for i in 0..n {
        if a[i] != b[i] {
            return Some(i);
        }
    }
    if a.len() != b.len() { Some(n) } else { None }
}

/// Build all four files and assert each `.text` matches its baseline.
///
/// One test covering all four cases: it reports a full per-file byte-diff
/// summary before failing, so an encoder regression is diagnosable from the
/// test output alone.
#[test]
fn paideia_os_m3_four_file_text_byte_snapshot() {
    let paideia_os = match find_paideia_os() {
        Some(p) => p,
        None => {
            eprintln!("SKIP: PaideiaOS not found (set PAIDEIA_OS_PATH); test deferred");
            return;
        }
    };
    let paideia_as = match find_paideia_as() {
        Some(p) => p,
        None => {
            eprintln!(
                "SKIP: target/release/paideia-as not built; \
                 run `cargo build --release -p paideia-as` first"
            );
            return;
        }
    };

    let kernel_dir = paideia_os.join("src/kernel");
    if !kernel_dir.exists() {
        eprintln!("SKIP: {} not present; test deferred", kernel_dir.display());
        return;
    }

    let tmpdir = std::env::temp_dir();
    let mut failures: Vec<String> = Vec::new();

    for case in CASES {
        let src = kernel_dir.join(case.rel_path);
        if !src.exists() {
            failures.push(format!(
                "{}: source missing at {}",
                case.name,
                src.display()
            ));
            continue;
        }
        let out = tmpdir.join(format!("pa8-m3-829-{}.o", case.name));

        let (ok, stderr) = build(&paideia_as, &src, &out);
        if !ok {
            failures.push(format!("{}: build failed: {}", case.name, stderr.trim()));
            continue;
        }

        let actual = match read_text_section(&out) {
            Some(t) => t,
            None => {
                failures.push(format!("{}: could not read .text section", case.name));
                continue;
            }
        };

        if actual.as_slice() == case.text {
            println!(
                "{}: .text byte-identical to baseline ({} bytes)",
                case.name,
                actual.len()
            );
        } else {
            let off = first_diff(&actual, case.text);
            failures.push(format!(
                "{}: .text MISMATCH (expected {} bytes, got {} bytes, first diff at offset {:?})\n  \
                 expected: {}\n  actual:   {}\n  \
                 If this change is EXPECTED (a deliberate encoder narrowing/mnemonic fix), \
                 update the baseline in this file and document the byte change in the commit.",
                case.name,
                case.text.len(),
                actual.len(),
                off,
                hex(case.text),
                hex(&actual),
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "m3 `.text` byte-snapshot gate failed for {} file(s):\n{}",
        failures.len(),
        failures.join("\n")
    );

    println!(
        "PA8 m3-829: all {} PaideiaOS files .text byte-identical to baseline",
        CASES.len()
    );
}
