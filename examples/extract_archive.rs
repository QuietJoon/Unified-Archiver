//! Extract an archive to a destination directory
//!
//! Usage: cargo run --example extract_archive -- <archive_path> [destination]

use std::env;
use std::ops::ControlFlow;
use std::path::PathBuf;
use unified_archive::{Archive, ExtractionOptions};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 || args.len() > 3 {
        eprintln!("Usage: {} <archive_path> [destination]", args[0]);
        std::process::exit(1);
    }

    let path = &args[1];
    let destination = if args.len() == 3 {
        PathBuf::from(&args[2])
    } else {
        PathBuf::from(".")
    };

    let archive = Archive::open(path).unwrap_or_else(|e| {
        eprintln!("Failed to open {}: {}", path, e);
        std::process::exit(1);
    });

    let entries = archive.list_files().unwrap_or_else(|e| {
        eprintln!("Failed to list files: {}", e);
        std::process::exit(1);
    });

    println!(
        "Extracting {} ({} entries) to {}",
        path,
        entries.len(),
        destination.display()
    );

    let options = ExtractionOptions {
        destination,
        progress: Some(Box::new(|current: u64, total: Option<u64>| {
            if let Some(t) = total {
                if t > 0 {
                    print!("\rProgress: {:.0}%", (current as f64 / t as f64) * 100.0);
                }
            }
            ControlFlow::Continue(())
        })),
        ..Default::default()
    };

    let result = archive.extract_all(options).unwrap_or_else(|e| {
        eprintln!("\nExtraction failed: {}", e);
        std::process::exit(1);
    });

    for warning in &result.warnings {
        eprintln!("\nwarning: {warning}");
    }

    println!("\nDone.");
}
