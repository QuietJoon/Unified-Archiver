//! Example: Calculate archive-level CRC32
//!
//! Demonstrates how to calculate the "archive CRC" that 7-Zip shows,
//! which is simply the arithmetic sum of all file CRC32s.
//!
//! This value will be the same regardless of:
//! - Archive format (ZIP, 7z, RAR)
//! - Compression method
//! - File order
//! - Archive metadata
//!
//! Usage:
//!   cargo run --example archive_crc <archive_file>

use std::env;
use unified_archive::Archive;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <archive_file>", args[0]);
        eprintln!("\nExample:");
        eprintln!("  {} file.zip", args[0]);
        eprintln!("  {} file.7z", args[0]);
        eprintln!("  {} file.rar", args[0]);
        std::process::exit(1);
    }

    let archive_path = &args[1];
    println!("Opening archive: {}\n", archive_path);

    match Archive::open(archive_path) {
        Ok(archive) => {
            println!("Format: {:?}", archive.format());

            // List files with their CRC32s
            match archive.list_files() {
                Ok(entries) => {
                    println!("\n=== File CRC32s ===");
                    for entry in entries.iter() {
                        if entry.is_file() {
                            if let Some(crc32) = entry.crc32 {
                                println!("  {:08X}  {}", crc32, entry.path);
                            } else {
                                println!("  ????????  {} (no CRC32)", entry.path);
                            }
                        }
                    }

                    println!("\n=== Archive CRC32 ===");

                    // Calculate archive-level CRC (sum of all file CRCs)
                    match archive.calculate_archive_crc() {
                        Ok(archive_crc) => {
                            println!("Archive CRC: {:08X}", archive_crc);
                            println!("           (decimal: {})", archive_crc);

                            println!("\n📝 Notes:");
                            println!(
                                "  - This is the same CRC shown in 7-Zip's archive properties"
                            );
                            println!(
                                "  - It's the arithmetic sum of all file CRC32s (with overflow)"
                            );
                            println!(
                                "  - Same files = same archive CRC, regardless of format/compression"
                            );
                            println!("  - File order doesn't matter (addition is commutative)");
                        }
                        Err(e) => {
                            eprintln!("Failed to calculate archive CRC: {:?}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to list files: {:?}", e);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to open archive: {:?}", e);
            std::process::exit(1);
        }
    }
}
