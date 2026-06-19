//! UEFI loader smoke harness.
//!
//! Phase-2-m6-008: the harness scaffolds + gates a boot smoke. A working
//! hello.efi that actually runs UEFI Boot Services and prints requires
//! a meaningful .pdx → .efi pipeline; that arrives in a later milestone
//! (when the elaborator threads real instructions into the PE emitter).
//! Until then, the boot test is `#[ignore]`'d behind both an env check
//! (OVMF + QEMU present) and a runtime build of the .efi.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use paideia_as_emitter_pe::{
    CHARACTERISTICS_TEXT, COFF_FILE_HEADER_SIZE, CoffFileHeader, DOS_HEADER_SIZE, DosHeader,
    IMAGE_FILE_EXECUTABLE_IMAGE, IMAGE_FILE_MACHINE_AMD64, NT_SIGNATURE,
    OPTIONAL_HEADER_PE32PLUS_SIZE, OptionalHeaderPe32Plus, SECTION_HEADER_SIZE, SectionHeader,
    align_up,
};
use paideia_as_encoder::CodeBuffer;

/// UEFI environment probed from the host.
///
/// Contains paths to OVMF firmware and QEMU if both are available.
#[derive(Debug, Clone)]
pub struct UefiEnv {
    /// Path to OVMF_CODE.fd (UEFI firmware code).
    pub ovmf_code: PathBuf,
    /// Path to OVMF_VARS.fd (UEFI firmware variables).
    pub ovmf_vars: PathBuf,
    /// Path to qemu-system-x86_64.
    pub qemu_system_x86_64: PathBuf,
}

impl UefiEnv {
    /// Probe the host for OVMF + QEMU.
    ///
    /// Checks for OVMF firmware files in `/usr/share/OVMF/` (tries both
    /// standard and 4M variants) and `qemu-system-x86_64` on PATH.
    ///
    /// Returns `Some(UefiEnv)` if all are present; `None` otherwise.
    pub fn probe() -> Option<Self> {
        // Try to find OVMF_CODE (prefer standard, fallback to 4M)
        let ovmf_code_candidates = [
            PathBuf::from("/usr/share/OVMF/OVMF_CODE.fd"),
            PathBuf::from("/usr/share/OVMF/OVMF_CODE_4M.fd"),
        ];
        let ovmf_code = ovmf_code_candidates.iter().find(|p| p.exists())?.clone();

        // Try to find OVMF_VARS (prefer standard, fallback to 4M)
        let ovmf_vars_candidates = [
            PathBuf::from("/usr/share/OVMF/OVMF_VARS.fd"),
            PathBuf::from("/usr/share/OVMF/OVMF_VARS_4M.fd"),
        ];
        let ovmf_vars = ovmf_vars_candidates.iter().find(|p| p.exists())?.clone();

        // Check for qemu-system-x86_64 on PATH
        let which_output = Command::new("which")
            .arg("qemu-system-x86_64")
            .output()
            .ok()?;

        if !which_output.status.success() {
            return None;
        }

        let qemu_path = String::from_utf8(which_output.stdout)
            .ok()?
            .trim()
            .to_string();

        Some(Self {
            ovmf_code,
            ovmf_vars,
            qemu_system_x86_64: PathBuf::from(qemu_path),
        })
    }
}

/// Build a minimal hello.efi by direct PE/COFF emission.
///
/// This produces a structurally-valid PE/COFF file with:
/// - DOS header
/// - NT signature
/// - COFF file header
/// - Optional header (PE32+)
/// - One .text section with minimal code
///
/// Phase-2-m6-008 minimum: structurally valid; m6-009+ will emit real code.
///
/// # Arguments
///
/// * `out_path` - Path where the .efi file will be written
///
/// # Panics
///
/// Panics if file I/O fails.
pub fn build_hello_efi(out_path: &Path) {
    // Emit minimal .text section code (10-byte UEFI thunk)
    let mut text_buf = CodeBuffer::new();
    paideia_as_emitter_pe::emit_uefi_thunk(&mut text_buf, 0);
    let text_code = text_buf.as_slice().to_vec();

    // Compute section offsets
    let num_sections = 1u16;
    let section_table_size = (num_sections as usize) * SECTION_HEADER_SIZE;
    let headers_size = DOS_HEADER_SIZE
        + 4
        + COFF_FILE_HEADER_SIZE
        + OPTIONAL_HEADER_PE32PLUS_SIZE
        + section_table_size;

    let text_file_offset = headers_size as u32;
    let text_section_size = text_code.len() as u32;
    let text_aligned_size = align_up(text_section_size, 512);

    // Build DOS header
    let dos_header = DosHeader::new();

    // Build COFF file header
    let coff_header = CoffFileHeader {
        machine: IMAGE_FILE_MACHINE_AMD64,
        number_of_sections: num_sections,
        time_date_stamp: 0,
        pointer_to_symbol_table: 0,
        number_of_symbols: 0,
        size_of_optional_header: OPTIONAL_HEADER_PE32PLUS_SIZE as u16,
        characteristics: IMAGE_FILE_EXECUTABLE_IMAGE,
    };

    // Build section header for .text
    let text_section = SectionHeader {
        name: {
            let mut name = [0u8; 8];
            name[0..5].copy_from_slice(b".text");
            name
        },
        virtual_size: text_section_size,
        virtual_address: 0x1000,
        size_of_raw_data: text_aligned_size,
        pointer_to_raw_data: text_file_offset,
        pointer_to_relocations: 0,
        pointer_to_line_numbers: 0,
        number_of_relocations: 0,
        number_of_line_numbers: 0,
        characteristics: CHARACTERISTICS_TEXT,
    };

    // Build Optional header
    let image_size = 0x2000u32;
    let mut opt_header = OptionalHeaderPe32Plus::new_efi_amd64();
    opt_header.major_linker_version = 14;
    opt_header.size_of_code = text_aligned_size;
    opt_header.address_of_entry_point = 0x1000;
    opt_header.base_of_code = 0x1000;
    opt_header.image_base = 0x140000000u64;
    opt_header.file_alignment = 512;
    opt_header.major_operating_system_version = 6;
    opt_header.major_subsystem_version = 6;
    opt_header.size_of_image = image_size;
    opt_header.size_of_headers = headers_size as u32;
    opt_header.size_of_stack_reserve = 0x100000;
    opt_header.size_of_stack_commit = 0x1000;
    opt_header.size_of_heap_reserve = 0x100000;
    opt_header.size_of_heap_commit = 0x1000;

    // Assemble the PE file
    let mut pe_bytes = Vec::new();

    // DOS header
    pe_bytes.extend_from_slice(&dos_header.to_bytes());

    // NT signature
    pe_bytes.extend_from_slice(&NT_SIGNATURE);

    // COFF file header
    pe_bytes.extend_from_slice(&coff_header.to_bytes());

    // Optional header
    pe_bytes.extend_from_slice(&opt_header.to_bytes());

    // Section header for .text
    pe_bytes.extend_from_slice(&text_section.to_bytes());

    // Pad to text_file_offset
    while pe_bytes.len() < text_file_offset as usize {
        pe_bytes.push(0);
    }

    // Text section content
    pe_bytes.extend_from_slice(&text_code);

    // Pad text section to alignment boundary
    while pe_bytes.len() < (text_file_offset as usize + text_aligned_size as usize) {
        pe_bytes.push(0);
    }

    // Ensure minimum file size of 1KB (standard for UEFI images)
    while pe_bytes.len() < 1024 {
        pe_bytes.push(0);
    }

    // Write to file
    std::fs::write(out_path, &pe_bytes).expect("Failed to write .efi file");
}

