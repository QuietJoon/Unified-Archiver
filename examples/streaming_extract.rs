//! Example: Chunked extraction with progress reporting
//!
//! Reads one entry through [`Archive::extract_to_stream`] in 64 KB chunks and
//! writes it out, so this process never has to hold the decoded entry in a
//! `Vec<u8>` of its own.
//!
//! # Which formats actually stream (R0001-0076)
//!
//! Only the libarchive-backed formats — TAR and its compressed wrappers
//! (`.tar`, `.tar.gz`, `.tar.bz2`, `.tar.xz`, `.tar.zst`) and ISO — hand back
//! a reader that pulls from the decompressor as you read it, which is the
//! case where peak memory really is one chunk. The native backends (ZIP, 7z,
//! RAR) materialize the whole entry before returning the reader (AD 0035 /
//! DEF-004), so there the loop below is chunked I/O over an already-decoded
//! buffer and peak memory is the entry size. Check the entry size with
//! `Archive::find_entry` first if that matters — this example deliberately
//! demonstrates on a TAR.GZ.
//!
//! The stream is bounded with `StreamBound::DeclaredSize`, which is a
//! two-sided contract: an archive that emits more bytes than its listing
//! promised fails the read with `InvalidData` (DCR-006), and one that ends
//! early fails with `UnexpectedEof` instead of handing back a short payload
//! (R0001-0011). Prefer that for anything untrusted; `StreamBound::Unbounded`
//! opts out of both, and `StreamBound::Cap(n)` is a ceiling-only budget.
//!
//! # Destination handling (R0001-0077)
//!
//! The entry is written under its base name in the current directory. An
//! existing file of that name is never overwritten unless `--force` is given,
//! and bytes land in a sibling `.part` file that is renamed into place only
//! after a clean EOF — a failed decode cannot truncate an existing file or
//! leave a half-written one behind.
//!
//! Usage:
//!   cargo run --example streaming_extract -- <archive_path> <file_to_extract> [--force]
//!
//! Example:
//!   cargo run --example streaming_extract -- backup.tar.gz large_video.mp4

use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use unified_archive::{Archive, StreamBound, StreamingExtractor};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let force = args.iter().any(|arg| arg == "--force");
    let positional: Vec<&String> = args
        .iter()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .collect();
    if positional.len() != 2 {
        eprintln!(
            "Usage: {} <archive_path> <file_to_extract> [--force]",
            args[0]
        );
        eprintln!();
        eprintln!("Example:");
        eprintln!("  {} backup.tar.gz document.pdf", args[0]);
        eprintln!();
        eprintln!("  --force  overwrite an existing destination file");
        std::process::exit(1);
    }

    let archive_path = PathBuf::from(positional[0]);
    let file_to_extract = positional[1].as_str();

    println!("Opening archive: {}", archive_path.display());

    // Open the archive
    let archive = Archive::open(&archive_path)?;

    println!("Archive format: {:?}", archive.format());
    println!("Extracting file: {}", file_to_extract);
    println!();

    // Create streaming extractor. R0001-0076: `StreamBound::DeclaredSize`
    // holds the reader to exactly the entry's declared size, reporting
    // over-production as `InvalidData` (AD 0062 A.2 / DCR-006) and an early
    // end-of-stream as `UnexpectedEof` (R0001-0011); the progress / size
    // accessors used below keep working under the bound, and report the
    // listing's declaration rather than a decoder-defined length.
    let mut stream = archive.extract_to_stream(file_to_extract, StreamBound::DeclaredSize)?;

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

    // Write to the entry's base name in the current directory. An entry
    // path with no file name component (`..`, a trailing separator) is
    // refused rather than guessed at.
    let final_path: PathBuf = Path::new(file_to_extract)
        .file_name()
        .map(PathBuf::from)
        .ok_or_else(|| format!("Entry path has no file name: {}", file_to_extract))?;

    // R0001-0077: never clobber existing data unless the user asked for it.
    // The check is a fail-fast convenience, not a race-free guarantee — the
    // crate's own extraction paths install through `persist_noclobber` for
    // that.
    if final_path.exists() && !force {
        return Err(format!(
            "Destination file already exists: {} (pass --force to overwrite)",
            final_path.display()
        )
        .into());
    }

    // R0001-0077: stage next to the destination, so the destination is only
    // touched by the rename that installs a fully decoded payload.
    let mut staged_name = final_path.clone().into_os_string();
    staged_name.push(format!(".{}.part", std::process::id()));
    let staged_path = PathBuf::from(staged_name);

    println!("Writing to: {}", final_path.display());
    println!("  (staged as {})", staged_path.display());
    println!();

    let bytes_written = match write_entry(&mut stream, &staged_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            // R0001-0077: the destination was never opened; drop the partial
            // staging file so a retry starts clean.
            let _ = fs::remove_file(&staged_path);
            eprintln!("\nError extracting stream: {}", e);
            return Err(Box::new(e));
        }
    };

    // R0001-0077: install atomically — a concurrent reader sees either the
    // previous file or the complete new one, never a partial write.
    if let Err(e) = fs::rename(&staged_path, &final_path) {
        let _ = fs::remove_file(&staged_path);
        eprintln!("\nFailed to install {}: {}", final_path.display(), e);
        return Err(Box::new(e));
    }

    // Final progress update
    println!();
    println!("✓ Extraction complete!");
    println!("  Total bytes read: {}", stream.bytes_read());
    println!("  Bytes written: {}", bytes_written);
    println!("  Output file: {}", final_path.display());

    Ok(())
}

/// Stream the entry into `staged_path` in chunks and flush it to disk.
///
/// Returns the number of bytes written. Every failure — including a cap
/// overrun from a lying header — propagates as an error instead of being
/// mistaken for EOF, so the caller can discard the staging file and leave
/// the destination untouched (R0001-0077).
fn write_entry(stream: &mut StreamingExtractor, staged_path: &Path) -> io::Result<u64> {
    // `create_new` refuses to reuse a staging file left behind by another
    // run rather than appending this extraction on top of it.
    let mut staged_file = File::create_new(staged_path)?;

    let chunk_size = 64 * 1024; // 64 KB chunks for good balance
    let mut buffer = vec![0u8; chunk_size];
    let mut written: u64 = 0;
    let mut last_progress = 0.0;

    loop {
        // Read errors (corruption, I/O, or a `StreamBound::DeclaredSize`
        // over- or under-production) surface here as `Err`, never as a
        // short read.
        match stream.read(&mut buffer)? {
            0 => break, // EOF
            n => {
                // Write chunk to the staging file
                staged_file.write_all(&buffer[..n])?;
                written += n as u64;

                // Display progress (update every 5%)
                if let Some(progress) = stream.progress() {
                    let progress_pct = (progress * 100.0).round();
                    if progress_pct >= last_progress + 5.0 || progress >= 1.0 {
                        print_progress(stream.bytes_read(), stream.total_size(), progress);
                        last_progress = progress_pct;
                    }
                }
            }
        }
    }

    // Flush the payload before the caller renames it into place; otherwise a
    // crash can install a file whose contents never reached the disk.
    staged_file.sync_all()?;
    Ok(written)
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
