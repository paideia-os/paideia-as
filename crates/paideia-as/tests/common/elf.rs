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

/// Extract bytes from a named section (not .text or .rodata specifically).
pub fn named_section_bytes(elf_bytes: &[u8], name: &str) -> Vec<u8> {
    section_bytes(elf_bytes, name)
}

/// Get the size of a named section.
pub fn section_size(elf_bytes: &[u8], name: &str) -> Option<u64> {
    let Ok(file) = object::File::parse(elf_bytes) else {
        return None;
    };
    for section in file.sections() {
        if section.name().unwrap_or("") == name {
            return Some(section.size());
        }
    }
    None
}

/// Check if a named section has the writable flag set.
pub fn section_is_writable(elf_bytes: &[u8], name: &str) -> bool {
    let Ok(file) = object::File::parse(elf_bytes) else {
        return false;
    };
    for section in file.sections() {
        if section.name().unwrap_or("") == name {
            // Check SHF_WRITE flag (0x1) using the debug output
            // The object crate's SectionFlags has format "Elf { sh_flags: N }"
            let flags_str = format!("{:?}", section.flags());
            // Extract the numeric value and check bit 0 (SHF_WRITE = 0x1)
            if let Some(start) = flags_str.find("sh_flags: ") {
                let rest = &flags_str[start + 10..];
                let num_str = rest.split('}').next().unwrap_or("0").trim();
                if let Ok(flags_val) = num_str.parse::<u64>() {
                    return (flags_val & 0x1) != 0;
                }
            }
            return false;
        }
    }
    false
}

/// Find a symbol by name and return its section index and address.
pub fn symbol_section_and_addr(elf_bytes: &[u8], symbol: &str) -> Option<(object::SectionIndex, u64)> {
    let Ok(file) = object::File::parse(elf_bytes) else {
        return None;
    };
    for sym in file.symbols() {
        let Ok(name) = sym.name() else { continue };
        if name == symbol {
            let section_idx = match sym.section() {
                object::SymbolSection::Section(idx) => idx,
                _ => continue,
            };
            return Some((section_idx, sym.address()));
        }
    }
    None
}

/// Get the index of a section by name.
pub fn section_index_by_name(elf_bytes: &[u8], name: &str) -> Option<object::SectionIndex> {
    let Ok(file) = object::File::parse(elf_bytes) else {
        return None;
    };
    for section in file.sections() {
        if section.name().unwrap_or("") == name {
            return Some(section.index());
        }
    }
    None
}