/// Spawn QEMU and capture serial output from a UEFI boot.
///
/// This function creates a temporary FAT image, places the .efi at the
/// UEFI boot path, and spawns QEMU with OVMF firmware. The VM is killed
/// after 30 seconds (hard timeout).
///
/// # Arguments
///
/// * `env` - UEFI environment (paths to OVMF, QEMU)
/// * `efi_path` - Path to the .efi file to boot
///
/// # Returns
///
/// `Ok(bytes)` containing the captured serial output; `Err` on spawn/IO failure.
///
/// # Errors
///
/// Returns an error if:
/// - FAT image creation tools are unavailable
/// - QEMU spawn fails
/// - I/O error occurs during capture
pub fn boot_and_capture_serial(env: &UefiEnv, efi_path: &Path) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    use std::process::Stdio;

    // Check for mkfs.vfat and mcopy
    let mkfs_available = Command::new("which")
        .arg("mkfs.vfat")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let mcopy_available = Command::new("which")
        .arg("mcopy")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !mkfs_available || !mcopy_available {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "mkfs.vfat or mcopy not available; skipping boot test",
        ));
    }

    // Create temporary FAT image
    let temp_dir = std::env::temp_dir().join("paideia-uefi-boot");
    std::fs::create_dir_all(&temp_dir)?;
    let fat_image = temp_dir.join("boot.fat");

    // Create a 10MB FAT image
    Command::new("mkfs.vfat")
        .arg("-C")
        .arg(&fat_image)
        .arg("10240")
        .output()
        .map_err(|e| std::io::Error::new(e.kind(), format!("Failed to create FAT image: {}", e)))?;

    // Copy .efi into the FAT image at EFI/BOOT/BOOTX64.EFI
    Command::new("mcopy")
        .arg("-i")
        .arg(&fat_image)
        .arg("-D")
        .arg("o")
        .arg(efi_path)
        .arg("::/EFI/BOOT/BOOTX64.EFI")
        .output()
        .map_err(|e| {
            std::io::Error::new(e.kind(), format!("Failed to copy EFI to FAT image: {}", e))
        })?;

    // Spawn QEMU with hard timeout of 30 seconds
    let mut qemu_cmd = Command::new(&env.qemu_system_x86_64);
    qemu_cmd
        .arg("-machine")
        .arg("q35")
        .arg("-cpu")
        .arg("host")
        .arg("-drive")
        .arg(format!(
            "if=pflash,format=raw,readonly=on,file={}",
            env.ovmf_code.display()
        ))
        .arg("-drive")
        .arg(format!(
            "if=pflash,format=raw,file={}",
            env.ovmf_vars.display()
        ))
        .arg("-drive")
        .arg(format!("format=raw,file={}", fat_image.display()))
        .arg("-serial")
        .arg("stdio")
        .arg("-display")
        .arg("none")
        .arg("-no-reboot")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = qemu_cmd.spawn()?;

    // Give QEMU 30 seconds to run
    std::thread::sleep(std::time::Duration::from_secs(30));

    // Kill the process
    let _ = child.kill();
    let _ = child.wait();

    // Collect stdout
    if let Some(mut stdout) = child.stdout.take() {
        let mut output = Vec::new();
        let _ = stdout.read_to_end(&mut output);
        Ok(output)
    } else {
        Ok(Vec::new())
    }
}
