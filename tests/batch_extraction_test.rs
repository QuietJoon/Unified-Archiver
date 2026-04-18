// Test batch extraction features: extract_files() and extract_by_ids()

use std::fs;
use std::path::PathBuf;
use unified_archive::{Archive, ExtractionOptions};

const TEST_TEMP_DIR: &str = "/Volumes/Temp/claude/7zip/batch_extraction";

fn setup_temp_dir(test_name: &str) -> PathBuf {
    let dir = PathBuf::from(TEST_TEMP_DIR).join(test_name);
    if dir.exists() {
        fs::remove_dir_all(&dir).ok();
    }
    fs::create_dir_all(&dir).expect("Failed to create temp directory");
    dir
}

fn cleanup_temp_dir(dir: &PathBuf) {
    if dir.exists() {
        fs::remove_dir_all(dir).ok();
    }
}

#[test]
fn test_extract_files_by_path_array_zip() {
    let temp_dir = setup_temp_dir("extract_files_zip");

    let archive =
        Archive::open("tests/fixtures/batch_test.zip").expect("Failed to open batch test ZIP");

    // Extract specific files by path
    archive
        .extract_files(
            &["file1.txt", "subdir/nested.txt"],
            ExtractionOptions {
                destination: temp_dir.clone(),
                ..Default::default()
            },
        )
        .expect("Failed to extract files");

    // Verify extracted files exist
    assert!(
        temp_dir.join("file1.txt").exists(),
        "file1.txt should be extracted"
    );
    assert!(
        temp_dir.join("subdir/nested.txt").exists(),
        "subdir/nested.txt should be extracted"
    );

    cleanup_temp_dir(&temp_dir);
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_extract_files_by_path_array_rar() {
    let temp_dir = setup_temp_dir("extract_files_rar");

    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open test RAR");

    // List files to get actual file name
    let entries = archive.list_files().expect("Failed to list files");

    if entries.is_empty() {
        cleanup_temp_dir(&temp_dir);
        return;
    }

    // Extract first file by its actual path
    let file_path = &entries[0].path;
    archive
        .extract_files(
            &[file_path.as_str()],
            ExtractionOptions {
                destination: temp_dir.clone(),
                ..Default::default()
            },
        )
        .expect("Failed to extract files");

    // Verify extracted file exists
    assert!(
        temp_dir.join(file_path).exists(),
        "{} should be extracted",
        file_path
    );

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_extract_files_empty_array() {
    let temp_dir = setup_temp_dir("extract_files_empty");

    let archive = Archive::open("tests/fixtures/test.zip").expect("Failed to open test ZIP");

    // Extract with empty array should succeed without error
    archive
        .extract_files(
            &[],
            ExtractionOptions {
                destination: temp_dir.clone(),
                ..Default::default()
            },
        )
        .expect("Empty array extraction should succeed");

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_extract_files_nonexistent_path() {
    let temp_dir = setup_temp_dir("extract_files_nonexistent");

    let archive = Archive::open("tests/fixtures/test.zip").expect("Failed to open test ZIP");

    // Try to extract a nonexistent file
    let result = archive.extract_files(
        &["nonexistent.txt"],
        ExtractionOptions {
            destination: temp_dir.clone(),
            ..Default::default()
        },
    );

    // Should return an error
    assert!(result.is_err(), "Extracting nonexistent file should fail");

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_extract_by_ids_zip() {
    let temp_dir = setup_temp_dir("extract_by_ids_zip");

    let archive =
        Archive::open("tests/fixtures/batch_test.zip").expect("Failed to open batch test ZIP");

    // List files to see IDs
    let entries = archive.list_files().expect("Failed to list files");

    // Verify IDs are sequential
    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(entry.id, i, "Entry ID should match its position");
    }

    // Extract first 2 files by ID
    archive
        .extract_by_ids(
            &[0, 1],
            ExtractionOptions {
                destination: temp_dir.clone(),
                ..Default::default()
            },
        )
        .expect("Failed to extract by IDs");

    // Verify extracted files exist
    if entries.len() >= 2 {
        let path0 = temp_dir.join(&entries[0].path);
        let path1 = temp_dir.join(&entries[1].path);

        if entries[0].is_file() {
            assert!(path0.exists(), "File with ID 0 should be extracted");
        }
        if entries[1].is_file() {
            assert!(path1.exists(), "File with ID 1 should be extracted");
        }
    }

    cleanup_temp_dir(&temp_dir);
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_extract_by_ids_rar() {
    let temp_dir = setup_temp_dir("extract_by_ids_rar");

    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open test RAR");

    // List files to see IDs
    let entries = archive.list_files().expect("Failed to list files");

    // Verify IDs are sequential
    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(entry.id, i, "Entry ID should match its position");
    }

    // Extract by ID 0
    if !entries.is_empty() {
        archive
            .extract_by_ids(
                &[0],
                ExtractionOptions {
                    destination: temp_dir.clone(),
                    ..Default::default()
                },
            )
            .expect("Failed to extract by ID");

        // Verify extracted file exists
        if entries[0].is_file() {
            let path = temp_dir.join(&entries[0].path);
            assert!(path.exists(), "File with ID 0 should be extracted");
        }
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_extract_by_ids_7z() {
    let temp_dir = setup_temp_dir("extract_by_ids_7z");

    let archive = Archive::open("tests/fixtures/test.7z").expect("Failed to open test 7z");

    // List files to see IDs
    let entries = archive.list_files().expect("Failed to list files");

    // Verify IDs are sequential
    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(entry.id, i, "Entry ID should match its position");
    }

    // Extract first file by ID
    if !entries.is_empty() {
        archive
            .extract_by_ids(
                &[0],
                ExtractionOptions {
                    destination: temp_dir.clone(),
                    ..Default::default()
                },
            )
            .expect("Failed to extract by ID");

        // Verify extracted file exists
        if entries[0].is_file() {
            let path = temp_dir.join(&entries[0].path);
            assert!(path.exists(), "File with ID 0 should be extracted");
        }
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_extract_by_ids_empty_array() {
    let temp_dir = setup_temp_dir("extract_by_ids_empty");

    let archive = Archive::open("tests/fixtures/test.zip").expect("Failed to open test ZIP");

    // Extract with empty ID array should succeed
    archive
        .extract_by_ids(
            &[],
            ExtractionOptions {
                destination: temp_dir.clone(),
                ..Default::default()
            },
        )
        .expect("Empty ID array extraction should succeed");

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_extract_by_ids_invalid_id() {
    let temp_dir = setup_temp_dir("extract_by_ids_invalid");

    let archive = Archive::open("tests/fixtures/test.zip").expect("Failed to open test ZIP");

    let entries = archive.list_files().expect("Failed to list files");
    let invalid_id = entries.len() + 10;

    // Try to extract with invalid ID
    let result = archive.extract_by_ids(
        &[invalid_id],
        ExtractionOptions {
            destination: temp_dir.clone(),
            ..Default::default()
        },
    );

    // Should return an error
    assert!(result.is_err(), "Extracting with invalid ID should fail");

    // Error message should mention the invalid ID
    if let Err(e) = result {
        let error_msg = format!("{}", e);
        assert!(
            error_msg.contains(&invalid_id.to_string()),
            "Error should mention invalid ID {}",
            invalid_id
        );
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_extract_by_ids_multiple_files() {
    let temp_dir = setup_temp_dir("extract_by_ids_multiple");

    let archive =
        Archive::open("tests/fixtures/batch_test.zip").expect("Failed to open batch test ZIP");

    let entries = archive.list_files().expect("Failed to list files");

    // Extract multiple files by ID (if archive has enough entries)
    if entries.len() >= 3 {
        archive
            .extract_by_ids(
                &[0, 1, 2],
                ExtractionOptions {
                    destination: temp_dir.clone(),
                    ..Default::default()
                },
            )
            .expect("Failed to extract multiple files by ID");

        // Verify all extracted files exist
        for (i, entry) in entries.iter().enumerate().take(3) {
            if entry.is_file() {
                let path = temp_dir.join(&entry.path);
                assert!(path.exists(), "File with ID {} should be extracted", i);
            }
        }
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_extract_files_parallel_extraction() {
    let temp_dir = setup_temp_dir("extract_files_parallel");

    let archive =
        Archive::open("tests/fixtures/batch_test.zip").expect("Failed to open batch test ZIP");

    let entries = archive.list_files().expect("Failed to list files");

    // Extract 4+ files to trigger parallel extraction
    if entries.len() >= 4 {
        let paths: Vec<&str> = entries
            .iter()
            .filter(|e| e.is_file())
            .take(4)
            .map(|e| e.path.as_str())
            .collect();

        if paths.len() >= 4 {
            archive
                .extract_files(
                    &paths,
                    ExtractionOptions {
                        destination: temp_dir.clone(),
                        ..Default::default()
                    },
                )
                .expect("Failed to extract files in parallel");

            // Verify all extracted files exist
            for path in &paths {
                let full_path = temp_dir.join(path);
                assert!(full_path.exists(), "File {} should be extracted", path);
            }
        }
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
#[serial_test::file_serial(rar)]
fn test_id_consistency_across_formats() {
    // Test that IDs are consistent (0-based sequential) across all formats

    let test_archives = vec![
        ("tests/fixtures/test.zip", "ZIP"),
        ("tests/fixtures/test.rar", "RAR"),
        ("tests/fixtures/test.7z", "7z"),
    ];

    for (path, format) in test_archives {
        if let Ok(archive) = Archive::open(path) {
            if let Ok(entries) = archive.list_files() {
                // Verify IDs are 0-based and sequential
                for (i, entry) in entries.iter().enumerate() {
                    assert_eq!(
                        entry.id, i,
                        "{} format: Entry {} should have ID {}, got {}",
                        format, entry.path, i, entry.id
                    );
                }
            }
        }
    }
}
