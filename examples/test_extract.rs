//! Quick test to debug extraction

use std::path::PathBuf;
use unified_archive::{Archive, ExtractionOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let temp = PathBuf::from("/Users/kazuki/Temp/claude/7zip-rbinding-test");
    std::fs::create_dir_all(&temp)?;

    println!("Opening archive...");
    let archive = Archive::open("tests/fixtures/test.rar")?;
    println!("Archive format: {:?}", archive.format());

    let options = ExtractionOptions {
        destination: temp.clone(),
        ..Default::default()
    };

    println!("Extracting to: {}", temp.display());
    archive.extract_all(options)?;
    println!("Extraction complete");

    // List files in destination
    println!("\nFiles in destination:");
    for entry in std::fs::read_dir(&temp)? {
        let entry = entry?;
        println!("  - {}", entry.file_name().to_string_lossy());
    }

    Ok(())
}
