//! Load-oriented regression coverage for larger synthetic archives.
//!
//! These tests intentionally stay below benchmark scale, but they exercise the
//! regular public API with enough entries to catch batching, caching, and
//! extraction-path issues that do not show up on tiny fixtures.

use std::fs;
use unified_archive::{
    Archive, ArchiveFormat, CompressionLevel, CompressionOptions, ExtractionOptions,
};
use walkdir::WalkDir;

const ENTRY_COUNT: usize = 512;

fn make_archive_path(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    dir.join(name)
}

fn entry_path(index: usize) -> String {
    format!("group_{:02}/file_{:04}.txt", index % 16, index)
}

fn entry_contents(index: usize) -> Vec<u8> {
    format!("payload-{index:04}-{}", "x".repeat(index % 31)).into_bytes()
}

fn build_large_zip_archive(path: &std::path::Path) {
    let mut options = CompressionOptions::new(ArchiveFormat::Zip);
    options.level = CompressionLevel::Store;

    let mut archive = Archive::create(path, options).unwrap();
    for index in 0..ENTRY_COUNT {
        archive
            .add_file_from_data(&entry_path(index), &entry_contents(index))
            .unwrap();
    }
    archive.finish().unwrap();
}

fn count_extracted_files(root: &std::path::Path) -> usize {
    WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .count()
}

#[test]
fn load_many_small_entries_roundtrip() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = make_archive_path(temp.path(), "load_roundtrip.zip");
    build_large_zip_archive(&archive_path);

    let archive = Archive::open(&archive_path).unwrap();
    let entries = archive.list_files().unwrap();
    assert_eq!(entries.len(), ENTRY_COUNT);
    assert_eq!(archive.entry_count().unwrap(), ENTRY_COUNT);

    let sample = archive.find_entry(&entry_path(ENTRY_COUNT - 1)).unwrap();
    assert!(sample.is_some());
    assert_eq!(sample.unwrap().path, entry_path(ENTRY_COUNT - 1));

    let (digest, total_size) = archive.calculate_manifest_summary().unwrap();
    assert!(!digest.is_empty());
    assert!(total_size > ENTRY_COUNT as u64);

    let report = archive.validate_integrity().unwrap();
    assert!(report.failed.is_empty());
    assert_eq!(report.total_entries, ENTRY_COUNT);
    assert_eq!(report.validated, ENTRY_COUNT);
}

#[test]
fn load_extraction_paths_handle_many_entries() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = make_archive_path(temp.path(), "load_extract.zip");
    build_large_zip_archive(&archive_path);

    let archive = Archive::open(&archive_path).unwrap();

    let extract_all_dir = temp.path().join("extract_all");
    archive
        .extract_all(ExtractionOptions {
            destination: extract_all_dir.clone(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(count_extracted_files(&extract_all_dir), ENTRY_COUNT);

    let subset_indices = [0usize, 7, 23, 101, 255, 511];
    let subset_paths: Vec<String> = subset_indices.iter().map(|&idx| entry_path(idx)).collect();
    let subset_refs: Vec<&str> = subset_paths.iter().map(String::as_str).collect();

    let extract_files_dir = temp.path().join("extract_files");
    archive
        .extract_files(
            &subset_refs,
            ExtractionOptions {
                destination: extract_files_dir.clone(),
                ..Default::default()
            },
        )
        .unwrap();
    for &idx in &subset_indices {
        let output_path = extract_files_dir.join(entry_path(idx));
        assert_eq!(fs::read(output_path).unwrap(), entry_contents(idx));
    }

    let extract_ids_dir = temp.path().join("extract_ids");
    archive
        .extract_by_ids(
            &subset_indices,
            ExtractionOptions {
                destination: extract_ids_dir.clone(),
                ..Default::default()
            },
        )
        .unwrap();
    for &idx in &subset_indices {
        let output_path = extract_ids_dir.join(entry_path(idx));
        assert_eq!(fs::read(output_path).unwrap(), entry_contents(idx));
    }

    let extract_filtered_dir = temp.path().join("extract_filtered");
    archive
        .extract_filtered(
            |entry| entry.path.starts_with("group_00/"),
            ExtractionOptions {
                destination: extract_filtered_dir.clone(),
                ..Default::default()
            },
        )
        .unwrap();

    let expected_filtered = (0..ENTRY_COUNT)
        .filter(|index| entry_path(*index).starts_with("group_00/"))
        .count();
    assert_eq!(
        count_extracted_files(&extract_filtered_dir),
        expected_filtered
    );
}
