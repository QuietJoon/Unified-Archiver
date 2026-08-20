//! Example: Detecting Self-Extracting Archives (SFX)
//!
//! This example demonstrates how to detect whether a file is a self-extracting
//! archive (executable with embedded archive data) and extract information about it.
//!
//! Usage:
//!   cargo run --example detect_sfx -- <file_path>
//!
//! Example:
//!   cargo run --example detect_sfx -- installer.exe

use std::env;
use std::process;
use unified_archive::{Archive, ArchiveError};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        eprintln!("Usage: {} <file_path>", args[0]);
        eprintln!("\nExample:");
        eprintln!("  {} installer.exe", args[0]);
        process::exit(1);
    }

    let file_path = &args[1];

    println!("🔍 Detecting SFX in: {}\n", file_path);

    match detect_sfx_example(file_path) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("❌ Error: {}", e);
            process::exit(1);
        }
    }
}

fn detect_sfx_example(file_path: &str) -> Result<(), ArchiveError> {
    // Detect if file is SFX
    let detection = Archive::detect_sfx(file_path)?;

    if !detection.is_sfx() {
        println!("❌ Not a self-extracting archive");
        println!("   This is a regular file or standard archive format.");
        return Ok(());
    }

    // Print detection results
    println!("✅ Self-Extracting Archive Detected!\n");
    println!("Detection Summary:");
    println!("  {}\n", detection.summary());

    // Detailed information. `payload_coordinates()` returns the format, offset
    // and stub type as a unit, so there is no way to read a half-populated
    // result — prefer it over unwrapping the three accessors separately.
    println!("Detailed Information:");
    if let Some((format, offset, stub)) = detection.payload_coordinates() {
        println!("  Executable Type: {stub:?}");
        println!("  Archive Format:  {format:?}");
        println!("  Archive Offset:  {offset} bytes (0x{offset:X})");
    }
    // `confidence()` is the whole answer here. Production detection emits only
    // `NotSfx` / `Probable`, so an `is_confirmed()` line would print the same
    // "No" for every input — `Confirmed` is reserved for callers that verify a
    // payload out of band.
    println!("  Confidence:      {:?}", detection.confidence());
    if !detection.evidence().is_empty() {
        println!("  Evidence:");
        for item in detection.evidence() {
            println!("    - {item}");
        }
    }

    // Try to list files in the embedded archive
    println!("\n📂 Attempting to list embedded archive contents...");
    match Archive::open_sfx(file_path) {
        Ok(archive) => match archive.list_files() {
            Ok(entries) => {
                println!("   Found {} file(s) in embedded archive:\n", entries.len());
                for (i, entry) in entries.iter().take(10).enumerate() {
                    println!(
                        "   {}. {} ({} bytes)",
                        i + 1,
                        entry.path,
                        entry.size.unwrap_or(0)
                    );
                }
                if entries.len() > 10 {
                    println!("   ... and {} more files", entries.len() - 10);
                }
            }
            Err(e) => {
                println!("   ⚠️  Could not list files: {}", e);
                println!("   (Archive format may not be fully supported yet)");
            }
        },
        Err(e) => {
            println!("   ⚠️  Could not open embedded archive: {}", e);
        }
    }

    // Security analysis: stub extraction
    println!("\n🔐 Security Analysis:");
    match Archive::extract_stub(file_path, &detection) {
        Ok(stub_data) => {
            println!("   Stub Size:       {} bytes", stub_data.len());
            println!("   Stub Type:       {:?}", detection.stub_type().unwrap());
            println!("   ");
            println!("   ⚠️  Security Note:");
            println!("   The executable stub contains code that runs before extraction.");
            println!("   For untrusted files, analyze the stub in a sandbox environment.");
        }
        Err(e) => {
            println!("   Could not extract stub: {}", e);
        }
    }

    Ok(())
}
