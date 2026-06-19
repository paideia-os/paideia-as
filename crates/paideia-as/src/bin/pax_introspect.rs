//! pax-introspect: dump a PAX file's header + section table.
//!
//! Usage: pax-introspect <path>

use paideia_as_emitter_pax::{PAX_HEADER_SIZE, PaxHeader, SectionTable};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: pax-introspect <path>");
        return ExitCode::from(2);
    }
    let bytes = match std::fs::read(&args[1]) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cannot read: {e}");
            return ExitCode::from(2);
        }
    };
    let header = match PaxHeader::from_bytes(&bytes) {
        Some(h) => h,
        None => {
            eprintln!("not a PAX file");
            return ExitCode::from(1);
        }
    };
    println!("PaxHeader:");
    println!("  format_version = {}", header.format_version);
    println!("  architecture   = {:?}", header.architecture);
    println!("  flags          = {:#x}", header.flags);
    println!("  section_table  @ {}", header.section_table_offset);
    println!("  section_count  = {}", header.section_count);
    println!("  blake3_hash    = {}", hex(&header.blake3_content_hash));
    // Table dump
    let table_bytes = &bytes[PAX_HEADER_SIZE..];
    if let Some(table) = SectionTable::from_bytes(table_bytes, header.section_count) {
        println!("SectionTable (count = {}):", table.len());
        for (i, s) in table.sections.iter().enumerate() {
            println!(
                "  [{i}] {:?} flags={:#x} align={} name={}",
                s.ty, s.flags, s.alignment, s.name
            );
        }
    }
    ExitCode::SUCCESS
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}
