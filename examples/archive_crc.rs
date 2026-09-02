//! Example: Archive-level CRC32, and what it is actually comparable to
//!
//! Demonstrates the "archive CRC" 7-Zip shows in its archive properties:
//! the wrapping arithmetic sum of every CRC32 the *listing* carries.
//!
//! # The sum is only format-independent for CRC-carrying formats (R0001-0078)
//!
//! ZIP, 7z and RAR expose a CRC32 per entry, so two archives in those formats
//! holding the same files produce the same sum regardless of compression
//! method, entry order (addition commutes) or archive metadata.
//!
//! The libarchive-backed formats do not: TAR and its compressed wrappers,
//! ISO, and standalone .gz/.bz2/.xz have no per-entry CRC32 in their listing,
//! and listing stays metadata-only by design (MADR-0001), so those entries
//! contribute nothing to the sum. Two unrelated TAR.GZ archives therefore
//! both print `00000000` — which is why a `0` here is ambiguous between an
//! empty archive, a listing with no CRC32s at all, and CRC32s that genuinely
//! wrapped to zero (a valid sum, AD 0012). This example reports which of
//! those it is instead of letting the reader assume content identity.
//!
//! Pass `--digest` for `Archive::calculate_content_multiset_digest_and_size`,
//! a content-identity hash that streams CRC-less entries to obtain a real
//! CRC32 (AD 0047) and so stays comparable across formats. It is opt-in
//! because on CRC-less formats it decompresses every entry. Its size term is
//! a `SizedContentTotal`, not a `u64`, for the same reason the CRC sum above
//! is ambiguous: raw `.gz`/`.bz2`/`.xz` members declare no uncompressed size
//! and contribute nothing, so this example reports the coverage instead of
//! printing a number that looks exact (OI-0001-007).
//!
//! Usage:
//!   cargo run --example archive_crc -- <archive_file> [--digest]

use std::env;
use unified_archive::Archive;

fn main() {
    let args: Vec<String> = env::args().collect();
    let want_digest = args.iter().any(|arg| arg == "--digest");
    let positional: Vec<&String> = args
        .iter()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .collect();
    if positional.len() != 1 {
        eprintln!("Usage: {} <archive_file> [--digest]", args[0]);
        eprintln!("\nExample:");
        eprintln!("  {} file.zip", args[0]);
        eprintln!("  {} file.7z", args[0]);
        eprintln!("  {} file.rar", args[0]);
        eprintln!("  {} file.tar.gz --digest", args[0]);
        eprintln!("\n  --digest  also print the cross-format content digest");
        std::process::exit(1);
    }

    let archive_path = positional[0];
    println!("Opening archive: {}\n", archive_path);

    match Archive::open(archive_path) {
        Ok(archive) => {
            println!("Format: {:?}", archive.format());

            // List files with their CRC32s
            match archive.list_files() {
                Ok(entries) => {
                    println!("\n=== File CRC32s ===");
                    let mut file_count = 0usize;
                    let mut without_crc = 0usize;
                    for entry in entries.iter() {
                        if entry.is_file() {
                            file_count += 1;
                            if let Some(crc32) = entry.crc32 {
                                println!("  {:08X}  {}", crc32, entry.path);
                            } else {
                                without_crc += 1;
                                println!("  ????????  {} (no CRC32)", entry.path);
                            }
                        }
                    }

                    println!("\n=== Archive CRC32 ===");

                    // Calculate archive-level CRC (sum of the listed file CRCs)
                    match archive.calculate_archive_crc() {
                        Ok(archive_crc) => {
                            println!("Archive CRC: {:08X}", archive_crc);
                            println!("           (decimal: {})", archive_crc);

                            println!("\n📝 Notes:");
                            println!(
                                "  - This is the same CRC shown in 7-Zip's archive properties"
                            );
                            println!(
                                "  - It's the arithmetic sum of all listed file CRC32s (with overflow)"
                            );
                            println!("  - File order doesn't matter (addition is commutative)");

                            // R0001-0078: only claim format independence when
                            // this listing actually carries a CRC32 for every
                            // file; otherwise say what the sum left out.
                            if file_count == 0 {
                                println!(
                                    "  - This archive has no file entries, so the sum is 0 by definition"
                                );
                            } else if without_crc == 0 {
                                println!(
                                    "  - Every file entry here carries a CRC32, so this sum matches any"
                                );
                                println!(
                                    "    other ZIP/7z/RAR holding the same contents, whatever the compression"
                                );
                            } else {
                                println!(
                                    "  - {} of {} file entries carry no CRC32 in this format's listing",
                                    without_crc, file_count
                                );
                                println!(
                                    "    (TAR family, ISO, standalone .gz/.bz2/.xz); they contributed"
                                );
                                println!(
                                    "    nothing, so this sum is NOT comparable with another archive"
                                );
                                println!(
                                    "  - Re-run with --digest for a value that covers those entries"
                                );
                            }
                            if archive_crc == 0 {
                                println!(
                                    "  - A sum of 0 is ambiguous: empty archive, no CRC32s in the listing,"
                                );
                                println!(
                                    "    or CRC32s that genuinely wrapped to 0 — never read it as \"no content\""
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to calculate archive CRC: {:?}", e);
                        }
                    }

                    // R0001-0078: the content-multiset digest is the value to
                    // compare across formats — it streams CRC-less entries to
                    // compute a real CRC32 instead of skipping them.
                    if want_digest {
                        println!("\n=== Content digest ===");
                        match archive.calculate_content_multiset_digest_and_size() {
                            Ok((digest, total_size)) if digest.is_empty() => {
                                println!("No file entries, so no content digest");
                                // `SizedContentTotal`'s Display already spells
                                // out an incomplete total, so no " bytes"
                                // suffix here (OI-0001-007).
                                println!("Total uncompressed size: {}", total_size);
                            }
                            Ok((digest, total_size)) => {
                                println!("Content digest: {}", digest);
                                println!("Total uncompressed size: {}", total_size);
                                println!("\n📝 Notes:");
                                println!(
                                    "  - Covers every file entry, including CRC-less formats (AD 0047),"
                                );
                                println!("    so it is comparable across ZIP/7z/RAR/TAR/ISO alike");
                                println!(
                                    "  - Content only: names, directory layout and timestamps are not hashed"
                                );
                                println!("  - Costs a full decompression pass on CRC-less formats");
                                // The same "a bare number is ambiguous" point
                                // this example makes about `calculate_archive_crc`,
                                // now made about the size term: raw .gz/.bz2/.xz
                                // members and unset-size libarchive entries
                                // declare no size and contribute nothing
                                // (OI-0001-007).
                                if total_size.is_complete() {
                                    println!(
                                        "  - Every one of the {} file entries declared a size, so the total is exact",
                                        total_size.file_entries()
                                    );
                                } else {
                                    println!(
                                        "  - INCOMPLETE total: {} of {} file entries declare no uncompressed size,",
                                        total_size.unsized_entries(),
                                        total_size.file_entries()
                                    );
                                    println!(
                                        "    so {} bytes is a lower bound, not the archive's size",
                                        total_size.sized_bytes()
                                    );
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to calculate content digest: {:?}", e);
                            }
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
