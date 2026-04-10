//! Inspect an archive's contents and metadata
//!
//! Usage: cargo run --example inspect_archive -- <archive_path>

use std::env;
use unified_archive::Archive;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <archive_path>", args[0]);
        std::process::exit(1);
    }

    let path = &args[1];
    let archive = Archive::open(path).unwrap_or_else(|e| {
        eprintln!("Failed to open {}: {}", path, e);
        std::process::exit(1);
    });

    println!("Archive: {}", path);
    println!("Format:  {:?}", archive.format());

    let entries = archive.list_files().unwrap_or_else(|e| {
        eprintln!("Failed to list files: {}", e);
        std::process::exit(1);
    });

    println!("Entries: {}\n", entries.len());
    println!("{:<50} {:>12} {:>12} {:>10}", "Path", "Size", "Compressed", "CRC32");
    println!("{}", "-".repeat(88));

    for entry in entries {
        let size = entry
            .size
            .map(|s| format!("{}", s))
            .unwrap_or_else(|| "-".to_string());
        let compressed = entry
            .compressed_size
            .map(|s| format!("{}", s))
            .unwrap_or_else(|| "-".to_string());
        let crc = entry
            .crc32
            .map(|c| format!("{:08X}", c))
            .unwrap_or_else(|| "-".to_string());

        println!("{:<50} {:>12} {:>12} {:>10}", entry.path, size, compressed, crc);
    }
}
