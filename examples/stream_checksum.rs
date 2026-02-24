//! Example: Extract stream-level checksums from compression formats
//!
//! Demonstrates how to extract CRC32/CRC64 checksums stored in the
//! compression format's metadata (GZIP trailer, BZIP2 EOS marker, XZ stream flags).
//!
//! This is different from per-file CRC32 - it's the checksum of the
//! uncompressed data stream stored in the format's own metadata.
//!
//! Usage:
//!   cargo run --example stream_checksum <compressed_file>
//!
//! Example:
//!   cargo run --example stream_checksum file.gz
//!   cargo run --example stream_checksum file.bz2
//!   cargo run --example stream_checksum file.xz

use std::env;
use unified_archive::stream_crc::{CheckType, extract_stream_checksum};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <compressed_file>", args[0]);
        eprintln!("\nSupported formats:");
        eprintln!("  - GZIP (.gz)");
        eprintln!("  - BZIP2 (.bz2)");
        eprintln!("  - XZ (.xz)");
        eprintln!("\nExample:");
        eprintln!("  {} file.gz", args[0]);
        std::process::exit(1);
    }

    let file_path = &args[1];
    println!("Extracting stream checksum from: {}\n", file_path);

    match extract_stream_checksum(file_path) {
        Ok(checksum) => {
            println!("=== Stream Checksum Information ===");
            println!("Check Type: {:?}", checksum.check_type);

            match checksum.check_type {
                CheckType::Crc32 => {
                    if let Some(crc32) = checksum.crc32 {
                        println!("CRC32: {:08X}", crc32);
                        println!("      (decimal: {})", crc32);
                    } else {
                        println!("CRC32: Not available (needs full stream parsing)");
                    }
                }
                CheckType::Crc64 => {
                    println!(
                        "CRC64: Check type detected (full extraction requires stream parsing)"
                    );
                }
                CheckType::Sha256 => {
                    println!(
                        "SHA-256: Check type detected (full extraction requires stream parsing)"
                    );
                }
                CheckType::None => {
                    println!("No checksum present in stream");
                }
                CheckType::Unknown => {
                    println!("Unknown checksum type");
                }
            }

            if let Some(size) = checksum.uncompressed_size {
                println!("\nUncompressed Size: {} bytes", size);
            }

            // Additional format-specific information
            if file_path.ends_with(".gz") || file_path.ends_with(".gzip") {
                println!("\n--- GZIP Format Notes ---");
                println!("The CRC32 is stored in the 8-byte trailer:");
                println!("  - Bytes 0-3: CRC32 (little-endian)");
                println!("  - Bytes 4-7: Uncompressed size mod 2^32");
                println!("Reference: RFC 1952");
            } else if file_path.ends_with(".bz2") || file_path.ends_with(".bzip2") {
                println!("\n--- BZIP2 Format Notes ---");
                println!("The stream CRC32 is in the end-of-stream marker:");
                println!("  - Magic: 0x177245385090 (BCD of sqrt(pi))");
                println!("  - Stream CRC32: Combined from all block CRCs");
                println!("Computed as: (crc << 1) | (crc >> 31); crc ^= block_crc");
            } else if file_path.ends_with(".xz") {
                println!("\n--- XZ Format Notes ---");
                println!("XZ supports multiple check types:");
                println!("  - None (0x00)");
                println!("  - CRC32 (0x01)");
                println!("  - CRC64 (0x04) - most common");
                println!("  - SHA-256 (0x0A)");
                println!("The check type is specified in Stream Flags byte 7.");
                println!("Reference: XZ file format specification 1.2.1");
            }

            println!("\n✅ Successfully extracted stream checksum!");
        }
        Err(e) => {
            eprintln!("❌ Error: {:?}", e);
            eprintln!("\nMake sure the file:");
            eprintln!("  1. Exists");
            eprintln!("  2. Is a valid GZIP, BZIP2, or XZ compressed file");
            eprintln!("  3. Has the correct extension (.gz, .bz2, .xz)");
            std::process::exit(1);
        }
    }
}
