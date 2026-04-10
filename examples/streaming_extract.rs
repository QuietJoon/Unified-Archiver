//! Example: Memory-efficient streaming extraction
//!
//! Demonstrates how to extract large files from archives without loading
//! entire contents into memory. Suitable for processing multi-gigabyte files
//! with minimal memory footprint.
//!
//! Usage:
//!   cargo run --example streaming_extract <archive_path> <file_to_extract>
//!
//! Example:
//!   cargo run --example streaming_extract test.rar large_video.mp4

use std::env;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use unified_archive::Archive;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <archive_path> <file_to_extract>", args[0]);
        eprintln!();
        eprintln!("Example:");
        eprintln!("  {} archive.rar document.pdf", args[0]);
        std::process::exit(1);
    }

    let archive_path = PathBuf::from(&args[1]);
    let file_to_extract = &args[2];

    println!("Opening archive: {}", archive_path.display());

    // Open the archive
    let archive = Archive::open(&archive_path)?;

    println!("Archive format: {:?}", archive.format());
    println!("Extracting file: {}", file_to_extract);
    println!();

    // Create streaming extractor
    let mut stream = archive.extract_to_stream(file_to_extract)?;

    // Show initial state
    if let Some(total_size) = stream.total_size() {
        println!(
            "File size: {} bytes ({:.2} MB)",
            total_size,
            total_size as f64 / 1_048_576.0
        );
    } else {
        println!("File size: unknown");
    }

    // Extract to output file with progress tracking
    let output_path = PathBuf::from(file_to_extract);
    let output_name = output_path
        .file_name()
        .unwrap_or_else(|| output_path.as_os_str());
    let mut output_file = File::create(output_name)?;

    println!("Writing to: {}", output_name.to_string_lossy());
    println!();

    // Read in chunks and write to file
    let chunk_size = 64 * 1024; // 64 KB chunks for good balance
    let mut buffer = vec![0u8; chunk_size];
    let mut last_progress = 0.0;

    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break, // EOF
            Ok(n) => {
                // Write chunk to output file
                output_file.write_all(&buffer[..n])?;

                // Display progress (update every 5%)
                if let Some(progress) = stream.progress() {
                    let progress_pct = (progress * 100.0).round();
                    if progress_pct >= last_progress + 5.0 || progress >= 1.0 {
                        print_progress(stream.bytes_read(), stream.total_size(), progress);
                        last_progress = progress_pct;
                    }
                }
            }
            Err(e) => {
                eprintln!("\nError reading stream: {}", e);
                return Err(Box::new(e));
            }
        }
    }

    // Final progress update
    println!();
    println!("✓ Extraction complete!");
    println!("  Total bytes read: {}", stream.bytes_read());
    println!("  Output file: {}", output_name.to_string_lossy());

    Ok(())
}

/// Print progress bar to terminal
fn print_progress(bytes_read: u64, total_size: Option<u64>, progress: f64) {
    let progress_pct = (progress * 100.0).round() as i32;

    if let Some(total) = total_size {
        let mb_read = bytes_read as f64 / 1_048_576.0;
        let mb_total = total as f64 / 1_048_576.0;
        print!(
            "\rProgress: [{:3}%] {:.2} MB / {:.2} MB",
            progress_pct, mb_read, mb_total
        );
    } else {
        let mb_read = bytes_read as f64 / 1_048_576.0;
        print!("\rProgress: {:.2} MB read", mb_read);
    }

    io::stdout().flush().unwrap();
}
