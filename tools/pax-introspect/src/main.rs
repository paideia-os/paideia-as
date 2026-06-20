//! pax-introspect: PAX file introspection tool.
//!
//! Reads and displays information from PAX (PaideiaOS Architectural Executable) files,
//! including optimization pass rewrite counts from the .paideia.opt-passes section.

use clap::Parser;
use paideia_as_emitter_pax::{OptPassesSection, PaxHeader, SectionTable, SectionType};
use std::fs;
use std::path::PathBuf;

type Result<T> = std::result::Result<T, String>;

#[derive(Parser, Debug)]
#[command(name = "pax-introspect")]
#[command(about = "Introspect PAX file contents", long_about = None)]
struct Args {
    /// Path to the PAX file to inspect
    #[arg(value_name = "FILE")]
    pax_file: PathBuf,

    /// Display per-pass aggregated rewrite counts (sum by pass name)
    #[arg(short, long)]
    by_pass: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Read the PAX file
    let data = fs::read(&args.pax_file).map_err(|e| {
        format!(
            "Failed to read PAX file: {}: {}",
            args.pax_file.display(),
            e
        )
    })?;

    // Parse the PAX header
    let header = PaxHeader::from_bytes(&data[..std::cmp::min(data.len(), 128)])
        .ok_or_else(|| "Failed to parse PAX header".to_string())?;

    // Verify magic number
    if header.magic != paideia_as_emitter_pax::PAX_MAGIC {
        return Err(format!("Invalid PAX magic number: {:?}", header.magic));
    }

    println!("PAX Header Information");
    println!("======================");
    println!("Format Version: {}", header.format_version);
    println!("Architecture: {:?}", header.architecture);
    println!("Section Count: {}", header.section_count);
    println!("Section Table Offset: 0x{:x}", header.section_table_offset);
    println!();

    // Parse section table
    let section_table_offset = header.section_table_offset as usize;
    let section_table =
        SectionTable::from_bytes(&data[section_table_offset..], header.section_count)
            .ok_or_else(|| "Failed to parse section table".to_string())?;

    // Find and display OptPasses section if present
    let opt_passes_section = section_table
        .sections
        .iter()
        .find(|s| s.ty == SectionType::OptPasses);

    if let Some(opt_section) = opt_passes_section {
        println!("Optimization Passes Information");
        println!("================================");
        println!("Section Name: {}", opt_section.name);
        println!("Content Offset: 0x{:x}", opt_section.content_offset);
        println!("Content Size: {} bytes", opt_section.content_size);
        println!();

        // Parse the OptPasses section content
        let section_offset = opt_section.content_offset as usize;
        let section_size = opt_section.content_size as usize;

        if section_offset + section_size <= data.len() {
            let section_data = &data[section_offset..section_offset + section_size];

            match OptPassesSection::from_bytes(section_data) {
                Some(opt_passes) => {
                    println!("Optimization Pass Records ({})", opt_passes.len());
                    println!("-----------------------------------------");

                    if args.by_pass {
                        // Aggregate by pass name
                        use std::collections::BTreeMap;
                        let mut by_pass: BTreeMap<String, u32> = BTreeMap::new();

                        for record in &opt_passes.records {
                            *by_pass.entry(record.pass_name.clone()).or_insert(0) +=
                                record.rewrite_count;
                        }

                        for (pass_name, total_count) in by_pass {
                            println!("  Pass: {:<40} Total Rewrites: {}", pass_name, total_count);
                        }
                    } else {
                        // Display each record individually
                        for record in &opt_passes.records {
                            println!(
                                "  Pass: {:<40} Function: i{:<6} Rewrites: {}",
                                record.pass_name,
                                record.function_id.get(),
                                record.rewrite_count
                            );
                        }
                    }
                    println!();
                }
                None => {
                    eprintln!("Warning: Failed to parse OptPasses section");
                }
            }
        } else {
            eprintln!("Warning: OptPasses section offset/size out of bounds");
        }
    } else {
        println!("No OptPasses section found in this PAX file.");
        println!();
    }

    // Display all sections summary
    println!("Section Table Summary");
    println!("====================");
    for (i, section) in section_table.sections.iter().enumerate() {
        println!(
            "[{}] {:<15} (0x{:02x}) Offset: 0x{:x} Size: {} bytes",
            i, section.name, section.ty as u32, section.content_offset, section.content_size
        );
    }

    Ok(())
}
