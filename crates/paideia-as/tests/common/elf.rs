//! ELF-parsing helpers on top of the `object` crate.
//!
//! Replaces the `extract_text_section` / symbol-name search snippets that
//! were open-coded across ~60 files.

use object::{Object, ObjectSection, ObjectSymbol};

/// Verify the buffer looks like a little-endian ELF64.
pub fn assert_elf64_magic(bytes: &[u8]) {
    assert!(bytes.len() >= 64, "ELF header is 64 bytes minimum");
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic missing");
    assert_eq!(bytes[4], 2, "expected ELF64 (class 2)");
    assert_eq!(bytes[5], 1, "expected little-endian (data 1)");
}

/// Extract `.text` bytes, or return an empty `Vec` if the section is missing
/// or the buffer fails to parse. Mirrors the historic `extract_text_section`
/// helper.
pub fn text_bytes(bytes: &[u8]) -> Vec<u8> {
    section_bytes(bytes, ".text")
}

/// Extract `.rodata` bytes.
pub fn rodata_bytes(bytes: &[u8]) -> Vec<u8> {
    section_bytes(bytes, ".rodata")
}

/// Extract `.data` bytes.
pub fn data_bytes(bytes: &[u8]) -> Vec<u8> {
    section_bytes(bytes, ".data")
}

/// Extract the bytes of a named section from an ELF buffer.
pub fn section_bytes(bytes: &[u8], name: &str) -> Vec<u8> {
    let Ok(file) = object::File::parse(bytes) else {
        return Vec::new();
    };
    for section in file.sections() {
        if section.name().unwrap_or("") == name {
            return section.data().unwrap_or(b"").to_vec();
        }
    }
    Vec::new()
}

/// Return `(address, size, bytes)` for the given symbol if it lives in
/// `.text` and has non-zero size.
pub fn symbol_bytes(elf_bytes: &[u8], symbol: &str) -> Option<Vec<u8>> {
    let file = object::File::parse(elf_bytes).ok()?;
    let text = text_bytes(elf_bytes);
    for sym in file.symbols() {
        let Ok(name) = sym.name() else { continue };
        if name != symbol {
            continue;
        }
        let addr = sym.address() as usize;
        let size = sym.size() as usize;
        if size == 0 || addr + size > text.len() {
            return None;
        }
        return Some(text[addr..addr + size].to_vec());
    }
    None
}

/// Do any of the symbols in the ELF have the given name?
pub fn has_symbol(elf_bytes: &[u8], symbol: &str) -> bool {
    let Ok(file) = object::File::parse(elf_bytes) else {
        return false;
    };
    file.symbols()
        .any(|s| s.name().map(|n| n == symbol).unwrap_or(false))
}

/// Does the ELF contain a section with this exact name?
pub fn has_section(elf_bytes: &[u8], name: &str) -> bool {
    let Ok(file) = object::File::parse(elf_bytes) else {
        return false;
    };
    file.sections()
        .any(|s| s.name().unwrap_or("") == name)
}
