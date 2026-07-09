//! Build script for paideia-stdlib.
//!
//! When the `regen-uefi-template` feature is enabled, regenerates the UEFI header
//! template binary from the PE emitter crate.

#[cfg(feature = "regen-uefi-template")]
fn main() {
    use std::fs;
    use std::path::PathBuf;

    // Import PE header structures
    use paideia_as_emitter_pe::header::{
        DosHeader, CoffFileHeader, OptionalHeaderPe32Plus, NT_SIGNATURE,
    };

    // Construct a minimal but valid PE32+ header
    let dos = DosHeader::new();
    let coff = CoffFileHeader::new_efi_amd64();
    let mut opt = OptionalHeaderPe32Plus::new_efi_amd64();

    // Set image base to the UEFI default
    opt.image_base = 0x140000000u64;

    // Serialize all header components
    let dos_bytes = dos.to_bytes();
    let coff_bytes = coff.to_bytes();
    let opt_bytes = opt.to_bytes();

    // Construct a 512-byte template: DOS + NT sig + COFF + Optional + padding
    let mut template = [0u8; 512];

    // Copy DOS header (64 bytes)
    template[0..64].copy_from_slice(&dos_bytes);

    // Copy NT signature (4 bytes) at offset 64
    template[64..68].copy_from_slice(&NT_SIGNATURE);

    // Copy COFF file header (20 bytes) at offset 68
    template[68..88].copy_from_slice(&coff_bytes);

    // Copy Optional header (240 bytes) at offset 88
    template[88..328].copy_from_slice(&opt_bytes);

    // Rest (328..512) remains zero-filled

    // Write to pdx/uefi_header_template.bin
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let out_path = PathBuf::from(manifest_dir)
        .join("pdx")
        .join("uefi_header_template.bin");

    fs::write(&out_path, &template)
        .expect("Failed to write UEFI header template");

    println!(
        "cargo:warning=Regenerated UEFI header template: {}",
        out_path.display()
    );
}

#[cfg(not(feature = "regen-uefi-template"))]
fn main() {
    // Feature disabled; do nothing at build time.
}
